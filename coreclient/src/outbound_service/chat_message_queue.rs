// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::{ChatId, MessageId};

pub(crate) struct ChatMessageQueue {
    chat_id: ChatId,
    message_id: MessageId,
}

impl ChatMessageQueue {
    pub(crate) fn new(chat_id: ChatId, message_id: MessageId) -> Self {
        Self {
            chat_id,
            message_id,
        }
    }
}

/// A dequeued, locked message ready to be sent.
pub(crate) struct DequeuedMessage {
    pub(crate) chat_id: ChatId,
    pub(crate) message_id: MessageId,
    /// Server errors this message already ran into.
    pub(crate) attempts: u32,
}

mod persistence {
    use aircommon::time::TimeStamp;
    use mimi_content::MessageStatus;
    use sqlx::{query, query_as, query_scalar};
    use tracing::debug;
    use uuid::Uuid;

    use crate::db::access::{WriteConnection, WriteDbTransaction};

    use super::*;

    impl ChatMessageQueue {
        pub(crate) async fn enqueue(
            &self,
            mut connection: impl WriteConnection,
        ) -> sqlx::Result<()> {
            debug!(
                ?self.message_id, "Enqueueing chat message"
            );

            let now = TimeStamp::now();

            query!(
                "INSERT INTO chat_message_queue
                    (chat_id, message_id, created_at)
                VALUES (?1, ?2, ?3)
                ON CONFLICT DO NOTHING",
                self.chat_id,
                self.message_id,
                now,
            )
            .execute(connection.as_mut())
            .await?;
            Ok(())
        }

        /// Dequeues the oldest message that is due. A message is only sent once
        /// every earlier message of its chat left the queue, so a message kept
        /// for a later run holds back the rest of its chat.
        pub(crate) async fn dequeue(
            txn: &mut WriteDbTransaction<'_>,
            task_id: Uuid,
        ) -> anyhow::Result<Option<DequeuedMessage>> {
            let now = TimeStamp::now();
            let Some(message_id) = query_scalar!(
                r#"
                SELECT message_id
                FROM chat_message_queue AS queued
                WHERE (locked_by IS NULL OR locked_by != ?1)
                    AND (retry_at IS NULL OR retry_at <= ?2)
                    AND NOT EXISTS (
                        SELECT 1
                        FROM chat_message_queue AS earlier
                        WHERE earlier.chat_id = queued.chat_id
                            AND (earlier.created_at, earlier.rowid)
                                < (queued.created_at, queued.rowid)
                    )
                ORDER BY created_at ASC, rowid ASC
                LIMIT 1
                "#,
                task_id,
                now,
            )
            .fetch_optional(txn.as_mut())
            .await?
            else {
                return Ok(None);
            };

            let dequeued = query_as!(
                DequeuedMessage,
                r#"
                UPDATE chat_message_queue
                SET locked_by = ?1
                WHERE message_id = ?2
                RETURNING
                    message_id AS "message_id: _",
                    chat_id AS "chat_id: _",
                    attempts AS "attempts: _"
                "#,
                task_id,
                message_id
            )
            .fetch_optional(txn.as_mut())
            .await?;

            Ok(dequeued)
        }

        /// Keeps the message queued until `retry_at`.
        pub(crate) async fn record_failed_attempt(
            txn: &mut WriteDbTransaction<'_>,
            message_id: MessageId,
            attempts: u32,
            retry_at: TimeStamp,
        ) -> sqlx::Result<()> {
            query!(
                "UPDATE chat_message_queue SET attempts = ?, retry_at = ? WHERE message_id = ?",
                attempts,
                retry_at,
                message_id,
            )
            .execute(txn.as_mut())
            .await?;
            Ok(())
        }

        pub(crate) async fn remove(
            txn: &mut WriteDbTransaction<'_>,
            message_id: MessageId,
        ) -> sqlx::Result<()> {
            query!(
                "DELETE FROM chat_message_queue WHERE message_id = ?",
                message_id
            )
            .execute(txn.as_mut())
            .await?;
            Ok(())
        }

        pub(crate) async fn remove_and_mark_as_failed(
            &self,
            txn: &mut WriteDbTransaction<'_>,
        ) -> sqlx::Result<()> {
            let failed_status: u8 = MessageStatus::Error.into();
            query!(
                "UPDATE message SET status = ? WHERE message_id = ?",
                failed_status,
                self.message_id
            )
            .execute(txn.as_mut())
            .await?;
            query!(
                "DELETE FROM chat_message_queue WHERE message_id = ?",
                self.message_id,
            )
            .execute(txn.as_mut())
            .await?;
            txn.notifier().update(self.message_id);
            Ok(())
        }

        /// This function does the following:
        ///
        /// - Remove all queued messages
        /// - Mark all messages as failed in the message table
        /// - Notify about all marked messages
        pub(crate) async fn remove_all_and_mark_as_failed(
            txn: &mut WriteDbTransaction<'_>,
        ) -> sqlx::Result<()> {
            let failed_status: u8 = MessageStatus::Error.into();
            let marked_messages: Vec<MessageId> = query_scalar!(
                r#"UPDATE message
                SET status = ?1
                WHERE message_id IN (
                    SELECT message_id FROM chat_message_queue
                );

                DELETE FROM chat_message_queue
                RETURNING message_id as "message_id: _"
                "#,
                failed_status
            )
            .fetch_all(txn.as_mut())
            .await?;

            for message_id in marked_messages {
                txn.notifier().update(message_id);
            }

            Ok(())
        }
    }

    #[cfg(test)]
    mod test {
        use chrono::{TimeDelta, Utc};
        use sqlx::SqlitePool;

        use crate::{
            ChatMessage,
            chats::{
                messages::persistence::tests::test_chat_message_with_salt,
                persistence::tests::test_chat,
            },
            clients::attachment::persistence::{
                PendingAttachmentRecord,
                test::{test_attachment_record, test_pending_attachment_record},
            },
            db::access::DbAccess,
        };

        use super::*;

        /// Stores a chat with two enqueued messages and returns them.
        async fn queued_messages(db: &DbAccess) -> anyhow::Result<(ChatMessage, ChatMessage)> {
            let chat = test_chat();
            chat.store(db.write().await?).await?;

            let first = test_chat_message_with_salt(chat.id(), [1; 16]);
            first.store(db.write().await?).await?;
            let second = test_chat_message_with_salt(chat.id(), [2; 16]);
            second.store(db.write().await?).await?;

            db.with_write_transaction(async |txn| -> anyhow::Result<_> {
                ChatMessageQueue::new(chat.id(), first.id())
                    .enqueue(&mut *txn)
                    .await?;
                ChatMessageQueue::new(chat.id(), second.id())
                    .enqueue(&mut *txn)
                    .await?;
                Ok(())
            })
            .await?;

            Ok((first, second))
        }

        async fn stored_status(
            db: &DbAccess,
            message_id: MessageId,
        ) -> anyhow::Result<MessageStatus> {
            let message = ChatMessage::load(db.read().await?, message_id)
                .await?
                .expect("missing message");
            Ok(message.status())
        }

        /// Dequeues everything left in the queue, using a single task id so that
        /// each entry is returned at most once.
        async fn drain(db: &DbAccess) -> anyhow::Result<Vec<MessageId>> {
            let task_id = Uuid::new_v4();
            let mut message_ids = Vec::new();
            while let Some(dequeued) = db
                .with_write_transaction(async |txn| ChatMessageQueue::dequeue(txn, task_id).await)
                .await?
            {
                message_ids.push(dequeued.message_id);
            }
            Ok(message_ids)
        }

        #[sqlx::test]
        async fn fail_single_message(pool: SqlitePool) -> anyhow::Result<()> {
            let db = DbAccess::for_tests(pool);
            let (failed, kept) = queued_messages(&db).await?;

            db.with_write_transaction(async |txn| -> anyhow::Result<_> {
                ChatMessageQueue::new(failed.chat_id(), failed.id())
                    .remove_and_mark_as_failed(txn)
                    .await?;
                Ok(())
            })
            .await?;

            assert_eq!(stored_status(&db, failed.id()).await?, MessageStatus::Error);
            assert_eq!(stored_status(&db, kept.id()).await?, kept.status());
            assert_eq!(drain(&db).await?, vec![kept.id()]);

            Ok(())
        }

        #[sqlx::test]
        async fn fail_all_messages(pool: SqlitePool) -> anyhow::Result<()> {
            let db = DbAccess::for_tests(pool);
            let (first, second) = queued_messages(&db).await?;

            db.with_write_transaction(async |txn| -> anyhow::Result<_> {
                ChatMessageQueue::remove_all_and_mark_as_failed(txn).await?;
                Ok(())
            })
            .await?;

            assert_eq!(stored_status(&db, first.id()).await?, MessageStatus::Error);
            assert_eq!(stored_status(&db, second.id()).await?, MessageStatus::Error);
            assert!(drain(&db).await?.is_empty());

            Ok(())
        }

        async fn remove(db: &DbAccess, message_id: MessageId) -> anyhow::Result<()> {
            db.with_write_transaction(async |txn| -> anyhow::Result<_> {
                ChatMessageQueue::remove(txn, message_id).await?;
                Ok(())
            })
            .await
        }

        async fn record_failed_attempt(
            db: &DbAccess,
            message_id: MessageId,
            attempts: u32,
            retry_in: TimeDelta,
        ) -> anyhow::Result<()> {
            let retry_at = TimeStamp::from(Utc::now() + retry_in);
            db.with_write_transaction(async |txn| -> anyhow::Result<_> {
                ChatMessageQueue::record_failed_attempt(txn, message_id, attempts, retry_at)
                    .await?;
                Ok(())
            })
            .await
        }

        #[sqlx::test]
        async fn queued_message_holds_back_its_chat(pool: SqlitePool) -> anyhow::Result<()> {
            let db = DbAccess::for_tests(pool);
            let (first, second) = queued_messages(&db).await?;

            let other_chat = test_chat();
            other_chat.store(db.write().await?).await?;
            let other = test_chat_message_with_salt(other_chat.id(), [3; 16]);
            other.store(db.write().await?).await?;
            db.with_write_transaction(async |txn| -> anyhow::Result<_> {
                ChatMessageQueue::new(other_chat.id(), other.id())
                    .enqueue(&mut *txn)
                    .await?;
                Ok(())
            })
            .await?;

            // `first` stays queued, so `second` waits while the other chat goes on
            assert_eq!(drain(&db).await?, vec![first.id(), other.id()]);

            remove(&db, first.id()).await?;
            remove(&db, other.id()).await?;
            assert_eq!(drain(&db).await?, vec![second.id()]);

            Ok(())
        }

        #[sqlx::test]
        async fn failed_attempt_defers_message(pool: SqlitePool) -> anyhow::Result<()> {
            let db = DbAccess::for_tests(pool);
            let (first, _second) = queued_messages(&db).await?;

            // Neither the deferred message nor the rest of its chat is due
            record_failed_attempt(&db, first.id(), 1, TimeDelta::hours(1)).await?;
            assert!(drain(&db).await?.is_empty());

            record_failed_attempt(&db, first.id(), 2, TimeDelta::seconds(-1)).await?;
            let dequeued = db
                .with_write_transaction(async |txn| {
                    ChatMessageQueue::dequeue(txn, Uuid::new_v4()).await
                })
                .await?
                .expect("message is due again");
            assert_eq!(dequeued.message_id, first.id());
            assert_eq!(dequeued.attempts, 2);

            Ok(())
        }

        /// Failing the whole queue leaves a message that was never queued, and
        /// the attachment it is still waiting to download, untouched.
        #[sqlx::test]
        async fn fail_all_keeps_unrelated_pending_attachment(
            pool: SqlitePool,
        ) -> anyhow::Result<()> {
            let db = DbAccess::for_tests(pool);
            let (first, _) = queued_messages(&db).await?;

            let inbound = test_chat_message_with_salt(first.chat_id(), [3; 16]);
            inbound.store(db.write().await?).await?;
            let attachment = test_attachment_record(first.chat_id(), inbound.id());
            attachment.store(db.write().await?, None).await?;
            let remote_attachment_id = attachment.remote_attachment_id.expect("no remote id");
            test_pending_attachment_record(remote_attachment_id)
                .store(db.write().await?, attachment.attachment_id)
                .await?;

            db.with_write_transaction(async |txn| -> anyhow::Result<_> {
                ChatMessageQueue::remove_all_and_mark_as_failed(txn).await?;
                Ok(())
            })
            .await?;

            assert!(
                PendingAttachmentRecord::load_pending(db.read().await?, remote_attachment_id)
                    .await?
                    .is_some(),
                "the pending attachment must survive an unrelated queue failure"
            );
            assert_eq!(stored_status(&db, inbound.id()).await?, inbound.status());

            Ok(())
        }
    }
}
