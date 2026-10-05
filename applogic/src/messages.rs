// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::messages::QueueMessage;
use aircoreclient::{
    ChatId, StoredRequest,
    clients::{
        ListenQueueError, SiblingClientStates, is_resource_exhausted_error, listen_response,
        process::process_qs::ProcessedQsMessages,
    },
};
use anyhow::Result;
use tokio::time::sleep;
use tokio_stream::StreamExt;
use tracing::{debug, error, warn};

use crate::{
    api::user::User,
    notifications::{ChatNotificationsBatch, NotificationContent},
    util::FibonacciBackoff,
};

/// Number of retries of a rate limited request
const RATE_LIMIT_RETRIES: usize = 3;

/// Runs `f`, and retries it after a backoff while it is rate limited.
async fn retry_rate_limited<T, E>(
    mut f: impl AsyncFnMut() -> Result<T, E>,
    is_rate_limited: impl Fn(&E) -> bool,
) -> Result<T, E> {
    let mut backoff = FibonacciBackoff::new();
    for _ in 0..RATE_LIMIT_RETRIES {
        match f().await {
            Err(error) if is_rate_limited(&error) => {
                let retry_in = backoff.next_backoff();
                warn!(?retry_in, "rate limited");
                sleep(retry_in).await;
            }
            result => return result,
        }
    }
    f().await
}

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
        let drained = loop {
            match stream.next().await {
                Some(Ok(response)) => match response.event {
                    // Empty event is the sentinel: the queue is drained.
                    Some(listen_response::Event::Empty(_)) => break true,
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
                    break false;
                }
                // EOF without our half-close (old server) => stream is over
                None => break false,
            }
        };

        // Invariant: messages are sorted by sequence number
        let max_sequence_number = messages.last().map(|m| m.sequence_number);

        let num_messages = messages.len();
        let processed_messages = self.user.fully_process_qs_messages(messages).await;
        if processed_messages.processed != num_messages {
            let dropped = num_messages - processed_messages.processed;
            error!(%dropped, "failed to fully process messages");
        }

        match max_sequence_number {
            // We received some messages, so we can ack them *after* they were fully
            // processed. In particular, the queue ratchet sequence number was written back
            // into the database.
            Some(n) if drained => {
                responder.ack(n + 1).await;
                // half-close the request stream, then wait for the server to apply the ack
                responder.close(&mut stream).await;
            }
            Some(n) => responder.ack(n + 1).await,
            None => {}
        }

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
        ) = retry_rate_limited(
            async || Box::pin(self.fetch_and_process_qs_messages()).await,
            ListenQueueError::is_resource_exhausted,
        )
        .await
        .map_err(|error| {
            if error.is_unsupported_version() {
                FetchAndProcessAllMessagesError::UnsupportedClientVersion
            } else if error.is_resource_exhausted() {
                FetchAndProcessAllMessagesError::RateLimited
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
        let new_username_connections = match retry_rate_limited(
            async || self.fetch_and_process_as_messages().await,
            is_resource_exhausted_error,
        )
        .await
        {
            Ok(stored) => stored,
            // Keeps the notifications of the already acked QS messages
            Err(error) if is_resource_exhausted_error(&error) => {
                warn!("Rate limited while fetching AS messages");
                Vec::new()
            }
            Err(error) => return Err(FetchAndProcessAllMessagesError::Fatal(error)),
        };
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
    #[error("Rate limited by the server")]
    RateLimited,
    #[error(transparent)]
    Fatal(anyhow::Error),
}

#[cfg(test)]
mod tests {
    use tokio::time::{Duration, Instant};

    use super::*;

    #[derive(Debug, PartialEq)]
    enum TestError {
        RateLimited,
        Other,
    }

    async fn run(errors: &[TestError]) -> (Result<(), TestError>, usize, Duration) {
        let mut errors = errors.iter();
        let mut attempts = 0;
        let started_at = Instant::now();
        let result = retry_rate_limited(
            async || {
                attempts += 1;
                match errors.next() {
                    Some(TestError::RateLimited) => Err(TestError::RateLimited),
                    Some(TestError::Other) => Err(TestError::Other),
                    None => Ok(()),
                }
            },
            |error| *error == TestError::RateLimited,
        )
        .await;
        (result, attempts, started_at.elapsed())
    }

    #[tokio::test(start_paused = true)]
    async fn rate_limited_requests_are_retried_with_backoff() {
        let (result, attempts, elapsed) =
            run(&[TestError::RateLimited, TestError::RateLimited]).await;
        assert_eq!(result, Ok(()));
        assert_eq!(attempts, 3);
        assert_eq!(elapsed, Duration::from_secs(3));
    }

    #[tokio::test(start_paused = true)]
    async fn rate_limited_requests_give_up_after_the_backoff() {
        let (result, attempts, elapsed) = run(&[
            TestError::RateLimited,
            TestError::RateLimited,
            TestError::RateLimited,
            TestError::RateLimited,
        ])
        .await;
        assert_eq!(result, Err(TestError::RateLimited));
        assert_eq!(attempts, 4);
        assert_eq!(elapsed, Duration::from_secs(6));
    }

    #[tokio::test(start_paused = true)]
    async fn other_errors_are_not_retried() {
        let (result, attempts, elapsed) = run(&[TestError::Other]).await;
        assert_eq!(result, Err(TestError::Other));
        assert_eq!(attempts, 1);
        assert_eq!(elapsed, Duration::ZERO);
    }
}
