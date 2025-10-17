// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use mimi_content::MessageStatus;

use crate::MessageId;

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
        ) -> anyhow::Result<Option<(ChatId, Vec<(MimiId, MessageStatus)>)>> {
            let mut txn = pool.begin_with("BEGIN IMMEDIATE").await?;

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
            .map(|record| {
                record.map(|record| (record.mimi_id, MessageStatus::from_repr(record.status)))
            })
            .collect::<Result<Vec<_>, _>>()
            .await?;

            txn.commit().await?;

            Ok(Some((chat_id, statuses)))
        }

        pub(crate) async fn remove(
            executor: impl SqliteExecutor<'_>,
            task_id: Uuid,
        ) -> sqlx::Result<()> {
            query!("DELETE FROM receipt_queue WHERE locked_by = ?", task_id)
                .execute(executor)
                .await?;
            Ok(())
        }
    }

    const LOCKED_THRESHOLD: Duration = Duration::from_secs(30);
}
