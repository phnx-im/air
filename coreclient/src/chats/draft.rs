// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::{codec::BlobDecoded, identifiers::MimiId};
use chrono::{DateTime, Utc};
use tokio_stream::StreamExt;

use crate::{
    MessageId,
    chats::messages::{InReplyToMessage, persistence::VersionedMessage},
};

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
    use aircommon::identifiers::{Fqdn, UserId};
    use sqlx::{SqliteExecutor, query, query_as, query_scalar};
    use uuid::Uuid;

    use crate::{ChatId, store::StoreNotifier};

    use super::*;

    #[derive(Debug)]
    struct SqlMessageDraft {
        /// The text currently composed in the draft.
        pub message: String,
        /// The id of the message we're replying to
        pub in_reply_to_mimi_id: Option<MimiId>,
        pub in_reply_to_message_id: Option<MessageId>,
        pub in_reply_to_sender_user_uuid: Option<Uuid>,
        pub in_reply_to_sender_user_domain: Option<Fqdn>,
        pub in_reply_to_content: Option<BlobDecoded<VersionedMessage>>,

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

    impl TryFrom<SqlMessageDraft> for MessageDraft {
        type Error = anyhow::Error;

        fn try_from(
            SqlMessageDraft {
                message,
                editing_id,
                updated_at,
                is_committed,
                in_reply_to_mimi_id,
                in_reply_to_message_id,
                in_reply_to_sender_user_uuid,
                in_reply_to_sender_user_domain,
                in_reply_to_content,
            }: SqlMessageDraft,
        ) -> Result<Self, Self::Error> {
            let in_reply_to = if let Some(message_id) = in_reply_to_message_id
                && let Some(sender_user_uuid) = in_reply_to_sender_user_uuid
                && let Some(sender_user_domain) = in_reply_to_sender_user_domain
            {
                Some(InReplyToMessage {
                    message_id,
                    sender: UserId::new(sender_user_uuid, sender_user_domain),
                    mimi_content: in_reply_to_content
                        .map(|BlobDecoded(v)| v.to_mimi_content())
                        .transpose()?,
                })
            } else {
                None
            };

            Ok(Self {
                message,
                in_reply_to: in_reply_to_mimi_id.map(|id| (id, in_reply_to)),
                editing_id,
                updated_at,
                is_committed,
            })
        }
    }

    impl MessageDraft {
        pub(crate) async fn load(
            executor: impl SqliteExecutor<'_>,
            chat_id: ChatId,
        ) -> sqlx::Result<Option<Self>> {
            query_as!(
                SqlMessageDraft,
                r#"
                    WITH reply_targets AS (
                        SELECT message_id, mimi_id, sender_user_uuid, sender_user_domain, content
                            FROM message
                        UNION ALL
                        SELECT m.message_id, me.mimi_id, m.sender_user_uuid, m.sender_user_domain, me.content
                            FROM message_edit me
                        LEFT JOIN message m ON m.message_id = me.message_id
                    )

                    SELECT
                        md.message,
                        md.editing_id AS "editing_id: _",
                        md.updated_at AS "updated_at: _",
                        md.is_committed,
                        md.in_reply_to_mimi_id AS "in_reply_to_mimi_id: _",
                        rt.message_id AS "in_reply_to_message_id: _",
                        rt.sender_user_uuid AS "in_reply_to_sender_user_uuid: _",
                        rt.sender_user_domain AS "in_reply_to_sender_user_domain: _",
                        rt.content AS "in_reply_to_content: _"
                    FROM message_draft AS md
                    LEFT JOIN reply_targets rt ON rt.mimi_id = in_reply_to_mimi_id
                    WHERE chat_id = ?
                "#,
                chat_id
            )
            .fetch_optional(executor)
            .await
            .map(|record: Option<SqlMessageDraft>| {
                record
                    .map(TryFrom::try_from)
                    .transpose()
                    .map_err(|e: anyhow::Error| sqlx::Error::Decode(e.into_boxed_dyn_error()))
            })?
        }

        pub(crate) async fn store(
            &self,
            executor: impl SqliteExecutor<'_>,
            notifier: &mut StoreNotifier,
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
            .execute(executor)
            .await?;
            if self.is_committed {
                notifier.update(chat_id);
            }
            Ok(())
        }

        pub(crate) async fn commit_all(
            executor: impl SqliteExecutor<'_>,
            notifier: &mut StoreNotifier,
        ) -> sqlx::Result<()> {
            let mut chat_ids = query_scalar!(
                r#"UPDATE message_draft SET is_committed = true
                RETURNING chat_id AS "chat_id: ChatId""#
            )
            .fetch(executor);
            while let Some(Ok(chat_id)) = chat_ids.next().await {
                notifier.update(chat_id);
            }
            Ok(())
        }

        pub(crate) async fn delete(
            executor: impl SqliteExecutor<'_>,
            notifier: &mut StoreNotifier,
            chat_id: ChatId,
        ) -> sqlx::Result<()> {
            query!("DELETE FROM message_draft WHERE chat_id = ?", chat_id)
                .execute(executor)
                .await?;
            notifier.update(chat_id);
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
            let loaded_draft = MessageDraft::load(&pool, chat.id()).await?;
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
            let loaded_draft = MessageDraft::load(&pool, chat.id()).await?;
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
            let loaded_draft = MessageDraft::load(&pool, chat.id()).await?;
            assert!(loaded_draft.is_some());
            let loaded_draft = loaded_draft.unwrap();
            assert_eq!(loaded_draft.message, "Updated message.");
            assert_eq!(loaded_draft.editing_id, None);
            assert_eq!(loaded_draft.updated_at, updated_now);

            // 6. Delete the draft
            MessageDraft::delete(&pool, &mut notifier, chat.id()).await?;

            // 7. Try to load it again (should be None)
            let loaded_draft_after_delete = MessageDraft::load(&pool, chat.id()).await?;
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
                MessageDraft::load(&pool, chat_a.id())
                    .await?
                    .unwrap()
                    .is_committed
            );
            assert!(
                MessageDraft::load(&pool, chat_b.id())
                    .await?
                    .unwrap()
                    .is_committed
            );

            Ok(())
        }
    }
}
