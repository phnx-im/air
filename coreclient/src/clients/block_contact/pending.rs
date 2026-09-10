// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! The user's blocked-contact changes that still have to reach the siblings.
//!
//! Blocking is device-local state that is mirrored to the user's other devices
//! through the self-group. This module is the outbox for that sync: a change
//! is applied locally right away and parked here until a self-group commit
//! carrying it is accepted. The outbound service turns the parked changes into
//! a commit whenever the self-group is free, so a change survives a lost epoch
//! race and a re-toggle while a commit is in flight.
//!
//! Unlike the settings sync in [`crate::clients::user_settings`], an update is
//! a per-contact diff, and there is no intent diffing against incoming
//! commits: a sibling's accepted commit overwrites the stored state, and a
//! parked change is simply re-sent by the next commit.

use aircommon::identifiers::UserId;
use airprotos::client::{
    group_bootstrap::PeerUserId,
    self_group::{BlockedContactEntry, ContactBlocked, ContactUnblocked},
};
use chrono::{DateTime, Utc};
use tracing::{debug, warn};

use crate::{
    db::access::{ReadConnection, WriteConnection, WriteDbTransaction},
    user_profiles::display_name::DisplayName,
};

use super::BlockedContact;

/// The blocked state of one contact, as stored in `blocked_contact`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum BlockedState {
    Blocked(BlockedContact),
    Unblocked { user_id: UserId },
}

/// NB: `blocked_at` loses sub-second precision here.
impl From<BlockedState> for BlockedContactEntry {
    fn from(state: BlockedState) -> Self {
        match state {
            BlockedState::Blocked(BlockedContact {
                user_id,
                last_display_name,
                blocked_at,
            }) => Self::Blocked(ContactBlocked {
                user_id: user_id.into(),
                blocked_at: blocked_at.timestamp().max(0) as u64,
                last_display_name: last_display_name.to_string(),
            }),
            BlockedState::Unblocked { user_id } => Self::Unblocked(ContactUnblocked {
                user_id: user_id.into(),
            }),
        }
    }
}

impl BlockedState {
    /// The state of an incoming entry.
    ///
    /// `None` for an entry this client cannot make sense of.
    fn from_entry(state: &BlockedContactEntry) -> Option<Self> {
        match state {
            BlockedContactEntry::Blocked(ContactBlocked {
                user_id,
                blocked_at,
                last_display_name,
            }) => {
                let Some(blocked_at) = i64::try_from(*blocked_at)
                    .ok()
                    .and_then(|seconds| DateTime::from_timestamp(seconds, 0))
                else {
                    warn!(
                        %blocked_at,
                        "Skipping a blocked-contact entry with an out-of-range timestamp"
                    );
                    return None;
                };
                let last_display_name = last_display_name
                    .parse()
                    .inspect_err(|error| {
                        warn!(
                            %error,
                            "Skipping a blocked-contact entry with an invalid display name"
                        );
                    })
                    .ok()?;
                Some(Self::Blocked(BlockedContact {
                    user_id: user_id.clone().try_into().ok()?,
                    last_display_name,
                    blocked_at,
                }))
            }
            BlockedContactEntry::Unblocked(ContactUnblocked { user_id }) => Some(Self::Unblocked {
                user_id: user_id.clone().try_into().ok()?,
            }),
            BlockedContactEntry::Unknown => {
                debug!("Skipping a blocked-contact entry with an unknown state");
                None
            }
        }
    }

    pub(crate) fn user_id(&self) -> &UserId {
        match self {
            Self::Blocked(contact) => &contact.user_id,
            Self::Unblocked { user_id } => user_id,
        }
    }
}

/// One contact's blocked-state change that still has to reach the siblings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PendingBlockedContactChange {
    /// The state we still intend to assert for this contact.
    intended: BlockedState,
}

impl PendingBlockedContactChange {
    pub(crate) fn user_id(&self) -> &UserId {
        self.intended.user_id()
    }
}

/// Applies the entries of a sibling's accepted blocked-contacts update.
///
/// Entries apply in order, so the last entry for a contact wins. An entry this
/// client cannot make sense of is skipped, see [`BlockedState::from_entry`] and
/// [`parse_peer_user_id`].
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
    }

    /// Both columns are written together, see the table's `CHECK` constraint.
    impl From<SqlPendingBlockedContactChange> for PendingBlockedContactChange {
        fn from(
            SqlPendingBlockedContactChange {
                user_uuid,
                user_domain,
                blocked_at,
                last_display_name,
            }: SqlPendingBlockedContactChange,
        ) -> Self {
            let user_id = UserId::new(user_uuid, user_domain);
            let intended = match blocked_at.zip(last_display_name) {
                Some((blocked_at, last_display_name)) => BlockedState::Blocked(BlockedContact {
                    user_id,
                    last_display_name,
                    blocked_at,
                }),
                None => BlockedState::Unblocked { user_id },
            };
            Self { intended }
        }
    }

    impl BlockedState {
        /// Reads the stored blocked state of a contact.
        pub(crate) async fn load(
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
                }) => Self::Blocked(BlockedContact {
                    user_id: user_id.clone(),
                    last_display_name,
                    blocked_at,
                }),
                None => Self::Unblocked {
                    user_id: user_id.clone(),
                },
            })
        }

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

    impl PendingBlockedContactChange {
        /// Applies a local blocked-state change and parks it for sending.
        ///
        /// Returns whether a commit has to be sent. A change that re-asserts the
        /// stored state while nothing is parked for the contact is a no-op.
        pub(crate) async fn record(
            txn: &mut WriteDbTransaction<'_>,
            intended: BlockedState,
        ) -> sqlx::Result<bool> {
            let user_id = intended.user_id();
            if Self::load(&mut *txn, user_id).await?.is_none()
                && BlockedState::load(&mut *txn, user_id).await? == intended
            {
                return Ok(false);
            }

            intended.apply(&mut *txn).await?;
            Self { intended }.store(txn).await?;

            Ok(true)
        }

        /// Completes the parked changes after one of our own commits was accepted.
        ///
        /// A contact re-toggled while the commit was in flight stays parked and is
        /// re-sent by the next commit.
        pub(crate) async fn complete_sent(
            txn: &mut WriteDbTransaction<'_>,
            sent: &[BlockedContactEntry],
        ) -> sqlx::Result<()> {
            for entry in sent {
                let Some(user_id) = entry.user_id().and_then(parse_peer_user_id) else {
                    continue;
                };
                let Some(pending) = Self::load(&mut *txn, &user_id).await? else {
                    continue;
                };
                if BlockedContactEntry::from(pending.intended) == *entry {
                    Self::delete(&mut *txn, &user_id).await?;
                }
            }
            Ok(())
        }

        /// Loads the parked changes as the entries a commit carries, sorted by
        /// user id so the encoding is canonical.
        pub(crate) async fn load_entries(
            connection: impl ReadConnection,
        ) -> sqlx::Result<Vec<BlockedContactEntry>> {
            Ok(Self::load_all(connection)
                .await?
                .into_iter()
                .map(|pending| BlockedContactEntry::from(pending.intended))
                .collect())
        }

        pub(super) async fn store(&self, mut connection: impl WriteConnection) -> sqlx::Result<()> {
            let user_id = self.user_id();
            let uuid = user_id.uuid();
            let domain = user_id.domain();
            let (blocked_at, last_display_name) = match &self.intended {
                BlockedState::Blocked(contact) => (
                    Some(contact.blocked_at),
                    Some(contact.last_display_name.clone()),
                ),
                BlockedState::Unblocked { .. } => (None, None),
            };
            query!(
                "INSERT INTO blocked_contact_change (
                    user_uuid,
                    user_domain,
                    blocked_at,
                    last_display_name
                ) VALUES (?1, ?2, ?3, ?4)
                ON CONFLICT (user_uuid, user_domain) DO UPDATE SET
                    blocked_at = excluded.blocked_at,
                    last_display_name = excluded.last_display_name",
                uuid,
                domain,
                blocked_at,
                last_display_name,
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
                    last_display_name AS "last_display_name: _"
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
                    last_display_name AS "last_display_name: _"
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

    fn blocked(user_id: &UserId, blocked_at: i64, last_display_name: &str) -> BlockedState {
        BlockedState::Blocked(BlockedContact {
            user_id: user_id.clone(),
            last_display_name: last_display_name.parse().unwrap(),
            blocked_at: DateTime::from_timestamp(blocked_at, 0).unwrap(),
        })
    }

    fn unblocked(user_id: &UserId) -> BlockedState {
        BlockedState::Unblocked {
            user_id: user_id.clone(),
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
    async fn record_applies_and_parks_a_block(pool: SqlitePool) -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(pool);
        let user = user(1);

        pool.with_write_transaction(async |txn| -> anyhow::Result<()> {
            assert!(
                PendingBlockedContactChange::record(txn, blocked(&user, 10, "Alice")).await?,
                "blocking an unblocked contact must need syncing"
            );

            let pending = PendingBlockedContactChange::load(&mut *txn, &user)
                .await?
                .expect("the change must be parked");
            assert_eq!(pending.intended, blocked(&user, 10, "Alice"));

            // The change is applied optimistically.
            assert_eq!(
                BlockedState::load(&mut *txn, &user).await?,
                blocked(&user, 10, "Alice")
            );
            Ok(())
        })
        .await
    }

    #[sqlx::test]
    async fn record_applies_and_parks_an_unblock(pool: SqlitePool) -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(pool);
        let user = user(1);

        pool.with_write_transaction(async |txn| -> anyhow::Result<()> {
            blocked(&user, 10, "Alice").apply(&mut *txn).await?;

            assert!(PendingBlockedContactChange::record(txn, unblocked(&user)).await?);

            let pending = PendingBlockedContactChange::load(&mut *txn, &user)
                .await?
                .expect("the change must be parked");
            assert_eq!(pending.intended, unblocked(&user));
            assert_eq!(
                BlockedState::load(&mut *txn, &user).await?,
                unblocked(&user)
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
            assert!(!PendingBlockedContactChange::record(txn, unblocked(&unblocked_user)).await?);

            blocked(&blocked_user, 10, "Alice").apply(&mut *txn).await?;
            assert!(
                !PendingBlockedContactChange::record(txn, blocked(&blocked_user, 10, "Alice"))
                    .await?
            );

            assert!(
                PendingBlockedContactChange::load_all(&mut *txn)
                    .await?
                    .is_empty(),
                "a no-op tap must not park a change"
            );
            Ok(())
        })
        .await
    }

    #[sqlx::test]
    async fn record_retoggle_folds_into_one_row(pool: SqlitePool) -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(pool);
        let user = user(1);

        pool.with_write_transaction(async |txn| -> anyhow::Result<()> {
            PendingBlockedContactChange::record(txn, blocked(&user, 10, "Alice")).await?;
            PendingBlockedContactChange::record(txn, unblocked(&user)).await?;
            PendingBlockedContactChange::record(txn, blocked(&user, 20, "Alice B")).await?;

            let pending = PendingBlockedContactChange::load_all(&mut *txn).await?;
            assert_eq!(pending.len(), 1, "a re-toggle must fold into the same row");
            assert_eq!(pending[0].intended, blocked(&user, 20, "Alice B"));
            Ok(())
        })
        .await
    }

    #[sqlx::test]
    async fn complete_sent_removes_a_matching_entry(pool: SqlitePool) -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(pool);
        let user = user(1);

        pool.with_write_transaction(async |txn| -> anyhow::Result<()> {
            PendingBlockedContactChange::record(txn, blocked(&user, 10, "Alice")).await?;
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
            PendingBlockedContactChange::record(txn, blocked(&user, 10, "Alice")).await?;
            let sent = PendingBlockedContactChange::load_entries(&mut *txn).await?;
            PendingBlockedContactChange::record(txn, unblocked(&user)).await?;

            PendingBlockedContactChange::complete_sent(txn, &sent).await?;

            let pending = PendingBlockedContactChange::load(&mut *txn, &user)
                .await?
                .expect("the re-toggled contact must stay parked");
            assert_eq!(pending.intended, unblocked(&user));
            Ok(())
        })
        .await
    }

    #[sqlx::test]
    async fn complete_sent_ignores_an_unknown_entry(pool: SqlitePool) -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(pool);
        let user = user(1);

        pool.with_write_transaction(async |txn| -> anyhow::Result<()> {
            PendingBlockedContactChange::record(txn, blocked(&user, 10, "Alice")).await?;

            PendingBlockedContactChange::complete_sent(txn, &[BlockedContactEntry::Unknown])
                .await?;

            assert!(
                PendingBlockedContactChange::load(&mut *txn, &user)
                    .await?
                    .is_some(),
                "an unknown entry must not complete a parked change"
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
            blocked(&second, 10, "Alice").apply(&mut *txn).await?;
            PendingBlockedContactChange::record(txn, blocked(&first, 20, "Bob")).await?;
            PendingBlockedContactChange::record(txn, unblocked(&second)).await?;

            let entries = PendingBlockedContactChange::load_entries(&mut *txn).await?;
            assert_eq!(
                entries,
                vec![blocked_entry(&first, 20, "Bob"), unblocked_entry(&second)]
            );
            Ok(())
        })
        .await
    }
}
