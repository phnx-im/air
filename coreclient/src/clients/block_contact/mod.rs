// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::identifiers::UserId;
use airprotos::client::self_group::BlockedContactEntry;
use chrono::{DateTime, Utc};

use crate::{clients::CoreUser, user_profiles::display_name::DisplayName};

use self::pending::BlockedState;

pub(crate) mod pending;

impl CoreUser {
    /// Blocks a contact.
    pub async fn block_contact(&self, user_id: UserId) -> anyhow::Result<()> {
        let profile = self.user_profile(&user_id).await;
        self.record_blocked_state(BlockedState::Blocked(BlockedContact {
            user_id,
            last_display_name: profile.display_name.clone(),
            blocked_at: Utc::now(),
        }))
        .await
    }

    /// Unblocks a contact.
    pub async fn unblock_contact(&self, user_id: UserId) -> anyhow::Result<()> {
        self.record_blocked_state(BlockedState::Unblocked { user_id })
            .await
    }

    /// Applies a blocked-state change locally right away (optimistic) and
    /// parks to send to other devices via the self-group.
    async fn record_blocked_state(&self, intended: BlockedState) -> anyhow::Result<()> {
        let entry = BlockedContactEntry::from(&intended);
        self.db()
            .with_write_transaction(async |txn| {
                intended.apply(&mut *txn).await?;
                pending::store_outgoing_entry(txn, &entry).await
            })
            .await?;

        self.outbound_service().notify_pending_chat_operations();

        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BlockedContact {
    pub(crate) user_id: UserId,
    pub(crate) last_display_name: DisplayName,
    pub(crate) blocked_at: DateTime<Utc>,
}

#[cfg(test)]
impl BlockedContact {
    pub(crate) fn new(user_id: UserId) -> Self {
        Self {
            last_display_name: DisplayName::from_user_id(&user_id),
            user_id,
            blocked_at: Utc::now(),
        }
    }
}

#[derive(Debug, thiserror::Error)]
#[error("Blocked contact")]
pub struct BlockedContactError;

mod persistence {
    use sqlx::{query, query_scalar};

    use crate::{
        ChatId,
        db::access::{ReadConnection, WriteConnection},
    };

    use super::*;

    struct SqlBlockedContact {
        user_uuid: uuid::Uuid,
        user_domain: aircommon::identifiers::Fqdn,
        last_display_name: DisplayName,
        blocked_at: DateTime<Utc>,
    }

    impl From<SqlBlockedContact> for BlockedContact {
        fn from(
            SqlBlockedContact {
                user_uuid,
                user_domain,
                last_display_name,
                blocked_at,
            }: SqlBlockedContact,
        ) -> Self {
            Self {
                user_id: UserId::new(user_uuid, user_domain),
                last_display_name,
                blocked_at,
            }
        }
    }

    impl BlockedContact {
        /// Stores the block, overwriting an existing one for the same user.
        pub(crate) async fn store(&self, mut connection: impl WriteConnection) -> sqlx::Result<()> {
            let uuid = self.user_id.uuid();
            let domain = self.user_id.domain();
            query!(
                "INSERT INTO blocked_contact (
                    user_uuid,
                    user_domain,
                    last_display_name,
                    blocked_at
                ) VALUES (?1, ?2, ?3, ?4)
                ON CONFLICT (user_uuid, user_domain) DO UPDATE SET
                    last_display_name = excluded.last_display_name,
                    blocked_at = excluded.blocked_at",
                uuid,
                domain,
                self.last_display_name,
                self.blocked_at,
            )
            .execute(connection.as_mut())
            .await?;

            connection.notifier().add(self.user_id.clone());

            Ok(())
        }

        /// Every stored block, sorted by user id so the encoding is canonical.
        pub(crate) async fn load_all(
            mut connection: impl ReadConnection,
        ) -> sqlx::Result<Vec<Self>> {
            let records = sqlx::query_as!(
                SqlBlockedContact,
                r#"SELECT
                    user_uuid AS "user_uuid: _",
                    user_domain AS "user_domain: _",
                    last_display_name AS "last_display_name: _",
                    blocked_at AS "blocked_at: _"
                FROM blocked_contact
                ORDER BY user_uuid, user_domain"#
            )
            .fetch_all(connection.as_mut())
            .await?;

            Ok(records.into_iter().map(From::from).collect())
        }

        pub(crate) async fn check_blocked(
            mut connection: impl ReadConnection,
            user_id: &UserId,
        ) -> sqlx::Result<bool> {
            let user_uuid = user_id.uuid();
            let user_domain = user_id.domain();
            query_scalar!(
                r#"SELECT EXISTS(
                    SELECT 1 FROM blocked_contact
                    WHERE user_uuid = ?1 AND user_domain = ?2
                ) AS "exists: _""#,
                user_uuid,
                user_domain,
            )
            .fetch_one(connection.as_mut())
            .await
        }

        /// Returns `true` if this is a 1:1 chat with a blocked contact.
        ///
        /// Note: Group chats that contain a blocked contact are not considered as blocked.
        /// Therefore, this function returns `false` in this case.
        pub(crate) async fn check_blocked_chat(
            mut connection: impl ReadConnection,
            chat_id: ChatId,
        ) -> sqlx::Result<bool> {
            query_scalar!(
                r#"SELECT EXISTS(
                    SELECT 1 FROM chat c
                    INNER JOIN blocked_contact b
                        ON b.user_uuid = c.connection_user_uuid
                        AND b.user_domain = c.connection_user_domain
                    WHERE chat_id = ?1
                ) AS "exists: _""#,
                chat_id,
            )
            .fetch_one(connection.as_mut())
            .await
        }

        pub(super) async fn delete_by_id(
            mut connection: impl WriteConnection,
            user_id: UserId,
        ) -> sqlx::Result<()> {
            let uuid = user_id.uuid();
            let domain = user_id.domain();
            query!(
                "DELETE FROM blocked_contact WHERE user_uuid = ?1 AND user_domain = ?2",
                uuid,
                domain,
            )
            .execute(connection.as_mut())
            .await?;

            connection.notifier().add(user_id.clone());

            Ok(())
        }
    }
}
