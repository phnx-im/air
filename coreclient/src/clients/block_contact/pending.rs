// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! The user's not-yet-synchronized blocked-contact changes.
//!
//! Blocking is device-local state that is mirrored to the user's other devices
//! through the self-group. This module holds the durable intent behind that
//! sync.
//!
//! The lifecycle mirrors the settings sync in
//! [`crate::clients::user_settings`], but at per-contact granularity. A
//! blocked-contacts update is a diff and communicates only the contacts it
//! changes, so two devices changing different contacts do not cancel each
//! other.

// The pending changes have no callers yet. The receive path and the send path
// land in the next stages.
#![allow(dead_code)]

use aircommon::identifiers::UserId;
use airprotos::client::{
    group_bootstrap::PeerUserId,
    self_group::{BlockedContactEntry, BlockedContactState, ContactBlocked, ContactUnblocked},
};
use chrono::{DateTime, Utc};
use tracing::warn;

use crate::{
    db::access::{ReadConnection, WriteConnection, WriteDbTransaction},
    user_profiles::display_name::DisplayName,
};

use super::BlockedContact;

/// The blocked state of one contact, as stored in `blocked_contact`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum BlockedState {
    Blocked {
        blocked_at: DateTime<Utc>,
        last_display_name: DisplayName,
    },
    Unblocked,
}

impl BlockedState {
    /// The wire form of this state.
    ///
    /// `blocked_at` loses sub-second precision here. Both sides of the
    /// comparison in [`PendingBlockedContactChange::complete_sent`] go through
    /// this conversion, so that is not a source of mismatch.
    fn to_entry_state(&self) -> BlockedContactState {
        match self {
            Self::Blocked {
                blocked_at,
                last_display_name,
            } => BlockedContactState::Blocked(ContactBlocked {
                blocked_at: blocked_at.timestamp().max(0) as u64,
                last_display_name: last_display_name.to_string(),
            }),
            Self::Unblocked => BlockedContactState::Unblocked(ContactUnblocked {}),
        }
    }
}

/// One contact's not-yet-synchronized blocked-state change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PendingBlockedContactChange {
    user_id: UserId,
    /// The state we still intend to assert for this contact.
    intended: BlockedState,
    /// The state stored before the contact was first touched, for rollback on
    /// a terminal send failure.
    previous: BlockedState,
}

impl PendingBlockedContactChange {
    /// Records a local blocked-state change.
    pub(crate) async fn record(
        txn: &mut WriteDbTransaction<'_>,
        user_id: &UserId,
        intended: BlockedState,
    ) -> sqlx::Result<bool> {
        let current = BlockedState::load(&mut *txn, user_id).await?;
        let previous = match Self::load(&mut *txn, user_id).await? {
            Some(pending) => pending.previous,
            None if current == intended => return Ok(false),
            None => current,
        };

        intended.apply(&mut *txn, user_id).await?;
        Self {
            user_id: user_id.clone(),
            intended,
            previous,
        }
        .store(txn)
        .await?;

        Ok(true)
    }

    /// Completes the pending changes after one of our own commits was accepted.
    pub(crate) async fn complete_sent(
        txn: &mut WriteDbTransaction<'_>,
        sent: &[BlockedContactEntry],
    ) -> sqlx::Result<()> {
        for entry in sent {
            let Some(user_id) = parse_peer_user_id(&entry.user_id) else {
                continue;
            };
            let Some(pending) = Self::load(&mut *txn, &user_id).await? else {
                continue;
            };
            if pending.intended.to_entry_state() == entry.state {
                Self::delete(&mut *txn, &user_id).await?;
            }
        }
        Ok(())
    }

    /// Drops the pending change of every contact a sibling's accepted update
    /// names.
    pub(crate) async fn remove_covered(
        txn: &mut WriteDbTransaction<'_>,
        incoming: &[BlockedContactEntry],
    ) -> sqlx::Result<()> {
        for entry in incoming {
            let Some(user_id) = parse_peer_user_id(&entry.user_id) else {
                continue;
            };
            Self::delete(&mut *txn, &user_id).await?;
        }
        Ok(())
    }

    /// Rolls the touched contacts back to the state stored before their first
    /// touch and clears the pending changes. Used when a send fails terminally.
    pub(crate) async fn roll_back_and_clear(txn: &mut WriteDbTransaction<'_>) -> sqlx::Result<()> {
        for pending in Self::load_all(&mut *txn).await? {
            let current = BlockedState::load(&mut *txn, &pending.user_id).await?;
            if current != pending.intended {
                continue;
            }
            pending.previous.apply(&mut *txn, &pending.user_id).await?;
        }
        Self::delete_all(txn).await
    }

    /// Loads the pending changes as the entries a commit carries, sorted by
    /// user id so the encoding is canonical.
    pub(crate) async fn load_entries(
        connection: impl ReadConnection,
    ) -> sqlx::Result<Vec<BlockedContactEntry>> {
        Ok(Self::load_all(connection)
            .await?
            .into_iter()
            .map(|pending| BlockedContactEntry {
                user_id: pending.user_id.into(),
                state: pending.intended.to_entry_state(),
            })
            .collect())
    }
}

/// Parses a user id off the wire, skipping ids that are not well-formed.
fn parse_peer_user_id(peer: &PeerUserId) -> Option<UserId> {
    UserId::try_from(peer.clone())
        .inspect_err(|error| {
            warn!(%error, "Skipping a blocked-contact entry with a malformed user id");
        })
        .ok()
}

mod persistence {
    use aircommon::identifiers::Fqdn;
    use sqlx::{query, query_as};
    use uuid::Uuid;

    use super::*;

    struct SqlBlockedState {
        blocked_at: DateTime<Utc>,
        last_display_name: DisplayName,
    }

    struct SqlPendingBlockedContactChange {
        user_uuid: Uuid,
        user_domain: Fqdn,
        blocked_at: Option<DateTime<Utc>>,
        last_display_name: Option<DisplayName>,
        previous_blocked_at: Option<DateTime<Utc>>,
        previous_last_display_name: Option<DisplayName>,
    }

    impl From<SqlPendingBlockedContactChange> for PendingBlockedContactChange {
        fn from(
            SqlPendingBlockedContactChange {
                user_uuid,
                user_domain,
                blocked_at,
                last_display_name,
                previous_blocked_at,
                previous_last_display_name,
            }: SqlPendingBlockedContactChange,
        ) -> Self {
            Self {
                user_id: UserId::new(user_uuid, user_domain),
                intended: blocked_state(blocked_at, last_display_name),
                previous: blocked_state(previous_blocked_at, previous_last_display_name),
            }
        }
    }

    /// Both columns of a state are written together, see the table's `CHECK`
    /// constraints.
    fn blocked_state(
        blocked_at: Option<DateTime<Utc>>,
        last_display_name: Option<DisplayName>,
    ) -> BlockedState {
        match blocked_at.zip(last_display_name) {
            Some((blocked_at, last_display_name)) => BlockedState::Blocked {
                blocked_at,
                last_display_name,
            },
            None => BlockedState::Unblocked,
        }
    }

    impl BlockedState {
        /// Reads the stored blocked state of a contact.
        pub(super) async fn load(
            mut connection: impl ReadConnection,
            user_id: &UserId,
        ) -> sqlx::Result<Self> {
            let uuid = user_id.uuid();
            let domain = user_id.domain();
            let record = query_as!(
                SqlBlockedState,
                r#"SELECT
                    blocked_at AS "blocked_at: _",
                    last_display_name AS "last_display_name: _"
                FROM blocked_contact
                WHERE user_uuid = ?1 AND user_domain = ?2"#,
                uuid,
                domain,
            )
            .fetch_optional(connection.as_mut())
            .await?;

            Ok(match record {
                Some(SqlBlockedState {
                    blocked_at,
                    last_display_name,
                }) => Self::Blocked {
                    blocked_at,
                    last_display_name,
                },
                None => Self::Unblocked,
            })
        }

        /// Writes this state to `blocked_contact`, emitting the same per-user
        /// change notification the local block and unblock paths emit.
        pub(super) async fn apply(
            &self,
            connection: impl WriteConnection,
            user_id: &UserId,
        ) -> sqlx::Result<()> {
            match self {
                Self::Blocked {
                    blocked_at,
                    last_display_name,
                } => {
                    BlockedContact {
                        user_id: user_id.clone(),
                        last_display_name: last_display_name.clone(),
                        blocked_at: *blocked_at,
                    }
                    .store(connection)
                    .await
                }
                Self::Unblocked => BlockedContact::delete_by_id(connection, user_id.clone()).await,
            }
        }

        /// The `(blocked_at, last_display_name)` column pair for this state.
        fn columns(&self) -> (Option<DateTime<Utc>>, Option<DisplayName>) {
            match self {
                Self::Blocked {
                    blocked_at,
                    last_display_name,
                } => (Some(*blocked_at), Some(last_display_name.clone())),
                Self::Unblocked => (None, None),
            }
        }
    }

    impl PendingBlockedContactChange {
        pub(super) async fn store(&self, mut connection: impl WriteConnection) -> sqlx::Result<()> {
            let uuid = self.user_id.uuid();
            let domain = self.user_id.domain();
            let (blocked_at, last_display_name) = self.intended.columns();
            let (previous_blocked_at, previous_last_display_name) = self.previous.columns();
            query!(
                "INSERT INTO blocked_contact_change (
                    user_uuid,
                    user_domain,
                    blocked_at,
                    last_display_name,
                    previous_blocked_at,
                    previous_last_display_name
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                ON CONFLICT (user_uuid, user_domain) DO UPDATE SET
                    blocked_at = excluded.blocked_at,
                    last_display_name = excluded.last_display_name,
                    previous_blocked_at = excluded.previous_blocked_at,
                    previous_last_display_name = excluded.previous_last_display_name",
                uuid,
                domain,
                blocked_at,
                last_display_name,
                previous_blocked_at,
                previous_last_display_name,
            )
            .execute(connection.as_mut())
            .await?;
            Ok(())
        }

        pub(super) async fn load(
            mut connection: impl ReadConnection,
            user_id: &UserId,
        ) -> sqlx::Result<Option<Self>> {
            let uuid = user_id.uuid();
            let domain = user_id.domain();
            query_as!(
                SqlPendingBlockedContactChange,
                r#"SELECT
                    user_uuid AS "user_uuid: _",
                    user_domain AS "user_domain: _",
                    blocked_at AS "blocked_at: _",
                    last_display_name AS "last_display_name: _",
                    previous_blocked_at AS "previous_blocked_at: _",
                    previous_last_display_name AS "previous_last_display_name: _"
                FROM blocked_contact_change
                WHERE user_uuid = ?1 AND user_domain = ?2"#,
                uuid,
                domain,
            )
            .fetch_optional(connection.as_mut())
            .await
            .map(|record| record.map(From::from))
        }

        pub(super) async fn load_all(
            mut connection: impl ReadConnection,
        ) -> sqlx::Result<Vec<Self>> {
            let records = query_as!(
                SqlPendingBlockedContactChange,
                r#"SELECT
                    user_uuid AS "user_uuid: _",
                    user_domain AS "user_domain: _",
                    blocked_at AS "blocked_at: _",
                    last_display_name AS "last_display_name: _",
                    previous_blocked_at AS "previous_blocked_at: _",
                    previous_last_display_name AS "previous_last_display_name: _"
                FROM blocked_contact_change
                ORDER BY user_uuid, user_domain"#
            )
            .fetch_all(connection.as_mut())
            .await?;

            Ok(records.into_iter().map(From::from).collect())
        }

        pub(super) async fn delete(
            mut connection: impl WriteConnection,
            user_id: &UserId,
        ) -> sqlx::Result<()> {
            let uuid = user_id.uuid();
            let domain = user_id.domain();
            query!(
                "DELETE FROM blocked_contact_change
                WHERE user_uuid = ?1 AND user_domain = ?2",
                uuid,
                domain,
            )
            .execute(connection.as_mut())
            .await?;
            Ok(())
        }

        pub(super) async fn delete_all(mut connection: impl WriteConnection) -> sqlx::Result<()> {
            query!("DELETE FROM blocked_contact_change")
                .execute(connection.as_mut())
                .await?;
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use sqlx::SqlitePool;
    use uuid::Uuid;

    use crate::db::access::DbAccess;

    use super::*;

    fn user(n: u128) -> UserId {
        UserId::new(Uuid::from_u128(n), "localhost".parse().unwrap())
    }

    fn blocked(blocked_at: i64, last_display_name: &str) -> BlockedState {
        BlockedState::Blocked {
            blocked_at: DateTime::from_timestamp(blocked_at, 0).unwrap(),
            last_display_name: last_display_name.parse().unwrap(),
        }
    }

    fn entry(user_id: &UserId, state: BlockedContactState) -> BlockedContactEntry {
        BlockedContactEntry {
            user_id: user_id.clone().into(),
            state,
        }
    }

    #[sqlx::test]
    async fn record_snapshots_unblocked_previous_state(pool: SqlitePool) -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(pool);
        let user = user(1);

        pool.with_write_transaction(async |txn| -> anyhow::Result<()> {
            assert!(
                PendingBlockedContactChange::record(txn, &user, blocked(10, "Alice")).await?,
                "blocking an unblocked contact must need syncing"
            );

            let pending = PendingBlockedContactChange::load(&mut *txn, &user)
                .await?
                .expect("the change must be pending");
            assert_eq!(pending.intended, blocked(10, "Alice"));
            assert_eq!(pending.previous, BlockedState::Unblocked);

            // The change is applied optimistically.
            assert_eq!(
                BlockedState::load(&mut *txn, &user).await?,
                blocked(10, "Alice")
            );
            Ok(())
        })
        .await
    }

    #[sqlx::test]
    async fn record_snapshots_blocked_previous_state(pool: SqlitePool) -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(pool);
        let user = user(1);

        pool.with_write_transaction(async |txn| -> anyhow::Result<()> {
            blocked(10, "Alice").apply(&mut *txn, &user).await?;

            assert!(
                PendingBlockedContactChange::record(txn, &user, BlockedState::Unblocked).await?
            );

            let pending = PendingBlockedContactChange::load(&mut *txn, &user)
                .await?
                .expect("the change must be pending");
            assert_eq!(pending.intended, BlockedState::Unblocked);
            assert_eq!(
                pending.previous,
                blocked(10, "Alice"),
                "the previous state must keep its timestamp and display name"
            );
            assert_eq!(
                BlockedState::load(&mut *txn, &user).await?,
                BlockedState::Unblocked
            );
            Ok(())
        })
        .await
    }

    #[sqlx::test]
    async fn record_noop_tap_records_nothing(pool: SqlitePool) -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(pool);
        let unblocked_user = user(1);
        let blocked_user = user(2);

        pool.with_write_transaction(async |txn| -> anyhow::Result<()> {
            assert!(
                !PendingBlockedContactChange::record(txn, &unblocked_user, BlockedState::Unblocked)
                    .await?
            );

            blocked(10, "Alice").apply(&mut *txn, &blocked_user).await?;
            assert!(
                !PendingBlockedContactChange::record(txn, &blocked_user, blocked(10, "Alice"))
                    .await?
            );

            assert!(
                PendingBlockedContactChange::load_all(&mut *txn)
                    .await?
                    .is_empty(),
                "a no-op tap must not record a pending change"
            );
            Ok(())
        })
        .await
    }

    #[sqlx::test]
    async fn record_retoggle_keeps_the_first_previous_state(
        pool: SqlitePool,
    ) -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(pool);
        let user = user(1);

        pool.with_write_transaction(async |txn| -> anyhow::Result<()> {
            blocked(10, "Alice").apply(&mut *txn, &user).await?;

            PendingBlockedContactChange::record(txn, &user, BlockedState::Unblocked).await?;
            PendingBlockedContactChange::record(txn, &user, blocked(20, "Alice B")).await?;

            let pending = PendingBlockedContactChange::load_all(&mut *txn).await?;
            assert_eq!(pending.len(), 1, "a re-toggle must fold into the same row");
            assert_eq!(pending[0].intended, blocked(20, "Alice B"));
            assert_eq!(pending[0].previous, blocked(10, "Alice"));
            Ok(())
        })
        .await
    }

    #[sqlx::test]
    async fn complete_sent_removes_a_matching_entry(pool: SqlitePool) -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(pool);
        let user = user(1);

        pool.with_write_transaction(async |txn| -> anyhow::Result<()> {
            PendingBlockedContactChange::record(txn, &user, blocked(10, "Alice")).await?;
            let sent = PendingBlockedContactChange::load_entries(&mut *txn).await?;

            PendingBlockedContactChange::complete_sent(txn, &sent).await?;

            assert!(
                PendingBlockedContactChange::load(&mut *txn, &user)
                    .await?
                    .is_none()
            );
            Ok(())
        })
        .await
    }

    #[sqlx::test]
    async fn complete_sent_keeps_a_retoggled_contact(pool: SqlitePool) -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(pool);
        let user = user(1);

        pool.with_write_transaction(async |txn| -> anyhow::Result<()> {
            PendingBlockedContactChange::record(txn, &user, blocked(10, "Alice")).await?;
            let sent = PendingBlockedContactChange::load_entries(&mut *txn).await?;
            PendingBlockedContactChange::record(txn, &user, BlockedState::Unblocked).await?;

            PendingBlockedContactChange::complete_sent(txn, &sent).await?;

            let pending = PendingBlockedContactChange::load(&mut *txn, &user)
                .await?
                .expect("the re-toggled contact must stay pending");
            assert_eq!(pending.intended, BlockedState::Unblocked);
            Ok(())
        })
        .await
    }

    #[sqlx::test]
    async fn complete_sent_ignores_an_unknown_state(pool: SqlitePool) -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(pool);
        let user = user(1);

        pool.with_write_transaction(async |txn| -> anyhow::Result<()> {
            PendingBlockedContactChange::record(txn, &user, blocked(10, "Alice")).await?;

            let sent = vec![entry(&user, BlockedContactState::Unknown)];
            PendingBlockedContactChange::complete_sent(txn, &sent).await?;

            assert!(
                PendingBlockedContactChange::load(&mut *txn, &user)
                    .await?
                    .is_some(),
                "an unknown state must not complete a pending change"
            );
            Ok(())
        })
        .await
    }

    #[sqlx::test]
    async fn remove_covered_drops_named_contacts_only(pool: SqlitePool) -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(pool);
        let named = user(1);
        let other = user(2);

        for incoming in [
            BlockedContactState::Blocked(ContactBlocked {
                blocked_at: 99,
                last_display_name: "Someone else".to_owned(),
            }),
            BlockedContactState::Unblocked(ContactUnblocked {}),
            BlockedContactState::Unknown,
        ] {
            pool.with_write_transaction(async |txn| -> anyhow::Result<()> {
                PendingBlockedContactChange::record(txn, &named, blocked(10, "Alice")).await?;
                PendingBlockedContactChange::record(txn, &other, blocked(20, "Bob")).await?;

                PendingBlockedContactChange::remove_covered(
                    txn,
                    &[entry(&named, incoming.clone())],
                )
                .await?;

                assert!(
                    PendingBlockedContactChange::load(&mut *txn, &named)
                        .await?
                        .is_none(),
                    "a named contact must be dropped, incoming state: {incoming:?}"
                );
                let survivor = PendingBlockedContactChange::load(&mut *txn, &other)
                    .await?
                    .expect("an unnamed contact must stay pending");
                assert_eq!(survivor.intended, blocked(20, "Bob"));

                // Reset for the next incoming state.
                PendingBlockedContactChange::delete_all(&mut *txn).await?;
                BlockedState::Unblocked.apply(&mut *txn, &named).await?;
                BlockedState::Unblocked.apply(&mut *txn, &other).await?;
                Ok(())
            })
            .await?;
        }
        Ok(())
    }

    #[sqlx::test]
    async fn roll_back_restores_the_previous_block(pool: SqlitePool) -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(pool);
        let user = user(1);

        pool.with_write_transaction(async |txn| -> anyhow::Result<()> {
            blocked(10, "Alice").apply(&mut *txn, &user).await?;
            PendingBlockedContactChange::record(txn, &user, BlockedState::Unblocked).await?;

            PendingBlockedContactChange::roll_back_and_clear(txn).await?;

            assert_eq!(
                BlockedState::load(&mut *txn, &user).await?,
                blocked(10, "Alice"),
                "the block must come back with its timestamp and display name"
            );
            assert!(
                PendingBlockedContactChange::load_all(&mut *txn)
                    .await?
                    .is_empty(),
                "rollback must clear the pending changes"
            );
            Ok(())
        })
        .await
    }

    #[sqlx::test]
    async fn roll_back_deletes_when_there_was_no_previous_block(
        pool: SqlitePool,
    ) -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(pool);
        let user = user(1);

        pool.with_write_transaction(async |txn| -> anyhow::Result<()> {
            PendingBlockedContactChange::record(txn, &user, blocked(10, "Alice")).await?;

            PendingBlockedContactChange::roll_back_and_clear(txn).await?;

            assert_eq!(
                BlockedState::load(&mut *txn, &user).await?,
                BlockedState::Unblocked
            );
            Ok(())
        })
        .await
    }

    #[sqlx::test]
    async fn roll_back_notifies_the_restored_contact(pool: SqlitePool) -> anyhow::Result<()> {
        use std::time::Duration;

        use tokio_stream::StreamExt;

        use crate::db::notification::DbEntityId;

        let pool = DbAccess::for_tests(pool);
        let user = user(1);

        pool.with_write_transaction(async |txn| -> anyhow::Result<()> {
            blocked(10, "Alice").apply(&mut *txn, &user).await?;
            PendingBlockedContactChange::record(txn, &user, BlockedState::Unblocked).await?;
            Ok(())
        })
        .await?;

        let mut notifications = std::pin::pin!(pool.notifier_tx.subscribe());
        pool.with_write_transaction(async |txn| {
            PendingBlockedContactChange::roll_back_and_clear(txn).await
        })
        .await?;

        let notification = tokio::time::timeout(Duration::from_secs(5), notifications.next())
            .await
            .expect("the rollback must notify")
            .expect("notification stream should be open");
        assert!(
            notification.ops.keys().any(|entity_id| matches!(
                entity_id,
                DbEntityId::User(notified) if *notified == user
            )),
            "expected a User notification for the restored contact"
        );

        Ok(())
    }

    #[sqlx::test]
    async fn roll_back_skips_an_overwritten_contact(pool: SqlitePool) -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(pool);
        let overwritten = user(1);
        let untouched_since = user(2);

        pool.with_write_transaction(async |txn| -> anyhow::Result<()> {
            blocked(10, "Alice").apply(&mut *txn, &overwritten).await?;
            PendingBlockedContactChange::record(txn, &overwritten, BlockedState::Unblocked).await?;
            PendingBlockedContactChange::record(txn, &untouched_since, blocked(20, "Bob")).await?;

            // A sibling's update blocks the contact again, with its own values.
            blocked(30, "Alice B")
                .apply(&mut *txn, &overwritten)
                .await?;

            PendingBlockedContactChange::roll_back_and_clear(txn).await?;

            assert_eq!(
                BlockedState::load(&mut *txn, &overwritten).await?,
                blocked(30, "Alice B"),
                "the incoming state must not be clobbered by the rollback"
            );
            assert_eq!(
                BlockedState::load(&mut *txn, &untouched_since).await?,
                BlockedState::Unblocked,
                "the other contact must be rolled back"
            );
            assert!(
                PendingBlockedContactChange::load_all(&mut *txn)
                    .await?
                    .is_empty()
            );
            Ok(())
        })
        .await
    }

    #[sqlx::test]
    async fn load_entries_carries_the_intended_states(pool: SqlitePool) -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(pool);
        let first = user(1);
        let second = user(2);

        pool.with_write_transaction(async |txn| -> anyhow::Result<()> {
            blocked(10, "Alice").apply(&mut *txn, &second).await?;
            PendingBlockedContactChange::record(txn, &first, blocked(20, "Bob")).await?;
            PendingBlockedContactChange::record(txn, &second, BlockedState::Unblocked).await?;

            let entries = PendingBlockedContactChange::load_entries(&mut *txn).await?;
            assert_eq!(
                entries,
                vec![
                    entry(
                        &first,
                        BlockedContactState::Blocked(ContactBlocked {
                            blocked_at: 20,
                            last_display_name: "Bob".to_owned(),
                        })
                    ),
                    entry(&second, BlockedContactState::Unblocked(ContactUnblocked {})),
                ]
            );
            Ok(())
        })
        .await
    }
}
