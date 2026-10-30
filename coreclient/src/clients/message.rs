// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::{identifiers::UserId, time::TimeStamp};
use anyhow::{Context, bail, ensure};
use mimi_content::{MessageStatus, MimiContent};

use crate::{
    Chat, ChatId, ChatMessage, ContentMessage, MessageId,
    chats::{
        StatusRecord,
        messages::{
            edit::{MessageEdit, check_own_edit, purge_deleted_message},
            persistence::{self, DeletedMessageId},
        },
    },
    clients::block_contact::BlockedContactError,
    db::access::WriteDbTransaction,
};

use super::CoreUser;

/// Whether sending a message also marks the chat as read up to that message.
///
/// Regular sends from an open chat move the last-read marker. Sends that
/// happen without the user looking at the chat (e.g. from the share
/// extension) must leave it untouched.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarkChatAsRead {
    Yes,
    No,
}

impl CoreUser {
    /// Delete a message and send the deletion to other group members.
    ///
    /// This sends a NullPart message that replaces the original message,
    /// notifying all group members that the message has been deleted.
    /// The message remains visible as a "deleted" placeholder.
    pub async fn delete_message_for_everyone(
        &self,
        chat_id: ChatId,
        message_id: MessageId,
    ) -> anyhow::Result<ChatMessage> {
        // Load the message to get its mimi_id
        let message = ChatMessage::load(self.db().read().await?, message_id)
            .await?
            .with_context(|| format!("Can't find message with id {message_id:?}"))?;

        // Create NullPart content
        let null_content = message.null_part_content()?;

        // The placeholder store and the purge of what the deleted message
        // leaves behind run atomically inside the send transaction.
        Box::pin(self.send_message(chat_id, null_content, Some(message), MarkChatAsRead::Yes)).await
    }

    /// Delete a message on this device and the user's other devices, without
    /// telling the other group members.
    ///
    /// This completely removes the message from the database, including edit
    /// history and status records. The message will no longer appear in the
    /// chat. A system message without an event ID only exists on this device.
    ///
    /// Fails for a system message the user can't delete, see
    /// [`crate::SystemMessage::is_deletable`].
    pub async fn delete_message(&self, message_id: MessageId) -> anyhow::Result<()> {
        self.db()
            .with_write_transaction(async |txn| {
                let message = ChatMessage::load(&mut *txn, message_id)
                    .await?
                    .with_context(|| format!("Can't find message with id {message_id:?}"))?;
                ensure!(
                    message.is_deletable(&mut *txn).await?,
                    "Message {message_id:?} is a system message that can't be deleted"
                );

                let deleted_id = match message.message().mimi_id() {
                    Some(mimi_id) => Some(DeletedMessageId::Content(*mimi_id)),
                    None => ChatMessage::load_event_id(&mut *txn, message_id)
                        .await?
                        .map(DeletedMessageId::Event),
                };
                persistence::erase(txn, &message).await?;
                if let Some(deleted_id) = deleted_id {
                    self.outbound_service()
                        .enqueue_deleted_message_in_transaction(txn, deleted_id)
                        .await?;
                }

                Ok(())
            })
            .await
    }

    /// Store a message and return it.
    ///
    /// The message is stored as unsent and enqueued for the outbound service,
    /// which sends it to the DS. If `mark_as_read` is [`MarkChatAsRead::Yes`],
    /// the chat is marked as read until this message.
    pub async fn send_message(
        &self,
        chat_id: ChatId,
        content: MimiContent,
        replaces: Option<ChatMessage>,
        mark_as_read: MarkChatAsRead,
    ) -> anyhow::Result<ChatMessage> {
        Box::pin(
            self.db()
                .with_write_transaction(async |txn| -> anyhow::Result<_> {
                    if Chat::is_blocked(&mut *txn, chat_id).await? {
                        bail!(BlockedContactError);
                    }

                    let message = UnsentContent {
                        chat_id,
                        message_id: MessageId::random(),
                        content,
                    }
                    .store_unsent_message(&mut *txn, self.user_id(), replaces)
                    .await?;
                    mark_as_read_until_message(
                        &mut *txn,
                        chat_id,
                        &message,
                        self.user_id(),
                        mark_as_read,
                    )
                    .await?;

                    self.outbound_service()
                        .enqueue_chat_message_in_transaction(txn, message.id())
                        .await?;

                    Ok(message)
                }),
        )
        .await
    }

    /// Stores a message inside an existing transaction, without enqueuing it.
    ///
    /// The content is not final yet (no MIMI ID).
    pub(crate) async fn store_provisional_message(
        &self,
        txn: &mut WriteDbTransaction<'_>,
        chat_id: ChatId,
        message_id: MessageId,
        content: MimiContent,
        mark_as_read: MarkChatAsRead,
    ) -> anyhow::Result<ChatMessage> {
        let message = ChatMessage::new_provisional_message(
            self.user_id().clone(),
            chat_id,
            message_id,
            content,
        );
        message.store(&mut *txn).await?;
        mark_as_read_until_message(txn, chat_id, &message, self.user_id(), mark_as_read).await?;

        Ok(message)
    }
}

/// Marks the chat as read until `message`, unless it is a deletion or the
/// send must leave the last-read marker untouched.
async fn mark_as_read_until_message(
    txn: &mut WriteDbTransaction<'_>,
    chat_id: ChatId,
    message: &ChatMessage,
    own_user: &UserId,
    mark_as_read: MarkChatAsRead,
) -> anyhow::Result<()> {
    if mark_as_read == MarkChatAsRead::No || message.status() == MessageStatus::Deleted {
        return Ok(());
    }
    Chat::mark_as_read_until_message_id(txn, chat_id, message.id(), own_user).await?;
    Ok(())
}

struct UnsentContent {
    chat_id: ChatId,
    message_id: MessageId,
    content: MimiContent,
}

impl UnsentContent {
    async fn store_unsent_message(
        self,
        txn: &mut WriteDbTransaction<'_>,
        sender: &UserId,
        replaces: Option<ChatMessage>,
    ) -> anyhow::Result<ChatMessage> {
        let UnsentContent {
            chat_id,
            message_id,
            mut content,
        } = self;

        let chat = Chat::load(&mut *txn, &chat_id)
            .await?
            .with_context(|| format!("Can't find chat with id {chat_id}"))?;

        let is_deletion = content.nested_part.is_null_part();

        let message = if let Some(replaces) = replaces {
            let original_mimi_content = replaces
                .message()
                .mimi_content()
                .context("Replaced message does not have mimi content")?;
            let original_mimi_id = replaces
                .message()
                .mimi_id()
                .context("Replaced message does not have mimi id")?;
            content.replaces = Some(original_mimi_id.as_slice().to_vec());
            let edit_created_at = TimeStamp::now();

            if !is_deletion {
                check_own_edit(&mut *txn, &replaces, sender, edit_created_at).await?;

                // Store the edit
                let edit = MessageEdit::new(
                    original_mimi_id,
                    replaces.id(),
                    edit_created_at,
                    original_mimi_content,
                );
                edit.store(&mut *txn).await?;
            }

            // Edit the original message and clear its status
            let is_sent = false;
            let mut updated = replaces.clone();
            updated.set_content_message(ContentMessage::new(
                sender.clone(),
                is_sent,
                content.clone(),
                chat.group_id(),
            ));
            if is_deletion {
                updated.set_status(MessageStatus::Deleted);
            } else {
                updated.set_status(MessageStatus::Unread);
                updated.set_edited_at(edit_created_at);
            }
            updated.update(&mut *txn).await?;
            StatusRecord::clear(&mut *txn, updated.id()).await?;

            if is_deletion {
                // The message is now a placeholder, purge what it left behind
                // and repoint replies at the placeholder's Mimi ID.
                purge_deleted_message(
                    txn,
                    updated.id(),
                    Some(original_mimi_id),
                    updated.message().mimi_id(),
                )
                .await?;
                updated.take_in_reply_to();
            }

            updated
        } else {
            // Store the message as unsent so that we don't lose it in case
            // something goes wrong.
            let message = ChatMessage::new_unsent_message(
                sender.clone(),
                chat_id,
                message_id,
                content.clone(),
                chat.group_id(),
            );
            message.store(&mut *txn).await?;
            message
        };

        Ok(message)
    }
}

#[cfg(test)]
mod tests {
    use aircommon::identifiers::MimiId;
    use chrono::{TimeDelta, Utc};
    use sqlx::SqlitePool;

    use crate::{
        EDIT_WINDOW, MAX_EDITS, MessageEditError,
        chats::{
            messages::persistence::tests::store_test_chat_message, persistence::tests::test_chat,
        },
        db::access::{DbAccess, WriteConnection},
    };

    use super::*;

    async fn replace(
        pool: &DbAccess,
        sender: &UserId,
        original: &ChatMessage,
        content: MimiContent,
    ) -> anyhow::Result<ChatMessage> {
        let mut connection = pool.write().await?;
        let mut txn = connection.begin().await?;
        let message = UnsentContent {
            chat_id: original.chat_id(),
            message_id: MessageId::random(),
            content,
        }
        .store_unsent_message(&mut txn, sender, Some(original.clone()))
        .await?;
        txn.commit().await?;
        Ok(message)
    }

    fn edit() -> MimiContent {
        MimiContent::simple_markdown_message("edited".to_owned(), [1; 16])
    }

    fn edit_error(error: &anyhow::Error) -> Option<MessageEditError> {
        error.downcast_ref::<MessageEditError>().copied()
    }

    #[sqlx::test]
    async fn own_edit_beyond_limit_is_refused(pool: SqlitePool) -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(pool);
        let chat = test_chat();
        chat.store(pool.write().await?).await?;
        let alice = UserId::random("localhost".parse().unwrap());
        let message = store_test_chat_message(&pool, &chat, &alice, true, TimeStamp::now()).await?;

        let content = message.message().mimi_content().unwrap().clone();
        for edit in 0..MAX_EDITS {
            let mimi_id = MimiId::from_slice(&[u8::try_from(edit)?; 32])?;
            MessageEdit::new(&mimi_id, message.id(), TimeStamp::now(), &content)
                .store(pool.write().await?)
                .await?;
        }

        let error = replace(&pool, &alice, &message, edit()).await.unwrap_err();
        assert_eq!(edit_error(&error), Some(MessageEditError::LimitReached));

        Ok(())
    }

    #[sqlx::test]
    async fn own_edit_after_window_is_refused(pool: SqlitePool) -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(pool);
        let chat = test_chat();
        chat.store(pool.write().await?).await?;
        let alice = UserId::random("localhost".parse().unwrap());
        let sent_at = TimeStamp::from(Utc::now() - EDIT_WINDOW - TimeDelta::seconds(1));
        let message = store_test_chat_message(&pool, &chat, &alice, true, sent_at).await?;

        let error = replace(&pool, &alice, &message, edit()).await.unwrap_err();
        assert_eq!(edit_error(&error), Some(MessageEditError::WindowPassed));

        // Deleting is not bound to the window.
        let deleted = replace(&pool, &alice, &message, message.null_part_content()?).await?;
        assert!(deleted.message().is_deleted());

        Ok(())
    }

    #[sqlx::test]
    async fn own_edit_of_unsent_message_is_refused(pool: SqlitePool) -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(pool);
        let chat = test_chat();
        chat.store(pool.write().await?).await?;
        let alice = UserId::random("localhost".parse().unwrap());
        let message =
            store_test_chat_message(&pool, &chat, &alice, false, TimeStamp::now()).await?;

        assert!(replace(&pool, &alice, &message, edit()).await.is_err());

        Ok(())
    }

    #[sqlx::test]
    async fn edit_of_other_users_message_is_refused(pool: SqlitePool) -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(pool);
        let chat = test_chat();
        chat.store(pool.write().await?).await?;
        let alice = UserId::random("localhost".parse().unwrap());
        let bob = UserId::random("localhost".parse().unwrap());
        let message = store_test_chat_message(&pool, &chat, &bob, true, TimeStamp::now()).await?;

        assert!(replace(&pool, &alice, &message, edit()).await.is_err());

        Ok(())
    }

    #[sqlx::test]
    async fn own_edit_within_limits_is_stored(pool: SqlitePool) -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(pool);
        let chat = test_chat();
        chat.store(pool.write().await?).await?;
        let alice = UserId::random("localhost".parse().unwrap());
        let sent_at = TimeStamp::from(Utc::now() - EDIT_WINDOW + TimeDelta::minutes(1));
        let message = store_test_chat_message(&pool, &chat, &alice, true, sent_at).await?;

        replace(&pool, &alice, &message, edit()).await?;

        let loaded = ChatMessage::load(pool.read().await?, message.id())
            .await?
            .unwrap();
        assert_eq!(
            loaded.message().mimi_content(),
            Some(&edit_with_replaces(&message))
        );
        assert_eq!(loaded.edits_left(), MAX_EDITS - 1);

        Ok(())
    }

    fn edit_with_replaces(original: &ChatMessage) -> MimiContent {
        let mut content = edit();
        content.replaces = original
            .message()
            .mimi_id()
            .map(|mimi_id| mimi_id.as_slice().to_vec());
        content
    }
}
