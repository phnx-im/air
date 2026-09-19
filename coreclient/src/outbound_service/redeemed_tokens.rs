// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Syncs which Privacy Pass tokens have been redeemed between sibling clients,
//! using a [`SelfGroupAppMessage`]. There is a bit of a delay between
//! redemption and sending out the broadcast for decorrelation reasons.

use airprotos::client::self_group::{RedeemedTokens, SelfGroupAppMessage};
use chrono::Utc;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info};

use crate::{
    Chat, clients::own_client_info::OwnClientInfo,
    job::pending_chat_operation::PendingChatOperation, outbound_service::resync::Resync,
    privacy_pass,
};

use super::{OutboundServiceContext, SendOutcome};

impl OutboundServiceContext {
    /// Sends every redemption whose broadcast delay has passed to the siblings,
    /// one message per batch.
    ///
    /// The rows are not deleted until the DS accepts the message, unless there
    /// are no linked devices.
    pub(super) async fn send_redeemed_tokens(
        &self,
        run_token: &CancellationToken,
    ) -> anyhow::Result<()> {
        let redeemed =
            privacy_pass::redeemed_tokens_to_broadcast(self.db.read().await?, Utc::now()).await?;
        if redeemed.is_empty() {
            return Ok(());
        }

        if privacy_pass::is_alone(&self.db).await? {
            debug!("no sibling to tell about redeemed privacy pass tokens");
            self.retire_redeemed(&redeemed).await?;
            return Ok(());
        }

        let Some(chat) = self.self_chat().await? else {
            debug!("no self chat yet, keeping redeemed privacy pass tokens for a later run");
            return Ok(());
        };

        if let Some(status) = Resync::status_for_chat(self.db.read().await?, &chat.id()).await? {
            debug!(
                ?status,
                "self group is resyncing, keeping redeemed privacy pass tokens for a later run"
            );
            return Ok(());
        }
        if PendingChatOperation::is_pending_for_chat(self.db.read().await?, chat.id()).await? {
            debug!("self group is busy, keeping redeemed privacy pass tokens for a later run");
            return Ok(());
        }

        for message in &redeemed {
            if run_token.is_cancelled() {
                return Ok(());
            }
            let content = SelfGroupAppMessage::RedeemedTokens(message.clone()).to_mimi_content()?;
            match self.send_application_message(&chat, content).await? {
                SendOutcome::Sent => {
                    info!(
                        operation_type = %message.operation_type,
                        allowance_epoch = message.allowance_epoch,
                        count = message.token_indices.len(),
                        "told the siblings about redeemed privacy pass tokens"
                    );
                    self.retire_redeemed(std::slice::from_ref(message)).await?;
                }
                SendOutcome::Collided => {
                    debug!("redeemed privacy pass tokens collided with a sibling, retrying later");
                    return Ok(());
                }
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

    /// The chat of the self group, if there is one.
    async fn self_chat(&self) -> anyhow::Result<Option<Chat>> {
        self.db
            .with_read_transaction(async |txn| -> anyhow::Result<_> {
                let Some(group_id) = OwnClientInfo::load_self_group_id(&mut *txn).await? else {
                    return Ok(None);
                };
                Ok(Chat::load_by_group_id(&mut *txn, &group_id).await?)
            })
            .await
    }
}
