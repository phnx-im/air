// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use airapiclient::{ApiClientInitError, as_api::AsRequestError, ds_api::DsRequestError};
use aircommon::codec;
use chrono::{DateTime, Utc};
use thiserror::Error;
use tracing::info;

use crate::{
    clients::api_clients::ApiClients, db_access::DbAccess, key_stores::MemoryUserKeyStore,
};

pub(crate) mod chat_operation;
pub(crate) mod create_chat;
pub(crate) mod operation;
pub(crate) mod pending_chat_operation;
pub(crate) mod profile;

pub(crate) struct JobContext<'a> {
    pub api_clients: &'a ApiClients,
    pub http_client: &'a reqwest::Client,
    pub db: &'a DbAccess,
    pub key_store: &'a MemoryUserKeyStore,
    pub now: DateTime<Utc>,
}

#[derive(Debug, Error)]
pub(crate) enum JobError<E> {
    #[error(transparent)]
    Domain(E),
    #[error("Network error")]
    NetworkError,
    #[error("Blocked")]
    Blocked,
    #[error("Not found")]
    NotFound,
    #[error(transparent)]
    Fatal(#[from] anyhow::Error),
}

impl<E> JobError<E> {
    pub(crate) fn fatal(error: impl Into<anyhow::Error>) -> Self {
        Self::Fatal(error.into())
    }

    pub(crate) fn domain(error: impl Into<E>) -> Self {
        Self::Domain(error.into())
    }
}

pub(crate) trait Job: Send {
    type Output;

    /// Error which can occur when executing the job and is specific to the jobs domain.
    ///
    /// When such an error occurs, the job is considered to be failed and cannot be retried. The
    /// error should be propagated to the user.
    type DomainError: std::error::Error + Send + Sync + 'static;

    fn execute(
        mut self,
        context: &mut JobContext<'_>,
    ) -> impl Future<Output = Result<Self::Output, JobError<Self::DomainError>>> + Send
    where
        Self: Sized,
        Self::Output: Send,
    {
        async move {
            Box::pin(self.execute_dependencies(context)).await?;
            Box::pin(self.execute_logic(context)).await
        }
    }

    fn execute_logic(
        self,
        context: &mut JobContext<'_>,
    ) -> impl Future<Output = Result<Self::Output, JobError<Self::DomainError>>> + Send;

    fn execute_dependencies(
        &mut self,
        _context: &mut JobContext<'_>,
    ) -> impl Future<Output = Result<(), JobError<Self::DomainError>>> + Send {
        async { Ok(()) }
    }
}

impl<E> From<AsRequestError> for JobError<E> {
    fn from(error: AsRequestError) -> Self {
        if error.is_network_error() {
            info!(?error, "Job failed due to network error");
            Self::NetworkError
        } else {
            Self::Fatal(error.into())
        }
    }
}

impl<E> From<DsRequestError> for JobError<E> {
    fn from(error: DsRequestError) -> Self {
        if error.is_not_found() {
            Self::NotFound
        } else if error.is_network_error() {
            info!(?error, "Job failed due to network error");
            Self::NetworkError
        } else {
            Self::Fatal(error.into())
        }
    }
}

impl<E> From<reqwest::Error> for JobError<E> {
    fn from(error: reqwest::Error) -> Self {
        if error.is_connect() || error.is_timeout() {
            info!(?error, "Job failed due to network error");
            Self::NetworkError
        } else {
            Self::Fatal(error.into())
        }
    }
}

// The following errors are universally considered fatal for jobs.
impl<E> From<sqlx::Error> for JobError<E> {
    fn from(err: sqlx::Error) -> Self {
        JobError::Fatal(anyhow::Error::new(err))
    }
}

impl<E> From<ApiClientInitError> for JobError<E> {
    fn from(err: ApiClientInitError) -> Self {
        JobError::Fatal(anyhow::Error::new(err))
    }
}

impl<E> From<codec::Error> for JobError<E> {
    fn from(err: codec::Error) -> Self {
        JobError::Fatal(anyhow::Error::new(err))
    }
}

impl<E> From<tls_codec::Error> for JobError<E> {
    fn from(err: tls_codec::Error) -> Self {
        JobError::Fatal(anyhow::Error::new(err))
    }
}
