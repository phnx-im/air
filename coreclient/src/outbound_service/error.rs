// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::time::Duration;

use airapiclient::{
    ApiClientInitError, ClassifyRequestError, RequestErrorKind, as_api::AsRequestError,
    ds_api::DsRequestError, qs_api::QsRequestError,
};
use tracing::{error, info};

use crate::{
    db::is_db_busy,
    job::{
        JobError,
        recoverable::{Recoverable, RequestFailure},
    },
    outbound_service::WorkAborted,
};

pub(crate) fn is_ds_not_found_error(error: &anyhow::Error) -> bool {
    error
        .downcast_ref::<DsRequestError>()
        .is_some_and(DsRequestError::is_not_found)
}

/// Errors that occur while running the outbound service.
///
/// Fatal and recoverable errors concern a single item: a fatal one drops it, a
/// recoverable one keeps it for a later run. Either way the run continues with
/// the next item or task. Network errors and rate limiting abort the run.
#[derive(Debug, thiserror::Error)]
pub(crate) enum OutboundServiceError {
    #[error("Network error")]
    NetworkError,
    #[error("Rate limited, retry after {retry_after:?}")]
    RateLimited { retry_after: Option<Duration> },
    #[error("Recoverable error: {0}")]
    Recoverable(Recoverable),
    #[error("Fatal error: {0}")]
    Fatal(anyhow::Error),
}

impl OutboundServiceError {
    /// Reports this error as fatal, unless the anyhow chain contains a rate
    /// limited or network request error, or a busy database. None of them is
    /// specific to the item, so they are reported as such instead of dropping
    /// the item.
    pub(crate) fn fatal(error: impl Into<anyhow::Error>) -> Self {
        let error = error.into();
        if let Some(transient) = transient_request_error(&error) {
            return transient;
        }
        let is_busy = error
            .chain()
            .any(|error| error.downcast_ref::<sqlx::Error>().is_some_and(is_db_busy));
        if is_busy {
            return Self::Recoverable(Recoverable::busy(error));
        }
        Self::Fatal(error)
    }
}

impl From<ApiClientInitError> for OutboundServiceError {
    fn from(error: ApiClientInitError) -> Self {
        // Building a client does not touch the network, so these are configuration
        // errors only.
        Self::fatal(error)
    }
}

/// The first rate limited or network request error in the chain of `error`,
/// as [`OutboundServiceError::RateLimited`] or
/// [`OutboundServiceError::NetworkError`].
fn transient_request_error(error: &anyhow::Error) -> Option<OutboundServiceError> {
    error.chain().find_map(|error| {
        let kind = error
            .downcast_ref::<AsRequestError>()
            .map(ClassifyRequestError::kind)
            .or_else(|| {
                error
                    .downcast_ref::<DsRequestError>()
                    .map(ClassifyRequestError::kind)
            })
            .or_else(|| {
                error
                    .downcast_ref::<QsRequestError>()
                    .map(ClassifyRequestError::kind)
            })?;
        match kind {
            RequestErrorKind::RateLimited { retry_after } => {
                Some(OutboundServiceError::RateLimited { retry_after })
            }
            RequestErrorKind::Network => Some(OutboundServiceError::NetworkError),
            RequestErrorKind::NotFound
            | RequestErrorKind::Rejected
            | RequestErrorKind::ServerError => None,
        }
    })
}

impl From<sqlx::Error> for OutboundServiceError {
    /// Only lock contention is recoverable, every other database error is
    /// fatal.
    fn from(error: sqlx::Error) -> Self {
        if is_db_busy(&error) {
            Self::Recoverable(Recoverable::busy(error))
        } else {
            Self::Fatal(error.into())
        }
    }
}

impl From<DsRequestError> for OutboundServiceError {
    fn from(error: DsRequestError) -> Self {
        Self::from_request_failure(RequestFailure::classify_ds(error))
    }
}

impl From<QsRequestError> for OutboundServiceError {
    fn from(error: QsRequestError) -> Self {
        Self::from_request_failure(RequestFailure::classify(error))
    }
}

impl OutboundServiceError {
    fn from_request_failure(failure: RequestFailure) -> Self {
        match failure {
            RequestFailure::RateLimited { retry_after, .. } => Self::RateLimited { retry_after },
            RequestFailure::Network(_) => Self::NetworkError,
            RequestFailure::Recoverable(recoverable) => Self::Recoverable(recoverable),
            // Not found keeps the request error, see `is_ds_not_found_error`
            RequestFailure::NotFound(error) | RequestFailure::Fatal(error) => Self::Fatal(error),
        }
    }
}

impl<E> From<JobError<E>> for OutboundServiceError
where
    E: std::error::Error + Send + Sync + 'static,
{
    fn from(error: JobError<E>) -> Self {
        match error {
            JobError::NetworkError => Self::NetworkError,
            JobError::RateLimited { retry_after } => Self::RateLimited { retry_after },
            JobError::Recoverable(error) => Self::Recoverable(error),
            JobError::Fatal(error) => Self::Fatal(error),
            error @ (JobError::Domain(_) | JobError::Blocked | JobError::NotFound) => {
                Self::Fatal(error.into())
            }
        }
    }
}

pub(super) trait RunResultExt {
    /// Logs fatal and recoverable errors and continues, aborts the run
    /// otherwise.
    fn or_abort(self, task: &'static str) -> Result<(), WorkAborted>;
}

impl RunResultExt for Result<(), OutboundServiceError> {
    fn or_abort(self, task: &'static str) -> Result<(), WorkAborted> {
        match self {
            Ok(()) => Ok(()),
            Err(OutboundServiceError::Fatal(error)) => {
                error!(%error, task, "Outbound service task failed");
                Ok(())
            }
            Err(OutboundServiceError::Recoverable(error)) => {
                error!(%error, task, "Outbound service task failed");
                Ok(())
            }
            Err(OutboundServiceError::NetworkError) => {
                info!(
                    task,
                    "Network appears unavailable, aborting outbound service run"
                );
                Err(WorkAborted::Interrupted)
            }
            Err(OutboundServiceError::RateLimited { retry_after }) => {
                info!(
                    task,
                    ?retry_after,
                    "Rate limited, aborting outbound service run"
                );
                Err(WorkAborted::BackOff { retry_after })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::assert_matches;

    use tonic::Status;

    use super::*;
    use crate::job::recoverable::RecoverableCause;

    #[test]
    fn rate_limits_are_found_in_anyhow_chains() {
        let mut status = Status::resource_exhausted("Too Many Requests!");
        status
            .metadata_mut()
            .insert("retry-after", "3".parse().unwrap());
        let error = anyhow::Error::from(AsRequestError::Tonic(status)).context("refreshing");
        assert_matches!(
            OutboundServiceError::fatal(error),
            OutboundServiceError::RateLimited { retry_after: Some(retry_after) }
                if retry_after == Duration::from_secs(3)
        );
    }

    #[test]
    fn server_errors_are_recoverable() {
        let error = QsRequestError::Tonic(Status::internal("boom"));
        assert_matches!(
            OutboundServiceError::from(error),
            OutboundServiceError::Recoverable(Recoverable {
                cause: RecoverableCause::Server,
                ..
            })
        );
    }

    #[test]
    fn network_errors_are_found_in_anyhow_chains() {
        let error = anyhow::Error::from(AsRequestError::Tonic(Status::unavailable("down")))
            .context("verifying credentials");
        assert_matches!(
            OutboundServiceError::fatal(error),
            OutboundServiceError::NetworkError
        );
        assert_matches!(
            OutboundServiceError::fatal(anyhow::anyhow!("invalid group state")),
            OutboundServiceError::Fatal(_)
        );
    }

    #[test]
    fn busy_database_is_found_in_anyhow_chains() {
        let error = anyhow::Error::from(sqlx::Error::PoolTimedOut).context("loading the chat");
        assert_matches!(
            OutboundServiceError::fatal(error),
            OutboundServiceError::Recoverable(Recoverable {
                cause: RecoverableCause::Busy,
                ..
            })
        );
        assert_matches!(
            OutboundServiceError::fatal(sqlx::Error::RowNotFound),
            OutboundServiceError::Fatal(_)
        );
    }
}
