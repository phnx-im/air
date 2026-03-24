// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use tokio_util::sync::CancellationToken;
use tracing::{debug, error};
use uuid::Uuid;

use crate::{
    job::{JobError, pending_chat_operation::PendingChatOperation},
    outbound_service::{OutboundServiceContext, error::OutboundServiceRunError},
    utils::connection_ext::ConnectionExt,
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

            let now = chrono::Utc::now();

            let pool = self.pool.clone();
            let pending_chat_operation = pool
                .with_transaction(async |txn| {
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
}
