// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::sync::Arc;

use aircommon::messages::QueueMessage;
use aircoreclient::clients::{
    QsListenResponder, QueueEvent, process::process_qs::ProcessedQsMessages, queue_event,
};
use flutter_rust_bridge::frb;
use tokio_stream::Stream;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, warn};

use crate::{
    api::user::User,
    util::{BackgroundStreamContext, BackgroundStreamTask},
};

use super::{AppState, CubitContext};

#[derive(Debug)]
#[frb(ignore)]
pub(super) struct QueueContext {
    cubit_context: CubitContext,
    responder: Option<Arc<QsListenResponder>>,
    /// Accumulated but not yet processed messages
    ///
    /// Note: It is safe to keep messages in memory here, because they are not yet acked. In case,
    /// the app is shut down, the messages will be received again.
    messages: Vec<QueueMessage>,
}

impl BackgroundStreamContext<QueueEvent> for QueueContext {
    async fn create_stream(&mut self) -> anyhow::Result<impl Stream<Item = QueueEvent> + 'static> {
        let (stream, responder) = self.cubit_context.core_user.listen_queue().await?;
        self.responder.replace(Arc::new(responder));
        Ok(stream)
    }

    async fn handle_event(&mut self, event: QueueEvent) {
        debug!(?event, "handling QS listen event");
        match event.event {
            Some(queue_event::Event::Payload(_)) => {
                // currently, we don't handle payload events
                warn!("ignoring QS listen payload event")
            }
            Some(queue_event::Event::Message(message)) => {
                let message = match message.try_into() {
                    Ok(message) => message,
                    Err(error) => {
                        error!(%error, "failed to convert QS message; dropping");
                        return;
                    }
                };
                // Invariant: after a message there is always an Empty event as sentinel
                // => accumelated messages will be processed there
                self.messages.push(message);
            }
            // Empty event indicates that the queue is empty
            Some(queue_event::Event::Empty(_)) => {
                if self.messages.is_empty() {
                    return; // no messages to process
                }

                // Invariant: messages are sorted by sequence number
                if let Some(max_sequence_number) = self.messages.last().map(|m| m.sequence_number) {
                    // we received some messages, so we can ack them
                    let responder = self
                        .responder
                        .as_ref()
                        .expect("logic error: no responder")
                        .clone();
                    responder
                        .ack(max_sequence_number + 1)
                        .await
                        .inspect_err(|error| {
                            error!(%error, "failed to ack QS messages");
                        })
                        .ok();
                }

                let core_user = self.cubit_context.core_user.clone();
                let user = User::from_core_user(core_user);

                // Invariant: messages are sorted by sequence number
                let max_sequence_number = self.messages.last().map(|m| m.sequence_number);

                let messages = std::mem::take(&mut self.messages);
                match user.user.fully_process_qs_messages(messages).await {
                    Ok(ProcessedQsMessages {
                        new_chats,
                        changed_chats: _,
                        new_messages,
                        errors: _,
                        rejoined_chats: _,
                    }) => {
                        let mut notifications =
                            Vec::with_capacity(new_chats.len() + new_messages.len());
                        user.new_chat_notifications(&new_chats, &mut notifications)
                            .await;
                        user.new_message_notifications(&new_messages, &mut notifications)
                            .await;
                        self.cubit_context.show_notifications(notifications).await;
                    }
                    Err(error) => {
                        error!(%error, "failed to process QS message");
                    }
                }

                if let Some(max_sequence_number) = max_sequence_number {
                    // We received some messages, so we can ack them *after* they were fully
                    // processed. In particular, the queue ratchet sequence number was written back
                    // into the database.
                    let responder = self
                        .responder
                        .as_ref()
                        .expect("logic error: no responder")
                        .clone();
                    responder
                        .ack(max_sequence_number + 1)
                        .await
                        .inspect_err(|error| {
                            error!(%error, "failed to ack QS messages");
                        })
                        .ok();
                }
            }
            None => {}
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
            responder: None,
            messages: Vec::new(),
        }
    }

    pub(super) fn into_task(
        self,
        cancel: CancellationToken,
    ) -> BackgroundStreamTask<Self, QueueEvent> {
        BackgroundStreamTask::new("qs", self, cancel)
    }
}
