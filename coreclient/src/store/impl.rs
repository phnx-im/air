// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{collections::HashSet, path::Path, sync::Arc};

use aircommon::{
    identifiers::{AttachmentId, MimiId, UserId, Username, UsernameHash},
    messages::client_as_out::UsernameDeleteResponse,
    time::TimeStamp,
};
use anyhow::Context;
use mimi_room_policy::VerifiedRoomState;
use tokio::task::spawn_blocking;
use tokio_stream::Stream;
use tracing::error;
use uuid::Uuid;

use crate::{
    AcceptContactRequestError, AddUsernameContactError, AttachmentContent, AttachmentStatus, Chat,
    ChatId, ChatMessage, Contact, InviteUsersError, MessageDraft, MessageId,
    ProvisionAttachmentError, UploadTaskError,
    clients::{
        CoreUser,
        attachment::{AttachmentRecord, progress::AttachmentProgress},
        safety_code::SafetyCode,
        user_settings::UserSettingRecord,
    },
    contacts::{ContactType, PartialContact, TargetedMessageContact, UsernameContact},
    db_access::WriteExecutor,
    store::UserSetting,
    user_profiles::UserProfile,
    usernames::UsernameRecord,
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

    async fn check_username_exists(&self, username: Username) -> StoreResult<Option<UsernameHash>> {
        let hash = spawn_blocking(move || username.calculate_hash()).await??;
        let username_exists = self.api_client()?.as_check_username_exists(hash).await?;
        Ok(username_exists.then_some(hash))
    }

    async fn usernames(&self) -> StoreResult<Vec<Username>> {
        Ok(UsernameRecord::load_all_usernames(self.pool()).await?)
    }

    async fn username_records(&self) -> StoreResult<Vec<UsernameRecord>> {
        Ok(UsernameRecord::load_all(self.pool()).await?)
    }

    async fn add_username(&self, username: Username) -> StoreResult<Option<UsernameRecord>> {
        self.add_username(username).await
    }

    async fn remove_username(&self, username: &Username) -> StoreResult<UsernameDeleteResponse> {
        self.remove_username(username).await
    }

    async fn create_chat(&self, title: String, picture: Option<Vec<u8>>) -> StoreResult<ChatId> {
        self.create_chat(title, picture).await
    }

    async fn set_chat_picture(&self, chat_id: ChatId, picture: Option<Vec<u8>>) -> StoreResult<()> {
        self.set_chat_picture(chat_id, picture).await
    }

    async fn set_chat_title(&self, chat_id: ChatId, title: String) -> StoreResult<()> {
        self.set_chat_title(chat_id, title).await
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
        self.update_key(chat_id, None).await
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
    ) -> StoreResult<Result<Vec<ChatMessage>, InviteUsersError>> {
        self.invite_users(chat_id, invited_users).await
    }

    async fn load_room_state(&self, chat_id: ChatId) -> StoreResult<(UserId, VerifiedRoomState)> {
        self.load_room_state(&chat_id).await
    }

    async fn add_contact(
        &self,
        username: Username,
        hash: UsernameHash,
    ) -> StoreResult<Result<ChatId, AddUsernameContactError>> {
        self.add_contact_via_username(username, hash).await
    }

    async fn add_contact_from_group(
        &self,
        chat_id: ChatId,
        user_id: UserId,
    ) -> StoreResult<ChatId> {
        self.add_contact_via_targeted_message(chat_id, user_id)
            .await
    }

    async fn block_contact(&self, user_id: UserId) -> StoreResult<()> {
        self.block_contact(user_id).await
    }

    async fn unblock_contact(&self, user_id: UserId) -> StoreResult<()> {
        self.unblock_contact(user_id).await
    }

    async fn accept_contact_request(
        &self,
        chat_id: ChatId,
    ) -> StoreResult<Result<(), AcceptContactRequestError>> {
        // boxing large future
        Box::pin(self.accept_contact_request(chat_id)).await
    }

    async fn contacts(&self) -> StoreResult<Vec<Contact>> {
        Ok(self.contacts().await?)
    }

    async fn contact(&self, user_id: &UserId) -> StoreResult<Option<ContactType>> {
        if let Some(contact) = self.try_contact(user_id).await? {
            Ok(Some(ContactType::Full(contact)))
        } else if let Some(targeted_message_contact) =
            self.try_targeted_message_contact(user_id).await?
        {
            Ok(Some(ContactType::Partial(PartialContact::TargetedMessage(
                targeted_message_contact,
            ))))
        } else {
            Ok(None)
        }
    }

    async fn username_contacts(&self) -> StoreResult<Vec<UsernameContact>> {
        Ok(self.username_contacts().await?)
    }

    async fn targeted_message_contacts(&self) -> StoreResult<Vec<TargetedMessageContact>> {
        Ok(self.targeted_message_contacts().await?)
    }

    async fn user_profile(&self, user_id: &UserId) -> UserProfile {
        self.user_profile(user_id).await
    }

    async fn messages(&self, chat_id: ChatId, limit: usize) -> StoreResult<Vec<ChatMessage>> {
        self.get_messages(chat_id, limit).await
    }

    async fn messages_before(
        &self,
        chat_id: ChatId,
        before: TimeStamp,
        before_id: MessageId,
        limit: usize,
    ) -> StoreResult<(Vec<ChatMessage>, bool)> {
        Ok(ChatMessage::load_before(
            self.pool().acquire().await?.as_mut(),
            chat_id,
            before,
            before_id,
            limit as u32,
        )
        .await?)
    }

    async fn messages_after(
        &self,
        chat_id: ChatId,
        after: TimeStamp,
        after_id: MessageId,
        limit: usize,
    ) -> StoreResult<(Vec<ChatMessage>, bool)> {
        Ok(ChatMessage::load_after(
            self.pool().acquire().await?.as_mut(),
            chat_id,
            after,
            after_id,
            limit as u32,
        )
        .await?)
    }

    async fn messages_from(
        &self,
        chat_id: ChatId,
        from: TimeStamp,
        from_id: MessageId,
        limit: usize,
    ) -> StoreResult<(Vec<ChatMessage>, bool)> {
        Ok(ChatMessage::load_starting_from(
            self.pool().acquire().await?.as_mut(),
            chat_id,
            from,
            from_id,
            limit as u32,
        )
        .await?)
    }

    async fn messages_around(
        &self,
        chat_id: ChatId,
        anchor: TimeStamp,
        anchor_id: MessageId,
        half_limit: usize,
    ) -> StoreResult<(Vec<ChatMessage>, bool, bool)> {
        Ok(ChatMessage::load_around(
            self.pool().acquire().await?.as_mut(),
            chat_id,
            anchor,
            anchor_id,
            half_limit as u32,
        )
        .await?)
    }

    async fn first_unread_message(&self, chat_id: ChatId) -> StoreResult<Option<ChatMessage>> {
        self.with_transaction(async |txn| {
            let chat = Chat::load(txn.as_mut(), &chat_id)
                .await?
                .with_context(|| format!("chat not found: {chat_id}"))?;
            Ok(
                ChatMessage::first_unread_message(txn.as_mut(), chat_id, chat.last_read.into())
                    .await?,
            )
        })
        .await
    }

    async fn message(&self, message_id: MessageId) -> StoreResult<Option<ChatMessage>> {
        self.message(message_id).await
    }

    async fn prev_message(
        &self,
        chat_id: ChatId,
        message_id: MessageId,
    ) -> StoreResult<Option<ChatMessage>> {
        self.prev_message(chat_id, message_id).await
    }

    async fn next_message(
        &self,
        chat_id: ChatId,
        message_id: MessageId,
    ) -> StoreResult<Option<ChatMessage>> {
        self.next_message(chat_id, message_id).await
    }

    async fn last_message(&self, chat_id: ChatId) -> StoreResult<Option<ChatMessage>> {
        Ok(ChatMessage::last_message(self.pool().acquire().await?.as_mut(), chat_id).await?)
    }

    async fn last_message_by_user(
        &self,
        chat_id: ChatId,
        user_id: &UserId,
    ) -> StoreResult<Option<ChatMessage>> {
        Ok(ChatMessage::last_content_message_by_user(
            self.pool().acquire().await?.as_mut(),
            chat_id,
            user_id,
        )
        .await?)
    }

    async fn message_draft(&self, chat_id: ChatId) -> StoreResult<Option<MessageDraft>> {
        Ok(MessageDraft::load(self.pool().acquire().await?.as_mut(), chat_id).await?)
    }

    async fn store_message_draft(
        &self,
        chat_id: ChatId,
        message_draft: Option<&MessageDraft>,
    ) -> StoreResult<()> {
        self.db()
            .with_write_transaction(async move |txn| {
                if let Some(message_draft) = message_draft {
                    message_draft.store(txn, chat_id).await?;
                } else {
                    MessageDraft::delete(txn, chat_id).await?;
                }
                Ok(())
            })
            .await
    }

    async fn commit_all_message_drafts(&self) -> StoreResult<()> {
        self.db()
            .with_write_transaction(async |txn| Ok(MessageDraft::commit_all(txn).await?))
            .await
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
        replaces: Option<ChatMessage>,
    ) -> StoreResult<ChatMessage> {
        self.send_message(chat_id, content, replaces).await
    }

    async fn delete_message_content_locally(&self, message_id: MessageId) -> StoreResult<()> {
        self.delete_message_content_locally(message_id).await
    }

    async fn delete_message_locally(&self, message_id: MessageId) -> StoreResult<()> {
        self.delete_message_locally(message_id).await
    }

    async fn upload_attachment(
        &self,
        chat_id: ChatId,
        path: &Path,
    ) -> StoreResult<
        Result<
            (
                AttachmentId,
                AttachmentProgress,
                impl Future<Output = Result<ChatMessage, UploadTaskError>> + use<>,
            ),
            ProvisionAttachmentError,
        >,
    > {
        self.upload_attachment(chat_id, path).await
    }

    async fn retry_upload_attachment(
        &self,
        attachment_id: AttachmentId,
    ) -> StoreResult<
        Result<
            (
                AttachmentId,
                AttachmentProgress,
                impl Future<Output = Result<ChatMessage, UploadTaskError>> + use<>,
            ),
            ProvisionAttachmentError,
        >,
    > {
        self.retry_upload_attachment(attachment_id).await
    }

    fn download_attachment(
        &self,
        attachment_id: AttachmentId,
    ) -> (
        AttachmentProgress,
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

    async fn attachment_status(
        &self,
        attachment_id: AttachmentId,
    ) -> StoreResult<Option<AttachmentStatus>> {
        Ok(AttachmentRecord::status(self.pool(), attachment_id).await?)
    }

    async fn attachment_ids_for_message(
        &self,
        message_id: MessageId,
    ) -> StoreResult<Vec<AttachmentId>> {
        Ok(AttachmentRecord::load_ids_by_message_id(self.pool(), message_id).await?)
    }

    async fn resend_message(&self, local_message_id: Uuid) -> StoreResult<()> {
        self.outbound_service()
            .enqueue_chat_message(MessageId::new(local_message_id), None)
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

    async fn safety_code(&self, user_id: &UserId) -> StoreResult<SafetyCode> {
        self.safety_code(user_id).await
    }
}
