// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::identifiers::AttachmentId;
use anyhow::Context;
use anyhow::anyhow;
use tokio_util::sync::CancellationToken;
use tracing::debug;
use tracing::warn;
use uuid::Uuid;

use crate::outbound_service::error::OutboundServiceError;
use crate::{
    Chat, ChatMessage, ChatStatus, Message, MessageId,
    outbound_service::chat_message_queue::ChatMessageQueue, utils::connection_ext::StoreExt,
};

use super::{OutboundService, OutboundServiceContext};

impl OutboundService {
    /// Enqueue a chat message to be sent by the outbound service.
    ///
    /// If an attachment ID is provided, the corresponding pending attachment
    /// record will be deleted if the message fails to send.
    pub async fn enqueue_chat_message(
        &self,
        message_id: MessageId,
        attachment_id: Option<AttachmentId>,
    ) -> anyhow::Result<()> {
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

        let message_queue = ChatMessageQueue::new(chat_id, message_id, attachment_id);
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

            let Some(message_id) = ChatMessageQueue::dequeue(&self.pool, task_id).await? else {
                return Ok(());
            };
            debug!(?message_id, "dequeued messages");

            if let Err(e) = self.send_chat_message(message_id).await {
                warn!(%e, ?message_id, "Failed to send chat message");
            };

            // Always delete the message from the queue. We don't want to automatically
            // retry here.
            self.with_transaction(async |txn| {
                ChatMessageQueue::remove(txn, message_id).await?;
                Ok(())
            })
            .await?;
        }
    }

    async fn send_chat_message(&self, message_id: MessageId) -> Result<(), anyhow::Error> {
        debug!(?message_id, "sending message");

        // load chat and message
        let (chat, mut message) = {
            let mut connection = self.pool.acquire().await?;
            let message = ChatMessage::load(&mut *connection, message_id)
                .await
                .map_err(OutboundServiceError::recoverable)?
                .with_context(|| format!("Can't find message with id {message_id:?}"))?;
            let chat_id = message.chat_id();
            let chat = Chat::load(&mut connection, &chat_id)
                .await?
                .with_context(|| format!("Can't find chat with id {chat_id}"))?;
            if let ChatStatus::Blocked = chat.status() {
                return Ok(());
            }
            (chat, message)
        };

        debug_assert!(!message.is_sent());

        let Message::Content(content) = message.message() else {
            return Err(anyhow!(
                "Messages scheduled for sending is not a content message."
            ));
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
            .await?;

        // post-processing:
        self.with_transaction_and_notifier(async |txn, notifier| {
            // adjust message status and edited_at timestamp
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
            message.update(txn.as_mut(), notifier).await?;

            // mark message as sent
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
