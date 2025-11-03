// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{collections::HashSet, path::Path, sync::Arc};

use aircommon::{
    identifiers::{AttachmentId, MimiId, UserHandle, UserId},
    messages::client_as_out::UserHandleDeleteResponse,
};
use mimi_room_policy::VerifiedRoomState;
use tokio_stream::Stream;
use tracing::error;
use uuid::Uuid;

use crate::{
    AttachmentContent, Chat, ChatId, ChatMessage, Contact, DownloadProgress, MessageDraft,
    MessageId,
    clients::{CoreUser, attachment::AttachmentRecord, user_settings::UserSettingRecord},
    contacts::HandleContact,
    store::UserSetting,
    user_handles::UserHandleRecord,
    user_profiles::UserProfile,
    utils::connection_ext::StoreExt,
};

use super::{Store, StoreNotification, StoreResult};

impl Store for CoreUser {
    fn user_id(&self) -> &UserId {
        self.user_id()
    }

    async fn own_user_profile(&self) -> StoreResult<UserProfile> {
        Ok(self.own_user_profile().await?)
    }

    async fn set_own_user_profile(&self, user_profile: UserProfile) -> StoreResult<UserProfile> {
        self.set_own_user_profile(user_profile).await
    }

    async fn report_spam(&self, spammer_id: UserId) -> anyhow::Result<()> {
        self.report_spam(spammer_id).await
    }

    async fn delete_account(&self, db_path: Option<&str>) -> anyhow::Result<()> {
        self.delete_account(db_path).await
    }

    async fn user_setting<T: UserSetting>(&self) -> Option<T> {
        match UserSettingRecord::load(self.pool(), T::KEY).await {
            Ok(Some(bytes)) => match T::decode(bytes) {
                Ok(value) => Some(value),
                Err(error) => {
                    error!(%error, "Failed to decode user setting; resetting to default");
                    None
                }
            },
            Ok(None) => None,
            Err(error) => {
                error!(%error, "Failed to load user setting; resetting to default");
                None
            }
        }
    }

    async fn set_user_setting<T: UserSetting>(&self, value: &T) -> StoreResult<()> {
        UserSettingRecord::store(self.pool(), T::KEY, T::encode(value)?).await?;
        Ok(())
    }

    async fn user_handles(&self) -> StoreResult<Vec<UserHandle>> {
        Ok(UserHandleRecord::load_all_handles(self.pool()).await?)
    }

    async fn user_handle_records(&self) -> StoreResult<Vec<UserHandleRecord>> {
        Ok(UserHandleRecord::load_all(self.pool()).await?)
    }

    async fn add_user_handle(
        &self,
        user_handle: UserHandle,
    ) -> StoreResult<Option<UserHandleRecord>> {
        self.add_user_handle(user_handle).await
    }

    async fn remove_user_handle(
        &self,
        user_handle: &UserHandle,
    ) -> StoreResult<UserHandleDeleteResponse> {
        self.remove_user_handle(user_handle).await
    }

    async fn create_chat(&self, title: String, picture: Option<Vec<u8>>) -> StoreResult<ChatId> {
        self.create_chat(title, picture).await
    }

    async fn set_chat_picture(&self, chat_id: ChatId, picture: Option<Vec<u8>>) -> StoreResult<()> {
        self.set_chat_picture(chat_id, picture).await
    }

    async fn chats(&self) -> StoreResult<Vec<Chat>> {
        Ok(self.chats().await?)
    }

    async fn ordered_chat_ids(&self) -> StoreResult<Vec<ChatId>> {
        Ok(Chat::load_ordered_ids(self.pool()).await?)
    }

    async fn chat(&self, chat_id: ChatId) -> StoreResult<Option<Chat>> {
        Ok(Chat::load(self.pool().acquire().await?.as_mut(), &chat_id).await?)
    }

    async fn chat_participants(&self, chat_id: ChatId) -> StoreResult<Option<HashSet<UserId>>> {
        self.try_chat_participants(chat_id).await
    }

    async fn delete_chat(&self, chat_id: ChatId) -> StoreResult<Vec<ChatMessage>> {
        self.delete_chat(chat_id).await
    }

    async fn leave_chat(&self, chat_id: ChatId) -> StoreResult<()> {
        self.leave_chat(chat_id).await
    }

    async fn erase_chat(&self, chat_id: ChatId) -> StoreResult<()> {
        self.erase_chat(chat_id).await
    }

    async fn update_key(&self, chat_id: ChatId) -> StoreResult<Vec<ChatMessage>> {
        self.update_key(chat_id).await
    }

    async fn remove_users(
        &self,
        chat_id: ChatId,
        target_users: Vec<UserId>,
    ) -> StoreResult<Vec<ChatMessage>> {
        self.remove_users(chat_id, target_users).await
    }

    async fn invite_users(
        &self,
        chat_id: ChatId,
        invited_users: &[UserId],
    ) -> StoreResult<Vec<ChatMessage>> {
        self.invite_users(chat_id, invited_users).await
    }

    async fn load_room_state(&self, chat_id: ChatId) -> StoreResult<(UserId, VerifiedRoomState)> {
        self.load_room_state(&chat_id).await
    }

    async fn add_contact(&self, handle: UserHandle) -> StoreResult<Option<ChatId>> {
        self.add_contact_via_handle(handle).await
    }

    async fn block_contact(&self, user_id: UserId) -> StoreResult<()> {
        self.block_contact(user_id).await
    }

    async fn unblock_contact(&self, user_id: UserId) -> StoreResult<()> {
        self.unblock_contact(user_id).await
    }

    async fn contacts(&self) -> StoreResult<Vec<Contact>> {
        Ok(self.contacts().await?)
    }

    async fn contact(&self, user_id: &UserId) -> StoreResult<Option<Contact>> {
        Ok(self.try_contact(user_id).await?)
    }

    async fn handle_contacts(&self) -> StoreResult<Vec<HandleContact>> {
        Ok(self.handle_contacts().await?)
    }

    async fn user_profile(&self, user_id: &UserId) -> UserProfile {
        self.user_profile(user_id).await
    }

    async fn messages(&self, chat_id: ChatId, limit: usize) -> StoreResult<Vec<ChatMessage>> {
        self.get_messages(chat_id, limit).await
    }

    async fn message(&self, message_id: MessageId) -> StoreResult<Option<ChatMessage>> {
        Ok(self.message(message_id).await?)
    }

    async fn prev_message(&self, message_id: MessageId) -> StoreResult<Option<ChatMessage>> {
        self.prev_message(message_id).await
    }

    async fn next_message(&self, message_id: MessageId) -> StoreResult<Option<ChatMessage>> {
        self.next_message(message_id).await
    }

    async fn last_message(&self, chat_id: ChatId) -> StoreResult<Option<ChatMessage>> {
        Ok(ChatMessage::last_content_message(self.pool(), chat_id).await?)
    }

    async fn last_message_by_user(
        &self,
        chat_id: ChatId,
        user_id: &UserId,
    ) -> StoreResult<Option<ChatMessage>> {
        Ok(ChatMessage::last_content_message_by_user(self.pool(), chat_id, user_id).await?)
    }

    async fn message_draft(&self, chat_id: ChatId) -> StoreResult<Option<MessageDraft>> {
        Ok(MessageDraft::load(self.pool(), chat_id).await?)
    }

    async fn store_message_draft(
        &self,
        chat_id: ChatId,
        message_draft: Option<&MessageDraft>,
    ) -> StoreResult<()> {
        let mut notifier = self.store_notifier();
        if let Some(message_draft) = message_draft {
            message_draft
                .store(self.pool(), &mut notifier, chat_id)
                .await?;
        } else {
            MessageDraft::delete(self.pool(), &mut notifier, chat_id).await?;
        }
        notifier.notify();
        Ok(())
    }

    async fn messages_count(&self, chat_id: ChatId) -> StoreResult<usize> {
        Ok(self.try_messages_count(chat_id).await?)
    }

    async fn unread_messages_count(&self, chat_id: ChatId) -> StoreResult<usize> {
        Ok(self.try_unread_messages_count(chat_id).await?)
    }

    async fn global_unread_messages_count(&self) -> StoreResult<usize> {
        Ok(self.global_unread_messages_count().await?)
    }

    async fn mark_chat_as_read(
        &self,
        chat_id: ChatId,
        until: MessageId,
    ) -> StoreResult<(bool, Vec<(MessageId, MimiId)>)> {
        self.with_transaction_and_notifier(async |txn, notifier| {
            Chat::mark_as_read_until_message_id(txn, notifier, chat_id, until, self.user_id())
                .await
                .map_err(From::from)
        })
        .await
    }

    async fn send_message(
        &self,
        chat_id: ChatId,
        content: mimi_content::MimiContent,
        replaces_id: Option<MessageId>,
    ) -> StoreResult<ChatMessage> {
        self.send_message(chat_id, content, replaces_id).await
    }

    async fn upload_attachment(&self, chat_id: ChatId, path: &Path) -> StoreResult<ChatMessage> {
        self.upload_attachment(chat_id, path).await
    }

    fn download_attachment(
        &self,
        attachment_id: AttachmentId,
    ) -> (
        DownloadProgress,
        impl Future<Output = StoreResult<()>> + use<>,
    ) {
        self.download_attachment(attachment_id)
    }

    async fn pending_attachments(&self) -> StoreResult<Vec<AttachmentId>> {
        Ok(AttachmentRecord::load_all_pending(self.pool()).await?)
    }

    async fn load_attachment(&self, attachment_id: AttachmentId) -> StoreResult<AttachmentContent> {
        Ok(AttachmentRecord::load_content(self.pool(), attachment_id).await?)
    }

    async fn resend_message(&self, local_message_id: Uuid) -> StoreResult<()> {
        self.outbound_service()
            .enqueue_chat_message(MessageId::new(local_message_id))
            .await?;
        Ok(())
    }

    fn notify(&self, notification: StoreNotification) {
        self.send_store_notification(notification);
    }

    fn subscribe(&self) -> impl Stream<Item = Arc<StoreNotification>> + Send + 'static {
        self.subscribe_to_store_notifications()
    }

    fn subscribe_iter(&self) -> impl Iterator<Item = Arc<StoreNotification>> + Send + 'static {
        self.subscribe_iter_to_store_notifications()
    }

    async fn enqueue_notification(&self, notification: &StoreNotification) -> StoreResult<()> {
        self.enqueue_store_notification(notification).await
    }

    async fn dequeue_notification(&self) -> StoreResult<StoreNotification> {
        self.dequeue_store_notification().await
    }
}
