// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use mimi_content::MessageStatus;

use crate::MessageId;

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

mod persistence {
    use std::time::Duration;

    use aircommon::{identifiers::MimiId, time::TimeStamp};
    use mimi_content::{MessageStatusReport, PerMessageStatus};
    use sqlx::{query, query_as, query_scalar};
    use tokio_stream::StreamExt;
    use tracing::debug;
    use uuid::Uuid;

    use crate::{ChatId, db::access::WriteConnection};

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
            .execute(connection.as_mut())
            .await?;
            Ok(())
        }

        pub(crate) async fn dequeue(
            mut connection: impl WriteConnection,
            task_id: Uuid,
        ) -> anyhow::Result<Option<(ChatId, Vec<(MimiId, MessageStatus)>)>> {
            let mut txn = connection.begin().await?;

            let now = TimeStamp::now();
            let locked_before = *now - LOCKED_THRESHOLD;

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

            struct Record {
                mimi_id: MimiId,
                status: u8,
            }

            let statuses = query_as!(
                Record,
                r#"UPDATE receipt_queue
                    SET locked_by = ?1, locked_at = ?2
                    WHERE chat_id = ?3 AND (locked_at IS NULL OR locked_at < ?4)
                RETURNING
                    mimi_id AS "mimi_id: _",
                    status AS "status: _"
                "#,
                task_id,
                now,
                chat_id,
                locked_before,
            )
            .fetch(txn.as_mut())
            .map(|record| record.map(|record| (record.mimi_id, MessageStatus::from(record.status))))
            .collect::<Result<Vec<_>, _>>()
            .await?;

            txn.commit().await?;

            Ok(Some((chat_id, statuses)))
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
    use aircommon::identifiers::MimiId;
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

        let (_, statuses) = ReceiptQueue::dequeue(db.write().await?, Uuid::new_v4())
            .await?
            .expect("no receipt queued");
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
        let (_, statuses) = ReceiptQueue::dequeue(db.write().await?, in_flight)
            .await?
            .expect("no receipt queued");
        assert_eq!(statuses, vec![(original, MessageStatus::Delivered)]);

        receipt.enqueue(db.write().await?, chat_id, &edited).await?;
        ReceiptQueue::remove(db.write().await?, in_flight).await?;

        let (_, statuses) = ReceiptQueue::dequeue(db.write().await?, Uuid::new_v4())
            .await?
            .expect("receipt for the edit was removed");
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
        ReceiptQueue::dequeue(db.write().await?, in_flight)
            .await?
            .expect("no receipt queued");

        receipt
            .enqueue(db.write().await?, chat_id, &original)
            .await?;
        ReceiptQueue::remove(db.write().await?, in_flight).await?;

        let queued = ReceiptQueue::dequeue(db.write().await?, Uuid::new_v4()).await?;
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

        let (_, statuses) = ReceiptQueue::dequeue(db.write().await?, Uuid::new_v4())
            .await?
            .expect("receipt for the current version was removed");
        assert_eq!(statuses, vec![(edited, MessageStatus::Delivered)]);
        Ok(())
    }
}
