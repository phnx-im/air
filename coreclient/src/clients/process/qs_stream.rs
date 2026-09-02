// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::messages::QueueMessage;
use airprotos::queue_service::v1::{ListenResponse, listen_response};
use tracing::{debug, error, warn};

use crate::clients::QsListenResponder;

use super::{CoreUser, process_qs::ProcessedQsMessages};

/// A processor for the streamed QS events.
///
/// This processor is meant to be used in the streaming context where the events are streamed one
/// by one and this process never finishes until the stream is closed. Each event is processed by
/// `[Self::process_event]`.
#[derive(Debug)]
pub struct QsStreamProcessor {
    responder: Option<QsListenResponder>,
    /// Accumulated but not yet processed messages
    ///
    /// Note: It is safe to keep messages in memory here, because they are not yet decrypted.
    /// Decryption increases the locally stored ratchet sequence number, which is used to determine
    /// which messages should be fetched from the server. In case, the app is shut down, the
    /// messages will be received again.
    messages: Vec<QueueMessage>,
}

impl QsStreamProcessor {
    pub fn new(responder: Option<QsListenResponder>) -> Self {
        Self {
            responder,
            messages: Vec::new(),
        }
    }

    pub fn replace_responder(&mut self, responder: QsListenResponder) {
        self.responder.replace(responder);
    }

    pub async fn process_event(
        &mut self,
        core_user: &CoreUser,
        response: ListenResponse,
    ) -> QsProcessEventResult {
        debug!(?response, "processing QS listen event");

        match response.event {
            None => {
                error!("received an empty event");
                QsProcessEventResult::Ignored
            }
            Some(listen_response::Event::Payload(_)) => {
                // currently, we don't handle payload events
                warn!("ignoring QS listen payload event");
                QsProcessEventResult::Ignored
            }
            Some(listen_response::Event::Message(message)) => match message.try_into() {
                Ok(message) => {
                    // Invariant: after a message there is always an Empty event as sentinel
                    // => accumulated messages will be processed there
                    self.messages.push(message);

                    // Stop the background task and wait until it is fully stopped
                    core_user.outbound_service().stop().await;

                    QsProcessEventResult::Accumulated
                }
                Err(error) => {
                    error!(%error, "failed to convert QS message; dropping");
                    QsProcessEventResult::Ignored
                }
            },
            // Empty event indicates that the queue is empty
            Some(listen_response::Event::Empty(_)) => {
                let max_sequence_number = self.messages.last().map(|m| m.sequence_number);

                let messages = std::mem::take(&mut self.messages);
                let num_messages = messages.len();

                let processed_messages = core_user.fully_process_qs_messages(messages).await;

                let result = if processed_messages.processed < num_messages {
                    error!(
                        processed_messages.processed,
                        num_messages, "failed to fully process messages"
                    );
                    QsProcessEventResult::PartiallyProcessed {
                        dropped: num_messages - processed_messages.processed,
                        processed: processed_messages,
                    }
                } else {
                    if let Some(max_sequence_number) = max_sequence_number {
                        // We received some messages, so we can ack them *after* they were fully
                        // processed. In particular, the queue ratchet sequence number has been already
                        // written back into the database.
                        if let Some(responder) = self.responder.as_ref() {
                            // Acks all messages before max_sequence_number + 1 (exclusive)
                            responder.ack(max_sequence_number + 1).await;
                        } else {
                            error!("logic error: no responder to ack QS messages");
                        }
                    }

                    QsProcessEventResult::FullyProcessed {
                        processed: processed_messages,
                    }
                };

                // Start the background task, but don't wait for it to start
                drop(core_user.outbound_service().start());

                result
            }
            Some(listen_response::Event::VersionStatus(_)) => QsProcessEventResult::Ignored,
        }
    }
}

#[derive(Debug)]
pub enum QsProcessEventResult {
    /// Event was accumulated to be processed later
    Accumulated,
    /// Event was ignored
    Ignored,
    /// All accumulated events where fully processed
    FullyProcessed { processed: ProcessedQsMessages },
    /// Accumulated events were partially processed, some events were dropped
    PartiallyProcessed {
        processed: ProcessedQsMessages,
        dropped: usize,
    },
}

impl QsProcessEventResult {
    pub fn processed(&self) -> usize {
        match self {
            Self::Accumulated => 0,
            Self::Ignored => 0,
            Self::FullyProcessed { processed } => processed.processed,
            Self::PartiallyProcessed { processed, .. } => processed.processed,
        }
    }

    pub fn is_partially_processed(&self) -> bool {
        matches!(self, Self::PartiallyProcessed { .. })
    }
}
