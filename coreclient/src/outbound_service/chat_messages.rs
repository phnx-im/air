// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::time::Duration;

use anyhow::Context;
use anyhow::anyhow;
use chrono::{DateTime, Utc};
use mimi_content::MessageStatus;
use tokio_util::sync::CancellationToken;
use tracing::warn;
use tracing::{debug, error};
use uuid::Uuid;

use crate::db::access::WriteDbTransaction;
use crate::groups::{Group, handle_group_not_found_on_ds};
use crate::job::JobError;
use crate::job::chat_operation::{ChatOperation, DerivationEpoch};
use crate::job::pending_chat_operation::PendingChatOperation;
use crate::outbound_service::error::OutboundServiceError;
use crate::outbound_service::resync::Resync;
use crate::{
    Chat, ChatId, ChatMessage, ChatStatus, Message, MessageId,
    outbound_service::chat_message_queue::ChatMessageQueue,
};

use super::{OutboundService, OutboundServiceContext};

/// How often we attempt to send a message before marking it as failed.
const MAX_SEND_ATTEMPTS: usize = 3;

/// Delay between send attempts.
const SEND_RETRY_DELAY: Duration = Duration::from_secs(1);

/// The outcome of attempting to send a single queued chat message.
enum SendOutcome {
    /// The message was sent, or there is nothing left to send: it was deleted
    /// locally, a sibling client already sent it, or the chat no longer accepts
    /// messages. It can be removed from the queue.
    Sent,
    /// The message collided with a sibling client on the DS. It is left in the
    /// queue and retried at a fresh generation by a later run.
    Collided,
}

/// Whether the outbound service continues with the next queued message.
enum RunControl {
    NextMessage,
    EndRun,
}

/// The outcome of committing the proposals pending in a chat's group.
enum CommitOutcome {
    Committed,
    ChatBlocked,
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

            match self
                .send_queued_message(run_token, chat_id, message_id)
                .await?
            {
                RunControl::NextMessage => continue,
                RunControl::EndRun => return Ok(()),
            }
        }
    }

    /// Sends a single queued message, retrying transient failures.
    ///
    /// A message that cannot be sent for its own reasons is marked as failed
    /// and dropped from the queue. Once the attempts of a transient failure are
    /// exhausted, we presume the network to be down and fail the whole queue.
    async fn send_queued_message(
        &self,
        run_token: &CancellationToken,
        chat_id: ChatId,
        message_id: MessageId,
    ) -> anyhow::Result<RunControl> {
        let mut attempt = 0;
        loop {
            attempt += 1;
            let error = match self.send_chat_message(message_id).await {
                Ok(SendOutcome::Sent) => {
                    self.db
                        .with_write_transaction(async |txn| -> anyhow::Result<_> {
                            ChatMessageQueue::remove(txn, message_id).await?;
                            Ok(())
                        })
                        .await?;
                    return Ok(RunControl::NextMessage);
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
                    return Ok(RunControl::NextMessage);
                }
                Err(OutboundServiceError::Fatal(error)) => {
                    error!(%error, ?message_id, "Failed to send chat message; marking it as failed");
                    self.db
                        .with_write_transaction(async |txn| -> anyhow::Result<_> {
                            ChatMessageQueue::new(chat_id, message_id)
                                .remove_and_mark_as_failed(txn)
                                .await?;
                            Ok(())
                        })
                        .await?;
                    return Ok(RunControl::NextMessage);
                }
                Err(OutboundServiceError::Recoverable(error)) => error,
            };

            if attempt >= MAX_SEND_ATTEMPTS {
                warn!(%error, ?message_id, "Failed to send chat message; failing the queue");
                self.db
                    .with_write_transaction(async |txn| -> anyhow::Result<_> {
                        Ok(ChatMessageQueue::remove_all_and_mark_as_failed(txn).await?)
                    })
                    .await?;
                return Ok(RunControl::EndRun);
            }

            warn!(%error, ?message_id, attempt, "Failed to send chat message; retrying");
            tokio::select! {
                () = tokio::time::sleep(SEND_RETRY_DELAY) => {}
                () = run_token.cancelled() => return Ok(RunControl::EndRun),
            }
        }
    }

    async fn send_chat_message(
        &self,
        message_id: MessageId,
    ) -> Result<SendOutcome, OutboundServiceError> {
        debug!(?message_id, "sending message");

        // load chat and message
        let Some((chat, mut message, has_pending_proposals)) = self
            .db
            .with_read_transaction(async |txn| -> anyhow::Result<_> {
                // A message deleted locally in the meantime has nothing left to
                // send.
                let Some(message) = ChatMessage::load(&mut *txn, message_id).await? else {
                    return Ok(None);
                };

                // A sibling client may have sent the message already.
                if message.is_sent() {
                    return Ok(None);
                }

                let chat_id = message.chat_id();
                let Some(chat) = Chat::load(&mut *txn, &chat_id).await? else {
                    return Ok(None);
                };

                // Don't send messages for blocked chats
                if let ChatStatus::Blocked = chat.status() {
                    return Ok(None);
                }

                // Don't send messages for chats with pending resync
                if Resync::is_pending_for_chat(&mut *txn, &chat_id).await? {
                    debug!(?chat_id, "Skipping sending message due to pending resync");
                    return Ok(None);
                }

                // The group is loaded as-is: a parked pending commit is a
                // normal state here and must not fail the send.
                let has_pending_proposals = Group::load_with_chat_id(&mut *txn, chat_id)
                    .await?
                    .is_some_and(|group| group.mls_group().has_pending_proposals());

                Ok(Some((chat, message, has_pending_proposals)))
            })
            .await
            .map_err(OutboundServiceError::fatal)?
        else {
            return Ok(SendOutcome::Sent);
        };

        if has_pending_proposals {
            match self.commit_pending_proposals(chat.id()).await? {
                CommitOutcome::Committed => (),
                // Nothing left to send in a blocked chat.
                CommitOutcome::ChatBlocked => return Ok(SendOutcome::Sent),
            }
        }

        let Message::Content(content) = message.message() else {
            return Err(OutboundServiceError::fatal(anyhow!(
                "Messages scheduled for sending is not a content message."
            )));
        };

        let api_client = self
            .api_clients
            .get(&chat.owner_domain())
            .map_err(OutboundServiceError::fatal)?;

        // load group and create MLS message
        let (group_state_ear_key, params, signer) = self
            .new_mls_message(&chat, content.content().clone(), None)
            .await
            .map_err(OutboundServiceError::fatal)?;
        let epoch = params.epoch;
        let sent_tags = params.collision_tags.clone();
        let generation = params.generation;

        // send MLS message to DS
        let ds_timestamp = match api_client
            .ds_send_message(params, &signer, &group_state_ear_key)
            .await
        {
            Ok(ts) => ts,
            Err(ds_error) => {
                if ds_error.is_not_found() {
                    self.db
                        .with_write_transaction(async |txn| {
                            handle_group_not_found_on_ds(txn, chat.group_id()).await
                        })
                        .await
                        .map_err(OutboundServiceError::fatal)?;
                    return Err(OutboundServiceError::fatal(ds_error));
                }

                // A collision here means a competing sibling client already sent
                // a different message at this generation. Our message was
                // rejected, so leave it in the queue to be re-encrypted at a
                // fresh generation and retried by a later run.
                if !ds_error.process_tag_collisions(&sent_tags).is_empty() {
                    return Ok(SendOutcome::Collided);
                }

                if ds_error.is_network_error() {
                    return Err(OutboundServiceError::recoverable(ds_error));
                }
                return Err(OutboundServiceError::fatal(
                    anyhow::Error::from(ds_error).context("DS rejected message"),
                ));
            }
        };

        // message accepted by DS, confirm.
        self.confirm_mls_message(&chat, epoch, generation)
            .await
            .inspect_err(|error| error!(%error, "failed to confirm MLS message"))
            .ok();

        // post-processing:
        let enqueued_at: DateTime<Utc> = message.timestamp();
        self.db
            .with_write_transaction(async |txn| -> anyhow::Result<_> {
                // A message that replaced an earlier one keeps that one's place
                // in the chat, so the DS timestamp becomes the edit time rather
                // than the row's timestamp. A deletion records no edit time.
                let is_replacement = message.message().is_replacement();
                let is_deletion = message.message().is_deleted();
                if is_replacement {
                    message
                        .mark_as_sent(&mut *txn, message.timestamp().into())
                        .await?;
                    if !is_deletion {
                        message.set_edited_at(ds_timestamp);
                    }
                } else {
                    message.mark_as_sent(&mut *txn, ds_timestamp).await?;
                }
                message.update(&mut *txn).await?;

                // Advance the last-read marker to the DS timestamp, but only
                // if the message was already marked as read when it was
                // enqueued. A send that left the marker untouched (e.g. from
                // the iOS share extension) must not move it here either.
                // Deletions never move it.
                let last_read = Chat::load_watermark(&mut *txn, message.chat_id())
                    .await?
                    .map(|(last_read, _)| last_read);
                if message.status() != MessageStatus::Deleted
                    && last_read.is_some_and(|last_read| last_read >= enqueued_at)
                {
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
            .await
            .map_err(OutboundServiceError::fatal)?;

        Ok(SendOutcome::Sent)
    }

    /// Commits the proposals pending in the chat's group.
    ///
    /// A message is encrypted at a clean group state, so pending proposals have
    /// to be committed before it can be sent.
    async fn commit_pending_proposals(
        &self,
        chat_id: ChatId,
    ) -> Result<CommitOutcome, OutboundServiceError> {
        match self
            .execute_job(ChatOperation::update(chat_id, None, DerivationEpoch::Keep))
            .await
        {
            Ok(_) => Ok(CommitOutcome::Committed),
            Err(JobError::Blocked) => Ok(CommitOutcome::ChatBlocked),
            Err(JobError::NetworkError) => Err(OutboundServiceError::recoverable(anyhow!(
                "Network error while committing pending proposals"
            ))),
            // The job already cleaned up the local state.
            Err(JobError::NotFound) => Err(OutboundServiceError::fatal(anyhow!(
                "Chat not found while committing pending proposals"
            ))),
            Err(error @ (JobError::Domain(_) | JobError::Fatal(_))) => {
                Err(OutboundServiceError::fatal(error))
            }
        }
    }
}
