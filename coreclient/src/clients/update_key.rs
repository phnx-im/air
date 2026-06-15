// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::{identifiers::UserId, time::TimeStamp};

use crate::{
    Chat, ChatAttributes, ChatId, ChatMessage, ChatType, SystemMessage,
    chats::messages::TimestampedMessage,
    db::access::{WriteConnection, WriteDbTransaction},
    job::chat_operation::ChatOperation,
};

use super::CoreUser;

impl CoreUser {
    /// Update the user's key material in the chat with the given
    /// [`ChatId`].
    ///
    /// Since this function causes the creation of an MLS commit, it can cause
    /// more than one effect on the group. As a result this function returns a
    /// vector of [`ChatMessage`]s that represents the changes to the
    /// group. Note that these returned message have already been persisted.
    pub async fn update_key(&self, chat_id: ChatId) -> anyhow::Result<Vec<ChatMessage>> {
        self.update_key_with_attributes(chat_id, None).await
    }

    /// Same as [`Self::update_key`], but also updates the PQ key material.
    pub async fn update_apq_key(&self, chat_id: ChatId) -> anyhow::Result<Vec<ChatMessage>> {
        let job = ChatOperation::apq_update(chat_id);
        Ok(self.execute_job(job).await?)
    }

    pub(crate) async fn update_key_with_attributes(
        &self,
        chat_id: ChatId,
        new_chat_attributes: Option<ChatAttributes>,
    ) -> anyhow::Result<Vec<ChatMessage>> {
        let job = ChatOperation::update(chat_id, new_chat_attributes);
        Ok(self.execute_job(job).await?)
    }
}

pub(crate) async fn update_chat_attributes(
    txn: &mut WriteDbTransaction<'_>,
    chat: &mut Chat,
    sender_id: &UserId,
    new_chat_attributes: ChatAttributes,
    ds_timestamp: TimeStamp,
    message_buffer: &mut Vec<TimestampedMessage>,
) -> anyhow::Result<()> {
    update_chat_title(
        &mut *txn,
        chat,
        sender_id,
        new_chat_attributes.title,
        ds_timestamp,
        message_buffer,
    )
    .await?;
    match &chat.chat_type {
        ChatType::Group(attrs) => {
            if attrs.picture != new_chat_attributes.picture {
                chat.set_picture(&mut *txn, new_chat_attributes.picture)
                    .await?;
                let system_message = SystemMessage::ChangePicture(sender_id.clone());
                let group_message =
                    TimestampedMessage::system_message(system_message, ds_timestamp);
                message_buffer.push(group_message);
            }
        }
        ChatType::HandleConnection(_)
        | ChatType::Connection(_)
        | ChatType::TargetedMessageConnection(_)
        | ChatType::PendingConnection(_) => {
            erase_connection_chat_picture(&mut *txn, chat.id, new_chat_attributes.picture).await?;
        }
    }

    Ok(())
}

async fn erase_connection_chat_picture(
    connection: impl WriteConnection,
    chat_id: ChatId,
    new_picture: Option<Vec<u8>>,
) -> anyhow::Result<()> {
    if new_picture.is_none() {
        Chat::update_picture(connection, chat_id, None).await?;
    }
    Ok(())
}

pub(crate) async fn update_chat_title(
    connection: impl WriteConnection,
    chat: &mut Chat,
    sender_id: &UserId,
    new_title: String,
    ds_timestamp: TimeStamp,
    message_buffer: &mut Vec<TimestampedMessage>,
) -> anyhow::Result<()> {
    match &chat.chat_type {
        ChatType::Group(attrs) => {
            if attrs.title == new_title {
                return Ok(());
            }
            let old_title = attrs.title.clone();
            chat.set_title(connection, new_title.clone()).await?;
            let system_message = SystemMessage::ChangeTitle {
                user_id: sender_id.clone(),
                old_title,
                new_title,
            };
            let group_message = TimestampedMessage::system_message(system_message, ds_timestamp);
            message_buffer.push(group_message);
        }
        ChatType::HandleConnection(_)
        | ChatType::Connection(_)
        | ChatType::TargetedMessageConnection(_)
        | ChatType::PendingConnection(_) => {
            erase_connection_chat_title(connection, chat.id, &new_title).await?;
        }
    }
    Ok(())
}

async fn erase_connection_chat_title(
    connection: impl WriteConnection,
    chat_id: ChatId,
    new_title: &str,
) -> anyhow::Result<()> {
    if new_title.is_empty() {
        Chat::update_title(connection, chat_id, "").await?;
    }
    Ok(())
}
