// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::{codec::PersistenceCodec, identifiers::UserId, time::TimeStamp};
use sqlx::SqliteConnection;

use crate::{
    Chat, ChatAttributes, ChatId, ChatMessage, SystemMessage, chats::messages::TimestampedMessage,
    groups::GroupData, job::chat_operation::ChatOperation, store::StoreNotifier,
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
    pub(crate) async fn update_key(
        &self,
        chat_id: ChatId,
        new_chat_attributes: Option<&ChatAttributes>,
    ) -> anyhow::Result<Vec<ChatMessage>> {
        let job = ChatOperation::update(chat_id, new_chat_attributes.cloned());
        self.execute_job(job).await
    }
}

pub(crate) async fn update_chat_attributes(
    connection: &mut SqliteConnection,
    notifier: &mut StoreNotifier,
    chat: &mut Chat,
    sender_id: UserId,
    group_data: GroupData,
    ds_timestamp: TimeStamp,
    message_buffer: &mut Vec<TimestampedMessage>,
) -> anyhow::Result<()> {
    let new_chat_attributes: ChatAttributes = PersistenceCodec::from_slice(group_data.bytes())?;
    let new_title = new_chat_attributes.title;
    let old_title = chat.attributes.title.clone();
    if chat.attributes.title != new_title {
        chat.set_title(&mut *connection, notifier, new_title.clone())
            .await?;
        let system_message = SystemMessage::ChangeTitle {
            user_id: sender_id.clone(),
            old_title,
            new_title,
        };
        let group_message = TimestampedMessage::system_message(system_message, ds_timestamp);
        message_buffer.push(group_message);
    }
    if chat.attributes.picture != new_chat_attributes.picture {
        chat.set_picture(connection, notifier, new_chat_attributes.picture)
            .await?;
        let system_message = SystemMessage::ChangePicture(sender_id);
        let group_message = TimestampedMessage::system_message(system_message, ds_timestamp);
        message_buffer.push(group_message);
    }

    Ok(())
}
