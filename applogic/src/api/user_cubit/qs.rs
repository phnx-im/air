// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircoreclient::clients::{
    QueueEvent,
    process::process_qs::{ProcessedQsMessages, QsNotificationProcessor, QsStreamProcessor},
};
use flutter_rust_bridge::frb;
use tokio_stream::Stream;
use tokio_util::sync::CancellationToken;

use crate::{
    api::user::User,
    util::{BackgroundStreamContext, BackgroundStreamTask},
};

use super::{AppState, CubitContext};

#[derive(Debug)]
#[frb(ignore)]
pub(super) struct QueueContext {
    cubit_context: CubitContext,
    handler: QsStreamProcessor,
}

impl QsNotificationProcessor for CubitContext {
    async fn show_notifications(
        &mut self,
        ProcessedQsMessages {
            new_chats,
            changed_chats: _,
            new_messages,
            errors: _,
            processed: _,
            new_connections,
        }: ProcessedQsMessages,
    ) {
        let mut notifications = Vec::with_capacity(new_chats.len() + new_messages.len());
        let user = User::from_core_user(self.core_user.clone());
        user.new_chat_notifications(&new_chats, &mut notifications)
            .await;
        user.new_message_notifications(&new_messages, &mut notifications)
            .await;
        user.new_connection_request_notifications(&new_connections, &mut notifications)
            .await;
        CubitContext::show_notifications(self, notifications).await;
    }
}

impl BackgroundStreamContext<QueueEvent> for QueueContext {
    async fn create_stream(&mut self) -> anyhow::Result<impl Stream<Item = QueueEvent> + 'static> {
        let (stream, responder) = self.cubit_context.core_user.listen_queue().await?;
        self.handler.replace_responder(responder);
        Ok(stream)
    }

    async fn handle_event(&mut self, event: QueueEvent) -> bool {
        let result = Box::pin(self.handler.process_event(event, &mut self.cubit_context)).await;
        // Stop stream if partially processed
        // => There is a hole in the sequence of the messages, therefore we cannot continue
        // processing them.
        !result.is_partially_processed()
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
        let handler = QsStreamProcessor::new(cubit_context.core_user.clone());
        Self {
            handler,
            cubit_context,
        }
    }

    pub(super) fn into_task(
        self,
        cancel: CancellationToken,
    ) -> BackgroundStreamTask<Self, QueueEvent> {
        BackgroundStreamTask::new("qs", self, cancel)
    }
}
