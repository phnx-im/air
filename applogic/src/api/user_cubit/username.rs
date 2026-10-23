// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{
    collections::{HashMap, HashSet},
    convert::identity,
    sync::Arc,
};

use aircommon::{identifiers::Username, time::TimeStamp};
use aircoreclient::{
    UsernameRecord,
    clients::{
        AsListenUsernameResponder, UsernameQueueMessage,
        process::process_as::keep_failed_username_message,
    },
    db::notification::DbEntityId,
};
use anyhow::{Context, bail};
use flutter_rust_bridge::frb;
use tokio::sync::{RwLock, watch};
use tokio_stream::{Stream, StreamExt};
use tokio_util::sync::{CancellationToken, DropGuard};
use tracing::{debug, error};
use uuid::Uuid;

use crate::{
    api::{user::User, user_cubit::VersionStatus},
    util::{BackgroundStreamContext, BackgroundStreamTask, spawn_from_sync},
};

use super::{AppState, CubitContext};

/// The context of the background task that listens to a username.
#[derive(Debug, Clone)]
#[frb(ignore)]
pub(super) struct UsernameContext {
    cubit_context: CubitContext,
    username_record: Arc<UsernameRecord>,
    responder: Arc<RwLock<Option<AsListenUsernameResponder>>>,
}

impl UsernameContext {
    pub(super) fn new(cubit_context: CubitContext, username_record: UsernameRecord) -> Self {
        Self {
            cubit_context,
            username_record: Arc::new(username_record),
            responder: Default::default(),
        }
    }

    /// Spawns a task to keep the username listeners and the UI state in line
    /// with the stored username records. When changes are made to usernames,
    /// the listeners are restarted and the UI state is updated.
    pub(super) fn spawn_sync(
        cubit_context: CubitContext,
        parent_cancel: CancellationToken,
    ) -> UsernameBackgroundTasks {
        let username_background_tasks = UsernameBackgroundTasks::default();
        let tasks = username_background_tasks.clone();
        spawn_from_sync(async move {
            let mut notifications = cubit_context.core_user.db_notifications();
            Self::reconcile(&cubit_context, &tasks, &parent_cancel, &HashSet::new()).await;
            loop {
                let notification = tokio::select! {
                    _ = parent_cancel.cancelled() => return,
                    notification = notifications.next() => match notification {
                        Some(notification) => notification,
                        None => return,
                    },
                };
                let changed: HashSet<&Username> = notification
                    .ops
                    .keys()
                    .filter_map(|entity_id| match entity_id {
                        DbEntityId::Username(username) => Some(username),
                        _ => None,
                    })
                    .collect();
                if !changed.is_empty() {
                    Self::reconcile(&cubit_context, &tasks, &parent_cancel, &changed).await;
                }
            }
        });
        username_background_tasks
    }

    async fn reconcile(
        cubit_context: &CubitContext,
        tasks: &UsernameBackgroundTasks,
        parent_cancel: &CancellationToken,
        changed: &HashSet<&Username>,
    ) {
        let records = match cubit_context.core_user.username_records().await {
            Ok(records) => records,
            Err(error) => {
                error!(%error, "failed to load username records; won't listen to usernames");
                return;
            }
        };

        let usernames: Vec<Username> = records
            .iter()
            .map(|record| record.username.clone())
            .collect();
        cubit_context.state_tx.send_if_modified(|state| {
            if state.inner.usernames == usernames {
                return false;
            }
            let inner = Arc::make_mut(&mut state.inner);
            inner.usernames = usernames.clone();
            true
        });

        tasks.retain(|username| usernames.contains(username));
        for record in records {
            let restart = changed.contains(&record.username);
            if restart || !tasks.contains(&record.username) {
                Self::new(cubit_context.clone(), record)
                    .into_task(parent_cancel.child_token(), tasks)
                    .spawn();
            }
        }
    }

    pub(super) fn into_task(
        self,
        cancel: CancellationToken,
        background_tasks: &UsernameBackgroundTasks,
    ) -> BackgroundStreamTask<Self, UsernameQueueMessage> {
        let username = self.username_record.username.clone();
        let name = format!("username-{}", username.truncated_plaintext());
        background_tasks.insert(username, cancel.clone());
        BackgroundStreamTask::new(name, self, cancel)
    }

    async fn ack(&self, message_id: Option<Uuid>) {
        if let Err(error) = self.try_ack(message_id).await {
            error!(%error, "failed to ack username queue message");
        }
    }

    async fn try_ack(&self, message_id: Option<Uuid>) -> anyhow::Result<()> {
        let message_id = message_id.context("no message id in username queue message")?;
        let response = self.responder.read().await;
        let Some(responder) = response.as_ref() else {
            bail!("logic error: no username queue responder");
        };
        debug!(?message_id, "acking username queue message");
        responder.ack(message_id).await;
        Ok(())
    }
}

impl BackgroundStreamContext<UsernameQueueMessage> for UsernameContext {
    async fn in_foreground(&self) {
        let _ = self
            .cubit_context
            .app_state
            .clone()
            .wait_for(|app_state| {
                matches!(
                    app_state,
                    AppState::Foreground | AppState::DesktopBackground
                )
            })
            .await;
    }

    async fn in_background(&self) {
        let _ = self
            .cubit_context
            .app_state
            .clone()
            .wait_for(|app_state| matches!(app_state, AppState::MobileBackground))
            .await;
    }

    async fn create_stream(
        &mut self,
    ) -> anyhow::Result<impl Stream<Item = UsernameQueueMessage> + 'static> {
        let (stream, responder) = match self
            .cubit_context
            .core_user
            .listen_username(&self.username_record)
            .await
        {
            Ok(stream) => {
                self.cubit_context.state_tx.send_if_modified(|state| {
                    if let VersionStatus::Supported = state.inner.version_status {
                        return false;
                    }
                    let inner = Arc::make_mut(&mut state.inner);
                    inner.version_status = VersionStatus::Supported;
                    true
                });
                stream
            }
            Err(error) if error.is_unsupported_version() => {
                self.cubit_context.state_tx.send_if_modified(|state| {
                    if let VersionStatus::Unsupported = state.inner.version_status {
                        return false;
                    }
                    let inner = Arc::make_mut(&mut state.inner);
                    inner.version_status = VersionStatus::Unsupported;
                    true
                });
                return Err(error.into());
            }
            Err(error) => return Err(error.into()),
        };
        self.responder.write().await.replace(responder);
        Ok(stream.filter_map(identity))
    }

    async fn handle_event(&mut self, message: UsernameQueueMessage) -> bool {
        let message_id = message.message_id.map(From::from);
        let created_at = message.created_at.map(TimeStamp::from);
        match self
            .cubit_context
            .core_user
            .process_username_queue_message(self.username_record.username.clone(), message)
            .await
        {
            Ok(Some(stored)) => {
                let user = User::from_core_user(self.cubit_context.core_user.clone());
                let mut notifications = Vec::with_capacity(1);
                user.new_connection_request_notifications(&[stored.chat_id], &mut notifications)
                    .await;
                self.cubit_context.show_notifications(notifications).await;
                if let Some(moved_from) = stored.moved_from {
                    self.cubit_context
                        .notification_service
                        .cancel_chat_notifications(vec![moved_from])
                        .await;
                }
            }
            Ok(None) => {}
            Err(error) => {
                error!(?error, "failed to process username queue message");
                if keep_failed_username_message(&error, created_at) {
                    return true; // keep the message in the queue
                }
            }
        }
        self.ack(message_id).await;
        true // continue
    }
}

/// Tracks the background tasks listening to usernames.
#[derive(Debug, Clone)]
#[frb(ignore)]
pub(super) struct UsernameBackgroundTasks {
    tx: watch::Sender<HashMap<Username, DropGuard>>,
}

impl UsernameBackgroundTasks {
    pub(super) fn new() -> Self {
        Self {
            tx: watch::channel(Default::default()).0,
        }
    }

    pub(super) fn insert(&self, username: Username, cancel: CancellationToken) {
        self.tx.send_modify(|usernames| {
            usernames.insert(username, cancel.drop_guard());
        });
    }

    fn contains(&self, username: &Username) -> bool {
        self.tx.borrow().contains_key(username)
    }

    fn retain(&self, mut keep: impl FnMut(&Username) -> bool) {
        self.tx.send_modify(|usernames| {
            usernames.retain(|username, _| keep(username));
        });
    }
}

impl Default for UsernameBackgroundTasks {
    fn default() -> Self {
        Self::new()
    }
}
