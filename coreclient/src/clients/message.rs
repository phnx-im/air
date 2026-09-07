// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::{identifiers::UserId, time::TimeStamp};
use anyhow::{Context, bail};
use mimi_content::{MessageStatus, MimiContent};

use crate::{
    Chat, ChatId, ChatMessage, ContentMessage, MessageId,
    chats::{
        StatusRecord,
        messages::edit::{MessageEdit, purge_deleted_message},
    },
    clients::block_contact::BlockedContactError,
    db::access::{WriteConnection, WriteDbTransaction},
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
    pub async fn delete_message(
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

    /// Delete a message locally without sending a network message.
    ///
    /// This completely removes the message from the database, including edit history
    /// and status records. The message will no longer appear in the chat.
    pub async fn delete_message_locally(&self, message_id: MessageId) -> anyhow::Result<()> {
        self.db()
            .with_write_transaction(async |txn| {
                let message = ChatMessage::load(&mut *txn, message_id)
                    .await?
                    .with_context(|| format!("Can't find message with id {message_id:?}"))?;

                // Find the IDs of all messages that are replies to the message we're deleting
                // and mark them as updated, to notify the UI.
                if let Some(replaces_mimi_id) = message.message().mimi_id() {
                    let message_ids_replied_to = ChatMessage::load_message_ids_in_reply_to_mimi_id(
                        &mut *txn,
                        replaces_mimi_id,
                    )
                    .await?;

                    for message_id in message_ids_replied_to {
                        txn.notifier().add(message_id);
                    }
                }

                // Delete the message (edit history and status records are cascade-deleted)
                ChatMessage::delete(txn, message_id).await?;

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
