// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Synchronizing blocked contacts with the user's other devices.
//!
//! Blocking is device-local state that is mirrored to the user's other devices
//! through the self-group.
use aircommon::codec::PersistenceCodec;
use airprotos::client::self_group::BlockedContactEntry;

use crate::{
    clients::{
        block_contact::{BlockedContact, pending::BlockedState},
        self_group_outbox::{self, OutboxKind},
    },
    db::access::{ReadConnection, WriteConnection, WriteDbTransaction},
};

impl BlockedState {
    /// Writes this state to `blocked_contact`, emitting the same per-user
    /// change notification the local block and unblock paths emit.
    pub(crate) async fn apply(&self, connection: impl WriteConnection) -> sqlx::Result<()> {
        match self {
            Self::Blocked(contact) => contact.store(connection).await,
            Self::Unblocked { user_id } => {
                BlockedContact::delete_by_id(connection, user_id.clone()).await
            }
        }
    }
}

/// Applies the entries of a sibling's accepted blocked-contacts update.
///
/// Entries apply in order, so the last entry for a contact wins. An entry this
/// client cannot make sense of is skipped, see [`BlockedState::from_entry`].
pub(crate) async fn apply_blocked_contacts_update(
    txn: &mut WriteDbTransaction<'_>,
    entries: &[BlockedContactEntry],
) -> sqlx::Result<()> {
    for entry in entries {
        let Some(state) = BlockedState::from_entry(entry) else {
            continue;
        };
        state.apply(&mut *txn).await?;
    }
    Ok(())
}

/// Every stored block, as the entries a provisioning package carries.
///
/// A device joining the self group through a Welcome cannot decrypt the commits
/// from before its join, so the blocks made until then have to travel in the
/// linking payload rather than as a diff.
pub(crate) async fn blocked_contacts_snapshot(
    connection: impl ReadConnection,
) -> sqlx::Result<Vec<BlockedContactEntry>> {
    Ok(BlockedContact::load_all(connection)
        .await?
        .iter()
        .map(BlockedContactEntry::from)
        .collect())
}

/// Parks a locally applied change for the next self-group commit.
pub(crate) async fn store_outgoing_entry(
    connection: impl WriteConnection,
    entry: &BlockedContactEntry,
) -> anyhow::Result<()> {
    let Some(key) = entry
        .user_id()
        .and_then(|id| PersistenceCodec::to_vec(id).ok())
    else {
        return Ok(());
    };
    self_group_outbox::stage(
        connection,
        OutboxKind::BlockedContact,
        &key,
        &PersistenceCodec::to_vec(entry)?,
        None,
    )
    .await?;
    Ok(())
}

pub(crate) async fn staged_entries(
    connection: impl ReadConnection,
) -> anyhow::Result<Vec<BlockedContactEntry>> {
    self_group_outbox::load_kind(connection, OutboxKind::BlockedContact)
        .await?
        .iter()
        .map(|entry| Ok(PersistenceCodec::from_slice(&entry.payload)?))
        .collect()
}

pub(crate) async fn complete_sent_entries(
    txn: &mut WriteDbTransaction<'_>,
    sent: &[BlockedContactEntry],
) -> anyhow::Result<()> {
    for entry in sent {
        let Some(key) = entry
            .user_id()
            .and_then(|id| PersistenceCodec::to_vec(id).ok())
        else {
            continue;
        };
        self_group_outbox::complete_sent(
            &mut *txn,
            OutboxKind::BlockedContact,
            &key,
            &PersistenceCodec::to_vec(entry)?,
        )
        .await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use aircommon::identifiers::UserId;
    use airprotos::client::self_group::{ContactBlocked, ContactUnblocked};
    use chrono::DateTime;
    use sqlx::SqlitePool;
    use uuid::Uuid;

    use crate::db::access::DbAccess;

    use super::*;

    fn user(n: u128) -> UserId {
        UserId::new(Uuid::from_u128(n), "localhost".parse().unwrap())
    }

    fn contact(user_id: &UserId, blocked_at: i64, last_display_name: &str) -> BlockedContact {
        BlockedContact {
            user_id: user_id.clone(),
            last_display_name: last_display_name.parse().unwrap(),
            blocked_at: DateTime::from_timestamp(blocked_at, 0).unwrap(),
        }
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

    #[sqlx::test]
    async fn entries_to_broadcast_is_sorted_by_user_id(pool: SqlitePool) -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(pool);
        let first = user(1);
        let second = user(2);

        pool.with_write_transaction(async |txn| -> anyhow::Result<()> {
            store_outgoing_entry(&mut *txn, &unblocked_entry(&second)).await?;
            store_outgoing_entry(&mut *txn, &blocked_entry(&first, 20, "Bob")).await?;

            assert_eq!(
                staged_entries(&mut *txn).await?,
                vec![blocked_entry(&first, 20, "Bob"), unblocked_entry(&second)]
            );
            Ok(())
        })
        .await
    }

    #[sqlx::test]
    async fn a_retoggle_folds_into_one_row(pool: SqlitePool) -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(pool);
        let user = user(1);

        pool.with_write_transaction(async |txn| -> anyhow::Result<()> {
            store_outgoing_entry(&mut *txn, &blocked_entry(&user, 10, "Alice")).await?;
            store_outgoing_entry(&mut *txn, &unblocked_entry(&user)).await?;
            store_outgoing_entry(&mut *txn, &blocked_entry(&user, 20, "Alice B")).await?;

            assert_eq!(
                staged_entries(&mut *txn).await?,
                vec![blocked_entry(&user, 20, "Alice B")]
            );
            Ok(())
        })
        .await
    }

    #[sqlx::test]
    async fn complete_sent_removes_a_matching_entry(pool: SqlitePool) -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(pool);
        let user = user(1);

        pool.with_write_transaction(async |txn| -> anyhow::Result<()> {
            store_outgoing_entry(&mut *txn, &blocked_entry(&user, 10, "Alice")).await?;
            let sent = staged_entries(&mut *txn).await?;

            complete_sent_entries(txn, &sent).await?;

            assert!(staged_entries(&mut *txn).await?.is_empty());
            Ok(())
        })
        .await
    }

    #[sqlx::test]
    async fn complete_sent_keeps_a_retoggled_contact(pool: SqlitePool) -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(pool);
        let user = user(1);

        pool.with_write_transaction(async |txn| -> anyhow::Result<()> {
            store_outgoing_entry(&mut *txn, &blocked_entry(&user, 10, "Alice")).await?;
            let sent = staged_entries(&mut *txn).await?;
            store_outgoing_entry(&mut *txn, &unblocked_entry(&user)).await?;

            complete_sent_entries(txn, &sent).await?;

            assert_eq!(
                staged_entries(&mut *txn).await?,
                vec![unblocked_entry(&user)],
                "the re-toggled contact must stay parked"
            );
            Ok(())
        })
        .await
    }

    #[sqlx::test]
    async fn complete_sent_ignores_an_unknown_entry(pool: SqlitePool) -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(pool);
        let user = user(1);

        pool.with_write_transaction(async |txn| -> anyhow::Result<()> {
            store_outgoing_entry(&mut *txn, &blocked_entry(&user, 10, "Alice")).await?;

            complete_sent_entries(txn, &[BlockedContactEntry::Unknown]).await?;

            assert_eq!(
                staged_entries(&mut *txn).await?,
                vec![blocked_entry(&user, 10, "Alice")],
                "an unknown entry names no contact, so it completes nothing"
            );
            Ok(())
        })
        .await
    }

    #[sqlx::test]
    async fn apply_writes_and_clears_the_stored_block(pool: SqlitePool) -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(pool);
        let user = user(1);

        pool.with_write_transaction(async |txn| -> anyhow::Result<()> {
            BlockedState::Blocked(contact(&user, 10, "Alice"))
                .apply(&mut *txn)
                .await?;
            assert!(BlockedContact::check_blocked(&mut *txn, &user).await?);

            BlockedState::Unblocked {
                user_id: user.clone(),
            }
            .apply(&mut *txn)
            .await?;
            assert!(!BlockedContact::check_blocked(&mut *txn, &user).await?);
            Ok(())
        })
        .await
    }
}
