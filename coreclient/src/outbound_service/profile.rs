// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::time::Duration;

use chrono::Utc;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error};
use uuid::Uuid;

use crate::{
    job::{JobError, operation::Operation, profile::FetchProfileOperation},
    outbound_service::OutboundServiceContext,
};

const NUM_RETRIES: usize = 5;
const RETRY_AFTER: Duration = Duration::from_secs(5);

impl OutboundServiceContext {
    /// Profiles are fetched in the background
    pub(super) fn spawn_fetch_profiles(
        &self,
        run_token: &CancellationToken,
    ) -> impl Future<Output = ()> {
        let task = run_token
            .clone()
            .run_until_cancelled_owned(self.clone().fetch_profiles());
        let handle = tokio::spawn(task);
        async move {
            if let Err(error) = handle.await {
                error!(%error, "Spawned fetch profiles task failed");
            }
        }
    }

    async fn fetch_profiles(self) {
        if let Err(error) = Self::try_fetch_profiles(self).await {
            error!(%error, "Failed to fetch profiles");
        }
    }

    async fn try_fetch_profiles(self) -> anyhow::Result<()> {
        let task_id = Uuid::new_v4();
        let now = Utc::now();

        while let Some(op) =
            Operation::<FetchProfileOperation>::dequeue(&self.pool, task_id, now).await?
        {
            debug!(?op.operation_id, "fetching profile");

            let (mut op, data) = op.take_data();
            let operation_id = &op.operation_id;

            match self.execute_job(data).await {
                Ok(()) => debug!(?operation_id, "fetched profile"),
                Err(JobError::NetworkError) => {
                    debug!(
                        ?operation_id,
                        "Failed to fetch profile due to network error"
                    );
                    if op.retries + 1 < NUM_RETRIES {
                        op.reschedule(&self.pool, now + RETRY_AFTER).await?;
                        return Ok(());
                    } else {
                        let retries = op.retries;
                        error!(
                            ?operation_id,
                            retries, "Reached max number of retries; giving up"
                        );
                        op.delete(&self.pool).await?;
                        continue;
                    }
                }
                Err(error @ (JobError::Blocked | JobError::FatalError(_))) => {
                    error!(?operation_id, %error, "Failed to fetch profile");
                    op.delete(&self.pool).await?;
                }
            }
        }

        Ok(())
    }
}
