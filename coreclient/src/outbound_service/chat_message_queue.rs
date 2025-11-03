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
    use sqlx::{SqliteExecutor, SqlitePool, query, query_as, query_scalar};
    use tracing::debug;
    use uuid::Uuid;

    use super::*;

    impl ChatMessageQueue {
        pub(crate) async fn enqueue(&self, executor: impl SqliteExecutor<'_>) -> sqlx::Result<()> {
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
            .execute(executor)
            .await?;
            Ok(())
        }

        pub(crate) async fn dequeue(
            pool: &SqlitePool,
            task_id: Uuid,
        ) -> anyhow::Result<Option<(ChatId, MessageId)>> {
            let mut txn = pool.begin_with("BEGIN IMMEDIATE").await?;

            let chat_id = query_scalar!(
                r#"SELECT chat_id AS "chat_id: _"
                    FROM chat_message_queue
                    ORDER BY created_at ASC
                    LIMIT 1
                "#,
            )
            .fetch_optional(txn.as_mut())
            .await?;
            let Some(chat_id) = chat_id else {
                return Ok(None);
            };

            let message_ids = query_as!(
                MessageId,
                r#"UPDATE chat_message_queue
                    SET locked_by = ?1
                    WHERE chat_id = ?2 
                RETURNING
                    message_id AS "uuid: _"
                "#,
                task_id,
                chat_id,
            )
            .fetch_one(txn.as_mut())
            .await?;

            txn.commit().await?;

            Ok(Some((chat_id, message_ids)))
        }

        pub(crate) async fn remove(
            executor: impl SqliteExecutor<'_>,
            message_id: MessageId,
        ) -> sqlx::Result<()> {
            query!(
                "DELETE FROM chat_message_queue WHERE message_id = ?",
                message_id
            )
            .execute(executor)
            .await?;
            Ok(())
        }
    }
}
