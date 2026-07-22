// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::{HashMap, hash_map::Entry};

use aircommon::identifiers::UserId;
use aircoreclient::{
    Asset, Chat, ChatId, ChatMessage, ChatNotificationEntry, ChatType, UserProfile,
    clients::{CoreUser, process::process_qs::ReactionNotification},
};
use mimi_content::{Disposition, MimiContent, NestedPart, content_container::PartSemantics};
use serde::{Deserialize, Serialize};
use tracing::error;
use uuid::Uuid;

use crate::api::{notifications::DartNotificationService, user::User};

/// Alert mode of a chat
enum AlertMode {
    Alert,
    Silent,
}

impl User {
    /// Rebuilds and returns notifications for all chats affected by a batch of new messages,
    /// reactions, and silent-rebuild triggers.
    ///
    /// - Chats referenced by `messages` or `reactions` alert
    /// - Chats in `changed_chats` (edits, remote deletes, reaction retractions) rebuild silently.
    /// - Alerting wins when a chat appears in both
    pub(crate) async fn message_and_reaction_notifications(
        &self,
        messages: &[ChatMessage],
        reactions: &[ReactionNotification],
        changed_chats: &[ChatId],
    ) -> ChatNotificationsBatch {
        // Load all chats at one to avoid multiple lookups in db
        let mut chats: HashMap<ChatId, (Chat, AlertMode)> = HashMap::new();

        // Chats to alert
        for chat_id in messages
            .iter()
            .map(|message| message.chat_id())
            .chain(reactions.iter().map(|reaction| reaction.chat_id))
        {
            if let Entry::Vacant(entry) = chats.entry(chat_id)
                && let Some(chat) = self.user.chat(&chat_id).await
                && !chat.is_muted()
            {
                entry.insert((chat, AlertMode::Alert));
            }
        }

        // Silent chats
        for &chat_id in changed_chats {
            if let Entry::Vacant(entry) = chats.entry(chat_id)
                && let Some(chat) = self.user.chat(&chat_id).await
            {
                entry.insert((chat, AlertMode::Silent));
            }
        }

        let mut batch = ChatNotificationsBatch::default();
        for (chat_id, (chat, alert)) in chats {
            match self.rebuild_chat_notification(chat, alert).await {
                ChatNotificationsRebuildOutcome::Notifications(content) => {
                    batch.additions.push(content);
                }
                ChatNotificationsRebuildOutcome::Empty => {
                    batch.empty_chats.push(chat_id);
                }
                ChatNotificationsRebuildOutcome::Skip => {}
            }
        }
        batch
    }

    async fn rebuild_chat_notification(
        &self,
        chat: Chat,
        alert: AlertMode,
    ) -> ChatNotificationsRebuildOutcome {
        if chat.is_muted() {
            return ChatNotificationsRebuildOutcome::Skip;
        }

        let rebuild = match self.user.chat_notification_rebuild_set(chat.id()).await {
            Ok(rebuild) => rebuild,
            Err(error) => {
                error!(%error, "Failed to load chat notification rebuild set");
                return ChatNotificationsRebuildOutcome::Skip;
            }
        };
        let Some(newest) = rebuild.rebuild_set.entries.last() else {
            return ChatNotificationsRebuildOutcome::Empty;
        };

        let title = chat_title(&self.user, &chat).await;
        let Some(body) = self.entry_body(&chat, newest, &rebuild.participants).await else {
            return ChatNotificationsRebuildOutcome::Skip;
        };

        let mut participants: Vec<ConversationParticipant> = rebuild
            .participants
            .into_iter()
            .map(|(user_id, profile)| conversation_participant(user_id, profile))
            .collect();
        participants.sort_unstable_by_key(|participant| participant.uuid);

        let messages = rebuild
            .rebuild_set
            .entries
            .iter()
            .filter_map(conversation_message)
            .collect();

        let chat_avatar = match chat.chat_type() {
            ChatType::Group(attrs) => attrs.picture().map(|bytes| bytes.to_vec()),
            _ => None,
        };

        let conversation = ConversationNotification {
            chat_title: title.clone(),
            is_group: matches!(chat.chat_type(), ChatType::Group(_)),
            own_display_name: rebuild.own_profile.display_name.to_string(),
            participants,
            messages,
            alert: match alert {
                AlertMode::Alert => true,
                AlertMode::Silent => false,
            },
            chat_avatar,
        };

        ChatNotificationsRebuildOutcome::Notifications(NotificationContent {
            identifier: NotificationId::for_chat(chat.id()),
            title,
            body: truncate_notification_text(body),
            chat_id: chat.id(),
            conversation: Some(conversation),
        })
    }

    async fn entry_body(
        &self,
        chat: &Chat,
        entry: &ChatNotificationEntry,
        participants: &HashMap<UserId, UserProfile>,
    ) -> Option<String> {
        match entry {
            ChatNotificationEntry::Message(message) => {
                message
                    .message()
                    .string_representation(&self.user, chat.chat_type(), true)
                    .await
            }
            ChatNotificationEntry::Reaction(reaction) => {
                let reactor = match participants.get(&reaction.reactor) {
                    Some(profile) => &profile.display_name,
                    None => &UserProfile::from_user_id(&reaction.reactor).display_name,
                };
                let target_text = match &reaction.target {
                    Some(message) => {
                        message
                            .message()
                            .string_representation(&self.user, chat.chat_type(), true)
                            .await
                    }
                    None => None,
                };
                let target_text = target_text.unwrap_or_else(|| "your message".to_owned());
                // TODO: Localization
                Some(format!(
                    "{reactor} reacted {} to {target_text}",
                    reaction.emoji
                ))
            }
        }
    }

    /// Send notifications for new chats.
    pub(crate) async fn new_chat_notifications(
        &self,
        chat_ids: &[ChatId],
        notifications: &mut Vec<NotificationContent>,
    ) {
        for chat_id in chat_ids {
            if let Some(chat) = self.user.chat(chat_id).await {
                if chat.is_muted() {
                    continue;
                }
                let title = format!(
                    "You were added to {}",
                    chat.attributes().map(|a| a.title()).unwrap_or("a group"),
                );
                let body = "Say hi to everyone".to_owned();
                notifications.push(NotificationContent {
                    identifier: NotificationId::random(),
                    title: title.to_owned(),
                    body: body.to_owned(),
                    chat_id: *chat_id,
                    conversation: None,
                });
            }
        }
    }

    /// Send notifications for new connection requests.
    pub(crate) async fn new_connection_request_notifications(
        &self,
        connection_chats: &[ChatId],
        notifications: &mut Vec<NotificationContent>,
    ) {
        for chat_id in connection_chats {
            if let Some(chat) = self.user.chat(chat_id).await {
                let (title, body) = match chat.chat_type() {
                    ChatType::Connection(user_id) => {
                        let contact_name = self.user.user_profile(user_id).await.display_name;
                        (
                            format!("New connection with {contact_name}"),
                            "Say hi".to_owned(),
                        )
                    }
                    ChatType::PendingConnection(user_id) => {
                        let contact_name = self.user.user_profile(user_id).await.display_name;
                        (
                            format!("New contact request from {contact_name}"),
                            "Tap to respond".to_owned(),
                        )
                    }
                    _ => continue,
                };
                notifications.push(NotificationContent {
                    identifier: NotificationId::random(),
                    title,
                    body,
                    chat_id: *chat_id,
                    conversation: None,
                });
            }
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct NotificationId(pub Uuid);

impl NotificationId {
    pub(crate) fn random() -> Self {
        Self(Uuid::new_v4())
    }

    pub(crate) fn update_required_id() -> Self {
        Self(uuid::uuid!("42d3fcea-3383-42d3-abd4-f3427e945311"))
    }

    /// Stable ID for a chat notification
    pub(crate) fn for_chat(chat_id: ChatId) -> Self {
        Self(chat_id.uuid())
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotificationContent {
    pub identifier: NotificationId,
    pub title: String,
    pub body: String,
    pub chat_id: ChatId,
    /// Structured conversation payload for Android `MessagingStyle` notifications.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub conversation: Option<ConversationNotification>,
}

/// Structured payload a chat rebuilt conversation notification.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationNotification {
    pub chat_title: String,
    pub is_group: bool,
    /// Device owner
    pub own_display_name: String,
    /// Senders/reactors referenced by `messages`, deduplicated
    pub participants: Vec<ConversationParticipant>,
    /// Chronological, as most CHAT_NOTIFICATION_REBUILD_LIMIT entries
    pub messages: Vec<ConversationMessage>,
    /// `false` for silent rebuilds (delete/edit/retraction)
    pub alert: bool,
    /// The group picture for group chats, absent for 1:1 chats
    ///
    /// Base64 on JNI JSON path
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "base64_avatar"
    )]
    pub chat_avatar: Option<Vec<u8>>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationParticipant {
    /// Stable `Person` key
    pub uuid: Uuid,
    pub display_name: String,
    /// Small image bytes (webp/jpeg)
    ///
    /// Base64 on JNI JSON path
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "base64_avatar"
    )]
    pub avatar: Option<Vec<u8>>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationMessage {
    /// References a participant uuid (reactor for reactions)
    pub sender_uuid: Uuid,
    pub text: String,
    /// Rendering hint: italize the line for reactions
    pub is_reaction: bool,
    /// Milliseconds since epoch
    pub timestamp: i64,
}

#[derive(Debug)]
pub struct NotificationHandle {
    pub identifier: NotificationId,
    pub chat_id: Option<ChatId>,
}

async fn chat_title(user: &CoreUser, chat: &Chat) -> String {
    match chat.chat_type() {
        ChatType::Group(attrs) => attrs.title().to_owned(),
        ChatType::HandleConnection(username) => username.plaintext().to_owned(),
        ChatType::Connection(user_id)
        | ChatType::PendingConnection(user_id)
        | ChatType::TargetedMessageConnection(user_id) => {
            user.user_profile(user_id).await.display_name.to_string()
        }
    }
}

fn conversation_participant(user_id: UserId, profile: UserProfile) -> ConversationParticipant {
    let avatar = profile.profile_picture.map(|asset| match asset {
        Asset::Value(bytes) => bytes,
    });
    ConversationParticipant {
        // TODO: should we include domain here?
        uuid: user_id.uuid(),
        display_name: profile.display_name.to_string(),
        avatar,
    }
}

fn conversation_message(entry: &ChatNotificationEntry) -> Option<ConversationMessage> {
    match entry {
        ChatNotificationEntry::Message(message) => {
            let sender = message.message().sender()?;
            let content = message.message().mimi_content()?;
            let text = render_message_text(content)?;
            Some(ConversationMessage {
                sender_uuid: sender.uuid(),
                text: truncate_notification_text(text),
                is_reaction: false,
                timestamp: message.timestamp().timestamp_millis(),
            })
        }
        ChatNotificationEntry::Reaction(reaction) => {
            let target_text = reaction
                .target
                .as_ref()
                .and_then(|message| render_message_text(message.message().mimi_content()?));
            let target_text = target_text.unwrap_or_else(|| "your message".to_owned());
            Some(ConversationMessage {
                sender_uuid: reaction.reactor.uuid(),
                // TODO: Localization
                text: truncate_notification_text(format!(
                    "Reacted {} to {target_text}",
                    reaction.emoji
                )),
                is_reaction: true,
                timestamp: reaction.created_at.timestamp_millis(),
            })
        }
    }
}

fn render_message_text(content: &MimiContent) -> Option<String> {
    match content.string_rendering() {
        Ok(text) => Some(text),
        Err(mimi_content::Error::UnsupportedContentType) => attachment_placeholder(content),
        Err(error) => {
            error!(%error, "Failed to render message content");
            None
        }
    }
}

fn attachment_placeholder(content: &MimiContent) -> Option<String> {
    let NestedPart::MultiPart {
        part_semantics: PartSemantics::ProcessAll,
        parts,
        ..
    } = &content.nested_part
    else {
        return None;
    };
    if !parts
        .iter()
        .any(|part| part.disposition() == Disposition::Attachment)
    {
        return None;
    };
    let is_image = parts.iter().any(|part| {
        matches!(
            &part,
            NestedPart::SinglePart {
                content_type,
                disposition: Disposition::Preview,
                ..
            } if content_type == "text/blurhash"
        )
    });
    Some(if is_image { "🖼️" } else { "📎" }.to_owned())
}

mod base64_avatar {
    use base64::{Engine, prelude::BASE64_STANDARD};
    use serde::{Deserialize, Deserializer, Serializer};

    pub(super) fn serialize<S>(avatar: &Option<Vec<u8>>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match avatar {
            Some(bytes) => serializer.serialize_str(&BASE64_STANDARD.encode(bytes)),
            None => serializer.serialize_none(),
        }
    }

    pub(super) fn deserialize<'de, D>(deserializer: D) -> Result<Option<Vec<u8>>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let encoded: Option<String> = Option::deserialize(deserializer)?;
        encoded
            .map(|value| {
                BASE64_STANDARD
                    .decode(value)
                    .map_err(serde::de::Error::custom)
            })
            .transpose()
    }
}

/// Cap on `NotificationContent::body` and every `ConversationMessage::text`
///
/// This makes sure that JNI JSON and the FRB payloads are bounded (up to 25 messages plus avatars
/// per rebuild).
const NOTIFICATION_TEXT_LIMIT: usize = 400;

fn truncate_notification_text(mut text: String) -> String {
    let Some(truncate_at) = text
        .char_indices()
        .nth(NOTIFICATION_TEXT_LIMIT)
        .map(|(idx, _)| idx)
    else {
        return text;
    };
    text.replace_range(truncate_at..text.len(), "...");
    text
}

#[derive(Debug, Clone)]
pub(crate) struct NotificationService {
    #[cfg(any(target_os = "ios", target_os = "android", target_os = "macos"))]
    dart_service: DartNotificationService,
    #[cfg(target_os = "linux")]
    zbus_connection: Option<zbus::blocking::Connection>,
}

impl NotificationService {
    #[allow(unused_variables)]
    pub(crate) fn new(dart_service: DartNotificationService) -> Self {
        Self {
            #[cfg(any(target_os = "ios", target_os = "android", target_os = "macos"))]
            dart_service,
            #[cfg(target_os = "linux")]
            zbus_connection: zbus::blocking::Connection::session()
                .inspect_err(|error| error!(%error, "failed to connect to D-Bus"))
                .ok(),
        }
    }

    pub(crate) async fn show_notification(&self, notification: NotificationContent) {
        #[cfg(any(target_os = "ios", target_os = "android", target_os = "macos"))]
        self.dart_service.send_notification(notification).await;
        #[cfg(target_os = "windows")]
        {
            if let Err(error) = notify_rust::Notification::new()
                .summary(notification.title.as_str())
                .body(notification.body.as_str())
                .show()
            {
                error!(%error, "Failed to send desktop notification");
            }
        }
        #[cfg(target_os = "linux")]
        if let Err(error) = self.send_xdg_portal_notification(notification) {
            error!(%error, "Failed to send desktop notification");
        }
    }

    // Version 4.x of `notify-rust` does not set the `sender-pid` hint, which is required for GNOME 46+ compatibility.
    // Doing it manually also lets us enable notifications grouping per chat.
    //
    // The future is to use the XDG Portal API instead, but it is only supported (= not buggy) on GNOME 46+
    // and does not support notifications grouping. It also currently has sparse support on
    // other Desktop Environments.
    #[cfg(target_os = "linux")]
    pub(crate) fn send_xdg_portal_notification(
        &self,
        NotificationContent {
            chat_id,
            title,
            body,
            ..
        }: NotificationContent,
    ) -> anyhow::Result<()> {
        use std::collections::HashMap;

        use zbus::{blocking::Proxy, zvariant::Value};

        let Some(zbus_connection) = self.zbus_connection.as_ref() else {
            return Ok(());
        };
        let proxy = Proxy::new(
            zbus_connection,
            "org.freedesktop.Notifications",
            "/org/freedesktop/Notifications",
            "org.freedesktop.Notifications",
        )?;

        let mut hints: HashMap<&str, Value> = HashMap::new();
        // for GNOME 46+ compatibility
        hints.insert("sender-pid", std::process::id().into());
        hints.insert("x-gnome-stack-group", format!("air-chat-{chat_id}").into());

        proxy.call_method(
            "Notify",
            &(
                "Air",              // app_name
                0u32,               // replaces_id
                "ms.air",           // icon
                title,              // summary
                body,               // body
                Vec::<&str>::new(), // actions
                hints,
                -1i32, // timeout (-1 = default)
            ),
        )?;

        Ok(())
    }

    pub(crate) async fn get_active_notifications(&self) -> Vec<NotificationHandle> {
        #[cfg(any(target_os = "ios", target_os = "android", target_os = "macos"))]
        {
            self.dart_service.get_active_notifications().await
        }
        #[cfg(any(target_os = "linux", target_os = "windows"))]
        {
            Vec::new()
        }
    }

    #[allow(unused_variables)]
    pub(crate) async fn cancel_notifications(&self, identifiers: Vec<NotificationId>) {
        #[cfg(any(target_os = "ios", target_os = "android", target_os = "macos"))]
        self.dart_service.cancel_notifications(identifiers).await;
    }
}

/// A batch of notifications for a batch of chats.
#[derive(Debug, Default)]
pub(crate) struct ChatNotificationsBatch {
    /// Notifications to show
    pub(crate) additions: Vec<NotificationContent>,
    /// Candidates for cancellation
    pub(crate) empty_chats: Vec<ChatId>,
}

/// Outcome of rebuilding a single chat's notification
#[derive(Debug)]
#[expect(clippy::large_enum_variant)]
enum ChatNotificationsRebuildOutcome {
    /// Chat has content to notify about
    /// => replace existing notification
    Notifications(NotificationContent),
    /// Chat build set is empty
    /// => cancel any existing notification
    Empty,
    /// The chat is muted, missing or its content failed to render
    /// => leave existing notification untouched
    Skip,
}
