// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{convert::Infallible, ops::ControlFlow, time::Duration};

use chrono::{DateTime, Utc};
use serde::de::DeserializeOwned;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info};
use uuid::Uuid;

use crate::{
    job::{
        Job, JobError,
        operation::{Operation, OperationData},
        profile::{
            FetchGroupProfileOperation, FetchRequestSenderProfileOperation,
            FetchUserProfileOperation,
        },
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
        self.fetch_queued_profiles::<FetchUserProfileOperation>(task_id, now)
            .await?;
        self.fetch_queued_profiles::<FetchRequestSenderProfileOperation>(task_id, now)
            .await?;
        self.fetch_queued_profiles::<FetchGroupProfileOperation>(task_id, now)
            .await?;
        Ok(())
    }

    /// Runs the queued profile fetches of one kind, until the queue is empty or
    /// a fetch is rescheduled.
    async fn fetch_queued_profiles<T>(
        &self,
        task_id: Uuid,
        now: DateTime<Utc>,
    ) -> anyhow::Result<()>
    where
        T: OperationData
            + Job<Output = (), DomainError = Infallible>
            + DeserializeOwned
            + Unpin
            + Send
            + 'static,
    {
        while let Some(op) = self
            .db
            .with_write_transaction(async |txn| Operation::<T>::dequeue(txn, task_id, now).await)
            .await?
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
                op.delete(self.db.write().await?).await?;
            }
            Err(JobError::NetworkError) => {
                debug!(
                    ?operation_id,
                    "Failed to fetch profile due to network error"
                );
                if op.retries + 1 < NUM_RETRIES {
                    op.reschedule(self.db.write().await?, now + RETRY_AFTER)
                        .await?;
                    return Ok(ControlFlow::Break(()));
                } else {
                    let retries = op.retries;
                    error!(
                        ?operation_id,
                        retries, "Reached max number of retries; giving up"
                    );
                    op.delete(self.db.write().await?).await?;
                    return Ok(ControlFlow::Continue(()));
                }
            }
            Err(JobError::RateLimited { retry_after }) => {
                // Never give up, the server did not process the request.
                info!(
                    ?operation_id,
                    ?retry_after,
                    "Rate limited while fetching profile"
                );
                let retry_after = retry_after.unwrap_or_default().max(RETRY_AFTER);
                op.reschedule(self.db.write().await?, now + retry_after)
                    .await?;
                return Ok(ControlFlow::Break(()));
            }
            Err(
                error @ (JobError::Blocked
                | JobError::Fatal(_)
                | JobError::NotFound
                | JobError::Domain(_)),
            ) => {
                // These error cases must not happen when fetching profiles.
                error!(?operation_id, %error, "Failed to fetch profile; deleting operation");
                op.delete(self.db.write().await?).await?;
            }
        }

        Ok(ControlFlow::Continue(()))
    }
}
