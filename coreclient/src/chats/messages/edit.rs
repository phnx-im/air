// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::{identifiers::MimiId, time::TimeStamp};
use mimi_content::MimiContent;

use crate::MessageId;

pub(crate) struct MessageEdit<'a> {
    mimi_id: &'a MimiId,
    message_id: MessageId,
    created_at: TimeStamp,
    mimi_content: &'a MimiContent,
}

impl<'a> MessageEdit<'a> {
    pub(crate) fn new(
        mimi_id: &'a MimiId,
        message_id: MessageId,
        created_at: TimeStamp,
        mimi_content: &'a MimiContent,
    ) -> Self {
        Self {
            mimi_id,
            message_id,
            created_at,
            mimi_content,
        }
    }
}

mod persistence {
    use aircommon::codec::BlobEncoded;
    use sqlx::{SqliteExecutor, query, query_scalar};

    use crate::chats::messages::persistence::VersionedMessage;

    use super::*;

    impl MessageEdit<'_> {
        pub(crate) async fn store(&self, executor: impl SqliteExecutor<'_>) -> anyhow::Result<()> {
            let versioned_message =
                BlobEncoded(VersionedMessage::from_mimi_content(self.mimi_content)?);
            query!(
                "INSERT INTO message_edit (
                    mimi_id,
                    message_id,
                    created_at,
                    content
                ) VALUES (?, ?, ?, ?)",
                self.mimi_id,
                self.message_id,
                self.created_at,
                versioned_message,
            )
            .execute(executor)
            .await?;
            Ok(())
        }

        pub(crate) async fn find_message_id(
            executor: impl SqliteExecutor<'_>,
            mimi_id: &MimiId,
        ) -> sqlx::Result<Option<MessageId>> {
            query_scalar!(
                r#"SELECT
                    message_id AS "message_id: _"
                FROM message_edit
                WHERE mimi_id = ?"#,
                mimi_id,
            )
            .fetch_optional(executor)
            .await
        }

        /// Delete all edit history for a message.
        pub(crate) async fn delete_by_message_id(
            executor: impl SqliteExecutor<'_>,
            message_id: MessageId,
        ) -> sqlx::Result<()> {
            query!("DELETE FROM message_edit WHERE message_id = ?", message_id,)
                .execute(executor)
                .await?;
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use aircommon::identifiers::MimiId;
    use mimi_content::MimiContent;
    use sqlx::SqlitePool;

    use crate::{
        MessageId,
        chats::{messages::persistence::tests::test_chat_message, persistence::tests::test_chat},
        store::StoreNotifier,
    };

    use super::*;

    #[sqlx::test]
    async fn delete_edit_history_by_message_id(pool: SqlitePool) -> anyhow::Result<()> {
        let mut store_notifier = StoreNotifier::noop();

        let chat = test_chat();
        chat.store(pool.acquire().await?.as_mut(), &mut store_notifier)
            .await?;

        let message = test_chat_message(chat.id());
        message.store(&pool, &mut store_notifier).await?;

        // Create multiple edit history entries
        let mimi_id_1 = MimiId::from_slice(&[1u8; 32])?;
        let mimi_id_2 = MimiId::from_slice(&[2u8; 32])?;
        let edit_content_1 =
            MimiContent::simple_markdown_message("First edit".to_string(), [1; 16]);
        let edit_content_2 =
            MimiContent::simple_markdown_message("Second edit".to_string(), [2; 16]);

        let edit_1 = MessageEdit::new(&mimi_id_1, message.id(), TimeStamp::now(), &edit_content_1);
        let edit_2 = MessageEdit::new(&mimi_id_2, message.id(), TimeStamp::now(), &edit_content_2);
        edit_1.store(&pool).await?;
        edit_2.store(&pool).await?;

        // Verify edit history exists
        assert_eq!(
            MessageEdit::find_message_id(&pool, &mimi_id_1).await?,
            Some(message.id())
        );
        assert_eq!(
            MessageEdit::find_message_id(&pool, &mimi_id_2).await?,
            Some(message.id())
        );

        // Delete edit history by message ID
        MessageEdit::delete_by_message_id(&pool, message.id()).await?;

        // Verify edit history is gone
        assert!(
            MessageEdit::find_message_id(&pool, &mimi_id_1)
                .await?
                .is_none()
        );
        assert!(
            MessageEdit::find_message_id(&pool, &mimi_id_2)
                .await?
                .is_none()
        );

        // Verify message still exists
        let loaded = crate::ChatMessage::load(&pool, message.id()).await?;
        assert!(loaded.is_some());

        Ok(())
    }

    #[sqlx::test]
    async fn delete_edit_history_nonexistent_message(pool: SqlitePool) -> anyhow::Result<()> {
        let mut store_notifier = StoreNotifier::noop();

        let chat = test_chat();
        chat.store(pool.acquire().await?.as_mut(), &mut store_notifier)
            .await?;

        // Try to delete edit history for a nonexistent message
        let fake_message_id = MessageId::random();
        let result = MessageEdit::delete_by_message_id(&pool, fake_message_id).await;

        // Should succeed without error (idempotent operation)
        assert!(result.is_ok());

        Ok(())
    }
}
