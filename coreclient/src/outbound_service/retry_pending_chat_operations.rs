// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use airprotos::client::self_group::SettingsUpdate;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error};
use uuid::Uuid;

use crate::{
    clients::{
        own_client_info::OwnClientInfo,
        user_settings::{SettingChanges, SettingsUpdateExt},
    },
    job::{JobError, pending_chat_operation::PendingChatOperation},
    outbound_service::{OutboundServiceContext, error::OutboundServiceRunError},
};

impl OutboundServiceContext {
    pub(super) async fn send_pending_chat_operations(
        &self,
        run_token: &CancellationToken,
    ) -> Result<(), OutboundServiceRunError> {
        // Used to identify locked receipts by this task
        let task_id = Uuid::new_v4();
        loop {
            if run_token.is_cancelled() {
                return Ok(()); // the task is being stopped
            }

            // Turn pending setting changes into a self-group commit when the
            // self-group is free. Runs every iteration: when a settings
            // operation completes with changes left pending (the user
            // re-toggled while it was in flight), the next iteration issues a
            // fresh commit.
            if let Err(error) = self.ensure_settings_operation().await {
                error!(%error, "Failed to stage pending setting changes");
            }

            let now = chrono::Utc::now();

            let pending_chat_operation = self
                .db
                .with_write_transaction(async |txn| {
                    PendingChatOperation::dequeue(txn, task_id, now).await
                })
                .await?;
            let Some(pending_chat_operation) = pending_chat_operation else {
                return Ok(());
            };

            let group_id = pending_chat_operation.group_id().clone();
            debug!(?group_id, "dequeued pending chat operation for retry");

            // The job manages its own retry count and deletion upon success.
            // We're just executing it here.
            match self.execute_job(pending_chat_operation).await {
                Err(JobError::NetworkError) => {
                    // If we're getting a network error, error out of the loop and wait for the next run.
                    return Err(OutboundServiceRunError::NetworkError);
                }
                Err(error @ (JobError::Fatal(_) | JobError::Domain(_))) => {
                    error!(%error, ?group_id, "Failed to execute pending chat operation");
                    // This job has a fatal error. Continue with the next one.
                    continue;
                }
                Err(JobError::Blocked | JobError::NotFound) => {
                    continue;
                }
                Ok(_) => (),
            }
        }
    }

    /// Stages a self-group commit for the pending [`SettingChanges`], if any.
    ///
    /// The commit carries the full current settings state and is stored as a
    /// [`PendingChatOperation`], the send attempt behind the pending changes.
    /// A no-op when no changes are pending, when there is no self-group, or
    /// when a self-group operation is already in flight (including one parked
    /// on a wrong-epoch rejection, which waits for the winning commit to
    /// arrive through the queue and delete it).
    async fn ensure_settings_operation(&self) -> anyhow::Result<()> {
        let info = OwnClientInfo::load(self.db.read().await?).await?;
        let Some(self_group_id) = info.self_group_id else {
            return Ok(());
        };
        let Some(signer) = info.self_group_signing_key else {
            return Ok(());
        };

        self.db
            .with_write_transaction(async |txn| {
                if SettingChanges::load(&mut *txn).await?.is_none() {
                    return Ok(());
                }
                if PendingChatOperation::load_by_group_id(&mut *txn, &self_group_id)
                    .await?
                    .is_some()
                {
                    return Ok(());
                }

                let update = SettingsUpdate::collect(&mut *txn).await?;
                PendingChatOperation::create_settings_update(txn, &signer, &self_group_id, update)
                    .await?;
                Ok(())
            })
            .await
    }
}
