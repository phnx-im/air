// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Group chats whose creation the DS has not confirmed yet.
//!
//! The client stores a new group as pending before it asks the DS to create
//! it, and creates the chat only on confirmation. The DS's answer and the
//! creation echo in the queue both confirm, whichever is processed first. When
//! the answer is lost, the group stays pending until the echo arrives, or until
//! a drained queue shows that the DS did not create the group.

use aircommon::{identifiers::UserId, time::TimeStamp};
use chrono::{DateTime, Duration, Utc};
use openmls::group::GroupId;
use tracing::info;

use crate::{
    Chat, ChatAttributes, ChatId, ChatMessage, SystemMessage, db::access::WriteDbTransaction,
    groups::Group,
};

/// Grace period that we wait until we expect the confirmation to show up in
/// the queue. If we get a "queue empty" after this time, we assume the group
/// wasn't created.
const CONFIRMATION_GRACE: Duration = Duration::minutes(2);

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PendingGroupCreation {
    pub(crate) group_id: GroupId,
    pub(crate) attributes: ChatAttributes,
    pub(crate) created_at: DateTime<Utc>,
}

impl PendingGroupCreation {
    pub(crate) fn new(group_id: GroupId, attributes: ChatAttributes) -> Self {
        Self {
            group_id,
            attributes,
            created_at: Utc::now(),
        }
    }

    /// Confirms the pending creation of `group_id` and creates its chat.
    ///
    /// Returns `None` when no creation of that group is pending, because it was
    /// confirmed or discarded before.
    pub(crate) async fn confirm(
        txn: &mut WriteDbTransaction<'_>,
        group_id: &GroupId,
        creator: &UserId,
        ds_timestamp: TimeStamp,
    ) -> anyhow::Result<Option<ChatId>> {
        let Some(pending) = Self::take(&mut *txn, group_id).await? else {
            return Ok(None);
        };

        let chat = Chat::new_group_chat(pending.group_id, pending.attributes);
        chat.store(&mut *txn).await?;
        ChatMessage::new_system_message(
            chat.id(),
            ds_timestamp,
            SystemMessage::CreateGroup(creator.clone()),
        )
        .store(&mut *txn)
        .await?;

        Ok(Some(chat.id()))
    }

    /// Discards the pending creation of `group_id` together with its group.
    pub(crate) async fn discard(
        txn: &mut WriteDbTransaction<'_>,
        group_id: &GroupId,
    ) -> sqlx::Result<()> {
        // Cascades to the pending creation
        Group::delete_from_db(txn, group_id).await
    }

    /// Discards the creations that were pending well before the drain of the
    /// queue began at `drained_at`.
    pub(crate) async fn discard_unconfirmed(
        txn: &mut WriteDbTransaction<'_>,
        drained_at: DateTime<Utc>,
    ) -> sqlx::Result<()> {
        let created_before = drained_at - CONFIRMATION_GRACE;
        for group_id in Self::load_created_before(&mut *txn, created_before).await? {
            info!(
                ?group_id,
                "Discarding a group creation the DS did not confirm"
            );
            Self::discard(txn, &group_id).await?;
        }
        Ok(())
    }
}

mod persistence {
    use sqlx::{query, query_as, query_scalar};

    use crate::{
        db::access::{ReadConnection, WriteConnection},
        utils::persistence::{GroupIdRefWrapper, GroupIdWrapper},
    };

    use super::*;

    struct SqlPendingGroupCreation {
        group_id: GroupIdWrapper,
        title: String,
        picture: Option<Vec<u8>>,
        created_at: DateTime<Utc>,
    }

    impl From<SqlPendingGroupCreation> for PendingGroupCreation {
        fn from(
            SqlPendingGroupCreation {
                group_id: GroupIdWrapper(group_id),
                title,
                picture,
                created_at,
            }: SqlPendingGroupCreation,
        ) -> Self {
            Self {
                group_id,
                attributes: ChatAttributes::new(title, picture),
                created_at,
            }
        }
    }

    impl PendingGroupCreation {
        pub(crate) async fn store(&self, mut connection: impl WriteConnection) -> sqlx::Result<()> {
            let group_id = GroupIdRefWrapper::from(&self.group_id);
            query!(
                "INSERT INTO pending_group_creation (group_id, title, picture, created_at)
                VALUES (?, ?, ?, ?)",
                group_id,
                self.attributes.title,
                self.attributes.picture,
                self.created_at,
            )
            .execute(connection.as_mut())
            .await?;
            Ok(())
        }

        #[cfg(any(test, feature = "test_utils"))]
        pub(crate) async fn count(mut connection: impl ReadConnection) -> sqlx::Result<i64> {
            query_scalar!(r#"SELECT COUNT(*) AS "count!: i64" FROM pending_group_creation"#)
                .fetch_one(connection.as_mut())
                .await
        }

        /// Removes and returns the pending creation of `group_id`.
        pub(super) async fn take(
            mut connection: impl WriteConnection,
            group_id: &GroupId,
        ) -> sqlx::Result<Option<Self>> {
            let group_id = GroupIdRefWrapper::from(group_id);
            let pending = query_as!(
                SqlPendingGroupCreation,
                r#"DELETE FROM pending_group_creation
                WHERE group_id = ?
                RETURNING
                    group_id AS "group_id: _",
                    title,
                    picture,
                    created_at AS "created_at: _""#,
                group_id,
            )
            .fetch_optional(connection.as_mut())
            .await?;
            Ok(pending.map(From::from))
        }

        pub(super) async fn load_created_before(
            mut connection: impl ReadConnection,
            created_before: DateTime<Utc>,
        ) -> sqlx::Result<Vec<GroupId>> {
            let group_ids = query_scalar!(
                r#"SELECT group_id AS "group_id: GroupIdWrapper"
                FROM pending_group_creation
                WHERE created_at < ?"#,
                created_before,
            )
            .fetch_all(connection.as_mut())
            .await?;
            Ok(group_ids.into_iter().map(|GroupIdWrapper(id)| id).collect())
        }
    }
}
