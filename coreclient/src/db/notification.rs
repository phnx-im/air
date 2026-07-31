// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{collections::BTreeMap, mem, sync::Arc};

use aircommon::identifiers::UserId;
use enumset::{EnumSet, EnumSetType};
use tokio::sync::broadcast;
use tokio_stream::wrappers::{BroadcastStream, errors::BroadcastStreamRecvError};
use tokio_stream::{Stream, StreamExt};
use tracing::{debug, error, warn};

use crate::{AttachmentId, ChatId, MessageId};

// 1024 * size_of::<Arc<DbNotification>>() = 1024 * 8 = 8 KiB
const NOTIFICATION_CHANNEL_SIZE: usize = 1024;

/// Bundles a notification sender and a notification.
///
/// Used to collect all notifications and eventually send them all at once.
#[derive(Debug)]
pub(crate) struct DbNotifier {
    tx: DbNotificationsSender,
    notification: DbNotification,
}

impl DbNotifier {
    /// Creates a new notifier which will send all notifications with the given sender.
    pub(crate) fn new(tx: DbNotificationsSender) -> Self {
        Self {
            tx,
            notification: DbNotification::empty(),
        }
    }

    /// Add a new entity to the notification.
    ///
    /// Notification will be sent when the `notify` function is called.
    pub(crate) fn add(&mut self, id: impl Into<DbEntityId>) -> &mut Self {
        self.notification
            .ops
            .entry(id.into())
            .or_default()
            .insert(DbOperation::Add);
        self
    }

    /// Update an existing entity in the notification.
    ///
    /// Notification will be sent when the `notify` function is called.
    pub(crate) fn update(&mut self, id: impl Into<DbEntityId>) -> &mut Self {
        self.notification
            .ops
            .entry(id.into())
            .or_default()
            .insert(DbOperation::Update);
        self
    }

    /// Remove an existing entity from the notification.
    ///
    /// Notification will be sent when the `notify` function is called.
    pub(crate) fn remove(&mut self, id: impl Into<DbEntityId>) -> &mut Self {
        self.notification
            .ops
            .entry(id.into())
            .or_default()
            .insert(DbOperation::Remove);
        self
    }

    /// Send collected notifications to the subscribers, if there are any.
    pub(crate) fn notify(mut self) {
        if !self.notification.ops.is_empty() {
            let notification = mem::take(&mut self.notification);
            self.tx.notify(Arc::new(notification));
        }
    }

    /// Clears accumulated notifications
    pub(crate) fn clear(&mut self) {
        self.notification.clear();
    }
}

impl Drop for DbNotifier {
    fn drop(&mut self) {
        if !self.notification.ops.is_empty() {
            // Note: This might be ok. E.g. an error might happen after some notifications were
            // added to the notifier.
            warn!(
                "DbNotifier dropped with notifications; \
                    did you forget to call notify()? notifications = {:?}",
                self.notification
            );
        }
    }
}

/// A channel for sending or subscribing to notifications
#[derive(Debug, Clone)]
pub(crate) struct DbNotificationsSender {
    tx: broadcast::Sender<Arc<DbNotification>>,
}

impl DbNotificationsSender {
    /// Create a new notification sender without any subscribers.
    pub(crate) fn new() -> Self {
        let (tx, _) = broadcast::channel(NOTIFICATION_CHANNEL_SIZE);
        Self { tx }
    }

    /// Sends a notification to all current subscribers.
    pub(crate) fn notify(&self, notification: impl Into<Arc<DbNotification>>) {
        let notification = notification.into();
        debug!(
            num_receivers = self.tx.receiver_count(),
            ?notification,
            "DbNotificationsSender::notify"
        );
        let _no_receivers = self.tx.send(notification);
    }

    /// Creates a new subscription to the notifications.
    ///
    /// The stream will contain all notifications from the moment this function is called.
    pub(crate) fn subscribe(&self) -> impl Stream<Item = Arc<DbNotification>> + 'static {
        BroadcastStream::new(self.tx.subscribe()).filter_map(|res| match res {
            Ok(notification) => Some(notification),
            Err(BroadcastStreamRecvError::Lagged(n)) => {
                error!(n, "DB notifications lagged");
                None
            }
        })
    }

    /// Returns all pending notifications.
    ///
    /// The pending notifications are the notifications captured starting at the call to this function.
    /// Getting the next item from the iterator gets the next pending notification is there is any,
    /// otherwise it returns `None`. Therefore, the iterator is not fused.
    ///
    /// This is useful for capturing all pending notifications synchronously.
    pub(crate) fn subscribe_iter(
        &self,
    ) -> impl Iterator<Item = Arc<DbNotification>> + Send + 'static {
        let mut rx = self.tx.subscribe();
        std::iter::from_fn(move || {
            loop {
                match rx.try_recv() {
                    Ok(notification) => return Some(notification),
                    Err(broadcast::error::TryRecvError::Lagged(n)) => {
                        error!(n, "DB notifications lagged");
                        continue;
                    }
                    Err(
                        broadcast::error::TryRecvError::Closed
                        | broadcast::error::TryRecvError::Empty,
                    ) => return None,
                }
            }
        })
    }
}

impl Default for DbNotificationsSender {
    fn default() -> Self {
        Self::new()
    }
}

/// A notification bundle about database changes.
///
/// Bundles all changes, that is, all entities that have been added, updated or removed.
#[derive(Debug, Default)]
#[cfg_attr(test, derive(PartialEq, Eq))]
pub struct DbNotification {
    pub ops: BTreeMap<DbEntityId, EnumSet<DbOperation>>,
}

impl DbNotification {
    fn empty() -> Self {
        Self::default()
    }

    pub fn is_empty(&self) -> bool {
        self.ops.is_empty()
    }

    fn clear(&mut self) {
        self.ops.clear();
    }
}

/// Operation which was performed in the database.
#[derive(Debug, PartialOrd, Ord, Hash, EnumSetType)]
pub enum DbOperation {
    Add,
    Update,
    Remove,
}

/// Identifier of an entity stored in the database.
///
/// Used to identify added, updated or removed entities in a [`DbNotification`].
// Note(perf): I would prefer this type to be copy and smaller in memory (currently 40 bytes), but
// `UserId` is not copy and quite large.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, derive_more::From)]
pub enum DbEntityId {
    User(UserId),
    Chat(ChatId),
    Message(MessageId),
    Attachment(AttachmentId),
    /// A synchronized user setting, identified by its setting key.
    UserSetting(String),
}

impl DbEntityId {
    pub(crate) fn kind(&self) -> DbEntityKind {
        match self {
            DbEntityId::User(_) => DbEntityKind::User,
            DbEntityId::Chat(_) => DbEntityKind::Chat,
            DbEntityId::Message(_) => DbEntityKind::Message,
            DbEntityId::Attachment(_) => DbEntityKind::Attachment,
            DbEntityId::UserSetting(_) => DbEntityKind::UserSetting,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum DbEntityKind {
    User = 0,
    Chat = 1,
    Message = 2,
    Attachment = 3,
    UserSetting = 4,
}

#[derive(Debug, thiserror::Error)]
#[error("Invalid DB entity kind: {0}")]
pub(crate) struct InvalidDbEntityKind(i64);

impl TryFrom<i64> for DbEntityKind {
    type Error = InvalidDbEntityKind;

    fn try_from(value: i64) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(DbEntityKind::User),
            1 => Ok(DbEntityKind::Chat),
            2 => Ok(DbEntityKind::Message),
            3 => Ok(DbEntityKind::Attachment),
            4 => Ok(DbEntityKind::UserSetting),
            _ => Err(InvalidDbEntityKind(value)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subscribe_iter() {
        let tx = DbNotificationsSender::new();

        let ops_1: BTreeMap<DbEntityId, EnumSet<DbOperation>> = [(
            DbEntityId::User(UserId::random("localhost".parse().unwrap())),
            DbOperation::Add.into(),
        )]
        .into_iter()
        .collect();

        let ops_2: BTreeMap<DbEntityId, EnumSet<DbOperation>> = [(
            DbEntityId::User(UserId::random("localhost".parse().unwrap())),
            DbOperation::Update.into(),
        )]
        .into_iter()
        .collect();

        let ops_3: BTreeMap<DbEntityId, EnumSet<DbOperation>> = [(
            DbEntityId::User(UserId::random("localhost".parse().unwrap())),
            DbOperation::Remove.into(),
        )]
        .into_iter()
        .collect();

        let ops_4: BTreeMap<DbEntityId, EnumSet<DbOperation>> = [(
            DbEntityId::User(UserId::random("localhost".parse().unwrap())),
            DbOperation::Add.into(),
        )]
        .into_iter()
        .collect();

        tx.notify(DbNotification {
            ops: ops_1.into_iter().collect(),
        });

        let mut iter = tx.subscribe_iter();

        tx.notify(DbNotification { ops: ops_2.clone() });

        // first notification is not observed, because it was sent before the subscription
        assert_eq!(iter.next().unwrap().ops, ops_2);
        assert_eq!(iter.next(), None);

        tx.notify(DbNotification { ops: ops_3.clone() });
        assert_eq!(iter.next().unwrap().ops, ops_3);
        tx.notify(DbNotification { ops: ops_4.clone() });
        assert_eq!(iter.next().unwrap().ops, ops_4);
        assert_eq!(iter.next(), None);
    }
}
