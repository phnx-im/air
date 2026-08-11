// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircoreclient::ChatId;
use flutter_rust_bridge::frb;
use tokio::sync::watch;

use crate::notifications::NotificationService;

use super::notifications::DartNotificationService;

/// What to do with OS notifications while the UI shows what it shows.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[frb(dart_metadata = ("freezed"))]
pub enum NotificationPolicy {
    /// Do not show any notifications
    #[default]
    SuppressAll,
    /// Show all notifications.
    AllowAll,
    /// Show all notifications, except for those from the given chat.
    SuppressChat { chat_id: ChatId },
}

/// Carries the UI's [`NotificationPolicy`] to the notification path.
#[frb(opaque)]
pub struct NotificationContextBase {
    policy_tx: watch::Sender<NotificationPolicy>,
    pub(crate) notification_service: NotificationService,
}

impl NotificationContextBase {
    #[frb(sync)]
    pub fn new(notification_service: &DartNotificationService) -> Self {
        let (policy_tx, _) = watch::channel(NotificationPolicy::default());
        Self {
            policy_tx,
            notification_service: NotificationService::new(notification_service.clone()),
        }
    }

    /// Records the policy the UI moved to.
    #[frb(sync)]
    pub fn set_policy(&self, policy: NotificationPolicy) {
        self.policy_tx.send_if_modified(|current| {
            let changed = *current != policy;
            *current = policy;
            changed
        });
    }

    /// Clears the notifications a chat has already posted, for when it opens.
    pub async fn chat_opened(&self, chat_id: ChatId) {
        self.notification_service
            .cancel_chat_notifications([chat_id])
            .await;
    }

    #[frb(ignore)]
    pub(crate) fn subscribe(&self) -> watch::Receiver<NotificationPolicy> {
        self.policy_tx.subscribe()
    }
}
