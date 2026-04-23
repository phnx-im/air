// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::identifiers::MimiId;
use chrono::{DateTime, Utc};
use tokio_stream::StreamExt;

use crate::{MessageId, chats::messages::InReplyToMessage};

/// A message draft which is currently composed in a chat.
///
/// Allows to persists drafts between opening and closing the chat and between sessions of
/// the app.
#[derive(Debug, Clone, PartialEq)]
pub struct MessageDraft {
    /// The text currently composed in the draft.
    pub message: String,
    /// Whether we're replying to an existing message, the content is either loaded or not.
    pub in_reply_to: Option<(MimiId, Option<InReplyToMessage>)>,
    /// The id of the message currently being edited, if any.
    pub editing_id: Option<MessageId>,
    /// The time when the draft was last updated.
    pub updated_at: DateTime<Utc>,
    /// When a draft is committed, it is loaded as part of the chat details data.
    ///
    /// Used for updating the draft during the edit process without immediately updating the chat
    /// details.
    pub is_committed: bool,
}

impl MessageDraft {
    pub fn empty() -> Self {
        Self {
            message: String::new(),
            in_reply_to: None,
            editing_id: None,
            updated_at: Utc::now(),
            is_committed: false,
        }
    }
}

mod persistence {
    use sqlx::{query, query_as, query_scalar};

    use crate::{
        ChatId,
        db_access::{ReadConnection, WriteConnection},
    };

    use super::*;

    #[derive(Debug)]
    struct SqlMessageDraft {
        /// The text currently composed in the draft.
        pub message: String,
        /// The id of the message we're replying to
        pub in_reply_to_mimi_id: Option<MimiId>,
        /// The data of the message we're replying to
        // pub in_reply_to_content: Option<BlobDecoded<VersionedMessage>>,
        /// The id of the message currently being edited, if any.
        pub editing_id: Option<MessageId>,
        /// The time when the draft was last updated.
        pub updated_at: DateTime<Utc>,
        /// When a draft is committed, it is loaded as part of the chat details data.
        ///
        /// Used for updating the draft during the edit process without immediately updating the chat
        /// details.
        pub is_committed: bool,
    }

    impl From<SqlMessageDraft> for MessageDraft {
        fn from(
            SqlMessageDraft {
                message,
                editing_id,
                updated_at,
                is_committed,
                in_reply_to_mimi_id,
            }: SqlMessageDraft,
        ) -> Self {
            Self {
                message,
                in_reply_to: in_reply_to_mimi_id.map(|id| (id, None)),
                editing_id,
                updated_at,
                is_committed,
            }
        }
    }

    impl MessageDraft {
        pub(crate) async fn load(
            mut connection: impl ReadConnection,
            chat_id: ChatId,
        ) -> sqlx::Result<Option<Self>> {
            let Some(mut message_draft) = query_as!(
                SqlMessageDraft,
                r#"
                    SELECT
                        message,
                        editing_id AS "editing_id: _",
                        updated_at AS "updated_at: _",
                        is_committed,
                        in_reply_to_mimi_id AS "in_reply_to_mimi_id: _"
                    FROM message_draft
                    WHERE chat_id = ?
                "#,
                chat_id
            )
            .fetch_optional(connection.as_mut())
            .await?
            .map(MessageDraft::from) else {
                return Ok(None);
            };

            if let Some((mimi_id, message)) = message_draft.in_reply_to.as_mut() {
                *message = InReplyToMessage::load(connection.as_mut(), mimi_id).await?;
            }

            Ok(Some(message_draft))
        }

        pub(crate) async fn store(
            &self,
            mut connection: impl WriteConnection,
            chat_id: ChatId,
        ) -> sqlx::Result<()> {
            let in_reply_to_mimi_id = self.in_reply_to.as_ref().map(|(mimi_id, _)| mimi_id);
            query!(
                "INSERT INTO message_draft (
                    chat_id,
                    message,
                    editing_id,
                    in_reply_to_mimi_id,
                    updated_at,
                    is_committed
                )
                VALUES (?, ?, ?, ?, ?, ?)
                ON CONFLICT(chat_id) DO UPDATE SET
                    message = excluded.message,
                    editing_id = excluded.editing_id,
                    in_reply_to_mimi_id = excluded.in_reply_to_mimi_id,
                    updated_at = excluded.updated_at,
                    is_committed = excluded.is_committed",
                chat_id,
                self.message,
                self.editing_id,
                in_reply_to_mimi_id,
                self.updated_at,
                self.is_committed,
            )
            .execute(connection.as_mut())
            .await?;
            if self.is_committed {
                connection.notifier().update(chat_id);
            }
            Ok(())
        }

        pub(crate) async fn commit_all(mut connection: impl WriteConnection) -> sqlx::Result<()> {
            let (connection, notifier) = connection.split();
            let mut chat_ids = query_scalar!(
                r#"UPDATE message_draft SET is_committed = true
                RETURNING chat_id AS "chat_id: ChatId""#
            )
            .fetch(connection);
            while let Some(Ok(chat_id)) = chat_ids.next().await {
                notifier.update(chat_id);
            }
            Ok(())
        }

        pub(crate) async fn delete(
            mut connection: impl WriteConnection,
            chat_id: ChatId,
        ) -> sqlx::Result<()> {
            query!("DELETE FROM message_draft WHERE chat_id = ?", chat_id)
                .execute(connection.as_mut())
                .await?;
            connection.notifier().update(chat_id);
            Ok(())
        }
    }

    #[cfg(test)]
    mod test {
        use chrono::SubsecRound;
        use sqlx::SqlitePool;

        use crate::{
            chats::{
                messages::persistence::tests::test_chat_message, persistence::tests::test_chat,
            },
            store::StoreNotifier,
        };

        use super::*;

        #[sqlx::test]
        async fn store_load_and_delete_message_draft(pool: SqlitePool) -> anyhow::Result<()> {
            let mut notifier = StoreNotifier::noop();

            let chat = test_chat();
            chat.store(pool.acquire().await?.as_mut(), &mut notifier)
                .await?;

            let message = test_chat_message(chat.id());
            message.store(&pool, &mut notifier).await?;

            // 1. Load non-existent draft (should be None)
            let loaded_draft =
                MessageDraft::load(pool.acquire().await?.as_mut(), chat.id()).await?;
            assert_eq!(loaded_draft, None);

            // 2. Store a new draft
            let now = Utc::now().round_subsecs(6); // Round to avoid precision issues with SQLite TEXT storage
            let draft = MessageDraft {
                message: "Hello, world!".to_string(),
                editing_id: Some(message.id()),
                in_reply_to: None,
                updated_at: now,
                is_committed: false,
            };
            draft.store(&pool, &mut notifier, chat.id()).await?;

            // 3. Load the stored draft and assert its contents
            let loaded_draft =
                MessageDraft::load(pool.acquire().await?.as_mut(), chat.id()).await?;
            assert!(loaded_draft.is_some());
            let loaded_draft = loaded_draft.unwrap();
            assert_eq!(loaded_draft.message, "Hello, world!".to_string());
            assert_eq!(loaded_draft.editing_id, draft.editing_id);
            assert_eq!(loaded_draft.updated_at, now);

            // 4. Update the draft and store again (INSERT OR REPLACE)
            let updated_now = Utc::now().round_subsecs(6);
            let updated_draft = MessageDraft {
                message: "Updated message.".to_string(),
                editing_id: None, // No longer editing
                in_reply_to: None,
                updated_at: updated_now,
                is_committed: false,
            };
            updated_draft.store(&pool, &mut notifier, chat.id()).await?;

            // 5. Load the updated draft and assert its new contents
            let loaded_draft =
                MessageDraft::load(pool.acquire().await?.as_mut(), chat.id()).await?;
            assert!(loaded_draft.is_some());
            let loaded_draft = loaded_draft.unwrap();
            assert_eq!(loaded_draft.message, "Updated message.");
            assert_eq!(loaded_draft.editing_id, None);
            assert_eq!(loaded_draft.updated_at, updated_now);

            // 6. Delete the draft
            MessageDraft::delete(&pool, &mut notifier, chat.id()).await?;

            // 7. Try to load it again (should be None)
            let loaded_draft_after_delete =
                MessageDraft::load(pool.acquire().await?.as_mut(), chat.id()).await?;
            assert_eq!(loaded_draft_after_delete, None);

            Ok(())
        }

        #[sqlx::test]
        async fn commit_all_drafts(pool: SqlitePool) -> anyhow::Result<()> {
            let mut notifier = StoreNotifier::noop();

            let chat_a = test_chat();
            chat_a
                .store(pool.acquire().await?.as_mut(), &mut notifier)
                .await?;

            let chat_b = test_chat();
            chat_b
                .store(pool.acquire().await?.as_mut(), &mut notifier)
                .await?;

            MessageDraft {
                message: "Hello, world!".to_string(),
                editing_id: None,
                in_reply_to: None,
                updated_at: Utc::now(),
                is_committed: false,
            }
            .store(&pool, &mut notifier, chat_a.id())
            .await?;

            MessageDraft {
                message: "Hello, world!".to_string(),
                editing_id: None,
                in_reply_to: None,
                updated_at: Utc::now(),
                is_committed: true,
            }
            .store(&pool, &mut notifier, chat_b.id())
            .await?;

            MessageDraft::commit_all(&pool, &mut notifier).await?;

            assert!(
                MessageDraft::load(pool.acquire().await?.as_mut(), chat_a.id())
                    .await?
                    .unwrap()
                    .is_committed
            );
            assert!(
                MessageDraft::load(pool.acquire().await?.as_mut(), chat_b.id())
                    .await?
                    .unwrap()
                    .is_committed
            );

            Ok(())
        }
    }
}
