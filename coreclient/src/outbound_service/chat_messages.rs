// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use anyhow::Context;
use anyhow::anyhow;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error};
use uuid::Uuid;

use crate::outbound_service::error::OutboundServiceError;
use crate::{
    Chat, ChatId, ChatMessage, ChatStatus, Message, MessageId,
    outbound_service::chat_message_queue::ChatMessageQueue, utils::connection_ext::StoreExt,
};

use super::{OutboundService, OutboundServiceContext};

impl OutboundService {
    pub async fn enqueue_chat_message(&self, message_id: MessageId) -> anyhow::Result<()> {
        let mut connection = self.context.pool.acquire().await?;

        // Load message to make sure it exists and get chat id
        let message = ChatMessage::load(&mut *connection, message_id)
            .await?
            .with_context(|| format!("Can't find message with id {message_id:?}"))?;
        let chat_id = message.chat_id();

        // Load chat to check status
        let chat = Chat::load(&mut connection, &chat_id)
            .await?
            .with_context(|| format!("Can't find chat with id {chat_id}"))?;
        if let ChatStatus::Blocked = chat.status() {
            return Ok(());
        }

        let message_queue = ChatMessageQueue::new(chat_id, message_id);
        message_queue.enqueue(&mut *connection).await?;

        self.notify_work();

        Ok(())
    }
}

impl OutboundServiceContext {
    pub(super) async fn send_queued_messages(
        &self,
        run_token: &CancellationToken,
    ) -> anyhow::Result<()> {
        // Used to identify locked messages by this task
        let task_id = Uuid::new_v4();
        loop {
            if run_token.is_cancelled() {
                return Ok(()); // the task is being stopped
            }

            let Some((chat_id, message_id)) =
                ChatMessageQueue::dequeue(&self.pool, task_id).await?
            else {
                return Ok(());
            };
            debug!(?chat_id, ?message_id, "dequeued messages");

            match self.send_chat_message(chat_id, message_id).await {
                Ok(_) => {
                    ChatMessageQueue::remove(&self.pool, message_id).await?;
                }
                Err(OutboundServiceError::Fatal(error)) => {
                    error!(%error, "Failed to send message; dropping");
                    ChatMessageQueue::remove(&self.pool, message_id).await?;
                    return Err(error);
                }
                Err(OutboundServiceError::Recoverable(error)) => {
                    error!(%error, "Failed to send message; will retry later");
                    continue;
                }
            };
        }
    }

    async fn send_chat_message(
        &self,
        chat_id: ChatId,
        message_id: MessageId,
    ) -> Result<(), OutboundServiceError> {
        debug!(%chat_id, ?message_id, "sending message");

        // load chat and message
        let (chat, mut message) = {
            let mut connection = self
                .pool
                .acquire()
                .await
                .map_err(OutboundServiceError::recoverable)?;
            let chat = Chat::load(&mut connection, &chat_id)
                .await
                .map_err(OutboundServiceError::recoverable)?
                .with_context(|| format!("Can't find chat with id {chat_id}"))
                .map_err(OutboundServiceError::fatal)?;
            if let ChatStatus::Blocked = chat.status() {
                return Ok(());
            }
            let message = ChatMessage::load(&mut *connection, message_id)
                .await
                .map_err(OutboundServiceError::recoverable)?
                .with_context(|| format!("Can't find message with id {message_id:?}"))
                .map_err(OutboundServiceError::fatal)?;
            (chat, message)
        };

        debug_assert!(!message.is_sent());

        let Message::Content(content) = message.message() else {
            return Err(OutboundServiceError::fatal(anyhow!(
                "Messages scheduled for sending is not a content message."
            )));
        };

        // load group and create MLS message
        let (group_state_ear_key, params) = self
            .new_mls_message(&chat, content.content().clone())
            .await?;

        // send MLS message to DS
        let ds_timestamp = self
            .api_clients
            .get(&chat.owner_domain())
            .map_err(OutboundServiceError::fatal)?
            .ds_send_message(params, &self.signing_key, &group_state_ear_key)
            .await
            .map_err(OutboundServiceError::recoverable)?;

        // mark message as sent
        self.with_transaction_and_notifier(async |txn, notifier| {
            if message.edited_at().is_some() {
                message
                    .mark_as_sent(&mut *txn, notifier, message.timestamp().into())
                    .await?;
                message.set_edited_at(ds_timestamp);
            } else {
                message
                    .mark_as_sent(&mut *txn, notifier, ds_timestamp)
                    .await?;
            }

            Chat::mark_as_read_until_message_id(
                txn,
                notifier,
                message.chat_id(),
                message.id(),
                self.user_id(),
            )
            .await?;
            Ok(())
        })
        .await
        .map_err(OutboundServiceError::fatal)?;

        Ok(())
    }
}
