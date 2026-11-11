// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::messages::QueueMessage;
use aircoreclient::{
    ChatId, StoredRequest,
    clients::{
        ListenQueueError, SiblingClientStates, listen_response,
        process::process_qs::ProcessedQsMessages,
    },
};
use anyhow::Result;
use chrono::Utc;
use tokio_stream::StreamExt;
use tracing::{debug, error, info, warn};

use crate::{
    api::user::User,
    notifications::{ChatNotificationsBatch, NotificationContent},
};

#[derive(Debug, Default)]
pub(crate) struct ProcessedMessages {
    pub(crate) notifications_content: Vec<NotificationContent>,
    /// Chats whole notifications rebuild set came back empty
    ///
    /// Candidates for cancellation.
    pub(crate) empty_chat_ids: Vec<ChatId>,
}

impl User {
    /// Fetch and process AS messages
    async fn fetch_and_process_as_messages(&self) -> Result<Vec<StoredRequest>> {
        self.user.fetch_and_process_username_messages().await
    }

    /// Fetch and process QS messages
    ///
    /// Also returns the sibling client states received meanwhile.
    async fn fetch_and_process_qs_messages(
        &self,
    ) -> Result<(ProcessedQsMessages, Option<SiblingClientStates>), ListenQueueError> {
        let (mut stream, responder) = self.user.listen_queue().await?;
        let mut sibling_client_states = self
            .user
            .sibling_client_states()
            .inspect_err(|error| error!(%error, "failed to track sibling client states"))
            .ok();

        let mut messages: Vec<QueueMessage> = Vec::new();
        let drained_at = loop {
            match stream.next().await {
                Some(Ok(response)) => match response.event {
                    // Empty event is the sentinel: the queue is drained.
                    Some(listen_response::Event::Empty(_)) => break Some(Utc::now()),
                    Some(listen_response::Event::Message(queue_message)) => {
                        if let Ok(queue_message) = queue_message.try_into() {
                            messages.push(queue_message);
                        }
                    }
                    // Arrives before any message, so it is known when building the
                    // notifications below.
                    Some(listen_response::Event::SiblingClientState(state)) => {
                        if let Some(states) = &mut sibling_client_states
                            && let Err(error) = states.try_apply(state)
                        {
                            error!(%error, "failed to apply sibling client state");
                        }
                    }
                    Some(listen_response::Event::Payload(_))
                    | Some(listen_response::Event::VersionStatus(_))
                    | None => {}
                },
                // Terminal status => stream is over, acks cannot be confirmed
                Some(Err(error)) => {
                    warn!(%error, "qs listen stream failed during drain");
                    break None;
                }
                // EOF without our half-close (old server) => stream is over
                None => break None,
            }
        };

        let drained = drained_at.is_some();

        // Invariant: messages are sorted by sequence number
        let min_sequence_number = messages.first().map(|m| m.sequence_number);
        let max_sequence_number = messages.last().map(|m| m.sequence_number);

        let num_messages = messages.len();
        let processed_messages = self.user.fully_process_qs_messages(messages).await;
        // Ack only after all messages were processed and the queue ratchet was
        // written back. Unprocessed messages remain in the queue for the next
        // fetch.
        let unprocessed = num_messages - processed_messages.processed;
        let acked_up_to = if unprocessed == 0 {
            if let Some(drained_at) = drained_at {
                self.user
                    .discard_unconfirmed_group_creations(drained_at)
                    .await
                    .inspect_err(
                        |error| error!(%error, "failed to discard unconfirmed group creations"),
                    )
                    .ok();
            }
            max_sequence_number.map(|n| n + 1)
        } else {
            error!(%unprocessed, "failed to fully process messages");
            None
        };
        if let Some(n) = acked_up_to {
            responder.ack(n).await;
            if drained {
                // half-close the request stream, then wait for the server to apply the ack
                responder.close(&mut stream).await;
            }
        }

        info!(
            fetched = num_messages,
            processed = processed_messages.processed,
            ?min_sequence_number,
            ?max_sequence_number,
            ?acked_up_to,
            drained,
            "processed QS messages"
        );

        self.user.outbound_service().run_once().await;

        Ok((processed_messages, sibling_client_states))
    }

    /// Fetch and process both QS and AS messages
    ///
    /// This function is intended to be called in the background service.
    pub(crate) async fn fetch_and_process_all_messages_in_background(
        &self,
    ) -> Result<ProcessedMessages, FetchAndProcessAllMessagesError> {
        let mut notifications = Vec::new();

        // Fetch QS messages
        debug!("fetch QS messages");
        let (
            ProcessedQsMessages {
                new_chats,
                new_messages,
                errors: _,
                processed: _,
                mut new_connections,
                reaction_notifications,
                chats_with_changed_notifications,
                removed_chats,
            },
            sibling_client_states,
        ) = Box::pin(self.fetch_and_process_qs_messages())
            .await
            .map_err(|error| {
                if error.is_unsupported_version() {
                    FetchAndProcessAllMessagesError::UnsupportedClientVersion
                } else {
                    FetchAndProcessAllMessagesError::Fatal(error.into())
                }
            })?;
        self.new_chat_notifications(&new_chats, &mut notifications)
            .await;
        let ChatNotificationsBatch {
            additions,
            mut empty_chats,
        } = self
            .message_and_reaction_notifications(
                &new_messages,
                &reaction_notifications,
                &chats_with_changed_notifications,
                sibling_client_states.as_ref(),
            )
            .await;
        notifications.extend(additions);
        empty_chats.extend(removed_chats);

        // Fetch AS connection requests
        debug!("fetch AS messages");
        let new_username_connections = self
            .fetch_and_process_as_messages()
            .await
            .map_err(FetchAndProcessAllMessagesError::Fatal)?;
        for stored in new_username_connections {
            new_connections.push(stored.chat_id);
            empty_chats.extend(stored.moved_from);
        }

        self.new_connection_request_notifications(&new_connections, &mut notifications)
            .await;

        Ok(ProcessedMessages {
            notifications_content: notifications,
            empty_chat_ids: empty_chats,
        })
    }
}

#[derive(Debug, thiserror::Error)]
pub enum FetchAndProcessAllMessagesError {
    #[error("Unsupported client version")]
    UnsupportedClientVersion,
    #[error(transparent)]
    Fatal(anyhow::Error),
}
