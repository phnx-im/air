// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::identifiers::MimiId;
use aircommon::messages::client_ds_out::MAX_COLLISION_TAGS_PER_REQUEST;
use mimi_content::MessageStatus;

use crate::{ChatId, MessageId};

/// A receipt scheduled for sending.
///
/// The queue holds one entry per Mimi ID and status. An edit changes the Mimi
/// ID of a message, so a receipt reports on one version of it. Applying an
/// edit drops the receipts still queued for the versions it replaces, and the
/// sender only counts the receipt for the version it stores.
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

/// The receipts of a chat, dequeued and locked together.
pub(crate) struct DequeuedReceipts {
    pub(crate) chat_id: ChatId,
    pub(crate) statuses: Vec<(MimiId, MessageStatus)>,
    /// Server errors these receipts already ran into, the most of any of them.
    pub(crate) attempts: u32,
}

/// Upper bound on the receipts sent in one message
///
/// Each receipt takes a collision tag, and the message generation takes one
/// more.
const MAX_RECEIPTS_PER_MESSAGE: usize = MAX_COLLISION_TAGS_PER_REQUEST - 1;

mod persistence {
    use std::time::Duration;

    use aircommon::time::TimeStamp;
    use mimi_content::{MessageStatusReport, PerMessageStatus};
    use sqlx::{query, query_as, query_scalar};
    use tracing::debug;
    use uuid::Uuid;

    use crate::db::access::WriteConnection;

    use super::*;

    impl ReceiptQueue {
        pub(crate) async fn enqueue(
            &self,
            mut connection: impl WriteConnection,
            chat_id: ChatId,
            mimi_id: &MimiId,
        ) -> sqlx::Result<()> {
            debug!(
                ?chat_id,
                ?self.message_id, ?mimi_id, ?self.message_status, "Enqueueing receipt"
            );

            let status: u8 = self.message_status.into();
            let delivered_status: u8 = MessageStatus::Delivered.into();
            let read_status: u8 = MessageStatus::Read.into();
            let now = TimeStamp::now();

            // A read receipt implies delivery, so we can skip queued delivery receipts for
            // the same message (helping decrease traffic and skip using a collision tag).
            if self.message_status == MessageStatus::Read {
                let locked_before = *now - LOCKED_THRESHOLD;
                query!(
                    "DELETE FROM receipt_queue
                    WHERE chat_id = ?1 AND mimi_id = ?2 AND status = ?3
                        AND (locked_at IS NULL OR locked_at < ?4)",
                    chat_id,
                    mimi_id,
                    delivered_status,
                    locked_before,
                )
                .execute(connection.as_mut())
                .await?;
            }

            query!(
                "INSERT INTO receipt_queue
                    (message_id,  chat_id, mimi_id, status, created_at)
                SELECT ?1, ?2, ?3, ?4, ?5
                WHERE NOT (
                    ?4 = ?6 AND EXISTS (
                        SELECT 1 FROM receipt_queue
                        WHERE chat_id = ?2 AND mimi_id = ?3 AND status = ?7
                    )
                )
                ON CONFLICT DO NOTHING",
                self.message_id,
                chat_id,
                mimi_id,
                status,
                now,
                delivered_status,
                read_status,
            )
            .execute(connection.as_mut())
            .await?;
            Ok(())
        }

        /// Dequeues the due receipts of the chat with the oldest one.
        pub(crate) async fn dequeue(
            mut connection: impl WriteConnection,
            task_id: Uuid,
            due_at: TimeStamp,
        ) -> anyhow::Result<Option<DequeuedReceipts>> {
            let mut txn = connection.begin().await?;

            let locked_before = *due_at - LOCKED_THRESHOLD;
            let limit = MAX_RECEIPTS_PER_MESSAGE as i64;

            let chat_id = query_scalar!(
                r#"SELECT chat_id AS "chat_id: _"
                    FROM receipt_queue
                    WHERE (locked_at IS NULL OR locked_at < ?1)
                        AND (retry_at IS NULL OR retry_at <= ?2)
                    ORDER BY created_at ASC
                    LIMIT 1
                "#,
                locked_before,
                due_at,
            )
            .fetch_optional(txn.as_mut())
            .await?;
            let Some(chat_id) = chat_id else {
                return Ok(None);
            };

            struct Record {
                mimi_id: MimiId,
                status: u8,
                attempts: u32,
            }

            let records = query_as!(
                Record,
                r#"UPDATE receipt_queue
                    SET locked_by = ?1, locked_at = ?2
                    WHERE rowid IN (
                        SELECT rowid FROM receipt_queue
                        WHERE chat_id = ?3 
                            AND (locked_at IS NULL OR locked_at < ?4)
                            AND (retry_at IS NULL OR retry_at <= ?2)
                        ORDER BY created_at ASC
                        LIMIT ?5
                    )
                RETURNING
                    mimi_id AS "mimi_id: _",
                    status AS "status: _",
                    attempts AS "attempts: _"
                "#,
                task_id,
                due_at,
                chat_id,
                locked_before,
                limit,
            )
            .fetch_all(txn.as_mut())
            .await?;

            txn.commit().await?;

            let attempts = records
                .iter()
                .map(|record| record.attempts)
                .max()
                .unwrap_or_default();
            let statuses = records
                .into_iter()
                .map(|record| (record.mimi_id, MessageStatus::from(record.status)))
                .collect();
            Ok(Some(DequeuedReceipts {
                chat_id,
                statuses,
                attempts,
            }))
        }

        /// Keeps the receipts locked by `task_id` queued until `retry_at`.
        pub(crate) async fn record_failed_attempt(
            mut connection: impl WriteConnection,
            task_id: Uuid,
            attempts: u32,
            retry_at: TimeStamp,
        ) -> sqlx::Result<()> {
            query!(
                "UPDATE receipt_queue
                 SET attempts = ?, retry_at = ?
                 WHERE locked_by = ?",
                attempts,
                retry_at,
                task_id,
            )
            .execute(connection.as_mut())
            .await?;
            Ok(())
        }

        pub(crate) async fn remove(
            mut connection: impl WriteConnection,
            task_id: Uuid,
        ) -> sqlx::Result<()> {
            query!("DELETE FROM receipt_queue WHERE locked_by = ?", task_id)
                .execute(connection.as_mut())
                .await?;
            Ok(())
        }

        /// Removes the receipts queued for earlier versions of a message. Once
        /// an edit is applied, only the version it produced is reported on.
        pub(crate) async fn remove_superseded(
            mut connection: impl WriteConnection,
            message_id: MessageId,
            current_mimi_id: &MimiId,
        ) -> sqlx::Result<()> {
            query!(
                "DELETE FROM receipt_queue WHERE message_id = ? AND mimi_id != ?",
                message_id,
                current_mimi_id,
            )
            .execute(connection.as_mut())
            .await?;
            Ok(())
        }

        /// Remove only the receipts a sibling already delivered, identified by
        /// their `(mimi_id, status)`. Rows still locked by `task_id` that are not
        /// listed remain queued so they are re-sent at a later generation.
        pub(crate) async fn remove_delivered(
            mut connection: impl WriteConnection,
            task_id: Uuid,
            delivered: &MessageStatusReport,
        ) -> sqlx::Result<()> {
            for PerMessageStatus { mimi_id, status } in &delivered.statuses {
                let mimi_id = mimi_id.as_slice();
                let status: u8 = (*status).into();
                query!(
                    "DELETE FROM receipt_queue
                        WHERE locked_by = ?1 AND mimi_id = ?2 AND status = ?3",
                    task_id,
                    mimi_id,
                    status,
                )
                .execute(connection.as_mut())
                .await?;
            }
            Ok(())
        }
    }

    const LOCKED_THRESHOLD: Duration = Duration::from_secs(30);
}

#[cfg(test)]
mod tests {
    use aircommon::{identifiers::MimiId, time::TimeStamp};
    use chrono::{TimeDelta, Utc};
    use sqlx::SqlitePool;
    use uuid::Uuid;

    use crate::{
        ChatId,
        chats::{
            messages::persistence::tests::test_chat_message_with_salt,
            persistence::tests::test_chat,
        },
        db::access::DbAccess,
    };

    use super::*;

    /// Stores a chat with one message and returns their ids and the Mimi ID
    /// of the message.
    async fn stored_message(db: &DbAccess) -> anyhow::Result<(ChatId, MessageId, MimiId)> {
        let chat = test_chat();
        chat.store(db.write().await?).await?;
        let message = test_chat_message_with_salt(chat.id(), [1; 16]);
        message.store(db.write().await?).await?;
        let mimi_id = *message.message().mimi_id().expect("no mimi id");
        Ok((chat.id(), message.id(), mimi_id))
    }

    #[sqlx::test]
    async fn enqueue_keeps_a_receipt_per_message_version(pool: SqlitePool) -> anyhow::Result<()> {
        let db = DbAccess::for_tests(pool);
        let (chat_id, message_id, original) = stored_message(&db).await?;
        let edited = MimiId::from_slice(&[9; 32])?;

        let receipt = ReceiptQueue::new(message_id, MessageStatus::Delivered);
        receipt
            .enqueue(db.write().await?, chat_id, &original)
            .await?;
        receipt.enqueue(db.write().await?, chat_id, &edited).await?;

        let statuses = ReceiptQueue::dequeue(db.write().await?, Uuid::new_v4(), TimeStamp::now())
            .await?
            .expect("no receipt queued")
            .statuses;
        assert_eq!(statuses.len(), 2);
        assert!(statuses.contains(&(original, MessageStatus::Delivered)));
        assert!(statuses.contains(&(edited, MessageStatus::Delivered)));
        Ok(())
    }

    #[sqlx::test]
    async fn enqueue_for_new_version_outlives_in_flight_receipt(
        pool: SqlitePool,
    ) -> anyhow::Result<()> {
        let db = DbAccess::for_tests(pool);
        let (chat_id, message_id, original) = stored_message(&db).await?;
        let edited = MimiId::from_slice(&[9; 32])?;

        let receipt = ReceiptQueue::new(message_id, MessageStatus::Delivered);
        receipt
            .enqueue(db.write().await?, chat_id, &original)
            .await?;
        let in_flight = Uuid::new_v4();
        let statuses = ReceiptQueue::dequeue(db.write().await?, in_flight, TimeStamp::now())
            .await?
            .expect("no receipt queued")
            .statuses;
        assert_eq!(statuses, vec![(original, MessageStatus::Delivered)]);

        receipt.enqueue(db.write().await?, chat_id, &edited).await?;
        ReceiptQueue::remove(db.write().await?, in_flight).await?;

        let statuses = ReceiptQueue::dequeue(db.write().await?, Uuid::new_v4(), TimeStamp::now())
            .await?
            .expect("receipt for the edit was removed")
            .statuses;
        assert_eq!(statuses, vec![(edited, MessageStatus::Delivered)]);
        Ok(())
    }

    #[sqlx::test]
    async fn enqueue_ignores_duplicate_receipt(pool: SqlitePool) -> anyhow::Result<()> {
        let db = DbAccess::for_tests(pool);
        let (chat_id, message_id, original) = stored_message(&db).await?;

        let receipt = ReceiptQueue::new(message_id, MessageStatus::Delivered);
        receipt
            .enqueue(db.write().await?, chat_id, &original)
            .await?;
        let in_flight = Uuid::new_v4();
        ReceiptQueue::dequeue(db.write().await?, in_flight, TimeStamp::now())
            .await?
            .expect("no receipt queued");

        receipt
            .enqueue(db.write().await?, chat_id, &original)
            .await?;
        ReceiptQueue::remove(db.write().await?, in_flight).await?;

        let queued =
            ReceiptQueue::dequeue(db.write().await?, Uuid::new_v4(), TimeStamp::now()).await?;
        assert!(queued.is_none(), "duplicate enqueue revived a sent receipt");
        Ok(())
    }

    #[sqlx::test]
    async fn remove_superseded_keeps_only_current_version(pool: SqlitePool) -> anyhow::Result<()> {
        let db = DbAccess::for_tests(pool);
        let (chat_id, message_id, original) = stored_message(&db).await?;
        let edited = MimiId::from_slice(&[9; 32])?;

        for status in [MessageStatus::Delivered, MessageStatus::Read] {
            ReceiptQueue::new(message_id, status)
                .enqueue(db.write().await?, chat_id, &original)
                .await?;
        }
        ReceiptQueue::new(message_id, MessageStatus::Delivered)
            .enqueue(db.write().await?, chat_id, &edited)
            .await?;

        ReceiptQueue::remove_superseded(db.write().await?, message_id, &edited).await?;

        let statuses = ReceiptQueue::dequeue(db.write().await?, Uuid::new_v4(), TimeStamp::now())
            .await?
            .expect("receipt for the current version was removed")
            .statuses;
        assert_eq!(statuses, vec![(edited, MessageStatus::Delivered)]);
        Ok(())
    }

    /// Lets the locks of all queued receipts expire.
    async fn expire_locks(db: &DbAccess) -> anyhow::Result<()> {
        db.with_write_transaction(async |txn| -> anyhow::Result<_> {
            sqlx::query("UPDATE receipt_queue SET locked_at = NULL")
                .execute(txn.as_mut())
                .await?;
            Ok(())
        })
        .await
    }

    #[sqlx::test]
    async fn failed_attempt_defers_receipts(pool: SqlitePool) -> anyhow::Result<()> {
        let db = DbAccess::for_tests(pool);
        let (chat_id, message_id, original) = stored_message(&db).await?;
        for status in [MessageStatus::Delivered, MessageStatus::Read] {
            ReceiptQueue::new(message_id, status)
                .enqueue(db.write().await?, chat_id, &original)
                .await?;
        }

        let task_id = Uuid::new_v4();
        let dequeued = ReceiptQueue::dequeue(db.write().await?, task_id, TimeStamp::now())
            .await?
            .expect("no receipt queued");
        assert_eq!(dequeued.attempts, 0);

        let later = TimeStamp::from(Utc::now() + TimeDelta::hours(1));
        ReceiptQueue::record_failed_attempt(db.write().await?, task_id, 1, later).await?;
        expire_locks(&db).await?;
        let queued =
            ReceiptQueue::dequeue(db.write().await?, Uuid::new_v4(), TimeStamp::now()).await?;
        assert!(queued.is_none(), "deferred receipts were dequeued");

        let earlier = TimeStamp::from(Utc::now() - TimeDelta::seconds(1));
        ReceiptQueue::record_failed_attempt(db.write().await?, task_id, 2, earlier).await?;
        // A receipt queued since then has not failed yet
        let edited = MimiId::from_slice(&[9; 32])?;
        ReceiptQueue::new(message_id, MessageStatus::Delivered)
            .enqueue(db.write().await?, chat_id, &edited)
            .await?;

        let dequeued = ReceiptQueue::dequeue(db.write().await?, Uuid::new_v4(), TimeStamp::now())
            .await?
            .expect("receipts are due again");
        assert_eq!(dequeued.statuses.len(), 3);
        assert_eq!(dequeued.attempts, 2);
        Ok(())
    }

    #[sqlx::test]
    async fn dequeue_splits_receipts_into_batches(pool: SqlitePool) -> anyhow::Result<()> {
        let db = DbAccess::for_tests(pool.clone());
        let (chat_id, message_id, _) = stored_message(&db).await?;

        let receipt = ReceiptQueue::new(message_id, MessageStatus::Read);
        for i in 0..=MAX_RECEIPTS_PER_MESSAGE {
            let mimi_id = MimiId::from_slice(&[i as u8; 32])?;
            receipt
                .enqueue(db.write().await?, chat_id, &mimi_id)
                .await?;
        }

        let first = Uuid::new_v4();
        let dequeued = ReceiptQueue::dequeue(db.write().await?, first, TimeStamp::now())
            .await?
            .expect("no receipt queued");
        assert_eq!(dequeued.statuses.len(), MAX_RECEIPTS_PER_MESSAGE);

        let second = Uuid::new_v4();
        let dequeued = ReceiptQueue::dequeue(db.write().await?, second, TimeStamp::now())
            .await?
            .expect("no second batch queued");
        assert_eq!(dequeued.statuses.len(), 1);

        // Sending the second batch keeps the first one queued
        ReceiptQueue::remove(db.write().await?, second).await?;
        let queued: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM receipt_queue")
            .fetch_one(&pool)
            .await?;
        assert_eq!(queued, MAX_RECEIPTS_PER_MESSAGE as i64);
        Ok(())
    }

    #[sqlx::test]
    async fn read_receipt_replaces_queued_delivery_receipt(pool: SqlitePool) -> anyhow::Result<()> {
        let db = DbAccess::for_tests(pool);
        let (chat_id, message_id, mimi_id) = stored_message(&db).await?;

        for status in [MessageStatus::Delivered, MessageStatus::Read] {
            ReceiptQueue::new(message_id, status)
                .enqueue(db.write().await?, chat_id, &mimi_id)
                .await?;
        }

        let statuses = ReceiptQueue::dequeue(db.write().await?, Uuid::new_v4(), TimeStamp::now())
            .await?
            .expect("no receipt queued")
            .statuses;
        assert_eq!(statuses, vec![(mimi_id, MessageStatus::Read)]);
        Ok(())
    }

    #[sqlx::test]
    async fn delivery_receipt_after_read_receipt_is_skipped(
        pool: SqlitePool,
    ) -> anyhow::Result<()> {
        let db = DbAccess::for_tests(pool);
        let (chat_id, message_id, mimi_id) = stored_message(&db).await?;

        for status in [MessageStatus::Read, MessageStatus::Delivered] {
            ReceiptQueue::new(message_id, status)
                .enqueue(db.write().await?, chat_id, &mimi_id)
                .await?;
        }

        let statuses = ReceiptQueue::dequeue(db.write().await?, Uuid::new_v4(), TimeStamp::now())
            .await?
            .expect("no receipt queued")
            .statuses;
        assert_eq!(statuses, vec![(mimi_id, MessageStatus::Read)]);
        Ok(())
    }

    #[sqlx::test]
    async fn read_receipt_to_another_chat_keeps_delivery_receipt(
        pool: SqlitePool,
    ) -> anyhow::Result<()> {
        let db = DbAccess::for_tests(pool.clone());
        let (chat_id, message_id, mimi_id) = stored_message(&db).await?;

        ReceiptQueue::new(message_id, MessageStatus::Delivered)
            .enqueue(db.write().await?, chat_id, &mimi_id)
            .await?;
        ReceiptQueue::new(message_id, MessageStatus::Read)
            .enqueue(db.write().await?, ChatId::random(), &mimi_id)
            .await?;

        let queued: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM receipt_queue")
            .fetch_one(&pool)
            .await?;
        assert_eq!(queued, 2);
        Ok(())
    }

    #[sqlx::test]
    async fn delivery_receipt_after_read_receipt_to_another_chat_is_queued(
        pool: SqlitePool,
    ) -> anyhow::Result<()> {
        let db = DbAccess::for_tests(pool.clone());
        let (chat_id, message_id, mimi_id) = stored_message(&db).await?;

        ReceiptQueue::new(message_id, MessageStatus::Read)
            .enqueue(db.write().await?, ChatId::random(), &mimi_id)
            .await?;
        ReceiptQueue::new(message_id, MessageStatus::Delivered)
            .enqueue(db.write().await?, chat_id, &mimi_id)
            .await?;

        let queued: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM receipt_queue")
            .fetch_one(&pool)
            .await?;
        assert_eq!(queued, 2);
        Ok(())
    }

    #[sqlx::test]
    async fn read_receipt_keeps_delivery_receipt_in_flight(pool: SqlitePool) -> anyhow::Result<()> {
        let db = DbAccess::for_tests(pool.clone());
        let (chat_id, message_id, mimi_id) = stored_message(&db).await?;

        ReceiptQueue::new(message_id, MessageStatus::Delivered)
            .enqueue(db.write().await?, chat_id, &mimi_id)
            .await?;
        ReceiptQueue::dequeue(db.write().await?, Uuid::new_v4(), TimeStamp::now())
            .await?
            .expect("no receipt queued");

        ReceiptQueue::new(message_id, MessageStatus::Read)
            .enqueue(db.write().await?, chat_id, &mimi_id)
            .await?;

        let queued: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM receipt_queue")
            .fetch_one(&pool)
            .await?;
        assert_eq!(queued, 2);
        Ok(())
    }
}
