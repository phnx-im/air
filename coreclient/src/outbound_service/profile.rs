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
                error!(%error, "spawned fetch profiles task failed");
            }
        }
    }

    async fn fetch_profiles(self) {
        debug!("fetching profiles");
        if let Err(error) = Self::try_fetch_profiles(self).await {
            error!(%error, "failed to fetch profiles");
        }
        debug!("fetching profiles done");
    }

    async fn try_fetch_profiles(self) -> anyhow::Result<()> {
        let task_id = Uuid::new_v4();
        let now = Utc::now();

        let stats = Operation::<FetchProfileOperation>::stats(&self.pool).await?;
        debug!(?stats, "fetching profiles");

        while let Some(mut op) =
            Operation::<FetchProfileOperation>::dequeue(&self.pool, task_id, now).await?
        {
            let operation_id = &op.operation_id;
            debug!(?operation_id, "fetching profile");

            if op.retries >= NUM_RETRIES {
                error!(?operation_id, "reached max number of retries; giving up");
                op.delete(&self.pool).await?;
                continue;
            }

            // TODO: technically, this clone is not necessary
            match self.execute_job(op.data.clone()).await {
                Ok(()) => debug!(?operation_id, "fetched profile"),
                Err(JobError::NetworkError) => {
                    debug!(
                        ?operation_id,
                        "failed to fetch profile due to network error; retrying"
                    );
                    op.reschedule(&self.pool, now + RETRY_AFTER).await?;
                    return Ok(());
                }
                Err(error @ (JobError::Blocked | JobError::FatalError(_))) => {
                    error!(?operation_id, %error, "failed to fetch profile");
                    op.delete(&self.pool).await?;
                }
            }
        }

        Ok(())
    }
}
