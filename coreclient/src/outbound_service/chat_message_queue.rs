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

        pub(crate) async fn dequeue(
            txn: &mut WriteDbTransaction<'_>,
            task_id: Uuid,
        ) -> anyhow::Result<Option<(ChatId, MessageId)>> {
            let Some(message_id) = query_scalar!(
                r#"
                SELECT message_id
                FROM chat_message_queue
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

            struct DequeuedMessage {
                message_id: Uuid,
                chat_id: Uuid,
            }
            let res = query_as!(
                DequeuedMessage,
                r#"
                UPDATE chat_message_queue
                SET locked_by = ?1
                WHERE message_id = ?2
                RETURNING message_id AS "message_id: _", chat_id AS "chat_id: _"
                "#,
                task_id,
                message_id
            )
            .fetch_optional(txn.as_mut())
            .await?;

            if let Some(DequeuedMessage {
                message_id,
                chat_id,
            }) = res
            {
                Ok(Some((ChatId::new(chat_id), MessageId::new(message_id))))
            } else {
                Ok(None)
            }
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
        /// - Delete all pending attachments associated with the queued messages
        /// - Notify about all marked messages
        pub(crate) async fn remove_all_and_and_mark_as_failed(
            txn: &mut WriteDbTransaction<'_>,
        ) -> sqlx::Result<()> {
            let failed_status: u8 = MessageStatus::Error.into();
            let marked_messages: Vec<MessageId> = query_scalar!(
                r#"UPDATE message
                SET status = ?1
                WHERE message_id IN (
                    SELECT message_id FROM chat_message_queue
                );
                DELETE FROM pending_attachment
                WHERE remote_attachment_id IN (
                    SELECT remote_attachment_id FROM chat_message_queue
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
}
