// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use anyhow::anyhow;
use anyhow::{Context, ensure};
use mimi_content::MessageStatus;
use tokio_util::sync::CancellationToken;
use tracing::debug;
use tracing::warn;
use uuid::Uuid;

use crate::db::access::WriteDbTransaction;
use crate::groups::handle_group_not_found_on_ds;
use crate::job::pending_chat_operation::PendingChatOperation;
use crate::outbound_service::resync::Resync;
use crate::{
    Chat, ChatMessage, ChatStatus, Message, MessageId,
    outbound_service::chat_message_queue::ChatMessageQueue,
};

use super::{OutboundService, OutboundServiceContext};

/// The outcome of attempting to send a single queued chat message.
enum SendOutcome {
    /// The message was sent (or no longer needs sending) and can be removed
    /// from the queue.
    Sent,
    /// The message collided with a sibling client on the DS. It is left in the
    /// queue and retried at a fresh generation by a later run.
    Collided,
}

impl OutboundService {
    /// Enqueue a chat message to be sent by the outbound service.
    pub async fn enqueue_chat_message(&self, message_id: MessageId) -> anyhow::Result<()> {
        self.context
            .db
            .with_write_transaction(async |txn| {
                self.enqueue_chat_message_in_transaction(txn, message_id)
                    .await
            })
            .await
    }

    pub(crate) async fn enqueue_chat_message_in_transaction(
        &self,
        txn: &mut WriteDbTransaction<'_>,
        message_id: MessageId,
    ) -> anyhow::Result<()> {
        // Load message to make sure it exists and get chat id
        let message = ChatMessage::load(&mut *txn, message_id)
            .await?
            .with_context(|| format!("Can't find message with id {message_id:?}"))?;
        let chat_id = message.chat_id();

        // Load chat to check status
        if Chat::is_blocked(&mut *txn, chat_id).await? {
            return Ok(());
        }

        let message_queue = ChatMessageQueue::new(chat_id, message_id);
        message_queue.enqueue(txn).await?;

        self.notify_work();

        Ok(())
    }

    pub async fn fail_enqueued_chat_message(&self, message_id: MessageId) -> anyhow::Result<()> {
        self.context
            .db
            .with_write_transaction(async |txn| -> anyhow::Result<_> {
                // Load message to make sure it exists and get chat id
                let message = ChatMessage::load(&mut *txn, message_id)
                    .await?
                    .with_context(|| format!("Can't find message with id {message_id:?}"))?;
                let chat_id = message.chat_id();

                // Load chat to check status
                if Chat::is_blocked(&mut *txn, chat_id).await? {
                    return Ok(());
                }

                let message_queue = ChatMessageQueue::new(message.chat_id(), message_id);

                message_queue.remove_and_mark_as_failed(txn).await?;
                Ok(())
            })
            .await?;

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

            let Some((chat_id, message_id)) = self
                .db
                .with_write_transaction(async |txn| ChatMessageQueue::dequeue(txn, task_id).await)
                .await?
            else {
                return Ok(());
            };
            debug!(?message_id, "dequeued messages");

            // If a chat operation is pending, we skip sending chat messages for
            // this chat
            if PendingChatOperation::is_pending_for_chat(self.db.read().await?, chat_id).await? {
                debug!(
                    ?chat_id,
                    "Skipping sending chat message due to pending chat operation"
                );
                continue;
            }

            match self.send_chat_message(message_id).await {
                Ok(SendOutcome::Sent) => {
                    // Always delete the message from the queue. We don't want
                    // to automatically retry here.
                    self.db
                        .with_write_transaction(async |txn| -> anyhow::Result<_> {
                            ChatMessageQueue::remove(txn, message_id).await?;
                            Ok(())
                        })
                        .await?;
                }
                Ok(SendOutcome::Collided) => {
                    // Leave the message in the queue so a later run retries it
                    // at a fresh generation instead of looping here. It stays
                    // locked by this task instance until then.
                    debug!(
                        ?message_id,
                        ?chat_id,
                        "Message collided, re-enqueuing for a later run"
                    );
                }
                Err(e) => {
                    warn!(%e, ?message_id, "Failed to send chat message");
                    // If the message fails, we mark it and all other queued
                    // messages as "failed" and delete them from the queue.
                    self.db
                        .with_write_transaction(async |txn| -> anyhow::Result<_> {
                            Ok(ChatMessageQueue::remove_all_and_and_mark_as_failed(txn).await?)
                        })
                        .await?;
                    return Ok(());
                }
            }
        }
    }

    async fn send_chat_message(&self, message_id: MessageId) -> anyhow::Result<SendOutcome> {
        debug!(?message_id, "sending message");

        // load chat and message
        let Some((chat, mut message)) = self
            .db
            .with_read_transaction(async |txn| {
                let message = ChatMessage::load(&mut *txn, message_id)
                    .await?
                    .with_context(|| format!("Can't find message with id {message_id:?}"))?;
                let chat_id = message.chat_id();
                let chat = Chat::load(&mut *txn, &chat_id)
                    .await?
                    .with_context(|| format!("Can't find chat with id {chat_id}"))?;

                // Don't send messages for blocked chats
                if let ChatStatus::Blocked = chat.status() {
                    return Ok(None);
                }

                // Don't send messages for chats with pending resync
                if Resync::is_pending_for_chat(&mut *txn, &chat_id).await? {
                    debug!(?chat_id, "Skipping sending message due to pending resync");
                    return Ok(None);
                }

                ensure!(!message.is_sent(), "Message is already sent");

                Ok(Some((chat, message)))
            })
            .await?
        else {
            return Ok(SendOutcome::Sent);
        };

        let Message::Content(content) = message.message() else {
            return Err(anyhow!(
                "Messages scheduled for sending is not a content message."
            ));
        };

        let api_client = self.api_clients.get(&chat.owner_domain())?;

        // load group and create MLS message
        let (group_state_ear_key, params) = self
            .new_mls_message(&chat, content.content().clone(), None)
            .await?;
        let sent_tags = params.collision_tags.clone();

        // send MLS message to DS
        let ds_timestamp = match api_client
            .ds_send_message(params, self.signing_key(), &group_state_ear_key)
            .await
        {
            Ok(ts) => ts,
            Err(ds_error) => {
                if ds_error.is_not_found() {
                    self.db
                        .with_write_transaction(async |txn| {
                            handle_group_not_found_on_ds(txn, chat.group_id()).await
                        })
                        .await?;
                    return Err(ds_error.into());
                }

                // A collision here means a competing sibling client already sent
                // a different message at this generation. Our message was
                // rejected, so leave it in the queue to be re-encrypted at a
                // fresh generation and retried by a later run.
                if !ds_error.process_tag_collisions(&sent_tags).is_empty() {
                    return Ok(SendOutcome::Collided);
                }
                return Err(ds_error.into());
            }
        };

        // post-processing:
        self.db
            .with_write_transaction(async |txn| -> anyhow::Result<_> {
                // adjust message status and edited_at timestamp
                if message.edited_at().is_some() {
                    message
                        .mark_as_sent(&mut *txn, message.timestamp().into())
                        .await?;
                    message.set_edited_at(ds_timestamp);
                } else {
                    message.mark_as_sent(&mut *txn, ds_timestamp).await?;
                }
                message.update(&mut *txn).await?;

                // Mark message as read, but only if it's not a deletion.
                if message.status() != MessageStatus::Deleted {
                    Chat::mark_as_read_until_message_id(
                        txn,
                        message.chat_id(),
                        message.id(),
                        self.user_id(),
                    )
                    .await?;
                }

                Ok(())
            })
            .await?;

        Ok(SendOutcome::Sent)
    }
}
