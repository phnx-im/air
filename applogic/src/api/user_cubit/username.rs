// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{collections::HashMap, convert::identity, sync::Arc};

use aircommon::identifiers::Username;
use aircoreclient::{
    UsernameRecord,
    clients::{AsListenUsernameResponder, HandleQueueMessage},
    store::Store,
};
use anyhow::{Context, bail};
use flutter_rust_bridge::frb;
use tokio::sync::{RwLock, watch};
use tokio_stream::{Stream, StreamExt};
use tokio_util::sync::{CancellationToken, DropGuard};
use tracing::{debug, error};
use uuid::Uuid;

use crate::{
    api::user::User,
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

    /// Spawns a task that loads all username records in the background and spawns a new listen
    /// username background task for each record.
    pub(super) fn spawn_loading(
        cubit_context: CubitContext,
        parent_cancel: CancellationToken,
    ) -> UsernameBackgroundTasks {
        let username_background_tasks = UsernameBackgroundTasks::default();
        let tasks_inner = username_background_tasks.clone();
        spawn_from_sync(async move {
            let records = match cubit_context.core_user.username_records().await {
                Ok(records) => records,
                Err(error) => {
                    error!(%error, "failed to load username records; won't listen to usernames");
                    return;
                }
            };
            for record in records {
                Self::new(cubit_context.clone(), record)
                    .into_task(parent_cancel.child_token(), &tasks_inner)
                    .spawn();
            }
        });
        username_background_tasks
    }

    pub(super) fn into_task(
        self,
        cancel: CancellationToken,
        background_tasks: &UsernameBackgroundTasks,
    ) -> BackgroundStreamTask<Self, HandleQueueMessage> {
        let username = self.username_record.username.clone();
        let (prefix, suffix_len) = username
            .plaintext()
            .split_at_checked(2)
            .map(|(prefix, suffix)| (prefix, suffix.len()))
            .unwrap_or(("unknown", 0));
        let name = format!("username-{prefix}<..{suffix_len}>");
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

impl BackgroundStreamContext<HandleQueueMessage> for UsernameContext {
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
    ) -> anyhow::Result<impl Stream<Item = HandleQueueMessage> + 'static> {
        let (stream, responder) = match self
            .cubit_context
            .core_user
            .listen_username(&self.username_record)
            .await
        {
            Ok(stream) => {
                self.cubit_context.state_tx.send_if_modified(|state| {
                    if !state.inner.unsupported_version {
                        return false;
                    }
                    let inner = Arc::make_mut(&mut state.inner);
                    inner.unsupported_version = false;
                    true
                });
                stream
            }
            Err(error) if error.is_unsupported_version() => {
                self.cubit_context.state_tx.send_if_modified(|state| {
                    if state.inner.unsupported_version {
                        return false;
                    }
                    let inner = Arc::make_mut(&mut state.inner);
                    inner.unsupported_version = true;
                    true
                });
                return Err(error.into());
            }
            Err(error) => return Err(error.into()),
        };
        self.responder.write().await.replace(responder);
        Ok(stream.filter_map(identity))
    }

    async fn handle_event(&mut self, message: HandleQueueMessage) -> bool {
        let message_id = message.message_id.map(From::from);
        match self
            .cubit_context
            .core_user
            .process_username_queue_message(self.username_record.username.clone(), message)
            .await
        {
            Ok(chat_id) => {
                let user = User::from_core_user(self.cubit_context.core_user.clone());
                let mut notifications = Vec::with_capacity(1);
                user.new_connection_request_notifications(&[chat_id], &mut notifications)
                    .await;
                self.cubit_context.show_notifications(notifications).await;
            }
            Err(error) => {
                error!(?error, "failed to process username queue message");
            }
        }
        // ack the message independently of the result of processing the message
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

    pub(super) fn remove(&self, username: Username) {
        self.tx.send_modify(|usernames| {
            usernames.remove(&username);
        });
    }
}

impl Default for UsernameBackgroundTasks {
    fn default() -> Self {
        Self::new()
    }
}
