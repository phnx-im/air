// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::sync::Arc;

use aircommon::time::TimeStamp;
use aircoreclient::clients::{
    ListenResponse, listen_response,
    process::{process_qs::ProcessedQsMessages, qs_stream::QsProcessEventResult},
};
use airprotos::queue_service;
use chrono::Utc;
use flutter_rust_bridge::frb;
use tokio_stream::{Stream, StreamExt};
use tokio_util::sync::CancellationToken;
use tonic::Code;
use tracing::{error, warn};

use crate::{
    api::{user::User, user_cubit::VersionStatus},
    util::{BackgroundStreamContext, BackgroundStreamTask},
};

use super::{AppState, CubitContext, UiUser};

#[derive(Debug)]
#[frb(ignore)]
pub(super) struct QueueContext {
    cubit_context: CubitContext,
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
        }: ProcessedQsMessages,
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
            )
            .await;
        notifications.extend(chat_notifications.additions);
        user.new_connection_request_notifications(&new_connections, &mut notifications)
            .await;
        self.show_notifications(notifications).await;

        if !chat_notifications.empty_chats.is_empty() {
            self.notification_service
                .cancel_chat_notifications(chat_notifications.empty_chats)
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
        // Update the version status communicated by the server. Note that the server can also clear
        // the state.
        if let ListenResponse {
            event:
                Some(listen_response::Event::VersionStatus(queue_service::v1::VersionStatus {
                    expires_at,
                })),
        } = &event
        {
            let expires_at = expires_at.map(|t| TimeStamp::from(t).into());
            self.cubit_context.state_tx.send_modify(|state| {
                let inner = Arc::make_mut(&mut state.inner);
                inner.version_status = match expires_at {
                    Some(expires_at) if expires_at < Utc::now() => {
                        VersionStatus::ExpiresAt(expires_at)
                    }
                    Some(_) => VersionStatus::Unsupported,
                    None => VersionStatus::Supported,
                };
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
                    .show_notifications_for_processed_qs_messages(processed)
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
        Self { cubit_context }
    }

    pub(super) fn into_task(
        self,
        cancel: CancellationToken,
    ) -> BackgroundStreamTask<Self, ListenResponse> {
        BackgroundStreamTask::new("qs", self, cancel)
    }
}
