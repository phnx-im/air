// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use airprotos::client::self_group::SettingsUpdate;
use anyhow::Result;
use chrono::Utc;

use crate::{
    chats::{
        self,
        connection_requests::{self, ConnectionRequestEffects},
    },
    clients::{
        block_contact,
        user_settings::{SettingChanges, apply_settings_update, merge_settings_update},
    },
    db::access::WriteDbTransaction,
    groups::self_group_message_key::SelfGroupPayload,
    privacy_pass, usernames,
};

/// Applies the messages a self-group commit carried.
///
/// `own_echo` marks the DS fanning one of our own commits back: the values are
/// applied locally already, and what the commit adds is that it was accepted, so
/// the pending intent behind it is completed rather than re-applied.
pub(super) async fn apply_self_group_payload(
    txn: &mut WriteDbTransaction<'_>,
    payload: &SelfGroupPayload,
    own_echo: bool,
) -> Result<ConnectionRequestEffects> {
    if !payload.updates.is_empty() {
        // Fold the extracted snapshots into one merged snapshot. This can be
        // empty when the update decoded to unknown-only fields sent by a newer
        // sibling; an empty snapshot covers nothing.
        let mut merged = SettingsUpdate::default();
        for update in &payload.updates {
            merge_settings_update(&mut merged, update).await?;
        }
        if own_echo {
            SettingChanges::complete_sent(txn, &merged).await?;
        } else {
            for update in &payload.updates {
                apply_settings_update(txn, update).await?;
            }
            // Fields a sibling's accepted commit covered are no longer ours to
            // change.
            SettingChanges::remove_covered(txn, &merged).await?;
        }
    }

    if own_echo {
        privacy_pass::complete_sent_seeds(txn, &payload.token_seeds).await?;
    } else {
        for seed in &payload.token_seeds {
            privacy_pass::apply_incoming_seed(txn, seed).await?;
        }
    }

    // We apply usernames before requests. A forwarded request can target a
    // username from the same commit.
    if own_echo {
        usernames::sync::complete_sent_entries(txn, &payload.usernames).await?;
    } else {
        usernames::sync::apply_usernames_update(txn, &payload.usernames).await?;
    }

    let effects = if own_echo {
        block_contact::persistence::complete_sent_entries(txn, &payload.blocked_contacts).await?;
        connection_requests::complete_sent_entries(txn, &payload.connection_requests).await?;
        ConnectionRequestEffects::default()
    } else {
        let declined_chats = block_contact::persistence::apply_blocked_contacts_update(
            txn,
            &payload.blocked_contacts,
        )
        .await?;
        let mut effects = connection_requests::apply_connection_requests_update(
            txn,
            &payload.connection_requests,
            Utc::now(),
        )
        .await?;
        effects.stale_chats.extend(declined_chats);
        effects
    };

    // Deletions go last, so a chat a sibling erased stays erased even if this
    // commit also carries a request for it.
    if own_echo {
        chats::persistence::remove_staged_deletion(txn, &payload.deleted_chats).await?;
    } else {
        chats::persistence::apply_deleted_chats(
            txn,
            &payload.deleted_chats,
            &effects.accepted_locally,
        )
        .await?;
    }

    Ok(effects)
}

#[cfg(test)]
mod tests {
    use aircommon::{
        codec::PersistenceCodec,
        identifiers::{QualifiedGroupId, UserId},
    };
    use airprotos::client::self_group::{
        BlockedContactEntry, ConnectionRequestDeclined, ConnectionRequestEntry, ContactBlocked,
        ContactUnblocked, DeletedChat,
    };
    use chrono::DateTime;
    use openmls::prelude::GroupId;
    use sqlx::SqlitePool;
    use uuid::Uuid;

    use super::*;
    use crate::{
        Chat,
        clients::{
            block_contact::{
                BlockedContact,
                pending::BlockedState,
                persistence::{staged_entries, store_outgoing_entry},
            },
            self_group_outbox::{self, OutboxKind},
        },
        db::access::DbAccess,
    };

    fn user(n: u128) -> UserId {
        UserId::new(Uuid::from_u128(n), "localhost".parse().unwrap())
    }

    fn blocked_contact(
        user_id: &UserId,
        blocked_at: i64,
        last_display_name: &str,
    ) -> BlockedContact {
        BlockedContact {
            user_id: user_id.clone(),
            last_display_name: last_display_name.parse().unwrap(),
            blocked_at: DateTime::from_timestamp(blocked_at, 0).unwrap(),
        }
    }

    fn blocked_state(user_id: &UserId, blocked_at: i64, last_display_name: &str) -> BlockedState {
        BlockedState::Blocked(blocked_contact(user_id, blocked_at, last_display_name))
    }

    /// What a local block or unblock does: apply optimistically, then park the
    /// change for the next self-group commit.
    async fn record_locally(
        txn: &mut WriteDbTransaction<'_>,
        intended: BlockedState,
    ) -> anyhow::Result<()> {
        intended.apply(&mut *txn).await?;
        store_outgoing_entry(txn, &BlockedContactEntry::from(&intended)).await?;
        Ok(())
    }

    fn blocked_entry(
        user_id: &UserId,
        blocked_at: u64,
        last_display_name: &str,
    ) -> BlockedContactEntry {
        BlockedContactEntry::Blocked(ContactBlocked {
            user_id: user_id.clone().into(),
            blocked_at,
            last_display_name: last_display_name.to_owned(),
        })
    }

    fn unblocked_entry(user_id: &UserId) -> BlockedContactEntry {
        BlockedContactEntry::Unblocked(ContactUnblocked {
            user_id: user_id.clone().into(),
        })
    }

    fn blocked_contacts_payload(entries: Vec<BlockedContactEntry>) -> SelfGroupPayload {
        SelfGroupPayload {
            blocked_contacts: entries,
            ..Default::default()
        }
    }

    #[sqlx::test]
    async fn sibling_update_blocks_and_unblocks_contacts(pool: SqlitePool) -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(pool);
        let to_block = user(1);
        let to_unblock = user(2);

        pool.with_write_transaction(async |txn| -> anyhow::Result<()> {
            blocked_state(&to_unblock, 10, "Alice")
                .apply(&mut *txn)
                .await?;

            let payload = blocked_contacts_payload(vec![
                blocked_entry(&to_block, 1_767_225_600, "Bob"),
                unblocked_entry(&to_unblock),
            ]);
            apply_self_group_payload(txn, &payload, false).await?;

            assert!(BlockedContact::check_blocked(&mut *txn, &to_block).await?);
            Ok(())
        })
        .await
    }

    #[sqlx::test]
    async fn sibling_update_wins_locally_but_keeps_our_parked_change(
        pool: SqlitePool,
    ) -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(pool);
        let contested = user(1);
        let untouched = user(2);

        pool.with_write_transaction(async |txn| -> anyhow::Result<()> {
            record_locally(txn, blocked_state(&contested, 10, "Alice")).await?;
            record_locally(txn, blocked_state(&untouched, 20, "Bob")).await?;

            let payload = blocked_contacts_payload(vec![unblocked_entry(&contested)]);
            apply_self_group_payload(txn, &payload, false).await?;

            assert!(!BlockedContact::check_blocked(&mut *txn, &contested).await?);
            assert_eq!(
                staged_entries(&mut *txn).await?,
                vec![
                    blocked_entry(&contested, 10, "Alice"),
                    blocked_entry(&untouched, 20, "Bob")
                ],
                "both parked changes must survive to be re-sent"
            );
            Ok(())
        })
        .await
    }

    /// We declined a request that a sibling accepted meanwhile. Another
    /// sibling's decline of it arrives after the accept.
    #[sqlx::test]
    async fn an_accepted_connection_drops_our_parked_deletion(
        pool: SqlitePool,
    ) -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(pool);
        let group_id: GroupId =
            QualifiedGroupId::new(Uuid::from_u128(7), "localhost".parse()?).into();

        pool.with_write_transaction(async |txn| -> anyhow::Result<()> {
            let chat = Chat::new_onboarding_connection_chat(group_id.clone(), user(1));
            chat.store(&mut *txn).await?;
            let declined = ConnectionRequestEntry::Declined(ConnectionRequestDeclined {
                chat_id: chat.id().uuid(),
                declined_at: DateTime::from_timestamp_millis(1_767_225_600_000).unwrap(),
            });
            self_group_outbox::stage(
                &mut *txn,
                OutboxKind::ConnectionRequest,
                chat.id().uuid().as_bytes(),
                &PersistenceCodec::to_vec(&declined)?,
                None,
            )
            .await?;
            chats::persistence::store_outgoing_deletion(&mut *txn, &group_id).await?;
            let payload = SelfGroupPayload {
                connection_requests: vec![declined],
                deleted_chats: vec![DeletedChat {
                    group_id: Some(group_id.clone()),
                }],
                ..Default::default()
            };

            apply_self_group_payload(txn, &payload, false).await?;

            assert!(Chat::load(&mut *txn, &chat.id()).await?.is_some());
            assert!(connection_requests::staged_entries(txn).await?.is_empty());
            assert!(
                chats::persistence::staged_deletions(&mut *txn)
                    .await?
                    .is_empty(),
                "a parked deletion of the accepted chat would erase it on the siblings"
            );
            Ok(())
        })
        .await
    }

    #[sqlx::test]
    async fn the_last_entry_for_a_contact_wins(pool: SqlitePool) -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(pool);
        let reblocked = user(1);
        let unblocked = user(2);

        pool.with_write_transaction(async |txn| -> anyhow::Result<()> {
            let payload = blocked_contacts_payload(vec![
                unblocked_entry(&reblocked),
                blocked_entry(&unblocked, 40, "Bob"),
                blocked_entry(&reblocked, 50, "Alice"),
                unblocked_entry(&unblocked),
            ]);
            apply_self_group_payload(txn, &payload, false).await?;

            assert!(BlockedContact::check_blocked(&mut *txn, &reblocked).await?);
            assert!(!BlockedContact::check_blocked(&mut *txn, &unblocked).await?);
            Ok(())
        })
        .await
    }
}
