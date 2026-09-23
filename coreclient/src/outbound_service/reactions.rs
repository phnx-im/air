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
    job::pending_chat_operation::PendingChatOperation,
    outbound_service::resync::Resync,
};

use super::{OutboundService, OutboundServiceContext, SendOutcome, reaction_queue::ReactionQueue};

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

            // Skip add-reactions whose row was removed before we could send them
            if let Some(reaction_mimi_id) = &dequeued.reaction_mimi_id {
                let exists = self
                    .db
                    .with_read_transaction(async |txn| {
                        Reaction::exists_by_mimi_id(txn, reaction_mimi_id).await
                    })
                    .await?;
                if !exists {
                    debug!(?chat_id, "Skipping reaction send: row no longer exists");
                    self.db
                        .with_write_transaction(async |txn| {
                            ReactionQueue::remove(txn, dequeued.id).await
                        })
                        .await?;
                    continue;
                }
            }

            // If a resync is pending/failed, skip sending reactions for this chat.
            if let Some(status) = Resync::status_for_chat(self.db.read().await?, &chat_id).await? {
                debug!(?chat_id, ?status, "Skipping sending reaction due to resync");
                continue;
            }

            // If a chat operation is pending, skip sending reactions for this chat.
            if PendingChatOperation::is_pending_for_chat(self.db.read().await?, chat_id).await? {
                debug!(
                    ?chat_id,
                    "Skipping sending reaction due to pending chat operation"
                );
                continue;
            }

            match self.send_reaction_message(&dequeued).await {
                Ok(SendOutcome::Sent) => {
                    self.db
                        .with_write_transaction(async |txn| {
                            ReactionQueue::remove(txn, dequeued.id).await
                        })
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
        self.send_application_message(&chat, content).await
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
