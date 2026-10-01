// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::sync::Arc;

use aircommon::messages::client_state::Suppression;
use aircoreclient::clients::{
    CoreUser, ListenResponse, QsListenResponder, SiblingClientStates, listen_response,
    process::{process_qs::ProcessedQsMessages, qs_stream::QsProcessEventResult},
};
use airprotos::queue_service;
use chrono::{DateTime, Utc};
use flutter_rust_bridge::frb;
use tokio::sync::watch;
use tokio_stream::{Stream, StreamExt};
use tokio_util::sync::CancellationToken;
use tonic::Code;
use tracing::{debug, error, warn};

use crate::{
    api::{notification_context::NotificationPolicy, user::User, user_cubit::VersionStatus},
    util::{BackgroundStreamContext, BackgroundStreamTask, spawn_from_sync},
};

use super::{AppState, CubitContext, UiUser};

#[derive(Debug)]
#[frb(ignore)]
pub(super) struct QueueContext {
    cubit_context: CubitContext,
    /// Stops publishing the client state over the current stream
    stop_client_state: Option<CancellationToken>,
    /// Sibling client states received over the current stream
    sibling_client_states: Option<SiblingClientStates>,
}

/// The notifications this client suppresses because the user is looking at it.
fn suppression(app_state: AppState, policy: NotificationPolicy) -> Suppression {
    match (app_state, policy) {
        (AppState::Foreground, NotificationPolicy::SuppressChat { chat_id }) => {
            Suppression::Chat(chat_id.uuid())
        }
        (AppState::Foreground, NotificationPolicy::SuppressAll) => Suppression::All,
        _ => Suppression::None,
    }
}

/// Reports the client state to the siblings whenever the suppression changes.
///
/// The state is also reported when nothing is suppressed, so that the QS cannot
/// tell the cases apart.
async fn report_client_state(
    core_user: CoreUser,
    responder: QsListenResponder,
    mut app_state: watch::Receiver<AppState>,
    mut policy: watch::Receiver<NotificationPolicy>,
) {
    loop {
        // Report the current state
        let reported = suppression(*app_state.borrow_and_update(), *policy.borrow_and_update());
        match core_user.encrypt_client_state(reported) {
            Ok(encrypted) => responder.report_client_state(encrypted).await,
            Err(error) => error!(%error, "failed to encrypt client state"),
        }

        // Wait for an app state or policy change, discard
        // irrelevant updates, then go back to waiting
        loop {
            tokio::select! {
                changed = app_state.changed() => if changed.is_err() {
                    return;
                },
                changed = policy.changed() => if changed.is_err() {
                    return;
                },
            }
            // Stop waiting if the suppression changed
            if suppression(*app_state.borrow(), *policy.borrow()) != reported {
                break;
            }
        }
    }
}

impl CubitContext {
    async fn show_notifications_for_processed_qs_messages(
        &self,
        ProcessedQsMessages {
            new_chats,
            new_messages,
            errors: _,
            processed: _,
            new_connections,
            reaction_notifications,
            chats_with_changed_notifications,
            removed_chats,
        }: ProcessedQsMessages,
        siblings: Option<&SiblingClientStates>,
    ) {
        let mut notifications = Vec::with_capacity(new_chats.len() + new_messages.len());
        let user = User::from_core_user(self.core_user.clone());
        user.new_chat_notifications(&new_chats, &mut notifications)
            .await;
        let chat_notifications = user
            .message_and_reaction_notifications(
                &new_messages,
                &reaction_notifications,
                &chats_with_changed_notifications,
                siblings,
            )
            .await;
        notifications.extend(chat_notifications.additions);
        user.new_connection_request_notifications(&new_connections, &mut notifications)
            .await;
        self.show_notifications(notifications).await;

        let mut stale_chats = chat_notifications.empty_chats;
        stale_chats.extend(removed_chats);
        if !stale_chats.is_empty() {
            self.notification_service
                .cancel_chat_notifications(stale_chats)
                .await;
        }
    }
}

impl BackgroundStreamContext<ListenResponse> for QueueContext {
    async fn create_stream(
        &mut self,
    ) -> anyhow::Result<impl Stream<Item = ListenResponse> + 'static> {
        let (stream, responder) = match self.cubit_context.core_user.listen_queue().await {
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
        self.sibling_client_states = Some(self.cubit_context.core_user.sibling_client_states()?);
        self.spawn_report_client_state(responder.clone());
        self.cubit_context
            .core_user
            .replace_qs_listen_responder(responder)
            .await;
        // The live listen treats any terminal status as stream end.
        // Reconnecting is up to the background stream task.
        Ok(stream.map_while(|result| {
            result
                .inspect_err(|error| {
                    match error.code() {
                        // Server shut down or stream was evicted
                        Code::Unavailable | Code::Aborted => {
                            warn!(%error, "qs listen stream closed");
                        }
                        _ => error!(%error, "qs listen stream closed"),
                    }
                })
                .ok()
        }))
    }

    async fn handle_event(&mut self, event: ListenResponse) -> bool {
        let event = match event {
            ListenResponse {
                event: Some(listen_response::Event::SiblingClientState(state)),
            } => {
                if let Some(states) = &mut self.sibling_client_states {
                    states.apply(state);
                }
                return true;
            }
            event => event,
        };

        // Update the version status communicated by the server. Note that the server can also clear
        // the status.
        if let ListenResponse {
            event:
                Some(listen_response::Event::VersionStatus(queue_service::v1::VersionStatus {
                    expires_at,
                    max_devices,
                })),
        } = &event
        {
            debug!(expires_at = ?expires_at, "Received version status");
            // Whole seconds: the dismissed expiry is persisted as such and has to compare equal to
            // the announced one afterwards.
            let expires_at = expires_at.and_then(|t| DateTime::from_timestamp(t.seconds, 0));
            self.cubit_context.state_tx.send_modify(|state| {
                let inner = Arc::make_mut(&mut state.inner);
                inner.version_status = match expires_at {
                    Some(expires_at) if Utc::now() < expires_at => {
                        VersionStatus::ExpiresAt(expires_at)
                    }
                    Some(_) => VersionStatus::Unsupported,
                    None => VersionStatus::Supported,
                };
                inner.max_devices = *max_devices;
            });
        }

        let result = match self.cubit_context.core_user.process_qs_event(event).await {
            Ok(result) => result,
            Err(error) => {
                error!(%error, "Failed to process QS event");
                return false;
            }
        };

        let is_partially_processed = result.is_partially_processed();
        match result {
            QsProcessEventResult::FullyProcessed { processed }
            | QsProcessEventResult::PartiallyProcessed { processed, .. } => {
                self.cubit_context
                    .show_notifications_for_processed_qs_messages(
                        processed,
                        self.sibling_client_states.as_ref(),
                    )
                    .await;
                // A commit in this batch may have removed this device from the
                // self group, which the app has to act on.
                UiUser::reload_account_unlinked(
                    &self.cubit_context.state_tx,
                    &self.cubit_context.core_user,
                )
                .await;
            }
            QsProcessEventResult::Accumulated | QsProcessEventResult::Ignored => (),
        };

        // Stop stream if partially processed
        // => There is a hole in the sequence of the messages, therefore we cannot continue
        // processing them.
        !is_partially_processed
    }

    async fn on_stream_end(&mut self) {
        if let Some(stop) = self.stop_client_state.take() {
            stop.cancel();
        }
    }

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
}

impl QueueContext {
    pub(super) fn new(cubit_context: CubitContext) -> Self {
        Self {
            cubit_context,
            stop_client_state: None,
            sibling_client_states: None,
        }
    }

    fn spawn_report_client_state(&mut self, responder: QsListenResponder) {
        let stop = CancellationToken::new();
        if let Some(previous) = self.stop_client_state.replace(stop.clone()) {
            previous.cancel();
        }
        spawn_from_sync(stop.run_until_cancelled_owned(report_client_state(
            self.cubit_context.core_user.clone(),
            responder,
            self.cubit_context.app_state.clone(),
            self.cubit_context.notification_policy.clone(),
        )));
    }

    pub(super) fn into_task(
        self,
        cancel: CancellationToken,
    ) -> BackgroundStreamTask<Self, ListenResponse> {
        BackgroundStreamTask::new("qs", self, cancel)
    }
}
