// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Synchronizing blocked contacts with the user's other devices.
//!
//! Blocking is device-local state that is mirrored to the user's other devices
//! through the self-group.
use aircommon::identifiers::UserId;
use airprotos::client::{
    group_bootstrap::PeerUserId,
    self_group::{BlockedContactEntry, ContactBlocked, ContactUnblocked},
};
use chrono::DateTime;
use sqlx::{query, query_as};
use tracing::{debug, warn};
use uuid::Uuid;

use crate::db::access::{ReadConnection, WriteConnection, WriteDbTransaction};

use super::BlockedContact;

/// The blocked state of one contact, as stored in `blocked_contact`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum BlockedState {
    Blocked(BlockedContact),
    Unblocked { user_id: UserId },
}

/// NB: `blocked_at` loses sub-second precision here.
impl From<&BlockedContact> for BlockedContactEntry {
    fn from(
        BlockedContact {
            user_id,
            last_display_name,
            blocked_at,
        }: &BlockedContact,
    ) -> Self {
        Self::Blocked(ContactBlocked {
            user_id: user_id.clone().into(),
            blocked_at: blocked_at.timestamp().max(0) as u64,
            last_display_name: last_display_name.to_string(),
        })
    }
}

impl From<&BlockedState> for BlockedContactEntry {
    fn from(state: &BlockedState) -> Self {
        match state {
            BlockedState::Blocked(contact) => contact.into(),
            BlockedState::Unblocked { user_id } => Self::Unblocked(ContactUnblocked {
                user_id: user_id.clone().into(),
            }),
        }
    }
}

impl BlockedState {
    /// The state an incoming entry asserts.
    ///
    /// `None` for an entry this client cannot make sense of.
    fn from_entry(entry: &BlockedContactEntry) -> Option<Self> {
        match entry {
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
                    user_id: user_id.try_into().ok()?,
                    last_display_name,
                    blocked_at,
                }))
            }
            BlockedContactEntry::Unblocked(ContactUnblocked { user_id }) => Some(Self::Unblocked {
                user_id: user_id.try_into().ok()?,
            }),
            BlockedContactEntry::Unknown => {
                debug!("Skipping a blocked-contact entry with an unknown state");
                None
            }
        }
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

struct SqlOutgoingEntry {
    user_uuid: Uuid,
    user_domain: String,
    blocked_at: Option<i64>,
    last_display_name: Option<String>,
}

impl From<SqlOutgoingEntry> for BlockedContactEntry {
    fn from(
        SqlOutgoingEntry {
            user_uuid,
            user_domain,
            blocked_at,
            last_display_name,
        }: SqlOutgoingEntry,
    ) -> Self {
        let user_id = PeerUserId {
            uuid: user_uuid,
            domain: user_domain,
        };
        match blocked_at.zip(last_display_name) {
            Some((blocked_at, last_display_name)) => Self::Blocked(ContactBlocked {
                user_id,
                blocked_at: blocked_at.max(0) as u64,
                last_display_name,
            }),
            None => Self::Unblocked(ContactUnblocked { user_id }),
        }
    }
}

/// Parks a locally applied change for the next self-group commit.
pub(crate) async fn store_outgoing_entry(
    mut connection: impl WriteConnection,
    entry: &BlockedContactEntry,
) -> sqlx::Result<()> {
    let (user_id, blocked_at, last_display_name) = match entry {
        BlockedContactEntry::Blocked(ContactBlocked {
            user_id,
            blocked_at,
            last_display_name,
        }) => (
            user_id,
            Some(*blocked_at as i64),
            Some(last_display_name.as_str()),
        ),
        BlockedContactEntry::Unblocked(ContactUnblocked { user_id }) => (user_id, None, None),
        BlockedContactEntry::Unknown => return Ok(()),
    };
    let uuid = user_id.uuid;
    let domain = &user_id.domain;
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

/// The parked entries a commit carries, sorted by user id so the encoding is
/// canonical.
pub(crate) async fn entries_to_broadcast(
    mut connection: impl ReadConnection,
) -> sqlx::Result<Vec<BlockedContactEntry>> {
    let records = query_as!(
        SqlOutgoingEntry,
        r#"SELECT
            user_uuid AS "user_uuid: _",
            user_domain,
            blocked_at,
            last_display_name
        FROM blocked_contact_change
        ORDER BY user_uuid, user_domain"#
    )
    .fetch_all(connection.as_mut())
    .await?;

    Ok(records.into_iter().map(From::from).collect())
}

/// Drops the parked entries that have been committed into the self-group.
pub(crate) async fn complete_sent_entries(
    txn: &mut WriteDbTransaction<'_>,
    sent: &[BlockedContactEntry],
) -> sqlx::Result<()> {
    for entry in sent {
        let Some(user_id) = entry.user_id() else {
            continue;
        };
        if load_outgoing_entry(&mut *txn, user_id).await?.as_ref() == Some(entry) {
            delete_outgoing_entry(&mut *txn, user_id).await?;
        }
    }
    Ok(())
}

async fn load_outgoing_entry(
    mut connection: impl ReadConnection,
    user_id: &PeerUserId,
) -> sqlx::Result<Option<BlockedContactEntry>> {
    let uuid = user_id.uuid;
    let domain = &user_id.domain;
    query_as!(
        SqlOutgoingEntry,
        r#"SELECT
            user_uuid AS "user_uuid: _",
            user_domain,
            blocked_at,
            last_display_name
        FROM blocked_contact_change
        WHERE user_uuid = ?1 AND user_domain = ?2"#,
        uuid,
        domain,
    )
    .fetch_optional(connection.as_mut())
    .await
    .map(|record| record.map(From::from))
}

async fn delete_outgoing_entry(
    mut connection: impl WriteConnection,
    user_id: &PeerUserId,
) -> sqlx::Result<()> {
    let uuid = user_id.uuid;
    let domain = &user_id.domain;
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

#[cfg(test)]
mod tests {
    use sqlx::SqlitePool;

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

    #[test]
    fn entry_conversion_round_trips() {
        let user = user(1);
        for state in [
            BlockedState::Blocked(contact(&user, 10, "Alice")),
            BlockedState::Unblocked {
                user_id: user.clone(),
            },
        ] {
            let entry = BlockedContactEntry::from(&state);
            assert_eq!(BlockedState::from_entry(&entry), Some(state));
        }
    }

    #[test]
    fn from_entry_rejects_unusable_entries() {
        let user = user(1);
        assert_eq!(
            BlockedState::from_entry(&blocked_entry(&user, u64::MAX, "Alice")),
            None,
            "an out-of-range timestamp must be rejected"
        );
        assert_eq!(
            BlockedState::from_entry(&blocked_entry(&user, 10, " \n ")),
            None,
            "a display name that parses to empty must be rejected"
        );
        assert_eq!(
            BlockedState::from_entry(&BlockedContactEntry::Unknown),
            None
        );
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
                entries_to_broadcast(&mut *txn).await?,
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
                entries_to_broadcast(&mut *txn).await?,
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
            let sent = entries_to_broadcast(&mut *txn).await?;

            complete_sent_entries(txn, &sent).await?;

            assert!(entries_to_broadcast(&mut *txn).await?.is_empty());
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
            let sent = entries_to_broadcast(&mut *txn).await?;
            store_outgoing_entry(&mut *txn, &unblocked_entry(&user)).await?;

            complete_sent_entries(txn, &sent).await?;

            assert_eq!(
                entries_to_broadcast(&mut *txn).await?,
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
                entries_to_broadcast(&mut *txn).await?,
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
