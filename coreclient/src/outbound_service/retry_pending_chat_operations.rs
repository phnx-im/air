// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use tokio_util::sync::CancellationToken;
use tracing::debug;
use uuid::Uuid;

use crate::{
    job::pending_chat_operation::PendingChatOperation, outbound_service::OutboundServiceContext,
};

impl OutboundServiceContext {
    pub(super) async fn retry_pending_chat_operations(
        &self,
        run_token: &CancellationToken,
    ) -> anyhow::Result<()> {
        // Used to identify locked receipts by this task
        let task_id = Uuid::new_v4();
        loop {
            if run_token.is_cancelled() {
                return Ok(()); // the task is being stopped
            }

            let mut connection = self.pool.acquire().await?;

            let Some(pending_chat_operation) =
                PendingChatOperation::dequeue(&mut connection, task_id).await?
            else {
                return Ok(());
            };

            let group_id = pending_chat_operation.group_id().clone();
            debug!(?group_id, "dequeued pending chat operation for retry");

            // The job manages its own retry count and deletion upon success.
            // We're just executing it here.
            self.execute_job(pending_chat_operation).await?;
        }
    }
}
