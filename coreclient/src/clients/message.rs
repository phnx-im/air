// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::{OpenMlsRand, RustCrypto, identifiers::UserId, time::TimeStamp};
use anyhow::{Context, bail};
use mimi_content::{MessageStatus, MimiContent, NestedPartContent};
use sqlx::SqliteTransaction;

use crate::{
    Chat, ChatId, ChatMessage, ChatStatus, ContentMessage, MessageId,
    chats::{StatusRecord, messages::edit::MessageEdit},
    clients::{attachment::AttachmentRecord, block_contact::BlockedContactError},
    utils::connection_ext::StoreExt,
};

use super::{CoreUser, Group, StoreNotifier};

/// Create a `MimiContent` with `NullPart` that replaces the given message.
fn null_part_content(message: &ChatMessage) -> anyhow::Result<MimiContent> {
    let salt: [u8; 16] = RustCrypto::default().random_array()?;
    Ok(MimiContent {
        salt: mimi_content::ByteBuf::from(salt.to_vec()),
        replaces: message
            .message()
            .mimi_id()
            .map(|id| id.as_slice().to_vec().into()),
        topic_id: Default::default(),
        expires: None,
        in_reply_to: None,
        extensions: Default::default(),
        nested_part: mimi_content::NestedPart {
            disposition: mimi_content::Disposition::Render,
            language: String::new(),
            part: NestedPartContent::NullPart,
        },
    })
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
        let message = self
            .with_transaction(async |txn| {
                ChatMessage::load(txn.as_mut(), message_id)
                    .await?
                    .with_context(|| format!("Can't find message with id {message_id:?}"))
            })
            .await?;

        // Create NullPart content
        let null_content = null_part_content(&message)?;

        // Send the deletion message
        self.send_message(chat_id, null_content, Some(message_id))
            .await
    }

    /// Delete a message locally without sending a network message.
    ///
    /// This completely removes the message from the database, including edit history
    /// and status records. The message will no longer appear in the chat.
    pub(crate) async fn delete_message_locally(&self, message_id: MessageId) -> anyhow::Result<()> {
        self.with_transaction_and_notifier(async |txn, notifier| {
            let message = ChatMessage::load(txn.as_mut(), message_id)
                .await?
                .with_context(|| format!("Can't find message with id {message_id:?}"))?;

            let chat_id = message.chat_id();

            // Delete the message (edit history and status records are cascade-deleted)
            ChatMessage::delete(txn.as_mut(), notifier, message_id, chat_id).await?;

            Ok(())
        })
        .await
    }

    /// Delete message content locally without sending a network message.
    ///
    /// This replaces the message content with NullPart and deletes the edit history.
    /// The message remains visible as a "deleted" placeholder.
    pub(crate) async fn delete_message_content_locally(
        &self,
        message_id: MessageId,
    ) -> anyhow::Result<()> {
        self.with_transaction_and_notifier(async |txn, notifier| {
            let mut message = ChatMessage::load(txn.as_mut(), message_id)
                .await?
                .with_context(|| format!("Can't find message with id {message_id:?}"))?;

            let chat = Chat::load(txn.as_mut(), &message.chat_id())
                .await?
                .with_context(|| format!("Can't find chat with id {:?}", message.chat_id()))?;

            let original_sender = message
                .message()
                .sender()
                .context("Message does not have sender")?
                .clone();

            // Delete edit history
            MessageEdit::delete_by_message_id(txn.as_mut(), message_id).await?;

            // Create NullPart content for deletion
            let null_content = null_part_content(&message)?;

            // Update the message with NullPart content
            message.set_content_message(ContentMessage::new(
                original_sender,
                true, // is_sent
                null_content,
                chat.group_id(),
            ));
            message.set_status(MessageStatus::Deleted);
            message.set_edited_at(TimeStamp::now());
            message.update(txn.as_mut(), notifier).await?;

            // Clear the status records
            StatusRecord::clear(txn.as_mut(), notifier, message_id).await?;

            // Delete attachments
            AttachmentRecord::delete_by_message_id(txn.as_mut(), notifier, message_id).await?;

            Ok(())
        })
        .await
    }

    /// Send a message and return it.
    ///
    /// The message is stored, then sent to the DS and finally returned. The
    /// chat is marked as read until this message.
    pub(crate) async fn send_message(
        &self,
        chat_id: ChatId,
        content: MimiContent,
        replaces_id: Option<MessageId>,
    ) -> anyhow::Result<ChatMessage> {
        let needs_update = self
            .with_transaction(async |txn| {
                let chat = Chat::load(txn.as_mut(), &chat_id)
                    .await?
                    .with_context(|| format!("Can't find chat with id {chat_id}"))?;
                if let ChatStatus::Blocked = chat.status() {
                    bail!(BlockedContactError);
                }
                let group_id = chat.group_id;
                let group = Group::load_clean(txn, &group_id)
                    .await?
                    .with_context(|| format!("Can't find group with id {group_id:?}"))?;
                Ok(group.mls_group().has_pending_proposals())
            })
            .await?;

        if needs_update {
            // TODO race condition: Before or after this update, new proposals could arrive
            self.update_key(chat_id, None).await?;
        }

        let unsent_group_message = self
            .with_transaction_and_notifier(async |txn, notifier| {
                UnsentContent {
                    chat_id,
                    message_id: MessageId::random(),
                    content,
                }
                .store_unsent_message(txn, notifier, self.user_id(), replaces_id)
                .await?
                .store_group_update(txn, notifier, self.user_id())
                .await
            })
            .await?;

        self.outbound_service()
            .enqueue_chat_message(unsent_group_message.message.id(), None)
            .await?;

        Ok(unsent_group_message.message)
    }

    // TODO: This should be merged with send_message as soon as we don't
    // automatically send updates before attempting to enqueue a message.
    pub(crate) async fn send_message_transactional(
        &self,
        txn: &mut SqliteTransaction<'_>,
        notifier: &mut StoreNotifier,
        chat_id: ChatId,
        message_id: MessageId,
        content: MimiContent,
    ) -> anyhow::Result<ChatMessage> {
        let unsent_group_message = UnsentContent {
            chat_id,
            message_id,
            content,
        }
        .store_unsent_message(txn, notifier, self.user_id(), None)
        .await?
        .store_group_update(txn, notifier, self.user_id())
        .await?;

        Ok(unsent_group_message.message)
    }
}

struct UnsentContent {
    chat_id: ChatId,
    message_id: MessageId,
    content: MimiContent,
}

impl UnsentContent {
    async fn store_unsent_message(
        self,
        txn: &mut SqliteTransaction<'_>,
        notifier: &mut StoreNotifier,
        sender: &UserId,
        replaces_id: Option<MessageId>,
    ) -> anyhow::Result<UnsentMessage<GroupUpdateNeeded>> {
        let UnsentContent {
            chat_id,
            message_id,
            mut content,
        } = self;

        let chat = Chat::load(txn.as_mut(), &chat_id)
            .await?
            .with_context(|| format!("Can't find chat with id {chat_id}"))?;

        let is_deletion = content.nested_part.part == NestedPartContent::NullPart;

        let message = if let Some(replaces_id) = replaces_id {
            // Load the original message and the Mimi ID of the original message
            let mut original = ChatMessage::load(txn.as_mut(), replaces_id)
                .await?
                .with_context(|| format!("Can't find message with id {replaces_id:?}"))?;
            let original_mimi_content = original
                .message()
                .mimi_content()
                .context("Replaced message does not have mimi content")?;
            let original_mimi_id = original
                .message()
                .mimi_id()
                .context("Replaced message does not have mimi id")?;
            content.replaces = Some(original_mimi_id.as_slice().to_vec().into());
            let edit_created_at = TimeStamp::now();

            if !is_deletion {
                // Store the edit
                let edit = MessageEdit::new(
                    original_mimi_id,
                    original.id(),
                    edit_created_at,
                    original_mimi_content,
                );
                edit.store(txn.as_mut()).await?;
            }

            // Edit the original message and clear its status
            let is_sent = false;
            original.set_content_message(ContentMessage::new(
                sender.clone(),
                is_sent,
                content.clone(),
                chat.group_id(),
            ));
            if is_deletion {
                original.set_status(MessageStatus::Deleted);
            } else {
                original.set_status(MessageStatus::Unread);
            }
            original.set_edited_at(edit_created_at);
            original.update(txn.as_mut(), notifier).await?;
            StatusRecord::clear(txn.as_mut(), notifier, original.id()).await?;

            // Delete attachments for this message on network deletion
            // (FK cascade handles local deletion where the message row is deleted)
            if is_deletion {
                AttachmentRecord::delete_by_message_id(txn.as_mut(), notifier, original.id())
                    .await?;
            }

            original
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
            message.store(txn.as_mut(), notifier).await?;
            message
        };

        let group_id = chat.group_id();
        let group = Group::load_clean(txn, group_id)
            .await?
            .with_context(|| format!("Can't find group with id {group_id:?}"))?;

        Ok(UnsentMessage {
            chat,
            group,
            message,
            group_update: GroupUpdateNeeded,
        })
    }
}

/// Message type state: Group update needed before sending the message
struct GroupUpdateNeeded;
/// Message type state: Group already updated, message can be sent
struct GroupUpdated;

struct UnsentMessage<GroupUpdate> {
    chat: Chat,
    group: Group,
    message: ChatMessage,
    group_update: GroupUpdate,
}

impl UnsentMessage<GroupUpdateNeeded> {
    async fn store_group_update(
        self,
        txn: &mut SqliteTransaction<'_>,
        notifier: &mut StoreNotifier,
        own_user: &UserId,
    ) -> anyhow::Result<UnsentMessage<GroupUpdated>> {
        let Self {
            chat,
            group,
            message,
            group_update: GroupUpdateNeeded,
        } = self;

        // Immediately write the group back. No need to wait for the DS to
        // confirm as this is just an application message.
        group.store_update(txn.as_mut()).await?;

        // Also, mark the message (and all messages preceeding it) as read.
        Chat::mark_as_read_until_message_id(txn, notifier, chat.id(), message.id(), own_user)
            .await?;

        Ok(UnsentMessage {
            chat,
            group,
            message,
            group_update: GroupUpdated,
        })
    }
}

#[cfg(test)]
mod tests {
    use aircommon::identifiers::UserId;
    use airserver_test_harness::utils::setup::TestBackend;
    use mimi_content::MessageStatus;

    use crate::{
        ChatMessage,
        chats::{messages::persistence::tests::test_chat_message, persistence::tests::test_chat},
        clients::{
            CoreUser,
            attachment::{AttachmentRecord, persistence::test::test_attachment_record},
        },
        store::StoreNotifier,
    };

    #[tokio::test(flavor = "multi_thread")]
    async fn delete_message_content_locally_cleans_up_attachments() -> anyhow::Result<()> {
        let backend = TestBackend::single().await;
        let user_id = UserId::random(backend.domain().clone());
        let user =
            CoreUser::new_ephemeral(user_id, backend.server_url(), None, "DUMMY007".to_owned())
                .await?;

        let pool = user.pool();
        let mut notifier = StoreNotifier::noop();

        // Set up test data: chat -> message -> attachment
        let chat = test_chat();
        chat.store(pool.acquire().await?.as_mut(), &mut notifier)
            .await?;

        let message = test_chat_message(chat.id());
        message.store(pool, &mut notifier).await?;

        let attachment = test_attachment_record(chat.id(), message.id());
        attachment.store(pool, &mut notifier, None).await?;

        // Verify attachment exists before deletion
        let ids = AttachmentRecord::load_ids_by_message_id(pool, message.id()).await?;
        assert_eq!(ids.len(), 1);

        // Call the actual function
        user.delete_message_content_locally(message.id()).await?;

        // Verify attachment is gone
        let ids = AttachmentRecord::load_ids_by_message_id(pool, message.id()).await?;
        assert!(ids.is_empty());

        // Verify message still exists with Deleted status
        let loaded = ChatMessage::load(pool, message.id()).await?.unwrap();
        assert_eq!(loaded.status(), MessageStatus::Deleted);

        Ok(())
    }
}
