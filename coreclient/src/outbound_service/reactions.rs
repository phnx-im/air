// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::identifiers::MimiId;
use anyhow::Context;
use mimi_content::MimiContent;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error};
use uuid::Uuid;

use crate::{
    Chat, ChatId, ChatMessage, ChatStatus,
    chats::reactions::Reaction,
    db::access::{WriteConnection, WriteDbTransaction},
    groups::handle_group_not_found_on_ds,
    job::pending_chat_operation::PendingChatOperation,
    outbound_service::resync::Resync,
};

use super::{OutboundService, OutboundServiceContext, reaction_queue::ReactionQueue};

/// The outcome of attempting to send a single queued reaction.
enum SendOutcome {
    /// The reaction was sent (or no longer needs sending) and can be removed
    /// from the queue.
    Sent,
    /// The reaction collided with a sibling client on the DS. It is left in the
    /// queue and retried at a fresh generation by a later run.
    Collided,
}

impl OutboundService {
    /// Enqueue a reaction MLS message to be sent by the outbound service.
    ///
    /// `reaction_mimi_id` identifies the optimistic `reaction` row to roll back
    /// if sending fails permanently; pass `None` for retraction tombstones.
    pub(crate) async fn enqueue_reaction_in_transaction(
        &self,
        txn: &mut WriteDbTransaction<'_>,
        chat_id: ChatId,
        reaction_mimi_id: Option<&MimiId>,
        content: &[u8],
    ) -> anyhow::Result<()> {
        if Chat::is_blocked(&mut *txn, chat_id).await? {
            return Ok(());
        }
        ReactionQueue::enqueue(&mut *txn, chat_id, reaction_mimi_id, content).await?;
        self.notify_work();
        Ok(())
    }
}

impl OutboundServiceContext {
    pub(super) async fn send_queued_reactions(
        &self,
        run_token: &CancellationToken,
    ) -> anyhow::Result<()> {
        // Used to identify locked reactions by this task
        let task_id = Uuid::new_v4();
        loop {
            if run_token.is_cancelled() {
                return Ok(()); // the task is being stopped
            }

            let Some(dequeued) = self
                .db
                .with_write_transaction(async |txn| ReactionQueue::dequeue(txn, task_id).await)
                .await?
            else {
                return Ok(());
            };
            let chat_id = dequeued.chat_id;
            debug!(?chat_id, "dequeued reaction");

            // If a resync is pending, skip sending reactions for this chat.
            if Resync::is_pending_for_chat(self.db.read().await?, &chat_id).await? {
                debug!(?chat_id, "Skipping sending reaction due to pending resync");
                continue;
            }

            // If a chat operation is pending, skip sending reactions for this chat.
            if PendingChatOperation::is_pending_for_chat(self.db.read().await?, chat_id).await? {
                debug!(?chat_id, "Skipping sending reaction due to pending chat operation");
                continue;
            }

            match self.send_reaction_message(&dequeued).await {
                Ok(SendOutcome::Sent) => {
                    self.db
                        .with_write_transaction(async |txn| ReactionQueue::remove(txn, dequeued.id).await)
                        .await?;
                }
                Ok(SendOutcome::Collided) => {
                    // Leave the reaction in the queue so a later run retries it
                    // at a fresh generation. It stays locked by this task until then.
                    debug!(?chat_id, "Reaction collided, re-enqueuing for a later run");
                }
                Err(error) => {
                    error!(%error, ?chat_id, "Failed to send reaction; dropping and rolling back");
                    self.rollback_failed_reaction(&dequeued).await?;
                }
            }
        }
    }

    async fn send_reaction_message(
        &self,
        dequeued: &super::reaction_queue::DequeuedReaction,
    ) -> anyhow::Result<SendOutcome> {
        // load chat
        let chat = self
            .db
            .with_read_transaction(async |txn| Chat::load(txn, &dequeued.chat_id).await)
            .await?
            .with_context(|| format!("Can't find chat with id {}", dequeued.chat_id))?;
        if let ChatStatus::Blocked = chat.status() {
            return Ok(SendOutcome::Sent);
        }

        let content = MimiContent::deserialize(&dequeued.content)
            .context("Failed to deserialize queued reaction content")?;

        // load group and create MLS message
        let (group_state_ear_key, params) = self.new_mls_message(&chat, content, None).await?;
        let sent_tags = params.collision_tags.clone();

        // send MLS message to DS
        if let Err(ds_error) = self
            .api_clients
            .get(&chat.owner_domain())?
            .ds_send_message(params, self.signing_key(), &group_state_ear_key)
            .await
        {
            if ds_error.is_not_found() {
                self.db
                    .with_write_transaction(async |txn| {
                        handle_group_not_found_on_ds(txn, chat.group_id()).await
                    })
                    .await?;
                return Err(ds_error.into());
            }

            // A collision means a competing sibling client took this generation;
            // leave the reaction queued to be re-encrypted and retried.
            if !ds_error.process_tag_collisions(&sent_tags).is_empty() {
                return Ok(SendOutcome::Collided);
            }
            return Err(ds_error.into());
        }

        Ok(SendOutcome::Sent)
    }

    /// Drop a permanently-failed reaction from the queue and roll back its
    /// optimistic `reaction` row (if any), notifying the targeted message.
    async fn rollback_failed_reaction(
        &self,
        dequeued: &super::reaction_queue::DequeuedReaction,
    ) -> anyhow::Result<()> {
        self.db
            .with_write_transaction(async |txn| -> anyhow::Result<()> {
                ReactionQueue::remove(txn, dequeued.id).await?;
                if let Some(reaction_mimi_id) = &dequeued.reaction_mimi_id
                    && let Some(target_mimi_id) =
                        Reaction::delete_by_mimi_id(&mut *txn, reaction_mimi_id).await?
                    && let Some(target) =
                        ChatMessage::load_by_mimi_id(&mut *txn, &target_mimi_id).await?
                {
                    txn.notifier().update(target.id());
                }
                Ok(())
            })
            .await
    }
}
