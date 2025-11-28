// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::sync::Arc;
use std::{collections::HashSet, path::Path};

use aircommon::identifiers::{AttachmentId, MimiId, UserHandle, UserId};
use aircommon::messages::client_as_out::UserHandleDeleteResponse;
use mimi_content::MimiContent;
use mimi_room_policy::VerifiedRoomState;
use tokio_stream::Stream;
use uuid::Uuid;

use crate::clients::add_contact::AddHandleContactResult;
use crate::contacts::{ContactType, TargetedMessageContact};
use crate::{
    AttachmentContent, AttachmentStatus, Chat, ChatId, ChatMessage, Contact, MessageDraft,
    MessageId, clients::attachment::progress::AttachmentProgress, contacts::HandleContact,
    user_handles::UserHandleRecord, user_profiles::UserProfile,
};

pub use notification::{StoreEntityId, StoreNotification, StoreOperation};
pub(crate) use notification::{StoreNotificationsSender, StoreNotifier};

mod r#impl;
mod notification;
mod persistence;

/// The result type of a failable [`Store`] method
pub type StoreResult<T> = anyhow::Result<T>;

/// Unified access to the client data
///
/// This trait is used to access the client data, e.g. the user profile, the chats or the messages.
/// Additionaly, it is used to listen to changes in the client data via the [`Self::subscribe`]
/// method and the [`StoreNotification`] type.
#[allow(async_fn_in_trait, reason = "trait is only used in the workspace")]
#[trait_variant::make(Send)]
pub trait Store {
    // user

    fn user_id(&self) -> &UserId;

    async fn own_user_profile(&self) -> StoreResult<UserProfile>;

    async fn set_own_user_profile(&self, user_profile: UserProfile) -> StoreResult<UserProfile>;

    async fn report_spam(&self, spammer_id: UserId) -> anyhow::Result<()>;

    /// Delete the account on the server and locally.
    ///
    /// If `db_path` is provided, the client database is also deleted.
    async fn delete_account(&self, db_path: Option<&str>) -> anyhow::Result<()>;

    /// Loads a user setting
    ///
    /// If the setting is not found, or loading or decoding failed, `None` is returned.
    async fn user_setting<T: UserSetting>(&self) -> Option<T>;

    async fn set_user_setting<T: UserSetting>(&self, value: &T) -> StoreResult<()>;

    // user handles

    /// Check whether a user handle exists on the AS. Relatively expensive
    /// operation, as it requires computation of a handle hash.
    async fn check_handle_exists(&self, user_handle: &UserHandle) -> StoreResult<bool>;

    async fn user_handles(&self) -> StoreResult<Vec<UserHandle>>;

    async fn user_handle_records(&self) -> StoreResult<Vec<UserHandleRecord>>;

    async fn add_user_handle(
        &self,
        user_handle: UserHandle,
    ) -> StoreResult<Option<UserHandleRecord>>;

    async fn remove_user_handle(
        &self,
        user_handle: &UserHandle,
    ) -> StoreResult<UserHandleDeleteResponse>;

    // chats

    /// Create new chat
    ///
    /// Returns the id of the newly created chat.
    async fn create_chat(&self, title: String, picture: Option<Vec<u8>>) -> StoreResult<ChatId>;

    async fn set_chat_picture(&self, chat_id: ChatId, picture: Option<Vec<u8>>) -> StoreResult<()>;

    async fn set_chat_title(&self, chat_id: ChatId, title: String) -> StoreResult<()>;

    /// Returns the list of all chat ids in the order they should be displayed:
    ///
    /// 1. First return all chats having a draft ordered by the timestamp of the draft, descending.
    /// 2. Then return all chats ordered by the timestamp of the last message, descending.
    async fn ordered_chat_ids(&self) -> StoreResult<Vec<ChatId>>;

    async fn chat(&self, chat_id: ChatId) -> StoreResult<Option<Chat>>;

    async fn chat_participants(&self, chat_id: ChatId) -> StoreResult<Option<HashSet<UserId>>>;

    /// Mark the chat with the given [`ChatId`] as read until the given message id (including).
    ///
    /// Returns whether the chat was marked as read and the message ids of the messages that were
    /// marked as read.
    async fn mark_chat_as_read(
        &self,
        chat_id: ChatId,
        until: MessageId,
    ) -> StoreResult<(bool, Vec<(MessageId, MimiId)>)>;

    /// Delete the chat with the given [`ChatId`].
    ///
    /// Since this function causes the creation of an MLS commit, it can cause more than one effect
    /// on the group. As a result this function returns a vector of [`ChatMessage`]s that
    /// represents the changes to the group. Note that these returned message have already been
    /// persisted.
    async fn delete_chat(&self, chat_id: ChatId) -> StoreResult<Vec<ChatMessage>>;

    async fn leave_chat(&self, chat_id: ChatId) -> StoreResult<()>;

    /// Erases the chat data with the given [`ChatId`].
    ///
    /// Must not be called before the chat is deleted.
    async fn erase_chat(&self, chat_id: ChatId) -> StoreResult<()>;

    // user management

    /// Update the user's key material in the chat with the given [`ChatId`].
    ///
    /// Since this function causes the creation of an MLS commit, it can cause more than one effect
    /// on the group. As a result this function returns a vector of [`ChatMessage`]s that
    /// represents the changes to the group. Note that these returned message have already been
    /// persisted.
    async fn update_key(&self, chat_id: ChatId) -> StoreResult<Vec<ChatMessage>>;

    /// Remove users from the chat with the given [`ChatId`].
    ///
    /// Since this function causes the creation of an MLS commit, it can cause more than one effect
    /// on the group. As a result this function returns a vector of [`ChatMessage`]s that
    /// represents the changes to the group. Note that these returned message have already been
    /// persisted.
    async fn remove_users(
        &self,
        chat_id: ChatId,
        target_users: Vec<UserId>,
    ) -> StoreResult<Vec<ChatMessage>>;

    /// Invite users to an existing chat.
    ///
    /// Since this function causes the creation of an MLS commit, it can cause more than one effect
    /// on the group. As a result this function returns a vector of [`ChatMessage`]s that
    /// represents the changes to the group. Note that these group. Note that these returned
    /// message have already been persisted.
    async fn invite_users(
        &self,
        chat_id: ChatId,
        invited_users: &[UserId],
    ) -> StoreResult<Vec<ChatMessage>>;

    async fn load_room_state(&self, chat_id: ChatId) -> StoreResult<(UserId, VerifiedRoomState)>;

    // contacts

    /// Create a connection with a new user via their user handle.
    ///
    /// Returns the [`ChatId`] of the newly created connection
    /// chat, or `None` if the user handle does not exist.
    async fn add_contact(&self, handle: UserHandle) -> StoreResult<AddHandleContactResult>;

    /// Create a connection with a new user via an existing group chat.
    ///
    /// The group chat must contain the user to connect to.
    /// Returns the [`ChatId`] of the newly created connection
    /// chat.
    async fn add_contact_from_group(&self, chat_id: ChatId, user_id: UserId)
    -> StoreResult<ChatId>;

    async fn block_contact(&self, user_id: UserId) -> StoreResult<()>;

    async fn unblock_contact(&self, user_id: UserId) -> StoreResult<()>;

    async fn contacts(&self) -> StoreResult<Vec<Contact>>;

    async fn contact(&self, user_id: &UserId) -> StoreResult<Option<ContactType>>;

    async fn handle_contacts(&self) -> StoreResult<Vec<HandleContact>>;

    async fn targeted_message_contacts(&self) -> StoreResult<Vec<TargetedMessageContact>>;

    async fn user_profile(&self, user_id: &UserId) -> UserProfile;

    // messages

    async fn messages(&self, chat_id: ChatId, limit: usize) -> StoreResult<Vec<ChatMessage>>;

    async fn message(&self, message_id: MessageId) -> StoreResult<Option<ChatMessage>>;

    async fn prev_message(&self, message_id: MessageId) -> StoreResult<Option<ChatMessage>>;

    async fn next_message(&self, message_id: MessageId) -> StoreResult<Option<ChatMessage>>;

    async fn last_message(&self, chat_id: ChatId) -> StoreResult<Option<ChatMessage>>;

    async fn last_message_by_user(
        &self,
        chat_id: ChatId,
        user_id: &UserId,
    ) -> StoreResult<Option<ChatMessage>>;

    async fn message_draft(&self, chat_id: ChatId) -> StoreResult<Option<MessageDraft>>;

    async fn store_message_draft(
        &self,
        chat_id: ChatId,
        message_draft: Option<&MessageDraft>,
    ) -> StoreResult<()>;

    async fn commit_all_message_drafts(&self) -> StoreResult<()>;

    async fn messages_count(&self, chat_id: ChatId) -> StoreResult<usize>;

    async fn unread_messages_count(&self, chat_id: ChatId) -> StoreResult<usize>;

    async fn global_unread_messages_count(&self) -> StoreResult<usize>;

    async fn send_message(
        &self,
        chat_id: ChatId,
        content: MimiContent,
        replaces_id: Option<MessageId>,
    ) -> StoreResult<ChatMessage>;

    async fn resend_message(&self, local_message_id: Uuid) -> StoreResult<()>;

    // attachments

    /// Stores local attachment incl. the corresponding message, and returns a task for uploading
    /// attachment and its uploading progress.
    ///
    /// The returned task uploads the attachment fully to the server and returns the message corresponding
    /// to the attachment. The message is *not* enqueued to the outbound service. It is the caller's
    /// responsibility to enqueue the message.
    async fn upload_attachment(
        &self,
        chat_id: ChatId,
        path: &Path,
    ) -> StoreResult<(
        AttachmentId,
        AttachmentProgress,
        impl Future<Output = StoreResult<ChatMessage>> + use<Self>,
    )>;

    async fn retry_upload_attachment(
        &self,
        attachment_id: AttachmentId,
    ) -> StoreResult<(
        AttachmentId,
        AttachmentProgress,
        impl Future<Output = StoreResult<ChatMessage>> + use<Self>,
    )>;

    fn download_attachment(
        &self,
        attachment_id: AttachmentId,
    ) -> (
        AttachmentProgress,
        impl Future<Output = StoreResult<()>> + use<Self>,
    );

    async fn pending_attachments(&self) -> StoreResult<Vec<AttachmentId>>;

    async fn load_attachment(&self, attachment_id: AttachmentId) -> StoreResult<AttachmentContent>;

    async fn attachment_status(
        &self,
        attachment_id: AttachmentId,
    ) -> StoreResult<Option<AttachmentStatus>>;

    // observability

    fn notify(&self, notification: StoreNotification);

    fn subscribe(&self) -> impl Stream<Item = Arc<StoreNotification>> + Send + Unpin + 'static;

    fn subscribe_iter(&self) -> impl Iterator<Item = Arc<StoreNotification>> + Send + 'static;

    async fn enqueue_notification(&self, notification: &StoreNotification) -> StoreResult<()>;

    async fn dequeue_notification(&self) -> StoreResult<StoreNotification>;
}

pub trait UserSetting: Send + Sync {
    const KEY: &'static str;

    fn encode(&self) -> StoreResult<Vec<u8>>;
    fn decode(bytes: Vec<u8>) -> StoreResult<Self>
    where
        Self: Sized;
}
