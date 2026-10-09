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
    /// Server errors this reaction already ran into.
    pub(crate) attempts: u32,
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

        /// Dequeues the oldest reaction that is due. Like messages, the
        /// reactions of a chat are sent in order, so adding and retracting the
        /// same reaction arrive in the order they were made.
        pub(crate) async fn dequeue(
            txn: &mut WriteDbTransaction<'_>,
            task_id: Uuid,
            due_at: TimeStamp,
        ) -> anyhow::Result<Option<DequeuedReaction>> {
            let Some(id) = query_scalar!(
                r#"
                SELECT id
                FROM reaction_queue AS queued
                WHERE (locked_by IS NULL OR locked_by != ?1)
                    AND (retry_at IS NULL OR retry_at <= ?2)
                    AND NOT EXISTS (
                        SELECT 1
                        FROM reaction_queue AS earlier
                        WHERE earlier.chat_id = queued.chat_id
                            AND (earlier.created_at, earlier.rowid)
                                < (queued.created_at, queued.rowid)
                    )
                ORDER BY created_at ASC, rowid ASC
                LIMIT 1
                "#,
                task_id,
                due_at,
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
                    content,
                    attempts AS "attempts: _"
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

        /// Keeps the reaction queued until `retry_at`.
        pub(crate) async fn record_failed_attempt(
            txn: &mut WriteDbTransaction<'_>,
            id: Uuid,
            attempts: u32,
            retry_at: TimeStamp,
        ) -> sqlx::Result<()> {
            query!(
                "UPDATE reaction_queue
                 SET attempts = ?, retry_at = ? 
                 WHERE id = ?",
                attempts,
                retry_at,
                id,
            )
            .execute(txn.as_mut())
            .await?;
            Ok(())
        }
    }

    #[cfg(test)]
    mod tests {
        use chrono::{TimeDelta, Utc};
        use sqlx::SqlitePool;

        use crate::{chats::persistence::tests::test_chat, db::access::DbAccess};

        use super::*;

        async fn stored_chat(db: &DbAccess) -> anyhow::Result<ChatId> {
            let chat = test_chat();
            chat.store(db.write().await?).await?;
            Ok(chat.id())
        }

        async fn enqueue(db: &DbAccess, chat_id: ChatId, content: u8) -> anyhow::Result<()> {
            ReactionQueue::enqueue(db.write().await?, chat_id, None, &[content]).await?;
            Ok(())
        }

        /// Dequeues everything that is due with a single task id and returns the
        /// contents.
        async fn drain(db: &DbAccess) -> anyhow::Result<Vec<Vec<u8>>> {
            let task_id = Uuid::new_v4();
            let mut contents = Vec::new();
            while let Some(dequeued) = db
                .with_write_transaction(async |txn| {
                    ReactionQueue::dequeue(txn, task_id, TimeStamp::now()).await
                })
                .await?
            {
                contents.push(dequeued.content);
            }
            Ok(contents)
        }

        #[sqlx::test]
        async fn queued_reaction_holds_back_its_chat(pool: SqlitePool) -> anyhow::Result<()> {
            let db = DbAccess::for_tests(pool);
            let chat_id = stored_chat(&db).await?;
            let other_chat_id = stored_chat(&db).await?;
            enqueue(&db, chat_id, 1).await?;
            enqueue(&db, chat_id, 2).await?;
            enqueue(&db, other_chat_id, 3).await?;

            assert_eq!(drain(&db).await?, vec![vec![1], vec![3]]);

            Ok(())
        }

        #[sqlx::test]
        async fn failed_attempt_defers_reaction(pool: SqlitePool) -> anyhow::Result<()> {
            let db = DbAccess::for_tests(pool);
            let chat_id = stored_chat(&db).await?;
            enqueue(&db, chat_id, 1).await?;

            let id = db
                .with_write_transaction(async |txn| {
                    ReactionQueue::dequeue(txn, Uuid::new_v4(), TimeStamp::now()).await
                })
                .await?
                .expect("reaction is queued")
                .id;
            let record = async |attempts, retry_in| -> anyhow::Result<()> {
                let retry_at = TimeStamp::from(Utc::now() + retry_in);
                db.with_write_transaction(async |txn| -> anyhow::Result<_> {
                    ReactionQueue::record_failed_attempt(txn, id, attempts, retry_at).await?;
                    Ok(())
                })
                .await
            };

            record(1, TimeDelta::hours(1)).await?;
            assert!(drain(&db).await?.is_empty());

            record(2, TimeDelta::seconds(-1)).await?;
            let dequeued = db
                .with_write_transaction(async |txn| {
                    ReactionQueue::dequeue(txn, Uuid::new_v4(), TimeStamp::now()).await
                })
                .await?
                .expect("reaction is due again");
            assert_eq!(dequeued.attempts, 2);

            Ok(())
        }
    }
}
