// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::identifiers::MimiId;
use mimi_content::MessageStatus;
use uuid::Uuid;

use crate::{ChatId, MessageId};

pub(crate) struct ReceiptQueue {
    message_id: MessageId,
    message_status: MessageStatus,
}

impl ReceiptQueue {
    pub(crate) fn new(message_id: MessageId, message_status: MessageStatus) -> Self {
        Self {
            message_id,
            message_status,
        }
    }
}

/// A unique identifier for a single dequeue operation.
///
/// Used to delete dequeued entries from the queue.
#[derive(Debug, Clone, Copy)]
pub(super) struct DequeueId {
    uuid: Uuid,
}

impl DequeueId {
    fn random() -> Self {
        Self {
            uuid: Uuid::new_v4(),
        }
    }
}

#[derive(Debug)]
pub(super) struct DequeueEntry {
    pub(super) dequeue_id: DequeueId,
    pub(super) chat_id: ChatId,
    pub(super) statuses: Vec<(MimiId, MessageStatus)>,
}

mod persistence {
    use std::time::Duration;

    use aircommon::{identifiers::MimiId, time::TimeStamp};
    use sqlx::{SqliteExecutor, SqlitePool, query, query_as, query_scalar};
    use tokio_stream::StreamExt;
    use tracing::debug;
    use uuid::Uuid;

    use crate::ChatId;

    use super::*;

    impl ReceiptQueue {
        pub(crate) async fn enqueue(
            &self,
            executor: impl SqliteExecutor<'_>,
            chat_id: ChatId,
            mimi_id: &MimiId,
        ) -> sqlx::Result<()> {
            debug!(
                ?chat_id,
                ?self.message_id, ?mimi_id, ?self.message_status, "Enqueueing receipt"
            );

            let status = self.message_status.repr();
            let now = TimeStamp::now();

            query!(
                "INSERT INTO receipt_queue
                    (message_id,  chat_id, mimi_id, status, created_at)
                VALUES (?1, ?2, ?3, ?4, ?5)
                ON CONFLICT DO NOTHING",
                self.message_id,
                chat_id,
                mimi_id,
                status,
                now,
            )
            .execute(executor)
            .await?;
            Ok(())
        }

        pub(crate) async fn dequeue(
            pool: &SqlitePool,
            task_id: Uuid,
        ) -> anyhow::Result<Option<DequeueEntry>> {
            Self::dequeue_with_timeout(pool, task_id, LOCKED_THRESHOLD).await
        }

        pub(crate) async fn dequeue_with_timeout(
            pool: &SqlitePool,
            task_id: Uuid,
            timeout: Duration,
        ) -> anyhow::Result<Option<DequeueEntry>> {
            let mut txn = pool.begin_with("BEGIN IMMEDIATE").await?;

            let now = TimeStamp::now();
            let locked_before = *now - timeout;

            let chat_id = query_scalar!(
                r#"SELECT chat_id AS "chat_id: _"
                    FROM receipt_queue
                    WHERE locked_at IS NULL OR locked_at < ?
                    ORDER BY created_at ASC
                    LIMIT 1
                "#,
                locked_before,
            )
            .fetch_optional(txn.as_mut())
            .await?;
            let Some(chat_id) = chat_id else {
                return Ok(None);
            };

            let dequeue_id = DequeueId::random();

            struct Record {
                mimi_id: MimiId,
                status: u8,
            }

            let statuses = query_as!(
                Record,
                r#"UPDATE receipt_queue
                    SET locked_by = ?1, locked_at = ?2, dequeue_id = ?3
                    WHERE chat_id = ?4 AND (locked_at IS NULL OR locked_at < ?5)
                RETURNING
                    mimi_id AS "mimi_id: _",
                    status AS "status: _"
                "#,
                task_id,
                now,
                dequeue_id.uuid,
                chat_id,
                locked_before,
            )
            .fetch(txn.as_mut())
            .map(|record| {
                record.map(|record| (record.mimi_id, MessageStatus::from_repr(record.status)))
            })
            .collect::<Result<Vec<_>, _>>()
            .await?;

            txn.commit().await?;

            Ok(Some(DequeueEntry {
                dequeue_id,
                chat_id,
                statuses,
            }))
        }

        pub(crate) async fn remove(
            executor: impl SqliteExecutor<'_>,
            dequeue_id: DequeueId,
        ) -> sqlx::Result<()> {
            query!(
                "DELETE FROM receipt_queue WHERE dequeue_id = ?",
                dequeue_id.uuid,
            )
            .execute(executor)
            .await?;
            Ok(())
        }
    }

    const LOCKED_THRESHOLD: Duration = Duration::from_secs(30);

    #[cfg(test)]
    mod tests {
        use crate::{
            chats::{
                messages::persistence::tests::test_chat_message, persistence::tests::test_chat,
            },
            store::StoreNotifier,
            utils::init_test_tracing,
        };

        use super::*;

        #[sqlx::test]
        fn enqueue_dequeue_delete(pool: SqlitePool) -> anyhow::Result<()> {
            init_test_tracing();

            // Fixtures
            let mut store_notifier = StoreNotifier::noop();

            let chat_a = test_chat();
            chat_a
                .store(pool.acquire().await?.as_mut(), &mut store_notifier)
                .await?;

            let chat_b = test_chat();
            chat_b
                .store(pool.acquire().await?.as_mut(), &mut store_notifier)
                .await?;

            let message_a1 = test_chat_message(chat_a.id());
            message_a1.store(&pool, &mut store_notifier).await?;

            let message_a2 = test_chat_message(chat_a.id());
            message_a2.store(&pool, &mut store_notifier).await?;

            let message_b1 = test_chat_message(chat_b.id());
            message_b1.store(&pool, &mut store_notifier).await?;

            for message in [&message_a1, &message_a2, &message_b1] {
                let mimi_id = message.message().mimi_id().unwrap();
                for status in [MessageStatus::Delivered, MessageStatus::Read] {
                    ReceiptQueue::new(message.id(), status)
                        .enqueue(&pool, message.chat_id(), mimi_id)
                        .await?;
                }
            }

            let chat_a_messages = [&message_a1, &message_a2];
            let chat_b_messages = [&message_b1];

            // Dequeue
            let task_id = Uuid::new_v4();
            let entry = ReceiptQueue::dequeue(&pool, task_id).await?.unwrap();
            assert_eq!(entry.chat_id, chat_a.id());
            assert_eq!(entry.statuses.len(), 4);

            for message in chat_a_messages {
                let mimi_id = message.message().mimi_id().unwrap();
                for status in [MessageStatus::Delivered, MessageStatus::Read] {
                    assert!(entry.statuses.contains(&(*mimi_id, status)));
                }
            }

            // Dequeue again with the same task ID returns the next chat
            let entry = ReceiptQueue::dequeue(&pool, task_id).await?.unwrap();
            assert_eq!(entry.chat_id, chat_b.id());
            assert_eq!(entry.statuses.len(), 2);

            for message in chat_b_messages {
                let mimi_id = message.message().mimi_id().unwrap();
                for status in [MessageStatus::Delivered, MessageStatus::Read] {
                    assert!(entry.statuses.contains(&(*mimi_id, status)));
                }
            }

            // Dequeue again with the same task ID returns nothing
            let entry = ReceiptQueue::dequeue(&pool, task_id).await?;
            assert!(entry.is_none());

            // Dequeue with a different task ID steals the first entry
            let task_id = Uuid::new_v4();
            let entry = ReceiptQueue::dequeue_with_timeout(&pool, task_id, Duration::ZERO)
                .await?
                .unwrap();
            assert_eq!(entry.chat_id, chat_a.id());
            assert_eq!(entry.statuses.len(), 4);

            for message in chat_a_messages {
                let mimi_id = message.message().mimi_id().unwrap();
                for status in [MessageStatus::Delivered, MessageStatus::Read] {
                    assert!(entry.statuses.contains(&(*mimi_id, status)));
                }
            }

            let rows = sqlx::query(
                "SELECT message_id, chat_id, locked_at FROM receipt_queue ORDER BY created_at",
            )
            .fetch_all(&pool)
            .await?;
            for row in rows {
                let message_id = row.get::<MessageId, _>("message_id");
                let chat_id = row.get::<ChatId, _>("chat_id");
                let locked_at = row.get::<Option<TimeStamp>, _>("locked_at");
                tracing::info!(?chat_id, ?message_id, ?locked_at, "row");
            }
            tracing::info!("done");

            // Deleting the first entry only removes the first entry and not the new data which was
            // added
            let message_a3 = test_chat_message(chat_a.id());
            message_a3.store(&pool, &mut store_notifier).await?;
            for status in [MessageStatus::Delivered, MessageStatus::Read] {
                ReceiptQueue::new(message_a3.id(), status)
                    .enqueue(&pool, chat_a.id(), message_a3.message().mimi_id().unwrap())
                    .await?;
            }
            ReceiptQueue::remove(&pool, entry.dequeue_id).await?;

            use sqlx::Row;

            let rows = sqlx::query(
                "SELECT message_id, chat_id, locked_at FROM receipt_queue ORDER BY created_at",
            )
            .fetch_all(&pool)
            .await?;
            for row in rows {
                let message_id = row.get::<MessageId, _>("message_id");
                let chat_id = row.get::<ChatId, _>("chat_id");
                let locked_at = row.get::<Option<TimeStamp>, _>("locked_at");
                tracing::info!(?chat_id, ?message_id, ?locked_at, "row");
            }
            tracing::info!("done");

            // Dequeue again with the same task ID returns the second chat
            let task_id = Uuid::new_v4();
            let entry = ReceiptQueue::dequeue_with_timeout(&pool, task_id, Duration::ZERO)
                .await?
                .unwrap();
            assert_eq!(entry.chat_id, chat_b.id());
            assert_eq!(entry.statuses.len(), 2);

            for message in chat_b_messages {
                let mimi_id = message.message().mimi_id().unwrap();
                for status in [MessageStatus::Delivered, MessageStatus::Read] {
                    assert!(entry.statuses.contains(&(*mimi_id, status)));
                }
            }

            // Dequeue again with the same task ID returns the new data which was added
            let entry = ReceiptQueue::dequeue_with_timeout(&pool, task_id, Duration::ZERO)
                .await?
                .unwrap();
            assert_eq!(entry.chat_id, chat_a.id());
            assert_eq!(entry.statuses.len(), 2);

            let mimi_id = message_a3.message().mimi_id().unwrap();
            for status in [MessageStatus::Delivered, MessageStatus::Read] {
                assert!(entry.statuses.contains(&(*mimi_id, status)));
            }

            Ok(())
        }
    }
}
