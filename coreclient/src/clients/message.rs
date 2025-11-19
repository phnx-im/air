// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::{identifiers::UserId, time::TimeStamp};
use anyhow::{Context, bail};
use mimi_content::{MessageStatus, MimiContent, NestedPartContent};
use sqlx::SqliteTransaction;

use crate::{
    Chat, ChatId, ChatMessage, ChatStatus, ContentMessage, MessageId,
    chats::{StatusRecord, messages::edit::MessageEdit},
    clients::block_contact::BlockedContactError,
    utils::connection_ext::StoreExt,
};

use super::{CoreUser, Group, StoreNotifier};

impl CoreUser {
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
