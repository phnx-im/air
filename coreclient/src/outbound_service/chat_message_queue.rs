// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::identifiers::AttachmentId;

use crate::{ChatId, MessageId};

pub(crate) struct ChatMessageQueue {
    chat_id: ChatId,
    message_id: MessageId,
    attachment_id: Option<AttachmentId>,
}

impl ChatMessageQueue {
    pub(crate) fn new(
        chat_id: ChatId,
        message_id: MessageId,
        attachment_id: Option<AttachmentId>,
    ) -> Self {
        Self {
            chat_id,
            message_id,
            attachment_id,
        }
    }
}

mod persistence {
    use aircommon::time::TimeStamp;
    use sqlx::{SqliteExecutor, SqliteTransaction, query, query_as, query_scalar};
    use tracing::debug;
    use uuid::Uuid;

    use crate::clients::attachment::persistence::PendingAttachmentRecord;

    use super::*;

    impl ChatMessageQueue {
        pub(crate) async fn enqueue(&self, executor: impl SqliteExecutor<'_>) -> sqlx::Result<()> {
            debug!(
                ?self.message_id, "Enqueueing chat message"
            );

            let now = TimeStamp::now();

            query!(
                "INSERT INTO chat_message_queue
                    (chat_id, message_id, attachment_id, created_at)
                VALUES (?1, ?2, ?3, ?4)
                ON CONFLICT DO NOTHING",
                self.chat_id,
                self.message_id,
                self.attachment_id,
                now,
            )
            .execute(executor)
            .await?;
            Ok(())
        }

        pub(crate) async fn dequeue(
            executor: impl SqliteExecutor<'_>,
            task_id: Uuid,
        ) -> anyhow::Result<Option<MessageId>> {
            let message_id = query_as!(
                MessageId,
                r#"
                UPDATE chat_message_queue
                SET locked_by = ?1
                WHERE message_id = (
                    SELECT message_id
                    FROM chat_message_queue
                    WHERE locked_by IS NULL OR locked_by != ?1
                    ORDER BY created_at ASC
                    LIMIT 1
                )
                RETURNING message_id AS "uuid: _"
                "#,
                task_id
            )
            .fetch_optional(executor)
            .await?;

            Ok(message_id)
        }

        pub(crate) async fn remove(
            txn: &mut SqliteTransaction<'_>,
            message_id: MessageId,
        ) -> sqlx::Result<()> {
            let attachment_id = query_scalar!(
                r#"DELETE FROM chat_message_queue 
                WHERE message_id = ?
                RETURNING attachment_id AS "uuid: _"
                "#,
                message_id
            )
            .fetch_one(txn.as_mut())
            .await?;

            if let Some(attachment_id) = attachment_id {
                PendingAttachmentRecord::delete(txn.as_mut(), attachment_id).await?;
            }

            Ok(())
        }
    }
}
