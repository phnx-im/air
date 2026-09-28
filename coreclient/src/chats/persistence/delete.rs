// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Synchronizing chat deletions with the user's other devices.
//!
//! Deleting a chat erases it locally and parks a [`DeletedChat`] for the next
//! self-group commit. The siblings erase their copy when the commit arrives.

use aircommon::codec::PersistenceCodec;
use airprotos::client::self_group::DeletedChat;
use openmls::group::GroupId;
use tracing::{error, warn};

use crate::{
    ChatType,
    chats::{Chat, PendingConnectionInfo},
    clients::self_group_outbox::{self, OutboxKind},
    db::access::{ReadConnection, WriteConnection, WriteDbTransaction},
    groups::Group,
};

/// Erases the chat together with its group.
pub(crate) async fn erase(txn: &mut WriteDbTransaction<'_>, chat: &Chat) -> anyhow::Result<()> {
    if let ChatType::PendingConnection(_) = chat.chat_type()
        && let Some(info) = PendingConnectionInfo::load(&mut *txn, chat.id()).await?
        && let Some(hash) = info.connection_offer_hash
        && let Err(error) = Group::delete_connection_offer_psk(&mut *txn, hash)
    {
        error!(%error, "failed to delete connection offer PSK, proceeding with chat deletion.");
    }

    if let Err(error) = Group::delete_from_db(txn, chat.group_id()).await {
        error!(%error, "failed to delete OpenMLS group; skipping");
    }

    Chat::delete(&mut *txn, chat.id()).await?;
    Ok(())
}

/// Parks a local deletion for the next self-group commit.
pub(crate) async fn store_outgoing_deletion(
    connection: impl WriteConnection,
    group_id: &GroupId,
) -> anyhow::Result<()> {
    let deleted = DeletedChat {
        group_id: Some(group_id.clone()),
    };
    self_group_outbox::stage(
        connection,
        OutboxKind::DeletedChat,
        group_id.as_slice(),
        &PersistenceCodec::to_vec(&deleted)?,
        None,
    )
    .await?;
    Ok(())
}

pub(crate) async fn staged_deletions(
    connection: impl ReadConnection,
) -> anyhow::Result<Vec<DeletedChat>> {
    Ok(
        self_group_outbox::load_kind(connection, OutboxKind::DeletedChat)
            .await?
            .iter()
            .filter_map(|entry| PersistenceCodec::from_slice(&entry.payload).ok())
            .collect(),
    )
}

/// Drops the parked deletions.
pub(crate) async fn remove_staged_deletion(
    txn: &mut WriteDbTransaction<'_>,
    deleted: &[DeletedChat],
) -> sqlx::Result<()> {
    for DeletedChat { group_id } in deleted {
        let Some(group_id) = group_id else {
            continue;
        };
        self_group_outbox::remove(&mut *txn, OutboxKind::DeletedChat, group_id.as_slice()).await?;
    }
    Ok(())
}

/// Erases the chats a sibling deleted.
pub(crate) async fn apply_deleted_chats(
    txn: &mut WriteDbTransaction<'_>,
    deleted: &[DeletedChat],
) -> anyhow::Result<()> {
    for DeletedChat { group_id } in deleted {
        let Some(group_id) = group_id else {
            warn!("Skipping a deleted chat without a group id");
            continue;
        };
        if let Some(chat) = Chat::load_by_group_id(&mut *txn, group_id).await? {
            erase(txn, &chat).await?;
        }
    }
    remove_staged_deletion(txn, deleted).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use sqlx::SqlitePool;

    use crate::{chats::persistence::tests::test_chat, db::access::DbAccess};

    use super::*;

    fn group_id(n: u8) -> GroupId {
        GroupId::from_slice(&[n; 16])
    }

    fn deleted(n: u8) -> DeletedChat {
        DeletedChat {
            group_id: Some(group_id(n)),
        }
    }

    #[sqlx::test]
    async fn staged_is_sorted_and_deduplicated(pool: SqlitePool) -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(pool);

        pool.with_write_transaction(async |txn| -> anyhow::Result<()> {
            store_outgoing_deletion(&mut *txn, &group_id(2)).await?;
            store_outgoing_deletion(&mut *txn, &group_id(1)).await?;
            store_outgoing_deletion(&mut *txn, &group_id(2)).await?;

            assert_eq!(
                staged_deletions(&mut *txn).await?,
                vec![deleted(1), deleted(2)]
            );
            Ok(())
        })
        .await
    }

    #[sqlx::test]
    async fn remove_staged_drops_only_the_named_chats(pool: SqlitePool) -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(pool);

        pool.with_write_transaction(async |txn| -> anyhow::Result<()> {
            store_outgoing_deletion(&mut *txn, &group_id(1)).await?;
            store_outgoing_deletion(&mut *txn, &group_id(2)).await?;

            remove_staged_deletion(txn, &[deleted(1), DeletedChat::default()]).await?;

            assert_eq!(staged_deletions(&mut *txn).await?, vec![deleted(2)]);
            Ok(())
        })
        .await
    }

    #[sqlx::test]
    async fn apply_skips_unknown_chats_and_drops_covered_ones(
        pool: SqlitePool,
    ) -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(pool);

        pool.with_write_transaction(async |txn| -> anyhow::Result<()> {
            store_outgoing_deletion(&mut *txn, &group_id(1)).await?;
            store_outgoing_deletion(&mut *txn, &group_id(2)).await?;

            apply_deleted_chats(txn, &[deleted(1), DeletedChat::default()]).await?;

            assert_eq!(staged_deletions(&mut *txn).await?, vec![deleted(2)]);
            Ok(())
        })
        .await
    }

    #[sqlx::test]
    async fn apply_erases_a_known_chat(pool: SqlitePool) -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(pool);
        let chat = test_chat();

        pool.with_write_transaction(async |txn| -> anyhow::Result<()> {
            chat.store(&mut *txn).await?;

            let deleted = DeletedChat {
                group_id: Some(chat.group_id().clone()),
            };
            apply_deleted_chats(txn, &[deleted]).await?;

            assert!(Chat::load(&mut *txn, &chat.id()).await?.is_none());
            Ok(())
        })
        .await
    }
}
