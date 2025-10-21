// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::messages::QueueMessage;
use aircoreclient::{
    ChatId,
    clients::{process::process_qs::ProcessedQsMessages, queue_event},
};
use anyhow::Result;
use tokio_stream::StreamExt;
use tracing::{debug, error};

use crate::{api::user::User, notifications::NotificationContent};

#[derive(Debug, Default)]
pub(crate) struct ProcessedMessages {
    pub(crate) notifications_content: Vec<NotificationContent>,
}

impl User {
    /// Fetch and process AS messages
    async fn fetch_and_process_as_messages(&self) -> Result<Vec<ChatId>> {
        self.user.fetch_and_process_handle_messages().await
    }

    /// Fetch and process QS messages
    async fn fetch_and_process_qs_messages(&self) -> Result<ProcessedQsMessages> {
        let (stream, responder) = self.user.listen_queue().await?;
        let mut stream = stream
            .take_while(|message| !matches!(message.event, Some(queue_event::Event::Empty(_))))
            .filter_map(|message| match message.event? {
                queue_event::Event::Empty(_) => unreachable!(),
                queue_event::Event::Message(queue_message) => queue_message.try_into().ok(),
                queue_event::Event::Payload(_) => None,
            });

        let mut messages: Vec<QueueMessage> = Vec::new();
        while let Some(message) = stream.next().await {
            messages.push(message);
        }

        // Invariant: messages are sorted by sequence number
        let max_sequence_number = messages.last().map(|m| m.sequence_number);

        let num_messages = messages.len();
        let processed_messages = self.user.fully_process_qs_messages(messages).await;
        if processed_messages.processed != num_messages {
            let dropped = num_messages - processed_messages.processed;
            error!(%dropped, "failed to fully process messages");
        }

        if let Some(max_sequence_number) = max_sequence_number {
            // We received some messages, so we can ack them *after* they were fully
            // processed. In particular, the queue ratchet sequence number was written back
            // into the database.
            responder
                .ack(max_sequence_number + 1)
                .await
                .inspect_err(|error| {
                    error!(%error, "failed to ack QS messages");
                })
                .ok();
        }
        drop(stream); // must be alive until the ack is sent

        self.user.outbound_service().run_once().await;

        Ok(processed_messages)
    }

    /// Fetch and process both QS and AS messages
    ///
    /// This function is intended to be called in the background service.
    pub(crate) async fn fetch_and_process_all_messages_in_background(
        &self,
    ) -> Result<ProcessedMessages> {
        let mut notifications = Vec::new();

        // Fetch QS messages
        debug!("fetch QS messages");
        let ProcessedQsMessages {
            new_chats,
            changed_chats: _,
            new_messages,
            errors: _,
            rejoined_chats: _,
            processed: _,
        } = self.fetch_and_process_qs_messages().await?;
        self.new_chat_notifications(&new_chats, &mut notifications)
            .await;
        self.new_message_notifications(&new_messages, &mut notifications)
            .await;

        // Fetch AS connection requests
        debug!("fetch AS messages");
        let new_connections = self.fetch_and_process_as_messages().await?;
        self.new_connection_request_notifications(&new_connections, &mut notifications)
            .await;

        Ok(ProcessedMessages {
            notifications_content: notifications,
        })
    }
}
