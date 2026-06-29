// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::identifiers::MimiId;

use crate::ChatId;

/// A reaction MLS message scheduled for being sent out.
///
/// Unlike the chat message queue, the queue carries the exact serialized
/// `MimiContent` to send: both adding a reaction and retracting one (which
/// deletes the `reaction` row) flow through the same send loop.
pub(crate) struct ReactionQueue;

/// A dequeued, locked reaction ready to be sent.
pub(crate) struct DequeuedReaction {
    pub(crate) id: uuid::Uuid,
    pub(crate) chat_id: ChatId,
    /// The reaction row to roll back if sending fails permanently. `None` for
    /// retraction tombstones (the row is already gone).
    pub(crate) reaction_mimi_id: Option<MimiId>,
    /// Serialized `MimiContent` to send.
    pub(crate) content: Vec<u8>,
}

mod persistence {
    use aircommon::time::TimeStamp;
    use sqlx::{query, query_as, query_scalar};
    use tracing::debug;
    use uuid::Uuid;

    use crate::db::access::{WriteConnection, WriteDbTransaction};

    use super::*;

    impl ReactionQueue {
        pub(crate) async fn enqueue(
            mut connection: impl WriteConnection,
            chat_id: ChatId,
            reaction_mimi_id: Option<&MimiId>,
            content: &[u8],
        ) -> sqlx::Result<()> {
            let id = Uuid::new_v4();
            let now = TimeStamp::now();
            debug!(?chat_id, ?reaction_mimi_id, "Enqueueing reaction");

            query!(
                "INSERT INTO reaction_queue
                    (id, chat_id, reaction_mimi_id, content, created_at)
                VALUES (?1, ?2, ?3, ?4, ?5)",
                id,
                chat_id,
                reaction_mimi_id,
                content,
                now,
            )
            .execute(connection.as_mut())
            .await?;
            Ok(())
        }

        pub(crate) async fn dequeue(
            txn: &mut WriteDbTransaction<'_>,
            task_id: Uuid,
        ) -> anyhow::Result<Option<DequeuedReaction>> {
            let Some(id) = query_scalar!(
                r#"
                SELECT id
                FROM reaction_queue
                WHERE locked_by IS NULL OR locked_by != ?1
                ORDER BY created_at ASC
                LIMIT 1
                "#,
                task_id
            )
            .fetch_optional(txn.as_mut())
            .await?
            else {
                return Ok(None);
            };

            let res = query_as!(
                DequeuedReaction,
                r#"
                UPDATE reaction_queue
                SET locked_by = ?1
                WHERE id = ?2
                RETURNING
                    id AS "id: _",
                    chat_id AS "chat_id: _",
                    reaction_mimi_id AS "reaction_mimi_id: _",
                    content
                "#,
                task_id,
                id
            )
            .fetch_optional(txn.as_mut())
            .await?;

            Ok(res)
        }

        pub(crate) async fn remove(txn: &mut WriteDbTransaction<'_>, id: Uuid) -> sqlx::Result<()> {
            query!("DELETE FROM reaction_queue WHERE id = ?", id)
                .execute(txn.as_mut())
                .await?;
            Ok(())
        }
    }
}
