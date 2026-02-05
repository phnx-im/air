// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use airapiclient::{ApiClientInitError, ds_api::DsRequestError};
use aircommon::codec;
use chrono::{DateTime, Utc};
use sqlx::SqlitePool;
use thiserror::Error;

use crate::{
    clients::api_clients::ApiClients, key_stores::MemoryUserKeyStore, store::StoreNotifier,
};

pub(crate) mod chat_operation;
pub(crate) mod create_chat;
pub(crate) mod pending_chat_operation;

pub(crate) struct JobContext<'a> {
    pub api_clients: &'a ApiClients,
    pub pool: SqlitePool,
    pub notifier: &'a mut StoreNotifier,
    pub key_store: &'a MemoryUserKeyStore,
    pub now: DateTime<Utc>,
}

#[derive(Debug, Error)]
pub(crate) enum JobError {
    #[error("Network error")]
    NetworkError,
    #[error("Fatal error: {0}")]
    FatalError(#[from] anyhow::Error),
}

pub(crate) trait Job {
    type Output;

    async fn execute(mut self, context: &mut JobContext<'_>) -> Result<Self::Output, JobError>
    where
        Self: Sized,
    {
        Box::pin(self.execute_dependencies(context)).await?;
        Box::pin(self.execute_logic(context)).await
    }

    async fn execute_logic(self, context: &mut JobContext<'_>) -> Result<Self::Output, JobError>;

    async fn execute_dependencies(
        &mut self,
        _context: &mut JobContext<'_>,
    ) -> Result<(), JobError> {
        Ok(())
    }
}

impl From<DsRequestError> for JobError {
    fn from(err: DsRequestError) -> Self {
        // Network erros can occur without any fault of the job itself, so we
        // only log info here.
        tracing::info!("Job failed due to network error: {:?}", err);
        Self::NetworkError
    }
}

// The following errors are universally considered fatal for jobs.
impl From<sqlx::Error> for JobError {
    fn from(err: sqlx::Error) -> Self {
        JobError::FatalError(anyhow::Error::new(err))
    }
}

impl From<ApiClientInitError> for JobError {
    fn from(err: ApiClientInitError) -> Self {
        JobError::FatalError(anyhow::Error::new(err))
    }
}

impl From<codec::Error> for JobError {
    fn from(err: codec::Error) -> Self {
        JobError::FatalError(anyhow::Error::new(err))
    }
}

impl From<tls_codec::Error> for JobError {
    fn from(err: tls_codec::Error) -> Self {
        JobError::FatalError(anyhow::Error::new(err))
    }
}
