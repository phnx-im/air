// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Syncs which Privacy Pass tokens have been redeemed between sibling clients,
//! using a [`SelfGroupAppMessage`]. There is a bit of a delay between
//! redemption and sending out the broadcast for decorrelation reasons.

use airprotos::client::self_group::{RedeemedTokens, SelfGroupAppMessage};
use chrono::Utc;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};

use crate::{
    outbound_service::{error::OutboundServiceError, retry::RetryPolicy},
    privacy_pass,
};

use super::{OutboundServiceContext, SendOutcome, self_chat::SelfChatReadiness};

impl OutboundServiceContext {
    /// Sends every redemption whose broadcast delay has passed to the siblings,
    /// one message per batch.
    ///
    /// The rows are not deleted until the DS accepts the message, unless there
    /// are no linked devices, the message failed fatally or its retry budget is
    /// used up.
    pub(super) async fn send_redeemed_tokens(
        &self,
        run_token: &CancellationToken,
    ) -> Result<(), OutboundServiceError> {
        let due = privacy_pass::redeemed_tokens_due(self.db.read().await?, Utc::now()).await?;
        if due.is_empty() {
            return Ok(());
        }
        let redeemed: Vec<RedeemedTokens> =
            due.iter().map(|(message, _)| message.clone()).collect();

        let chat = match self
            .self_chat_for_app_message()
            .await
            .map_err(OutboundServiceError::fatal)?
        {
            SelfChatReadiness::NoSiblings => {
                debug!("no sibling to tell about redeemed privacy pass tokens");
                self.retire_redeemed(&redeemed)
                    .await
                    .map_err(OutboundServiceError::fatal)?;
                return Ok(());
            }
            SelfChatReadiness::NotReady => {
                debug!("keeping redeemed privacy pass tokens for a later run");
                return Ok(());
            }
            SelfChatReadiness::Ready(chat) => chat,
        };

        let policy = RetryPolicy::SELF_GROUP_MESSAGES;
        for (message, attempts) in &due {
            if run_token.is_cancelled() {
                return Ok(());
            }
            let app_message = SelfGroupAppMessage::RedeemedTokens(message.clone());
            let result = self.send_self_group_message(&chat, app_message).await;
            match policy.fatal_when_exhausted(result, *attempts) {
                Ok(SendOutcome::Sent) => {
                    info!(
                        operation_type = %message.operation_type,
                        allowance_epoch = message.allowance_epoch,
                        count = message.token_indices.len(),
                        "told the siblings about redeemed privacy pass tokens"
                    );
                    self.retire_redeemed(std::slice::from_ref(message))
                        .await
                        .map_err(OutboundServiceError::fatal)?;
                }
                Ok(SendOutcome::Collided) => {
                    debug!("redeemed privacy pass tokens collided with a sibling, retrying later");
                    return Ok(());
                }
                Err(OutboundServiceError::Fatal(error)) => {
                    error!(
                        %error,
                        operation_type = %message.operation_type,
                        allowance_epoch = message.allowance_epoch,
                        "Failed to tell the siblings about redeemed privacy pass tokens; dropping"
                    );
                    self.retire_redeemed(std::slice::from_ref(message))
                        .await
                        .map_err(OutboundServiceError::fatal)?;
                }
                // Keep the message for a later run, the next ones may still go out
                Err(OutboundServiceError::Recoverable(error)) => {
                    warn!(
                        %error,
                        operation_type = %message.operation_type,
                        allowance_epoch = message.allowance_epoch,
                        "Failed to tell the siblings about redeemed privacy pass tokens; retrying later"
                    );
                    if let Some((attempts, broadcast_after)) = policy.defer(error.cause, *attempts)
                    {
                        self.db
                            .with_write_transaction(async |txn| {
                                privacy_pass::defer_redeemed_broadcasts(
                                    txn,
                                    message,
                                    attempts,
                                    broadcast_after,
                                )
                                .await
                            })
                            .await
                            .map_err(OutboundServiceError::fatal)?;
                    }
                }
                Err(error) => return Err(error),
            }
        }
        Ok(())
    }

    /// Clears the broadcast time of the positions `redeemed` names.
    async fn retire_redeemed(&self, redeemed: &[RedeemedTokens]) -> anyhow::Result<()> {
        self.db
            .with_write_transaction(async |txn| {
                privacy_pass::retire_redeemed_broadcasts(txn, redeemed).await
            })
            .await
    }
}
