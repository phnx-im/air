// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Types exposed to the Flutter app
//!
//! Some types are mirrored, especially identifiers. All types that are not mirrored are prefixed
//! with `Ui`.

use std::fmt;

pub use aircommon::identifiers::UserHandle;
use aircommon::identifiers::UserId;
use aircoreclient::{
    Asset, ChatAttributes, ChatMessage, ChatStatus, ChatType, Contact, ContentMessage, DisplayName,
    ErrorMessage, EventMessage, InactiveChat, Message, SystemMessage, UserProfile, store::Store,
};
pub use aircoreclient::{ChatId, MessageDraft, MessageId};
use chrono::{DateTime, Duration, Local, Utc};
use flutter_rust_bridge::frb;
use mimi_content::MessageStatus;
use uuid::Uuid;

use crate::api::message_content::UiMimiContent;

/// Mirror of the [`ChatId`] type
#[doc(hidden)]
#[frb(mirror(ChatId))]
#[frb(dart_code = "
    @override
    String toString() => 'ChatId($uuid)';
")]
pub struct _ChatId {
    pub uuid: Uuid,
}

/// UI representation of an [`UserId`]
#[derive(Debug, Clone, Eq, PartialEq, Hash, Ord, PartialOrd)]
#[frb(dart_code = "
    @override
    String toString() => '$uuid@$domain';
")]
pub struct UiUserId {
    pub uuid: Uuid,
    pub domain: String,
}

impl From<UserId> for UiUserId {
    fn from(user_id: UserId) -> Self {
        let (uuid, domain) = user_id.into_parts();
        Self {
            uuid,
            domain: domain.into(),
        }
    }
}

impl From<UiUserId> for UserId {
    fn from(user_id: UiUserId) -> Self {
        UserId::new(
            user_id.uuid,
            user_id.domain.parse().expect("logic error: invalid data"),
        )
    }
}

/// A chat which is a 1:1 connection or a group chat
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct UiChat {
    pub id: ChatId,
    pub status: UiChatStatus,
    pub chat_type: UiChatType,
    pub attributes: UiChatAttributes,
}

/// Details of a chat
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
#[frb(type_64bit_int)]
pub struct UiChatDetails {
    pub id: ChatId,
    pub status: UiChatStatus,
    pub chat_type: UiChatType,
    pub last_used: DateTime<Local>,
    pub attributes: UiChatAttributes,
    pub messages_count: usize,
    pub unread_messages: usize,
    pub last_message: Option<UiChatMessage>,
    pub draft: Option<MessageDraft>,
}

impl UiChatDetails {
    pub(crate) fn connection_user_id(&self) -> Option<&UiUserId> {
        match &self.chat_type {
            UiChatType::Connection(profile) => Some(&profile.user_id),
            _ => None,
        }
    }
}

/// UI representation of a [`MessageDraft`]
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
#[doc(hidden)]
#[frb(mirror(MessageDraft))]
#[frb(dart_metadata = ("freezed"))]
pub struct _MessageDraft {
    pub message: String,
    pub editing_id: Option<MessageId>,
    pub updated_at: DateTime<Utc>,
    pub is_committed: bool,
}

/// Status of a chat
///
/// A chat can be inactive or active, or blocked in case this is a 1:1 chat and the contact is
/// blocked.
#[derive(Eq, PartialEq, Debug, Clone, Hash)]
pub enum UiChatStatus {
    Inactive(UiInactiveChat),
    Active,
    Blocked,
}

impl From<ChatStatus> for UiChatStatus {
    fn from(status: ChatStatus) -> Self {
        match status {
            ChatStatus::Inactive(inactive) => {
                UiChatStatus::Inactive(UiInactiveChat::from(inactive))
            }
            ChatStatus::Active => UiChatStatus::Active,
            ChatStatus::Blocked => UiChatStatus::Blocked,
        }
    }
}

/// Inactive chat with past members
#[derive(Eq, PartialEq, Debug, Clone, Hash)]
pub struct UiInactiveChat {
    pub past_members: Vec<UiUserId>,
}

impl From<InactiveChat> for UiInactiveChat {
    fn from(inactive: InactiveChat) -> Self {
        Self {
            past_members: inactive
                .past_members()
                .iter()
                .cloned()
                .map(From::from)
                .collect(),
        }
    }
}

/// Type of a chat
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub enum UiChatType {
    /// A connection chat which was established via a handle and is not yet confirmed by
    /// the other party.
    HandleConnection(UiUserHandle),
    /// A connection chat that is confirmed by the other party and for which we have
    /// received the necessary secrets.
    Connection(UiUserProfile),
    /// A group chat, that is, it can contains multiple participants.
    Group,
}

impl UiChatType {
    /// Converts [`ChatType`] to [`UiChatType`] but also load the corresponding
    /// user profile.
    ///
    /// If the user profile cannot be loaded, or is not set, a minimal user profile is returned
    /// with the display name derived from the client id.
    #[frb(ignore)]
    pub(crate) async fn load_from_chat_type(store: &impl Store, chat_type: ChatType) -> Self {
        match chat_type {
            ChatType::HandleConnection(handle) => {
                Self::HandleConnection(UiUserHandle::from(handle))
            }
            ChatType::Connection(user_id) => {
                let user_profile = store.user_profile(&user_id).await;
                let profile = UiUserProfile::from_profile(user_profile);
                Self::Connection(profile)
            }
            ChatType::Group => Self::Group,
            // TODO: UI implementation for targeted message connections
            ChatType::TargetedMessageConnection(_) => unreachable!(),
        }
    }
}

/// Attributes of a chat
#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub struct UiChatAttributes {
    /// Title of the chat
    pub title: String,
    /// Optional picture of the chat
    pub picture: Option<ImageData>,
}

impl From<ChatAttributes> for UiChatAttributes {
    fn from(attributes: ChatAttributes) -> Self {
        Self {
            title: attributes.title().to_string(),
            picture: attributes
                .picture()
                .map(|a| ImageData::from_bytes(a.to_vec())),
        }
    }
}

/// Mirror of the [`MessageId`] type
#[doc(hidden)]
#[frb(mirror(MessageId))]
#[frb(dart_code = "
    @override
    String toString() => 'MessageId($uuid)';
")]
pub struct _MessageId {
    pub uuid: Uuid,
}

/// A message in a chat
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
#[frb(dart_metadata = ("freezed"))]
pub struct UiChatMessage {
    pub chat_id: ChatId,
    pub id: MessageId,
    pub timestamp: DateTime<Local>,
    pub message: UiMessage,
    pub position: UiFlightPosition,
    pub status: UiMessageStatus,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum UiMessageStatus {
    Sending,
    /// The message was sent to the server.
    Sent,
    /// The message was received by at least one user in the chat.
    Delivered,
    /// The message was read by at least one user in the chat.
    Read,
    /// The message was hidden because it is from a blocked contact.
    Hidden,
}

impl From<ChatMessage> for UiChatMessage {
    #[frb(ignore)]
    fn from(message: ChatMessage) -> Self {
        let status = match message.status() {
            _ if !message.is_sent() => UiMessageStatus::Sending,
            MessageStatus::Read => UiMessageStatus::Read,
            MessageStatus::Delivered => UiMessageStatus::Delivered,
            MessageStatus::Hidden => UiMessageStatus::Hidden,
            _ => UiMessageStatus::Sent,
        };

        Self {
            chat_id: message.chat_id(),
            id: message.id(),
            timestamp: message.timestamp().with_timezone(&Local),
            message: UiMessage::from(message.message().clone()),
            position: UiFlightPosition::Single,
            status,
        }
    }
}

impl UiChatMessage {
    pub(crate) fn timestamp(&self) -> DateTime<Utc> {
        self.timestamp.with_timezone(&Utc)
    }
}

/// The actual message in a chat
///
/// Can be either a message to display (e.g. a system message) or a message from a user.
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
#[frb(dart_metadata = ("freezed"))]
pub enum UiMessage {
    Content(Box<UiContentMessage>),
    Display(UiEventMessage),
}

impl From<Message> for UiMessage {
    fn from(message: Message) -> Self {
        match message {
            Message::Content(content_message) => {
                UiMessage::Content(Box::new(UiContentMessage::from(*content_message)))
            }
            Message::Event(display_message) => {
                UiMessage::Display(UiEventMessage::from(display_message))
            }
        }
    }
}

/// Content of a message including the sender and whether it was sent
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
#[frb(dart_metadata = ("freezed"))]
pub struct UiContentMessage {
    pub sender: UiUserId,
    pub sent: bool,
    pub content: UiMimiContent,
    pub edited: bool,
}

impl From<ContentMessage> for UiContentMessage {
    fn from(content_message: ContentMessage) -> Self {
        let sent = content_message.was_sent();
        let edited = content_message.edited_at().is_some();
        let (sender, content) = content_message.into_sender_and_content();
        Self {
            sender: sender.into(),
            sent,
            edited,
            content: UiMimiContent::from(content),
        }
    }
}

/// Event message (e.g. a message from the system)
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
#[frb(dart_metadata = ("freezed"))]
pub enum UiEventMessage {
    System(UiSystemMessage),
    Error(UiErrorMessage),
}

impl From<EventMessage> for UiEventMessage {
    fn from(event_message: EventMessage) -> Self {
        match event_message {
            EventMessage::System(message) => UiEventMessage::System(message.into()),
            EventMessage::Error(message) => UiEventMessage::Error(message.into()),
        }
    }
}

/// System message
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
#[frb(dart_metadata = ("freezed"))]
pub enum UiSystemMessage {
    Add(UiUserId, UiUserId),
    Remove(UiUserId, UiUserId),
    ChangeTitle(UiUserId, String, String),
    ChangePicture(UiUserId),
}

impl From<SystemMessage> for UiSystemMessage {
    fn from(system_message: SystemMessage) -> Self {
        match system_message {
            SystemMessage::Add(user_id, contact_id) => {
                UiSystemMessage::Add(user_id.into(), contact_id.into())
            }
            SystemMessage::Remove(user_id, contact_id) => {
                UiSystemMessage::Remove(user_id.into(), contact_id.into())
            }
            SystemMessage::ChangeTitle {
                user_id,
                old_title,
                new_title,
            } => UiSystemMessage::ChangeTitle(user_id.into(), old_title, new_title),
            SystemMessage::ChangePicture(user_id) => UiSystemMessage::ChangePicture(user_id.into()),
        }
    }
}

/// Error message
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
#[frb(dart_metadata = ("freezed"))]
pub struct UiErrorMessage {
    pub message: String,
}

impl From<ErrorMessage> for UiErrorMessage {
    fn from(error_message: ErrorMessage) -> Self {
        Self {
            message: error_message.into(),
        }
    }
}

/// Position of a chat message in a flight
///
/// A flight is a sequence of messages that are grouped to be displayed together.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum UiFlightPosition {
    /// The message is the only message in the flight.
    Single,
    /// The message is the first message in the flight and the flight has more than one message.
    Start,
    /// The message is in the middle of the flight and the flight has more than one message.
    Middle,
    /// The message is the last message in the flight and the flight has more than one message.
    End,
}

impl UiFlightPosition {
    /// Calculate the position of a `message` in a flight.
    ///
    /// The position is determined by the message and its previous and next messages in the
    /// chat timeline.
    ///
    /// The implementation of this function defines which messages are grouped together in a
    /// flight.
    pub(crate) fn calculate(
        message: &UiChatMessage,
        prev_message: Option<&UiChatMessage>,
        next_message: Option<&UiChatMessage>,
    ) -> Self {
        match (prev_message, next_message) {
            (None, None) => Self::Single,
            (Some(prev), None) => {
                if Self::flight_break_condition(prev, message) {
                    Self::Single
                } else {
                    Self::End
                }
            }
            (None, Some(next)) => {
                if Self::flight_break_condition(message, next) {
                    Self::Single
                } else {
                    Self::Start
                }
            }
            (Some(prev), Some(next)) => {
                let at_flight_start = Self::flight_break_condition(prev, message);
                let at_flight_end = Self::flight_break_condition(message, next);
                match (at_flight_start, at_flight_end) {
                    (true, true) => Self::Single,
                    (true, false) => Self::Start,
                    (false, true) => Self::End,
                    (false, false) => Self::Middle,
                }
            }
        }
    }

    /// Returns true if there is a flight break between the messages `a` and `b`.
    fn flight_break_condition(a: &UiChatMessage, b: &UiChatMessage) -> bool {
        const TIME_THRESHOLD: Duration = Duration::minutes(1);
        match (&a.message, &b.message) {
            (UiMessage::Content(a_content), UiMessage::Content(b_content)) => {
                a_content.sender != b_content.sender
                    || TIME_THRESHOLD <= b.timestamp().signed_duration_since(a.timestamp()).abs()
            }
            // all non-content messages are considered to be flight breaks
            _ => true,
        }
    }
}

/// Contact of the logged-in user
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct UiContact {
    pub user_id: UiUserId,
}

impl From<Contact> for UiContact {
    fn from(contact: Contact) -> Self {
        Self {
            user_id: contact.user_id.into(),
        }
    }
}

/// Profile of a user
#[derive(Eq, PartialEq, Debug, Clone, Hash)]
pub struct UiUserProfile {
    /// ID of the user
    pub user_id: UiUserId,
    /// Display name
    pub display_name: String,
    /// Optional profile picture
    pub profile_picture: Option<ImageData>,
}

impl UiUserProfile {
    pub(crate) fn from_profile(user_profile: UserProfile) -> Self {
        Self {
            user_id: user_profile.user_id.into(),
            display_name: user_profile.display_name.into_string(),
            profile_picture: user_profile.profile_picture.map(ImageData::from_asset),
        }
    }

    pub(crate) fn from_user_id(user_id: UserId) -> Self {
        let display_name = DisplayName::from_user_id(&user_id);
        Self {
            user_id: user_id.into(),
            display_name: display_name.into_string(),
            profile_picture: None,
        }
    }
}

/// Image binary data together with its hashsum
///
/// Two images are considered equal in Dart if they have the same hashsum.
#[derive(Clone, PartialEq, Eq, Hash)]
#[frb(
    non_hash,
    non_eq,
    dart_code = "
    @override
    String toString() => 'ImageData(hash: $hash, len: ${data.length})';

    @override
    int get hashCode => hash.hashCode;

    @override
    bool operator ==(Object other) =>
      identical(this, other) ||
      other is ImageData &&
          runtimeType == other.runtimeType &&
          hash == other.hash;
"
)]
pub struct ImageData {
    /// The image data
    pub(crate) data: Vec<u8>,
    /// Opaque hash of the image data as hex string
    pub(crate) hash: String,
}

impl fmt::Debug for ImageData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ImageData")
            .field("data", &self.data.len())
            .field("hash", &self.hash)
            .finish()
    }
}

impl ImageData {
    pub(crate) fn from_bytes(data: Vec<u8>) -> Self {
        let hash = Self::compute_hash(&data);
        Self { data, hash }
    }

    pub(crate) fn from_asset(asset: Asset) -> Self {
        match asset {
            Asset::Value(bytes) => Self::from_bytes(bytes),
        }
    }

    /// Computes opaque hashsum of the data and returns it as a hex string.
    #[frb(sync, positional)]
    pub fn compute_hash(bytes: &[u8]) -> String {
        let hash = blake3::hash(bytes);
        hash.to_hex().to_string()
    }
}

/// Client record of a user
///
/// Each user has a client record which identifies the users database.
#[derive(Debug)]
pub struct UiClientRecord {
    /// The unique identifier of the user
    ///
    /// Also used for identifying the client database path.
    pub(crate) user_id: UiUserId,
    pub(crate) created_at: DateTime<Utc>,
    pub(crate) user_profile: UiUserProfile,
    pub(crate) is_finished: bool,
}

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
#[frb(dart_metadata = ("freezed"))]
pub struct UiUserHandle {
    pub(crate) plaintext: String,
}

impl UiUserHandle {
    /// Returns `None` if the handle is valid, otherwise returns an error message why it is
    /// invalid.
    #[frb(sync)]
    pub fn validation_error(&self) -> Option<String> {
        if let Err(error) = UserHandle::new(self.plaintext.clone()) {
            Some(error.to_string())
        } else {
            None
        }
    }
}

impl From<UserHandle> for UiUserHandle {
    fn from(user_handle: UserHandle) -> Self {
        Self {
            plaintext: user_handle.into_plaintext(),
        }
    }
}
