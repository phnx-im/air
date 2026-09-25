// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Tells sibling clients which messages were deleted locally, using a
//! [`SelfGroupAppMessage`].

use aircommon::identifiers::MimiId;
use airprotos::client::self_group::{
    DeletedMessages, MAX_DELETED_MESSAGES_PER_MESSAGE, SelfGroupAppMessage,
};
use tokio_util::sync::CancellationToken;
use tracing::{debug, info};

use crate::{chats::messages::persistence, db::access::WriteDbTransaction};

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
    /// devices.
    pub(super) async fn send_deleted_messages(
        &self,
        run_token: &CancellationToken,
    ) -> anyhow::Result<()> {
        let staged = persistence::staged_deletions(self.db.read().await?).await?;
        if staged.is_empty() {
            return Ok(());
        }

        let chat = match self.self_chat_for_app_message().await? {
            SelfChatReadiness::NoSiblings => {
                debug!("no sibling to tell about deleted messages");
                self.remove_staged_deletions(&staged).await?;
                return Ok(());
            }
            SelfChatReadiness::NotReady => {
                debug!("keeping deleted messages for a later run");
                return Ok(());
            }
            SelfChatReadiness::Ready(chat) => chat,
        };

        for batch in staged.chunks(MAX_DELETED_MESSAGES_PER_MESSAGE) {
            if run_token.is_cancelled() {
                return Ok(());
            }
            let content = SelfGroupAppMessage::DeletedMessages(DeletedMessages {
                mimi_ids: batch.to_vec(),
            })
            .to_mimi_content()?;
            match self.send_application_message(&chat, content).await? {
                SendOutcome::Sent => {
                    info!(
                        count = batch.len(),
                        "told the siblings about deleted messages"
                    );
                    self.remove_staged_deletions(batch).await?;
                }
                SendOutcome::Collided => {
                    debug!("deleted messages collided with a sibling, retrying later");
                    return Ok(());
                }
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
