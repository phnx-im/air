// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::time::Duration;

use airapiclient::{
    ApiClientInitError, ClassifyRequestError, RequestErrorKind, as_api::AsRequestError,
    ds_api::DsRequestError,
};
use aircommon::{codec, identifiers::QsClientId};
use chrono::{DateTime, Utc};
use sqlx::SqliteConnection;
use thiserror::Error;
use tracing::{info, warn};

use crate::{
    clients::api_clients::ApiClients,
    db::{
        access::{
            DbAccess, ReadConnection, ReadDbConnection, ReadDbTransaction, WriteConnection,
            WriteDbConnection, WriteDbTransaction,
        },
        notification::DbNotifier,
    },
    key_stores::MemoryUserKeyStore,
};

pub(crate) mod add_members;
pub(crate) mod chat_operation;
pub(crate) mod create_chat;
pub(crate) mod operation;
pub(crate) mod pending_chat_operation;
pub(crate) mod profile;

pub(crate) struct JobContext<'a, 'c> {
    pub api_clients: &'a ApiClients,
    pub http_client: &'a reqwest::Client,
    pub db: JobContextDb<'a, 'c>,
    pub key_store: &'a MemoryUserKeyStore,
    pub now: DateTime<Utc>,
    pub qs_client_id: &'a QsClientId,
}

pub(crate) enum JobContextDb<'a, 'c> {
    Db(DbAccess),
    Transaction(&'a mut WriteDbTransaction<'c>),
}

pub(crate) enum JobContextReadConnection<'s, 'c> {
    Connection(ReadDbConnection),
    Transaction(&'s mut WriteDbTransaction<'c>),
}

impl<'s, 'c> ReadConnection for JobContextReadConnection<'s, 'c> {}

impl<'s, 'c> AsMut<SqliteConnection> for JobContextReadConnection<'s, 'c> {
    fn as_mut(&mut self) -> &mut SqliteConnection {
        use JobContextReadConnection::*;
        match self {
            Connection(db) => db.as_mut(),
            Transaction(txn) => txn.as_mut(),
        }
    }
}

impl<'s, 'c> JobContextReadConnection<'s, 'c> {
    pub(crate) async fn begin(&mut self) -> sqlx::Result<ReadDbTransaction<'_>> {
        use JobContextReadConnection::*;
        Ok(match self {
            Connection(db) => db.begin().await?,
            Transaction(txn) => txn.begin_read().await?,
        })
    }
}

impl<'a, 'c> JobContextDb<'a, 'c> {
    pub(crate) async fn read<'s>(&'s mut self) -> sqlx::Result<JobContextReadConnection<'s, 'c>>
    where
        'a: 's,
    {
        use JobContextDb::*;
        match self {
            Db(db) => db.read().await.map(JobContextReadConnection::Connection),
            Transaction(txn) => Ok(JobContextReadConnection::Transaction(txn)),
        }
    }

    pub(crate) async fn write<'s>(&'s mut self) -> sqlx::Result<JobContextWriteConnection<'s, 'c>>
    where
        'a: 's,
    {
        use JobContextDb::*;
        match self {
            Db(db) => db.write().await.map(JobContextWriteConnection::Connection),
            Transaction(txn) => Ok(JobContextWriteConnection::Transaction(txn)),
        }
    }
}

pub(crate) enum JobContextWriteConnection<'a, 'c> {
    Connection(WriteDbConnection),
    Transaction(&'a mut WriteDbTransaction<'c>),
}

impl<'a, 'c> ReadConnection for JobContextWriteConnection<'a, 'c> {}

impl<'a, 'c> WriteConnection for JobContextWriteConnection<'a, 'c> {
    fn split(&mut self) -> (&mut SqliteConnection, &mut DbNotifier) {
        use JobContextWriteConnection::*;
        match self {
            Connection(connection) => connection.split(),
            Transaction(txn) => txn.split(),
        }
    }

    fn notifier(&mut self) -> &mut DbNotifier {
        use JobContextWriteConnection::*;
        match self {
            Connection(connection) => connection.notifier(),
            Transaction(txn) => txn.notifier(),
        }
    }

    async fn with_transaction<T, E>(
        &mut self,
        f: impl AsyncFnOnce(&mut WriteDbTransaction<'_>) -> Result<T, E>,
    ) -> Result<T, E>
    where
        T: Send,
        E: From<sqlx::Error>,
    {
        use JobContextWriteConnection::*;
        match self {
            Connection(db) => db.with_transaction(f).await,
            Transaction(txn) => txn.with_transaction(f).await,
        }
    }
}

impl<'a, 'c> AsMut<SqliteConnection> for JobContextWriteConnection<'a, 'c> {
    fn as_mut(&mut self) -> &mut SqliteConnection {
        use JobContextWriteConnection::*;
        match self {
            Connection(db) => db.as_mut(),
            Transaction(txn) => txn.as_mut(),
        }
    }
}

#[derive(Debug, Error)]
pub(crate) enum JobError<E> {
    #[error(transparent)]
    Domain(E),
    #[error("Network error")]
    NetworkError,
    /// The server rate limited the request and did not process it.
    #[error("Rate limited, retry after {retry_after:?}")]
    RateLimited {
        /// The wait time the server asked for, if any.
        retry_after: Option<Duration>,
    },
    #[error("Blocked")]
    Blocked,
    #[error("Not found")]
    NotFound,
    #[error("Recoverable error: {0}")]
    Recoverable(#[from] anyhow::Error),
    #[error(transparent)]
    Fatal(anyhow::Error),
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
        context: &mut JobContext<'_, '_>,
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
        context: &mut JobContext<'_, '_>,
    ) -> impl Future<Output = Result<Self::Output, JobError<Self::DomainError>>> + Send;

    fn execute_dependencies(
        &mut self,
        _context: &mut JobContext<'_, '_>,
    ) -> impl Future<Output = Result<(), JobError<Self::DomainError>>> + Send {
        async { Ok(()) }
    }
}

impl<E> From<AsRequestError> for JobError<E> {
    fn from(error: AsRequestError) -> Self {
        Self::from_request_error(error)
    }
}

impl<E> From<DsRequestError> for JobError<E> {
    fn from(error: DsRequestError) -> Self {
        Self::from_request_error(error)
    }
}

impl<E> JobError<E> {
    fn from_request_error(
        error: impl ClassifyRequestError + std::error::Error + Send + Sync + 'static,
    ) -> Self {
        match error.kind() {
            RequestErrorKind::RateLimited { retry_after } => {
                info!(?error, "Job failed due to rate limiting");
                Self::RateLimited { retry_after }
            }
            RequestErrorKind::Network => {
                info!(?error, "Job failed due to network error");
                Self::NetworkError
            }
            RequestErrorKind::NotFound => Self::NotFound,
            RequestErrorKind::ServerError => Self::Recoverable(error.into()),
            RequestErrorKind::Rejected => Self::Fatal(error.into()),
        }
    }
}

impl<E> From<reqwest::Error> for JobError<E> {
    fn from(error: reqwest::Error) -> Self {
        match error.status() {
            Some(reqwest::StatusCode::TOO_MANY_REQUESTS) => {
                // The response headers are not kept by reqwest's error
                info!(?error, "Job failed due to rate limiting");
                Self::RateLimited { retry_after: None }
            }
            Some(status) if status.is_server_error() => Self::Recoverable(error.into()),
            Some(_) => Self::Fatal(error.into()),
            // Failed to send the request or to read the response
            None if error.is_connect()
                || error.is_timeout()
                || error.is_request()
                || error.is_body() =>
            {
                warn!(?error, "Job failed due to network error");
                Self::NetworkError
            }
            None => Self::Fatal(error.into()),
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

#[cfg(test)]
mod tests {
    use std::{assert_matches, convert::Infallible};

    use airprotos::common::v1::{
        DeviceLimitReachedDetail, StatusDetails, StatusDetailsCode, status_details,
    };
    use tonic::{Code, Status};

    use super::*;

    fn rate_limited() -> Status {
        let mut status = Status::resource_exhausted("Too Many Requests! Wait for 3s");
        status
            .metadata_mut()
            .insert("retry-after", "3".parse().unwrap());
        status
    }

    #[test]
    fn rate_limited_requests_map_to_rate_limited() {
        let error: JobError<Infallible> = AsRequestError::Tonic(rate_limited()).into();
        assert_matches!(
            error,
            JobError::RateLimited {
                retry_after: Some(retry_after)
            } if retry_after == Duration::from_secs(3)
        );

        let error: JobError<Infallible> = DsRequestError::Tonic(rate_limited()).into();
        assert_matches!(
            error,
            JobError::RateLimited {
                retry_after: Some(retry_after)
            } if retry_after == Duration::from_secs(3)
        );

        let status = Status::resource_exhausted("Too Many Requests!");
        let error: JobError<Infallible> = DsRequestError::Tonic(status).into();
        assert_matches!(error, JobError::RateLimited { retry_after: None });
    }

    #[test]
    fn resource_exhausted_with_details_is_not_rate_limited() {
        let status = StatusDetails {
            code: StatusDetailsCode::DeviceLimitReached.into(),
            detail: Some(status_details::Detail::DeviceLimitReached(
                DeviceLimitReachedDetail { max_devices: 2 },
            )),
        }
        .to_status(Code::ResourceExhausted, "max devices exceeded");
        let error: JobError<Infallible> = DsRequestError::Tonic(status).into();
        assert_matches!(error, JobError::Fatal(_));
    }
}
