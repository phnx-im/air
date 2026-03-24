// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{convert::Infallible, ops::ControlFlow, time::Duration};

use chrono::{DateTime, Utc};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error};
use uuid::Uuid;

use crate::{
    job::{
        Job, JobError,
        operation::{Operation, OperationData},
        profile::{FetchGroupProfileOperation, FetchUserProfileOperation},
    },
    outbound_service::OutboundServiceContext,
};

const NUM_RETRIES: usize = 5;
const RETRY_AFTER: Duration = Duration::from_secs(5);

impl OutboundServiceContext {
    /// Spawn a task that fetches user and group profiles in the background.
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

        // fetch user profiles
        while let Some(op) =
            Operation::<FetchUserProfileOperation>::dequeue(&self.pool, task_id, now).await?
        {
            match self.fetch_profile(op, now).await? {
                ControlFlow::Continue(_) => (),
                ControlFlow::Break(_) => break,
            }
        }

        // fetch group profiles
        while let Some(op) =
            Operation::<FetchGroupProfileOperation>::dequeue(&self.pool, task_id, now).await?
        {
            match self.fetch_profile(op, now).await? {
                ControlFlow::Continue(_) => (),
                ControlFlow::Break(_) => break,
            }
        }

        Ok(())
    }

    async fn fetch_profile<T>(
        &self,
        op: Operation<T>,
        now: DateTime<Utc>,
    ) -> anyhow::Result<ControlFlow<()>>
    where
        T: OperationData + Job<Output = (), DomainError = Infallible>,
    {
        debug!(?op.operation_id, kind = ?T::kind(), "fetching profile");

        let (mut op, data) = op.take_data();
        let operation_id = &op.operation_id;

        match self.execute_job(data).await {
            Ok(()) => {
                debug!(?operation_id, "fetched profile");
                op.delete(&self.pool).await?;
            }
            Err(JobError::NetworkError) => {
                debug!(
                    ?operation_id,
                    "Failed to fetch profile due to network error"
                );
                if op.retries + 1 < NUM_RETRIES {
                    op.reschedule(&self.pool, now + RETRY_AFTER).await?;
                    return Ok(ControlFlow::Break(()));
                } else {
                    let retries = op.retries;
                    error!(
                        ?operation_id,
                        retries, "Reached max number of retries; giving up"
                    );
                    op.delete(&self.pool).await?;
                    return Ok(ControlFlow::Continue(()));
                }
            }
            Err(
                error @ (JobError::Blocked
                | JobError::Fatal(_)
                | JobError::NotFound
                | JobError::Domain(_)),
            ) => {
                // These error cases must not happen when fetching profiles.
                error!(?operation_id, %error, "Failed to fetch profile; deleting operation");
                op.delete(&self.pool).await?;
            }
        }

        Ok(ControlFlow::Continue(()))
    }
}
