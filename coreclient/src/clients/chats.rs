// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::identifiers::UserId;
use anyhow::{Context, Result, anyhow, bail};
use mimi_room_policy::VerifiedRoomState;
use tracing::error;

use crate::{
    ChatAttributes, MessageId,
    chats::{Chat, messages::ChatMessage},
    groups::Group,
    job::{chat_operation::ChatOperation, create_chat::CreateChat},
    utils::{connection_ext::StoreExt, image::resize_profile_image},
};

use super::{ChatId, CoreUser};

impl CoreUser {
    /// Create new chat.
    ///
    /// Returns the id of the newly created chat.
    pub(crate) async fn create_chat(
        &self,
        title: String,
        picture: Option<Vec<u8>>,
    ) -> Result<ChatId> {
        let resized_picture = match picture {
            Some(picture) => {
                Some(tokio::task::spawn_blocking(move || resize_profile_image(&picture)).await??)
            }
            None => None,
        };

        let chat_attributes = ChatAttributes::new(title, resized_picture);
        let client_reference = self.create_own_client_reference();

        let job = CreateChat::new(chat_attributes, client_reference);
        let chat_id = self.execute_job(job).await?;

        Ok(chat_id)
    }

    /// Delete the chat with the given [`ChatId`].
    ///
    /// Since this function causes the creation of an MLS commit, it can cause
    /// more than one effect on the group. As a result this function returns a
    /// vector of [`ChatMessage`]s that represents the changes to the
    /// group. Note that these returned message have already been persisted.
    pub(crate) async fn delete_chat(&self, chat_id: ChatId) -> Result<Vec<ChatMessage>> {
        let job = ChatOperation::delete_chat(chat_id);
        self.execute_job(job).await
    }

    pub(crate) async fn erase_chat(&self, chat_id: ChatId) -> Result<()> {
        self.with_transaction_and_notifier(async |txn, notifier| {
            let chat = Chat::load(txn.as_mut(), &chat_id)
                .await?
                .context("missing chat for deletion")?;
            Group::delete_from_db(txn, chat.group_id())
                .await
                .inspect_err(|error| {
                    error!(%error, "failed to delete group; skipping");
                })
                .ok();
            Chat::delete(txn.as_mut(), notifier, chat.id()).await?;
            Ok(())
        })
        .await
    }

    pub(crate) async fn leave_chat(&self, chat_id: ChatId) -> Result<()> {
        let job = ChatOperation::leave_chat(chat_id);
        self.execute_job(job).await?;
        Ok(())
    }

    pub(crate) async fn set_chat_picture(
        &self,
        chat_id: ChatId,
        picture: Option<Vec<u8>>,
    ) -> Result<()> {
        let chat = Chat::load(self.pool().acquire().await?.as_mut(), &chat_id)
            .await?
            .ok_or_else(|| {
                let id = chat_id.uuid();
                anyhow!("Can't find chat with id {id}")
            })?;
        let resized_picture_option = tokio::task::spawn_blocking(|| {
            picture.and_then(|picture| resize_profile_image(&picture).ok())
        })
        .await?;
        if resized_picture_option == chat.attributes().picture {
            // No change
            return Ok(());
        }
        let new_attributes = ChatAttributes::new(chat.attributes.title, resized_picture_option);

        // Update the group and send out the update
        self.update_key(chat_id, Some(&new_attributes)).await?;

        Ok(())
    }

    pub(crate) async fn set_chat_title(&self, chat_id: ChatId, title: String) -> Result<()> {
        let chat = Chat::load(self.pool().acquire().await?.as_mut(), &chat_id)
            .await?
            .ok_or_else(|| {
                let id = chat_id.uuid();
                anyhow!("Can't find chat with id {id}")
            })?;
        if title == chat.attributes().title {
            // No change
            return Ok(());
        }
        let new_attributes = ChatAttributes::new(title, chat.attributes.picture);

        // Update the group and send out the update
        self.update_key(chat_id, Some(&new_attributes)).await?;

        Ok(())
    }

    pub(crate) async fn message(&self, message_id: MessageId) -> sqlx::Result<Option<ChatMessage>> {
        ChatMessage::load(self.pool(), message_id).await
    }

    pub(crate) async fn prev_message(
        &self,
        chat_id: ChatId,
        message_id: MessageId,
    ) -> Result<Option<ChatMessage>> {
        Ok(ChatMessage::prev_message(self.pool(), chat_id, message_id).await?)
    }

    pub(crate) async fn next_message(
        &self,
        chat_id: ChatId,
        message_id: MessageId,
    ) -> Result<Option<ChatMessage>> {
        Ok(ChatMessage::next_message(self.pool(), chat_id, message_id).await?)
    }

    pub async fn chat(&self, chat: &ChatId) -> Option<Chat> {
        Chat::load(self.pool().acquire().await.ok()?.as_mut(), chat)
            .await
            .ok()
            .flatten()
    }

    /// Get the most recent `number_of_messages` messages from the chat with the given [`ChatId`].
    pub(crate) async fn get_messages(
        &self,
        chat_id: ChatId,
        number_of_messages: usize,
    ) -> Result<Vec<ChatMessage>> {
        let messages =
            ChatMessage::load_multiple(self.pool(), chat_id, number_of_messages as u32).await?;
        Ok(messages)
    }

    pub async fn load_room_state(&self, chat_id: &ChatId) -> Result<(UserId, VerifiedRoomState)> {
        if let Some(chat_id) = self.chat(chat_id).await {
            let mut connection = self.pool().acquire().await?;
            if let Some(group) = Group::load(&mut connection, chat_id.group_id()).await? {
                return Ok((self.user_id().clone(), group.room_state));
            }
        }
        bail!("Room does not exist")
    }
}
