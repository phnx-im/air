// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::identifiers::MimiId;
use mimi_content::{
    Disposition, MessageStatus, MimiContent, NestedPartContent, content_container::PartSemantics,
};
use tracing::{error, warn};

use crate::{
    groups::Group,
    store::{Store, StoreNotifier},
};

use super::*;

pub(crate) mod edit;
pub(crate) mod persistence;

#[derive(PartialEq, Debug, Clone)]
pub(crate) struct TimestampedMessage {
    timestamp: TimeStamp,
    message: Message,
}

impl TimestampedMessage {
    /// Returns the timestamp of the message. If the message was sent, it's the
    /// timestamp issues by the DS, otherwise it's the timestamp when the
    /// message was created.
    pub(crate) fn timestamp(&self) -> TimeStamp {
        self.timestamp
    }

    /// Mark the message as sent and update the timestamp. If the message was
    /// already marked as sent, nothing happens.
    pub(super) fn mark_as_sent(&mut self, ds_timestamp: TimeStamp) {
        if let Message::Content(content) = &mut self.message {
            self.timestamp = ds_timestamp;
            content.sent = true
        }
    }

    /// Creates a new timestamped message from a MimiContent.
    ///
    /// If content is an error, a chat message containing an error event is created
    /// instead.
    pub(crate) fn from_mimi_content_result(
        content: mimi_content::Result<MimiContent>,
        timestamp: TimeStamp,
        user_id: &UserId,
        group: &Group,
    ) -> Self {
        let message = match content {
            Ok(content) => Message::Content(Box::new(ContentMessage::new(
                user_id.clone(),
                true,
                content,
                group.group_id(),
            ))),
            Err(error) => {
                warn!(%error, "Invalid message content");
                Message::Event(EventMessage::Error(ErrorMessage::new(
                    "Invalid message content".to_owned(),
                )))
            }
        };
        Self { timestamp, message }
    }

    pub(crate) fn system_message(system_message: SystemMessage, ds_timestamp: TimeStamp) -> Self {
        let message = Message::Event(EventMessage::System(system_message));
        Self {
            message,
            timestamp: ds_timestamp,
        }
    }
}

/// Identifier of a message in a chat
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct MessageId {
    pub uuid: Uuid,
}

impl MessageId {
    pub(crate) fn random() -> Self {
        Self {
            uuid: Uuid::new_v4(),
        }
    }

    pub fn new(uuid: Uuid) -> Self {
        Self { uuid }
    }

    pub fn uuid(&self) -> Uuid {
        self.uuid
    }
}

#[derive(PartialEq, Debug, Clone)]
pub struct ChatMessage {
    pub(super) chat_id: ChatId,
    pub(super) message_id: MessageId,
    pub(super) timestamped_message: TimestampedMessage,
    pub(super) status: MessageStatus,
}

impl ChatMessage {
    /// Create a new chat message from a group message. New messages are marked as unread by
    /// default.
    pub(crate) fn new(
        chat_id: ChatId,
        message_id: MessageId,
        timestamped_message: TimestampedMessage,
    ) -> Self {
        Self {
            chat_id,
            message_id,
            timestamped_message,
            status: MessageStatus::Unread,
        }
    }

    pub(crate) fn new_system_message(
        chat_id: ChatId,
        ds_timestamp: TimeStamp,
        system_message: SystemMessage,
    ) -> Self {
        Self::new(
            chat_id,
            MessageId::random(),
            TimestampedMessage::system_message(system_message, ds_timestamp),
        )
    }

    pub fn new_for_test(
        chat_id: ChatId,
        message_id: MessageId,
        timestamp: TimeStamp,
        message: Message,
    ) -> Self {
        Self {
            chat_id,
            message_id,
            timestamped_message: TimestampedMessage { timestamp, message },
            status: MessageStatus::Unread,
        }
    }

    pub(crate) fn new_unsent_message(
        sender: UserId,
        chat_id: ChatId,
        message_id: MessageId,
        content: MimiContent,
        group_id: &GroupId,
    ) -> Self {
        let message = Message::Content(Box::new(ContentMessage::new(
            sender, false, content, group_id,
        )));
        let timestamped_message = TimestampedMessage {
            message,
            timestamp: TimeStamp::now(),
        };
        Self {
            chat_id,
            message_id,
            timestamped_message,
            status: MessageStatus::Unread,
        }
    }

    /// Mark the message as sent and update the timestamp.
    pub(crate) async fn mark_as_sent(
        &mut self,
        connection: &mut sqlx::SqliteConnection,
        notifier: &mut StoreNotifier,
        ds_timestamp: TimeStamp,
    ) -> sqlx::Result<()> {
        Self::update_sent_status(connection, notifier, self.id(), ds_timestamp, true).await?;
        self.timestamped_message.mark_as_sent(ds_timestamp);
        Ok(())
    }

    pub fn id_ref(&self) -> &MessageId {
        &self.message_id
    }

    pub fn id(&self) -> MessageId {
        self.message_id
    }

    pub fn timestamp(&self) -> DateTime<Utc> {
        *self.timestamped_message.timestamp()
    }

    #[cfg(test)]
    pub(crate) fn set_timestamp(&mut self, at: TimeStamp) {
        self.timestamped_message.timestamp = at;
    }

    pub fn edited_at(&self) -> Option<TimeStamp> {
        if let Message::Content(content_message) = &self.timestamped_message.message {
            content_message.edited_at
        } else {
            None
        }
    }

    pub fn set_edited_at(&mut self, edit_created_at: TimeStamp) {
        if let Message::Content(content_message) = &mut self.timestamped_message.message {
            content_message.edited_at = Some(edit_created_at)
        }
    }

    pub fn is_sent(&self) -> bool {
        if let Message::Content(content) = &self.timestamped_message.message {
            content.was_sent()
        } else {
            true
        }
    }

    pub fn status(&self) -> MessageStatus {
        self.status
    }

    pub(crate) fn set_status(&mut self, status: MessageStatus) {
        self.status = status;
    }

    pub fn chat_id(&self) -> ChatId {
        self.chat_id
    }

    pub fn message(&self) -> &Message {
        &self.timestamped_message.message
    }

    pub fn set_content_message(&mut self, message: ContentMessage) {
        self.timestamped_message.message = Message::Content(Box::new(message));
    }

    pub fn message_mut(&mut self) -> &mut Message {
        &mut self.timestamped_message.message
    }
}

// WARNING: If this type is changed, a new `VersionedMessage` variant must be
// introduced and the storage logic changed accordingly.
#[derive(PartialEq, Debug, Clone, Serialize, Deserialize)]
pub enum Message {
    Content(Box<ContentMessage>),
    Event(EventMessage),
}

impl Message {
    pub fn with_content(content: ContentMessage) -> Self {
        Self::Content(Box::new(content))
    }

    /// Returns a string representation of the message for use in UI
    /// notifications.
    pub async fn string_representation(
        &self,
        store: &impl Store,
        chat_type: &ChatType,
    ) -> Option<String> {
        match self {
            Message::Content(content_message) => {
                let content = match content_message.content.string_rendering() {
                    Ok(content) => content,
                    Err(mimi_content::Error::UnsupportedContentType) => {
                        match self.attachment_type() {
                            Some(AttachmentType::Image) => "🖼️".to_owned(),
                            Some(AttachmentType::File) => "📎".to_owned(),
                            None => {
                                error!("Unsupported content type");
                                return None;
                            }
                        }
                    }
                    Err(error) => {
                        error!(%error, "Failed to render message content");
                        return None;
                    }
                };
                let repr = match chat_type {
                    ChatType::TargetedMessageConnection(_) | ChatType::Group => {
                        let display_name = store
                            .user_profile(&content_message.sender)
                            .await
                            .display_name;
                        format!("{display_name}: {content}")
                    }
                    ChatType::HandleConnection(handle) => {
                        format!("{handle}: {content}", handle = handle.plaintext())
                    }
                    ChatType::Connection(_) | ChatType::PendingConnection(_) => content,
                };
                Some(repr)
            }
            Message::Event(event_message) => match &event_message {
                EventMessage::System(system) => Some(system.string_representation(store).await),
                EventMessage::Error(error) => Some(error.message().to_string()),
            },
        }
    }

    pub fn mimi_id(&self) -> Option<&MimiId> {
        match self {
            Message::Content(content_message) => content_message.mimi_id(),
            Message::Event(_) => None,
        }
    }

    pub fn sender(&self) -> Option<&UserId> {
        match self {
            Message::Content(content_message) => Some(content_message.sender()),
            Message::Event(_) => None,
        }
    }

    pub fn mimi_content(&self) -> Option<&MimiContent> {
        match self {
            Message::Content(content_message) => Some(content_message.content()),
            Message::Event(_) => None,
        }
    }

    pub(crate) fn mimi_content_mut(&mut self) -> Option<&mut MimiContent> {
        match self {
            Message::Content(content_message) => Some(content_message.as_mut().content_mut()),
            Message::Event(_) => None,
        }
    }

    /// Returns `Some(AttachmentType)` if the message contains an attachment.
    ///
    /// Otherwise, returns `None`.
    fn attachment_type(&self) -> Option<AttachmentType> {
        let Self::Content(content_message) = self else {
            return None;
        };

        match &content_message.content().nested_part.part {
            // Attachment
            NestedPartContent::MultiPart {
                part_semantics: PartSemantics::ProcessAll,
                parts,
            } if parts
                .iter()
                .any(|part| part.disposition == Disposition::Attachment) =>
            {
                // Blurhash preview indicates an image
                let is_image = parts.iter().any(|part| {
                    part.disposition == Disposition::Preview
                        && matches!(
                            &part.part,
                            NestedPartContent::SinglePart {
                                content_type,
                                ..
                            } if content_type == "text/blurhash"
                        )
                });
                Some(if is_image {
                    AttachmentType::Image
                } else {
                    AttachmentType::File
                })
            }
            _ => None,
        }
    }
}

enum AttachmentType {
    Image,
    File,
}

// WARNING: If this type is changed, a new `VersionedMessage` variant must be
// introduced and the storage logic changed accordingly.
#[derive(PartialEq, Debug, Clone, Serialize, Deserialize)]
pub struct ContentMessage {
    pub(super) mimi_id: Option<MimiId>,
    pub(super) sender: UserId,
    pub(super) sent: bool,
    pub(super) content: MimiContent,
    pub(super) edited_at: Option<TimeStamp>,
}

impl ContentMessage {
    pub fn new(sender: UserId, sent: bool, content: MimiContent, group_id: &GroupId) -> Self {
        // Calculating Mimi ID should never fail, however, since it is calculated partially from
        // the input from the user, we don't want to rely on panicking. Instead, the message does
        // not have a valid Mimi ID.
        let mimi_id = MimiId::calculate(group_id, &sender, &content)
            .inspect_err(|error| error!(%error, "Failed to calculate Mimi ID"))
            .ok();
        Self {
            mimi_id,
            sender,
            sent,
            content,
            edited_at: None,
        }
    }

    pub fn into_sender_and_content(self) -> (UserId, MimiContent) {
        (self.sender, self.content)
    }

    pub fn sender(&self) -> &UserId {
        &self.sender
    }

    /// Mimi ID of the message
    ///
    /// Might be missing if it could not be calculated. Or it was not calculated for this message.
    pub fn mimi_id(&self) -> Option<&MimiId> {
        self.mimi_id.as_ref()
    }

    pub fn was_sent(&self) -> bool {
        self.sent
    }

    pub fn content(&self) -> &MimiContent {
        &self.content
    }

    pub fn content_mut(&mut self) -> &mut MimiContent {
        &mut self.content
    }

    pub fn edited_at(&self) -> Option<TimeStamp> {
        self.edited_at
    }
}

// WARNING: If this type is changed, a new `VersionedMessage` variant must be
// introduced and the storage logic changed accordingly.
#[derive(PartialEq, Debug, Clone, Serialize, Deserialize)]
pub enum EventMessage {
    System(SystemMessage),
    Error(ErrorMessage),
}

// WARNING: If this type is changed, a new `VersionedMessage` variant must be
// introduced and the storage logic changed accordingly.
#[derive(PartialEq, Debug, Clone, Serialize, Deserialize)]
pub enum SystemMessage {
    // The first UserName is the adder/remover the second is the added/removed.
    Add(UserId, UserId),
    Remove(UserId, UserId),
    ChangeTitle {
        user_id: UserId,
        old_title: String,
        new_title: String,
    },
    ChangePicture(UserId),
    /// We received a connection request from another user. The String is the
    /// name of the chat through which the request was made.
    ReceivedDirectConnectionRequest {
        sender: UserId,
        chat_name: String,
    },
    ReceivedHandleConnectionRequest {
        sender: UserId,
        user_handle: UserHandle,
    },
    /// We accepted a connection request from another user. The UserHandle is
    /// the handle through which the connection was made.
    AcceptedConnectionRequest {
        contact: UserId,
        user_handle: Option<UserHandle>,
    },
    /// We received a confirmation for a connection request we sent to another user. The optional
    /// UserHandle is the handle through which the connection was made, if any.
    ReceivedConnectionConfirmation {
        sender: UserId,
        user_handle: Option<UserHandle>,
    },
    /// We requested a connection with another user through a user handle.
    NewHandleConnectionChat(UserHandle),
    /// We requested a connection with another user through a group.
    NewDirectConnectionChat(UserId),
    CreateGroup(UserId),
}

impl SystemMessage {
    async fn string_representation(&self, store: &impl Store) -> String {
        match self {
            SystemMessage::Add(adder, added) => {
                let adder_display_name = store.user_profile(adder).await.display_name;
                let added_display_name = store.user_profile(added).await.display_name;
                format!("{adder_display_name} added {added_display_name} to the chat")
            }
            SystemMessage::Remove(remover, removed) => {
                let remover_display_name = store.user_profile(remover).await.display_name;
                let removed_display_name = store.user_profile(removed).await.display_name;
                format!("{remover_display_name} removed {removed_display_name} from the chat")
            }
            SystemMessage::ChangeTitle {
                user_id,
                old_title,
                new_title,
            } => {
                let user_display_name = store.user_profile(user_id).await.display_name;
                format!(
                    "{user_display_name} changed the group name from \"{old_title}\" to \"{new_title}\""
                )
            }
            SystemMessage::ChangePicture(user_id) => {
                let user_display_name = store.user_profile(user_id).await.display_name;
                format!("{user_display_name} changed the group picture")
            }
            SystemMessage::NewHandleConnectionChat(user_handle) => {
                let handle_str = user_handle.plaintext();
                format!("You requested a connection with {handle_str}")
            }
            SystemMessage::ReceivedConnectionConfirmation {
                sender: user_id,
                user_handle,
            }
            | SystemMessage::AcceptedConnectionRequest {
                contact: user_id,
                user_handle,
            } => {
                let user_display_name = store.user_profile(user_id).await.display_name;
                let base_str = format!("You connected with {user_display_name}");
                if let Some(handle) = user_handle {
                    let handle_str = handle.plaintext();
                    format!("{base_str} through handle {handle_str}")
                } else {
                    base_str
                }
            }
            SystemMessage::ReceivedHandleConnectionRequest {
                sender: user_id,
                user_handle,
            } => {
                let display_name = store.user_profile(user_id).await.display_name;
                let username = user_handle.plaintext();
                format!(
                    "{display_name} sent you a contact request through your username {username}."
                )
            }
            SystemMessage::ReceivedDirectConnectionRequest {
                sender: user_id,
                chat_name,
            } => {
                let display_name = store.user_profile(user_id).await.display_name;
                format!(
                    "{display_name} sent you a contact request through the group chat {chat_name}."
                )
            }
            SystemMessage::NewDirectConnectionChat(user_id) => {
                let display_name = store.user_profile(user_id).await.display_name;
                format!("You requested a connection with {display_name}")
            }
            SystemMessage::CreateGroup(user_id) => {
                let user_display_name = store.user_profile(user_id).await.display_name;
                format!("{user_display_name} created the group")
            }
        }
    }
}

// WARNING: If this type is changed, the storage and loading logic in the
// `crate::chat::messages::persistence` module must be updated
// accordingly and the `MESSAGE_CONTENT_FORMAT_VERSION` constant must be
// incremented by one.
#[derive(PartialEq, Debug, Clone, Serialize, Deserialize)]
pub struct ErrorMessage {
    message: String,
}

impl ErrorMessage {
    pub fn new(message: String) -> Self {
        Self { message }
    }

    pub fn message(&self) -> &str {
        self.message.as_ref()
    }
}

impl From<ErrorMessage> for String {
    fn from(ErrorMessage { message }: ErrorMessage) -> String {
        message
    }
}
