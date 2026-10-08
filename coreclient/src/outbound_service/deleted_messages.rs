// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Tells sibling clients which messages were deleted locally, using a
//! [`SelfGroupAppMessage`].

use aircommon::{identifiers::MimiId, time::TimeStamp};
use airprotos::client::self_group::{
    DeletedMessages, MAX_DELETED_MESSAGES_PER_MESSAGE, SelfGroupAppMessage,
};
use chrono::Utc;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};

use crate::{
    chats::messages::persistence,
    db::access::WriteDbTransaction,
    outbound_service::{
        error::OutboundServiceError,
        retry::{RetryDecision, RetryPolicy},
    },
};

use super::{OutboundService, OutboundServiceContext, SendOutcome, self_chat::SelfChatReadiness};

impl OutboundService {
    /// Parks a local deletion to be sent to the siblings.
    pub(crate) async fn enqueue_deleted_message_in_transaction(
        &self,
        txn: &mut WriteDbTransaction<'_>,
        mimi_id: &MimiId,
    ) -> anyhow::Result<()> {
        persistence::store_outgoing_deletion(txn, mimi_id).await?;
        self.notify_work();
        Ok(())
    }
}

impl OutboundServiceContext {
    /// Sends the parked deletions to the siblings, in batches.
    ///
    /// A batch stays parked until the DS accepts it, unless there are no linked
    /// devices, the batch failed fatally or its retry budget is used up.
    pub(super) async fn send_deleted_messages(
        &self,
        run_token: &CancellationToken,
    ) -> Result<(), OutboundServiceError> {
        let due = persistence::due_deletions(self.db.read().await?, TimeStamp::now()).await?;
        if due.is_empty() {
            return Ok(());
        }
        let staged: Vec<MimiId> = due.iter().map(|(mimi_id, _)| *mimi_id).collect();

        let chat = match self
            .self_chat_for_app_message()
            .await
            .map_err(OutboundServiceError::fatal)?
        {
            SelfChatReadiness::NoSiblings => {
                debug!("no sibling to tell about deleted messages");
                self.remove_staged_deletions(&staged)
                    .await
                    .map_err(OutboundServiceError::fatal)?;
                return Ok(());
            }
            SelfChatReadiness::NotReady => {
                debug!("keeping deleted messages for a later run");
                return Ok(());
            }
            SelfChatReadiness::Ready(chat) => chat,
        };

        let policy = RetryPolicy::SELF_GROUP_MESSAGES;
        for due_batch in due.chunks(MAX_DELETED_MESSAGES_PER_MESSAGE) {
            if run_token.is_cancelled() {
                return Ok(());
            }
            let batch: Vec<MimiId> = due_batch.iter().map(|(mimi_id, _)| *mimi_id).collect();
            let batch = batch.as_slice();
            let attempts = due_batch
                .iter()
                .map(|(_, attempts)| *attempts)
                .max()
                .unwrap_or_default();
            let content = SelfGroupAppMessage::DeletedMessages(DeletedMessages {
                mimi_ids: batch.to_vec(),
            })
            .to_mimi_content();
            let result = match content {
                Ok(content) => self.send_application_message(&chat, content).await,
                Err(error) => Err(OutboundServiceError::fatal(error)),
            };
            match policy.fatal_when_exhausted(result, attempts) {
                Ok(SendOutcome::Sent) => {
                    info!(
                        count = batch.len(),
                        "told the siblings about deleted messages"
                    );
                    self.remove_staged_deletions(batch)
                        .await
                        .map_err(OutboundServiceError::fatal)?;
                }
                Ok(SendOutcome::Collided) => {
                    debug!("deleted messages collided with a sibling, retrying later");
                    return Ok(());
                }
                Err(OutboundServiceError::Fatal(error)) => {
                    error!(
                        %error,
                        count = batch.len(),
                        "Failed to tell the siblings about deleted messages; dropping"
                    );
                    self.remove_staged_deletions(batch)
                        .await
                        .map_err(OutboundServiceError::fatal)?;
                }
                // Keep the batch for a later run, the next batches may still go out
                Err(OutboundServiceError::Recoverable(error)) => {
                    warn!(%error, count = batch.len(), "Failed to tell the siblings about deleted messages; retrying later");
                    if let RetryDecision::Backoff { attempts, retry_in } = policy.decide(attempts) {
                        let retry_at = TimeStamp::from(Utc::now() + retry_in);
                        persistence::defer_deletions(
                            self.db.write().await?,
                            batch,
                            attempts,
                            retry_at,
                        )
                        .await?;
                    }
                }
                Err(error) => return Err(error),
            }
        }
        Ok(())
    }

    async fn remove_staged_deletions(&self, mimi_ids: &[MimiId]) -> anyhow::Result<()> {
        self.db
            .with_write_transaction(async |txn| {
                Ok(persistence::remove_staged_deletion(txn, mimi_ids).await?)
            })
            .await
    }
}
