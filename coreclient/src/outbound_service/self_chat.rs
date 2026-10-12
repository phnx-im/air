// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use airprotos::client::self_group::SelfGroupAppMessage;
use tracing::debug;

use crate::{
    Chat,
    groups::self_group::SelfGroup,
    job::pending_chat_operation::PendingChatOperation,
    outbound_service::{error::OutboundServiceError, resync::Resync},
};

use super::{OutboundServiceContext, SendOutcome};

/// Whether the self group can take an application message.
pub(super) enum SelfChatReadiness {
    /// No sibling would receive the message.
    NoSiblings,
    /// The self group can't take a message now, a later run tries again.
    NotReady,
    Ready(Chat),
}

impl OutboundServiceContext {
    pub(super) async fn self_chat_for_app_message(&self) -> anyhow::Result<SelfChatReadiness> {
        if !SelfGroup::load_and_check_if_has_linked_device(self.db.read().await?).await? {
            return Ok(SelfChatReadiness::NoSiblings);
        }

        let Some(chat) = self
            .db
            .with_read_transaction(async |txn| SelfGroup::load_chat(txn).await)
            .await?
        else {
            debug!("no self chat yet");
            return Ok(SelfChatReadiness::NotReady);
        };

        if let Some(status) = Resync::status_for_chat(self.db.read().await?, &chat.id()).await? {
            debug!(?status, "self group is resyncing");
            return Ok(SelfChatReadiness::NotReady);
        }
        if PendingChatOperation::is_pending_for_chat(self.db.read().await?, chat.id()).await? {
            debug!("self group is busy");
            return Ok(SelfChatReadiness::NotReady);
        }

        Ok(SelfChatReadiness::Ready(chat))
    }

    /// Sends a self-group app message to the siblings. A message that cannot
    /// be built is fatal.
    pub(super) async fn send_self_group_message(
        &self,
        chat: &Chat,
        message: SelfGroupAppMessage,
    ) -> Result<SendOutcome, OutboundServiceError> {
        let content = message
            .to_mimi_content()
            .map_err(OutboundServiceError::fatal)?;
        self.send_application_message(chat, content).await
    }
}
