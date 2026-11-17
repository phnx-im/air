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
        chats::persistence::SqlChatStatus,
        db::access::{ReadConnection, WriteConnection},
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
        /// Loads the draft of an active chat that is not blocked.
        pub(crate) async fn load(
            mut connection: impl ReadConnection,
            chat_id: ChatId,
        ) -> sqlx::Result<Option<Self>> {
            let Some(mut message_draft) = query_as!(
                SqlMessageDraft,
                r#"
                    SELECT
                        d.message,
                        d.editing_id AS "editing_id: _",
                        d.updated_at AS "updated_at: _",
                        d.is_committed,
                        d.in_reply_to_mimi_id AS "in_reply_to_mimi_id: _"
                    FROM message_draft d
                    INNER JOIN chat c ON c.chat_id = d.chat_id
                    LEFT JOIN blocked_contact b ON b.user_uuid = c.connection_user_uuid
                        AND b.user_domain = c.connection_user_domain
                    WHERE d.chat_id = ?
                        AND c.status = ?
                        AND b.user_uuid IS NULL
                "#,
                chat_id,
                SqlChatStatus::Active,
            )
            .fetch_optional(connection.as_mut())
            .await?
            .map(MessageDraft::from) else {
                return Ok(None);
            };

            if let Some((mimi_id, message)) = message_draft.in_reply_to.as_mut() {
                *message = InReplyToMessage::load(&mut connection, mimi_id).await?;
            }

            Ok(Some(message_draft))
        }

        /// Stores the draft of an active chat that is not blocked.
        pub(crate) async fn store(
            &self,
            mut connection: impl WriteConnection,
            chat_id: ChatId,
        ) -> sqlx::Result<()> {
            let in_reply_to_mimi_id = self.in_reply_to.as_ref().map(|(mimi_id, _)| mimi_id);
            let result = query!(
                r#"INSERT INTO message_draft (
                    chat_id,
                    message,
                    editing_id,
                    in_reply_to_mimi_id,
                    updated_at,
                    is_committed
                )
                SELECT ?1, ?2, ?3, ?4, ?5, ?6
                WHERE EXISTS (
                    SELECT 1
                    FROM chat c
                    LEFT JOIN blocked_contact b ON b.user_uuid = c.connection_user_uuid
                        AND b.user_domain = c.connection_user_domain
                    WHERE c.chat_id = ?1
                        AND c.status = ?7
                        AND b.user_uuid IS NULL
                )
                ON CONFLICT(chat_id) DO UPDATE SET
                    message = excluded.message,
                    editing_id = excluded.editing_id,
                    in_reply_to_mimi_id = excluded.in_reply_to_mimi_id,
                    updated_at = excluded.updated_at,
                    is_committed = excluded.is_committed"#,
                chat_id,
                self.message,
                self.editing_id,
                in_reply_to_mimi_id,
                self.updated_at,
                self.is_committed,
                SqlChatStatus::Active,
            )
            .execute(connection.as_mut())
            .await?;
            if result.rows_affected() > 0 {
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

        /// Deletes the draft only when it still carries `updated_at`. A newer
        /// draft stored concurrently is left untouched.
        pub(crate) async fn delete_version(
            mut connection: impl WriteConnection,
            chat_id: ChatId,
            updated_at: DateTime<Utc>,
        ) -> sqlx::Result<()> {
            let result = query!(
                "DELETE FROM message_draft WHERE chat_id = ? AND updated_at = ?",
                chat_id,
                updated_at
            )
            .execute(connection.as_mut())
            .await?;
            if result.rows_affected() > 0 {
                connection.notifier().update(chat_id);
            }
            Ok(())
        }
    }

    #[cfg(test)]
    mod test {
        use aircommon::identifiers::UserId;
        use chrono::SubsecRound;
        use sqlx::SqlitePool;

        use crate::{
            Chat, ChatStatus, ChatType,
            chats::{
                messages::persistence::tests::test_chat_message, persistence::tests::test_chat,
            },
            clients::block_contact::BlockedContact,
            db::access::DbAccess,
        };

        use super::*;

        #[sqlx::test]
        async fn store_load_and_delete_message_draft(pool: SqlitePool) -> anyhow::Result<()> {
            let pool = DbAccess::for_tests(pool);

            let chat = test_chat();
            chat.store(pool.write().await?).await?;

            let message = test_chat_message(chat.id());
            message.store(pool.write().await?).await?;

            // 1. Load non-existent draft (should be None)
            let loaded_draft = MessageDraft::load(pool.read().await?, chat.id()).await?;
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
            draft.store(pool.write().await?, chat.id()).await?;

            // 3. Load the stored draft and assert its contents
            let loaded_draft = MessageDraft::load(pool.read().await?, chat.id()).await?;
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
            updated_draft.store(pool.write().await?, chat.id()).await?;

            // 5. Load the updated draft and assert its new contents
            let loaded_draft = MessageDraft::load(pool.read().await?, chat.id()).await?;
            assert!(loaded_draft.is_some());
            let loaded_draft = loaded_draft.unwrap();
            assert_eq!(loaded_draft.message, "Updated message.");
            assert_eq!(loaded_draft.editing_id, None);
            assert_eq!(loaded_draft.updated_at, updated_now);

            // 6. Delete the draft
            MessageDraft::delete(pool.write().await?, chat.id()).await?;

            // 7. Try to load it again (should be None)
            let loaded_draft_after_delete =
                MessageDraft::load(pool.read().await?, chat.id()).await?;
            assert_eq!(loaded_draft_after_delete, None);

            Ok(())
        }

        #[sqlx::test]
        async fn delete_message_draft_version(pool: SqlitePool) -> anyhow::Result<()> {
            let pool = DbAccess::for_tests(pool);

            let chat = test_chat();
            chat.store(pool.write().await?).await?;

            let updated_at = Utc::now().round_subsecs(6);
            let draft = MessageDraft {
                message: "A newer draft".to_string(),
                editing_id: None,
                in_reply_to: None,
                updated_at,
                is_committed: false,
            };
            draft.store(pool.write().await?, chat.id()).await?;

            // Deleting an outdated version leaves the newer draft in place.
            let outdated = updated_at - chrono::Duration::seconds(1);
            MessageDraft::delete_version(pool.write().await?, chat.id(), outdated).await?;
            let loaded = MessageDraft::load(pool.read().await?, chat.id()).await?;
            assert_eq!(loaded.as_ref().map(|d| d.updated_at), Some(updated_at));

            // Deleting the current version removes the draft.
            MessageDraft::delete_version(pool.write().await?, chat.id(), updated_at).await?;
            let loaded = MessageDraft::load(pool.read().await?, chat.id()).await?;
            assert_eq!(loaded, None);

            Ok(())
        }

        #[sqlx::test]
        async fn commit_all_drafts(pool: SqlitePool) -> anyhow::Result<()> {
            let pool = DbAccess::for_tests(pool);

            let chat_a = test_chat();
            chat_a.store(pool.write().await?).await?;

            let chat_b = test_chat();
            chat_b.store(pool.write().await?).await?;

            MessageDraft {
                message: "Hello, world!".to_string(),
                editing_id: None,
                in_reply_to: None,
                updated_at: Utc::now(),
                is_committed: false,
            }
            .store(pool.write().await?, chat_a.id())
            .await?;

            MessageDraft {
                message: "Hello, world!".to_string(),
                editing_id: None,
                in_reply_to: None,
                updated_at: Utc::now(),
                is_committed: true,
            }
            .store(pool.write().await?, chat_b.id())
            .await?;

            MessageDraft::commit_all(pool.write().await?).await?;

            assert!(
                MessageDraft::load(pool.read().await?, chat_a.id())
                    .await?
                    .unwrap()
                    .is_committed
            );
            assert!(
                MessageDraft::load(pool.read().await?, chat_b.id())
                    .await?
                    .unwrap()
                    .is_committed
            );

            Ok(())
        }

        #[sqlx::test]
        async fn hidden_draft_is_not_overwritten(pool: SqlitePool) -> anyhow::Result<()> {
            let pool = DbAccess::for_tests(pool);
            let mut connection = pool.write().await?;
            let updated_at = Utc::now().round_subsecs(6);

            let group = test_chat();
            group.store(&mut connection).await?;
            let message = test_chat_message(group.id());
            message.store(&mut connection).await?;
            let edit = MessageDraft {
                message: "Edited".to_string(),
                editing_id: Some(message.id()),
                in_reply_to: None,
                updated_at,
                is_committed: true,
            };
            edit.store(&mut connection, group.id()).await?;

            let blocked_user = UserId::random("localhost".parse().unwrap());
            let mut contact = test_chat();
            contact.chat_type = ChatType::Connection(blocked_user.clone());
            contact.store(&mut connection).await?;
            let message = test_chat_message(contact.id());
            message.store(&mut connection).await?;
            let reply_to = *message.message().mimi_id().unwrap();
            MessageDraft {
                message: "Reply".to_string(),
                editing_id: None,
                in_reply_to: Some((reply_to, None)),
                updated_at,
                is_committed: true,
            }
            .store(&mut connection, contact.id())
            .await?;

            let mut txn = connection.begin().await?;
            Chat::update_status(&mut txn, group.id(), &ChatStatus::inactive(Vec::new())).await?;
            txn.commit().await?;
            BlockedContact::new(blocked_user)
                .store(&mut connection)
                .await?;

            // The closing composer stores its text without the context.
            let plain = MessageDraft {
                message: "Plain".to_string(),
                ..MessageDraft::empty()
            };
            plain.store(&mut connection, group.id()).await?;
            plain.store(&mut connection, contact.id()).await?;

            let mut txn = connection.begin().await?;
            Chat::update_status(&mut txn, group.id(), &ChatStatus::Active).await?;
            txn.commit().await?;
            sqlx::query("DELETE FROM blocked_contact")
                .execute(connection.as_mut())
                .await?;

            let loaded = MessageDraft::load(&mut connection, group.id()).await?;
            assert_eq!(loaded, Some(edit));
            let loaded = MessageDraft::load(&mut connection, contact.id())
                .await?
                .map(|draft| (draft.message, draft.in_reply_to.map(|(id, _)| id)));
            assert_eq!(loaded, Some(("Reply".to_string(), Some(reply_to))));

            Ok(())
        }
    }
}
