// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::{
    identifiers::{MimiId, UserId},
    time::TimeStamp,
};
use anyhow::{Context, Result, anyhow, bail};
use mimi_room_policy::VerifiedRoomState;
use tracing::error;

use crate::{
    ChatAttributes, ChatType, MessageDraft, MessageId,
    chats::{Chat, PendingConnectionInfo, messages::ChatMessage},
    groups::Group,
    job::{chat_operation::ChatOperation, create_chat::CreateChat},
    utils::image::resize_profile_image,
};

use super::{ChatId, CoreUser};

impl CoreUser {
    /// Create new chat.
    ///
    /// Returns the id of the newly created chat.
    pub async fn create_chat(
        &self,
        title: String,
        picture: Option<Vec<u8>>,
        is_apq: bool,
    ) -> Result<ChatId> {
        let resized_picture = match picture {
            Some(picture) => {
                Some(tokio::task::spawn_blocking(move || resize_profile_image(&picture)).await??)
            }
            None => None,
        };

        let chat_attributes = ChatAttributes::new(title, resized_picture);
        let client_reference = self.create_own_client_reference();

        let job = CreateChat::new(chat_attributes, client_reference, is_apq);
        let chat_id = self.execute_job(job).await?;

        Ok(chat_id)
    }

    /// Delete the chat with the given [`ChatId`].
    ///
    /// Since this function causes the creation of an MLS commit, it can cause
    /// more than one effect on the group. As a result this function returns a
    /// vector of [`ChatMessage`]s that represents the changes to the
    /// group. Note that these returned message have already been persisted.
    pub async fn delete_chat(&self, chat_id: ChatId) -> Result<Vec<ChatMessage>> {
        let job = ChatOperation::delete_chat(chat_id);
        Ok(self.execute_job(job).await?)
    }

    /// Returns the list of all chat ids in the order they should be displayed:
    ///
    /// 1. First return all chats having a draft ordered by the timestamp of the draft, descending.
    /// 2. Then return all chats ordered by the timestamp of the last message, descending.
    pub async fn ordered_chat_ids(&self) -> anyhow::Result<Vec<ChatId>> {
        Ok(Chat::load_ordered_ids(self.db().read().await?).await?)
    }

    /// Erases the chat data with the given [`ChatId`].
    ///
    /// Must not be called before the chat is deleted.
    pub async fn erase_chat(&self, chat_id: ChatId) -> Result<()> {
        self.db()
            .with_write_transaction(async |txn| {
                let chat = Chat::load(&mut *txn, &chat_id)
                    .await?
                    .context("missing chat for deletion")?;
                if let ChatType::PendingConnection(_) = chat.chat_type()
                    && let Some(info) = PendingConnectionInfo::load(&mut *txn, chat_id).await?
                    && let Some(hash) = info.connection_offer_hash
                {
                    Group::delete_connection_offer_psk(&mut *txn, hash)?;
                }
                Group::delete_from_db(txn, chat.group_id())
                    .await
                    .inspect_err(|error| {
                        error!(%error, "failed to delete group; skipping");
                    })
                    .ok();
                Chat::delete(&mut *txn, chat.id()).await?;
                Ok(())
            })
            .await
    }

    pub async fn leave_chat(&self, chat_id: ChatId) -> Result<()> {
        let job = ChatOperation::leave_chat(chat_id);
        self.execute_job(job).await?;
        Ok(())
    }

    pub async fn set_chat_picture(&self, chat_id: ChatId, picture: Option<Vec<u8>>) -> Result<()> {
        let chat = self
            .db()
            .with_read_transaction(async |txn| Chat::load(txn, &chat_id).await)
            .await?
            .ok_or_else(|| {
                let id = chat_id.uuid();
                anyhow!("Can't find chat with id {id}")
            })?;
        let ChatType::Group(attributes) = chat.chat_type else {
            bail!("Cannot set picture for non-group chat");
        };

        let resized_picture_option = tokio::task::spawn_blocking(|| {
            picture.and_then(|picture| resize_profile_image(&picture).ok())
        })
        .await?;
        if resized_picture_option == attributes.picture {
            // No change
            return Ok(());
        }
        let new_attributes = ChatAttributes::new(attributes.title, resized_picture_option);

        // Update the group and send out the update
        self.update_key_with_attributes(chat_id, Some(new_attributes))
            .await?;

        Ok(())
    }

    pub async fn set_chat_title(&self, chat_id: ChatId, title: String) -> Result<()> {
        let chat = self
            .db()
            .with_read_transaction(async |txn| Chat::load(txn, &chat_id).await)
            .await?
            .ok_or_else(|| {
                let id = chat_id.uuid();
                anyhow!("Can't find chat with id {id}")
            })?;
        let ChatType::Group(attributes) = chat.chat_type else {
            bail!("Cannot set title for non-group chat");
        };
        if title == attributes.title {
            // No change
            return Ok(());
        }
        let new_attributes = ChatAttributes::new(title, attributes.picture);

        // Update the group and send out the update
        self.update_key_with_attributes(chat_id, Some(new_attributes))
            .await?;

        Ok(())
    }

    /// Mark the chat with the given [`ChatId`] as read until the given message id (including).
    ///
    /// Returns whether the chat was marked as read and the message ids of the messages that were
    /// marked as read.
    pub async fn mark_chat_as_read(
        &self,
        chat_id: ChatId,
        until: MessageId,
    ) -> anyhow::Result<(bool, Vec<(MessageId, MimiId)>)> {
        self.db()
            .with_write_transaction(async |txn| {
                Chat::mark_as_read_until_message_id(txn, chat_id, until, self.user_id())
                    .await
                    .map_err(From::from)
            })
            .await
    }

    pub async fn message(&self, message_id: MessageId) -> anyhow::Result<Option<ChatMessage>> {
        ChatMessage::load(self.db().read().await?, message_id)
            .await
            .map_err(Into::into)
    }

    pub async fn prev_message(
        &self,
        chat_id: ChatId,
        message_id: MessageId,
    ) -> Result<Option<ChatMessage>> {
        ChatMessage::prev_message(self.db().read().await?, chat_id, message_id)
            .await
            .map_err(Into::into)
    }

    pub async fn next_message(
        &self,
        chat_id: ChatId,
        message_id: MessageId,
    ) -> Result<Option<ChatMessage>> {
        ChatMessage::next_message(self.db().read().await?, chat_id, message_id)
            .await
            .map_err(Into::into)
    }

    pub async fn first_unread_message(
        &self,
        chat_id: ChatId,
    ) -> anyhow::Result<Option<ChatMessage>> {
        self.db()
            .with_read_transaction(async |txn| {
                let chat = Chat::load(&mut *txn, &chat_id)
                    .await?
                    .with_context(|| format!("chat not found: {chat_id}"))?;
                Ok(ChatMessage::first_unread_message(txn, chat_id, chat.last_read.into()).await?)
            })
            .await
    }

    pub async fn last_message(&self, chat_id: ChatId) -> anyhow::Result<Option<ChatMessage>> {
        Ok(ChatMessage::last_message(self.db().read().await?, chat_id).await?)
    }

    pub async fn last_message_by_user(
        &self,
        chat_id: ChatId,
        user_id: &UserId,
    ) -> anyhow::Result<Option<ChatMessage>> {
        Ok(
            ChatMessage::last_content_message_by_user(self.db().read().await?, chat_id, user_id)
                .await?,
        )
    }

    pub async fn message_draft(&self, chat_id: ChatId) -> anyhow::Result<Option<MessageDraft>> {
        Ok(MessageDraft::load(self.db().read().await?, chat_id).await?)
    }

    pub async fn store_message_draft(
        &self,
        chat_id: ChatId,
        message_draft: Option<&MessageDraft>,
    ) -> anyhow::Result<()> {
        self.db()
            .with_write_transaction(async |txn| {
                if let Some(message_draft) = message_draft {
                    message_draft.store(txn, chat_id).await?;
                } else {
                    MessageDraft::delete(txn, chat_id).await?;
                }
                Ok(())
            })
            .await
    }

    pub async fn commit_all_message_drafts(&self) -> anyhow::Result<()> {
        self.db()
            .with_write_transaction(async |txn| Ok(MessageDraft::commit_all(txn).await?))
            .await
    }

    pub async fn messages_count(&self, chat_id: ChatId) -> anyhow::Result<usize> {
        Ok(self.try_messages_count(chat_id).await?)
    }

    pub async fn chat(&self, chat_id: &ChatId) -> Option<Chat> {
        self.db()
            .with_read_transaction(async |txn| Chat::load(txn, chat_id).await)
            .await
            .inspect_err(|error| {
                error!(%chat_id, %error, "Failed to load chat");
            })
            .ok()
            .flatten()
    }

    /// Get the most recent `number_of_messages` messages from the chat with the given [`ChatId`].
    pub async fn messages(
        &self,
        chat_id: ChatId,
        number_of_messages: usize,
    ) -> Result<Vec<ChatMessage>> {
        ChatMessage::load_multiple(self.db().read().await?, chat_id, number_of_messages as u32)
            .await
            .map_err(Into::into)
    }

    pub async fn messages_before(
        &self,
        chat_id: ChatId,
        before: TimeStamp,
        before_id: MessageId,
        limit: usize,
    ) -> anyhow::Result<(Vec<ChatMessage>, bool)> {
        Ok(ChatMessage::load_before(
            self.db().read().await?,
            chat_id,
            before,
            before_id,
            limit as u32,
        )
        .await?)
    }

    pub async fn messages_after(
        &self,
        chat_id: ChatId,
        after: TimeStamp,
        after_id: MessageId,
        limit: usize,
    ) -> anyhow::Result<(Vec<ChatMessage>, bool)> {
        Ok(ChatMessage::load_after(
            self.db().read().await?,
            chat_id,
            after,
            after_id,
            limit as u32,
        )
        .await?)
    }

    pub async fn messages_from(
        &self,
        chat_id: ChatId,
        from: TimeStamp,
        from_id: MessageId,
        limit: usize,
    ) -> anyhow::Result<(Vec<ChatMessage>, bool)> {
        Ok(ChatMessage::load_starting_from(
            self.db().read().await?,
            chat_id,
            from,
            from_id,
            limit as u32,
        )
        .await?)
    }

    pub async fn messages_around(
        &self,
        chat_id: ChatId,
        anchor: TimeStamp,
        anchor_id: MessageId,
        half_limit: usize,
    ) -> anyhow::Result<(Vec<ChatMessage>, bool, bool)> {
        Ok(ChatMessage::load_around(
            self.db().read().await?,
            chat_id,
            anchor,
            anchor_id,
            half_limit as u32,
        )
        .await?)
    }

    pub async fn load_room_state(&self, chat_id: &ChatId) -> Result<(UserId, VerifiedRoomState)> {
        if let Some(chat_id) = self.chat(chat_id).await
            && let Some(group) = Group::load(self.db().read().await?, chat_id.group_id()).await?
        {
            return Ok((self.user_id().clone(), group.room_state));
        }
        bail!("Room does not exist")
    }
}
