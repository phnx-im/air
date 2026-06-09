// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircoreclient::{ChatId, ChatMessage, ChatType};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::api::{notifications::DartNotificationService, user::User};

impl User {
    /// Send notifications for new messages.
    pub(crate) async fn new_message_notifications(
        &self,
        messages: &[ChatMessage],
        notifications: &mut Vec<NotificationContent>,
    ) {
        for message in messages {
            if let Some(chat) = self.user.chat(&message.chat_id()).await {
                if chat.is_muted() {
                    continue;
                }
                let title = match chat.chat_type() {
                    ChatType::TargetedMessageConnection(user_id)
                    | ChatType::PendingConnection(user_id)
                    | ChatType::Connection(user_id) => self
                        .user
                        .user_profile(user_id)
                        .await
                        .display_name
                        .to_string(),
                    ChatType::HandleConnection(handle) => handle.plaintext().to_owned(),
                    ChatType::Group(attrs) => attrs.title().to_owned(),
                };
                let Some(body) = message
                    .message()
                    .string_representation(&self.user, chat.chat_type())
                    .await
                else {
                    continue;
                };
                notifications.push(NotificationContent {
                    identifier: NotificationId::random(),
                    title: title.to_owned(),
                    body: body.to_owned(),
                    chat_id: chat.id(),
                });
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
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotificationContent {
    pub identifier: NotificationId,
    pub title: String,
    pub body: String,
    pub chat_id: ChatId,
}

#[derive(Debug)]
pub struct NotificationHandle {
    pub identifier: NotificationId,
    pub chat_id: Option<ChatId>,
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
                .inspect_err(|error| tracing::error!(%error, "failed to connect to D-Bus"))
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
                tracing::error!(%error, "Failed to send desktop notification");
            }
        }
        #[cfg(target_os = "linux")]
        if let Err(error) = self.send_xdg_portal_notification(notification) {
            tracing::error!(%error, "Failed to send desktop notification");
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
