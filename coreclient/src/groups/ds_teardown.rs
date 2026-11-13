// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use airapiclient::ds_api::DsRequestError;
use openmls::prelude::GroupId;
use tracing::error;

use crate::{
    ChatStatus,
    clients::own_client_info::OwnClientInfo,
    db::access::WriteDbTransaction,
    groups::Group,
    outbound_service::resync::{Resync, ResyncReason},
};

/// Handles the DS reporting a group gone, by tearing it down with
/// [`tear_down_group`].
///
/// The own self group is kept instead, and a queued resync of it is marked
/// failed. The DS reply is not authenticated, and ending the self group cuts
/// this device off from its siblings. Only a resync that finds a tombstone in
/// the signed group info ends it.
pub(crate) async fn handle_group_not_found_on_ds(
    txn: &mut WriteDbTransaction<'_>,
    group_id: &GroupId,
) -> anyhow::Result<()> {
    if OwnClientInfo::is_own_self_group(&mut *txn, group_id).await? {
        error!(
            ?group_id,
            "the DS reports the own self group gone, keeping it"
        );
        Resync::mark_failed(&mut *txn, group_id, "the DS reports the self group gone").await?;
        return Ok(());
    }
    tear_down_group(txn, group_id).await
}

/// Deactivates the chat of a group and deletes the MLS group.
///
/// Mirrors what happens when we process a deletion commit from another member:
/// the chat is marked inactive (preserving history) and the MLS group is
/// deleted. The group deletion cascades to `resync_queue`,
/// `pending_chat_operation`, and `group_membership` via foreign keys.
///
/// This function is idempotent, safe to call even if the group or chat is
/// already gone.
pub(crate) async fn tear_down_group(
    txn: &mut WriteDbTransaction<'_>,
    group_id: &GroupId,
) -> anyhow::Result<()> {
    // Collect past members before deleting the group.
    let past_members = match Group::load(&mut *txn, group_id).await? {
        Some(group) => group.members().collect(),
        None => Vec::new(),
    };

    // Mark the chat as inactive so the user sees it's dead. We do this even
    // for blocked chats so they stay inactive if the user later unblocks the
    // contact.
    if let Some(mut chat) = crate::Chat::load_by_group_id(&mut *txn, group_id).await?
        && !matches!(chat.status(), crate::ChatStatus::Inactive(_))
    {
        chat.set_status(&mut *txn, ChatStatus::inactive(past_members))
            .await?;
    }

    // Remove any pending resync for this group (FK is on chat_id, not
    // group_id, so it won't cascade from Group::delete_from_db).
    Resync::remove(&mut *txn, group_id).await?;

    // Delete the MLS group. This cascades to pending_chat_operation and
    // group_membership via FK.
    Group::delete_from_db(txn, group_id).await?;

    Ok(())
}

/// Handles the DS rejecting a request because the group is closed, i.e. its
/// context carries a tombstone which we missed when it was added.
///
/// Schedules a resync of the group, which finds the tombstone and ends the chat
/// with a message naming who closed the group. Without an active chat there is
/// nothing to announce, and the group is cleaned up like one the DS doesn't
/// know.
///
/// A closed own self group means that the account is deleted. The DS reply is
/// not authenticated, so for the self group too this only schedules the resync,
/// which [`Resync::enqueue_or_reset`] queues as a self-group check. The check
/// verifies the tombstone in the signed group info and then records the
/// deletion.
pub(crate) async fn handle_group_closed_on_ds(
    txn: &mut WriteDbTransaction<'_>,
    group_id: &GroupId,
) -> anyhow::Result<()> {
    let Some(group) = Group::load(&mut *txn, group_id).await? else {
        return handle_group_not_found_on_ds(txn, group_id).await;
    };
    let Some(chat) = crate::Chat::load_by_group_id(&mut *txn, group_id).await? else {
        return handle_group_not_found_on_ds(txn, group_id).await;
    };
    if chat.status().is_inactive() {
        return handle_group_not_found_on_ds(txn, group_id).await;
    }
    Resync::for_group(chat.id(), &group, ResyncReason::FutureEpoch)
        .enqueue_or_reset(&mut *txn)
        .await?;
    Ok(())
}

/// Handles a DS request `error` for which [`DsRequestError::is_not_found`]
/// holds, which includes a closed group.
pub(crate) async fn handle_group_gone_on_ds(
    txn: &mut WriteDbTransaction<'_>,
    group_id: &GroupId,
    error: &DsRequestError,
) -> anyhow::Result<()> {
    if error.is_group_closed() {
        handle_group_closed_on_ds(txn, group_id).await
    } else {
        handle_group_not_found_on_ds(txn, group_id).await
    }
}

#[cfg(test)]
mod tests {
    use aircommon::{
        credentials::test_utils::create_test_credentials,
        crypto::aead::keys::IdentityLinkWrapperKey,
        identifiers::{QsClientId, QsUserId, QualifiedGroupId, UserId},
    };
    use airprotos::client::group::GroupData;
    use chrono::Utc;
    use sqlx::query;
    use uuid::Uuid;

    use super::*;
    use crate::{
        Chat, ChatAttributes, ChatId, ChatStatus,
        clients::{block_contact::BlockedContact, own_client_info::OwnClientInfo},
        db::access::{DbAccess, WriteConnection},
        groups::NewGroupContext,
        outbound_service::resync::ResyncStatus,
        utils::persistence::open_db_in_memory,
    };

    /// A group with an active chat that `own_client_info` names as the own self
    /// group, and a queued resync of it.
    async fn setup_own_self_group() -> anyhow::Result<(DbAccess, GroupId, ChatId)> {
        let pool = DbAccess::for_tests(open_db_in_memory().await?);
        let mut connection = pool.write().await?;

        let own_user_id = UserId::random("example.com".parse()?);
        let (_as_signing_key, user_signing_key) = create_test_credentials(own_user_id);
        let qgid = QualifiedGroupId::new(Uuid::new_v4(), "example.com".parse()?);
        let group_id = GroupId::from(qgid);

        let (group, _) = Group::create_group(
            &mut connection,
            &user_signing_key,
            IdentityLinkWrapperKey::random()?,
            group_id.clone(),
            NewGroupContext::LegacyChat(GroupData::empty()),
            None,
        )?;
        group.store(&mut connection).await?;

        OwnClientInfo {
            qs_user_id: QsUserId::random(),
            qs_client_id: QsClientId::random(&mut rand::rng()),
            user_id: user_signing_key.credential().user_id().clone(),
            client_id: Uuid::new_v4(),
            self_group_id: Some(group_id.clone()),
            self_group_signing_key: None,
        }
        .store(&mut connection)
        .await?;

        let chat = Chat::new_group_chat(
            group_id.clone(),
            ChatAttributes::new("Self chat".into(), None),
        );
        let chat_id = chat.id();
        chat.store(&mut connection).await?;

        Resync::for_group(chat_id, &group, ResyncReason::FutureEpoch)
            .enqueue_or_reset(&mut connection)
            .await?;

        drop(connection);
        Ok((pool, group_id, chat_id))
    }

    async fn assert_self_group_kept(
        pool: &DbAccess,
        group_id: &GroupId,
        chat_id: ChatId,
    ) -> anyhow::Result<()> {
        assert!(Group::load(pool.read().await?, group_id).await?.is_some());
        let chat = pool
            .with_read_transaction(async |txn| Chat::load(txn, &chat_id).await)
            .await?
            .expect("the self chat is kept");
        assert!(!chat.status().is_inactive());
        assert_eq!(
            OwnClientInfo::account_unlink_reason(pool.read().await?).await?,
            None
        );
        Ok(())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn not_found_reply_keeps_own_self_group_and_fails_its_resync() -> anyhow::Result<()> {
        let (pool, group_id, chat_id) = setup_own_self_group().await?;

        pool.write()
            .await?
            .with_transaction(async |txn| handle_group_not_found_on_ds(txn, &group_id).await)
            .await?;

        assert_self_group_kept(&pool, &group_id, chat_id).await?;
        assert_eq!(
            Resync::status(pool.read().await?, &group_id).await?,
            Some(ResyncStatus::Failed)
        );
        Ok(())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn closed_reply_for_own_self_group_only_schedules_a_resync() -> anyhow::Result<()> {
        let (pool, group_id, chat_id) = setup_own_self_group().await?;
        Resync::mark_failed(pool.write().await?, &group_id, "earlier failure").await?;

        pool.write()
            .await?
            .with_transaction(async |txn| handle_group_closed_on_ds(txn, &group_id).await)
            .await?;

        assert_self_group_kept(&pool, &group_id, chat_id).await?;
        assert_eq!(
            Resync::status(pool.read().await?, &group_id).await?,
            Some(ResyncStatus::Pending)
        );
        let resync = pool
            .write()
            .await?
            .with_transaction(async |txn| Resync::dequeue(txn, Uuid::new_v4(), Utc::now()).await)
            .await?
            .expect("the entry is due");
        assert_eq!(resync.reason, ResyncReason::SelfGroupCheck);
        Ok(())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn handle_group_not_found_marks_blocked_chat_inactive_under_block() -> anyhow::Result<()>
    {
        let pool = DbAccess::for_tests(open_db_in_memory().await?);
        let mut connection = pool.write().await?;

        let own_user_id = UserId::random("example.com".parse().unwrap());
        let blocked_user_id = UserId::random("example.com".parse().unwrap());
        let (_as_signing_key, user_signing_key) = create_test_credentials(own_user_id);

        let qgid = QualifiedGroupId::new(Uuid::new_v4(), "example.com".parse().unwrap());
        let group_id = GroupId::from(qgid);

        let (group, _) = Group::create_group(
            &mut connection,
            &user_signing_key,
            IdentityLinkWrapperKey::random()?,
            group_id.clone(),
            NewGroupContext::LegacyChat(GroupData::empty()),
            None,
        )?;
        group.store(&mut connection).await?;

        // Loading a group resolves the owner's identity, which requires an own_client_info row.
        OwnClientInfo {
            qs_user_id: QsUserId::random(),
            qs_client_id: QsClientId::random(&mut rand::rng()),
            user_id: user_signing_key.credential().user_id().clone(),
            client_id: Uuid::new_v4(),
            self_group_id: None,
            self_group_signing_key: None,
        }
        .store(&mut connection)
        .await?;

        let chat = Chat::new_targeted_message_chat(group_id.clone(), blocked_user_id.clone());
        let chat_id = chat.id();
        chat.store(&mut connection).await?;

        BlockedContact::new(blocked_user_id.clone())
            .store(&mut connection)
            .await?;

        let mut txn = connection.begin().await?;

        assert!(matches!(
            Chat::load(&mut txn, &chat_id)
                .await?
                .expect("chat should exist")
                .status(),
            ChatStatus::Blocked
        ));

        handle_group_not_found_on_ds(&mut txn, &group_id).await?;

        assert!(Group::load(&mut txn, &group_id).await?.is_none());
        assert!(matches!(
            Chat::load(&mut txn, &chat_id)
                .await?
                .expect("chat should still exist")
                .status(),
            ChatStatus::Blocked
        ));

        query("DELETE FROM blocked_contact WHERE user_uuid = ?1 AND user_domain = ?2")
            .bind(blocked_user_id.uuid())
            .bind(blocked_user_id.domain().to_string())
            .execute(txn.as_mut())
            .await?;

        assert!(matches!(
            Chat::load(&mut txn, &chat_id)
                .await?
                .expect("chat should still exist after unblock")
                .status(),
            ChatStatus::Inactive(_)
        ));

        Ok(())
    }
}
