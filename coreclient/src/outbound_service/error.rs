// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::time::Duration;

use airapiclient::{as_api::AsRequestError, ds_api::DsRequestError, qs_api::QsRequestError};
use tracing::{error, info};

use crate::{job::JobError, outbound_service::WorkAborted};

pub(crate) fn is_ds_not_found_error(error: &anyhow::Error) -> bool {
    error
        .downcast_ref::<DsRequestError>()
        .is_some_and(DsRequestError::is_not_found)
}

/// Whether the DS rejected a commit because the group moved on in the meantime.
pub(crate) fn is_ds_wrong_epoch_error(error: &anyhow::Error) -> bool {
    error
        .downcast_ref::<DsRequestError>()
        .is_some_and(DsRequestError::is_wrong_epoch)
}

/// Whether the DS answered and refused a request.
pub(crate) fn is_ds_rejection_error(error: &anyhow::Error) -> bool {
    // Anything which is not a network error
    error
        .downcast_ref::<DsRequestError>()
        .is_some_and(|error| !error.is_network_error())
}

/// Errors that occur while running the outbound service.
///
/// Fatal and recoverable errors concern a single item: a fatal one drops it, a
/// recoverable one keeps it for a later run. Either way the run continues with
/// the next item or task. Network errors and rate limiting abort the run.
#[derive(Debug, thiserror::Error)]
pub(crate) enum OutboundServiceRunError {
    #[error("Network error")]
    NetworkError,
    #[error("Rate limited, retry after {retry_after:?}")]
    RateLimited { retry_after: Option<Duration> },
    #[error("Recoverable error: {0}")]
    Recoverable(anyhow::Error),
    #[error("Fatal error: {0}")]
    Fatal(anyhow::Error),
}

impl OutboundServiceRunError {
    pub(crate) fn fatal(error: impl Into<anyhow::Error>) -> Self {
        Self::Fatal(error.into())
    }

    pub(crate) fn recoverable(error: impl Into<anyhow::Error>) -> Self {
        Self::Recoverable(error.into())
    }
}

/// A rate limit is never specific to an item, so it is recognized anywhere in
/// the error chain. Everything else is recoverable.
impl From<anyhow::Error> for OutboundServiceRunError {
    fn from(error: anyhow::Error) -> Self {
        match rate_limited(&error) {
            Some(retry_after) => Self::RateLimited { retry_after },
            None => Self::Recoverable(error),
        }
    }
}

/// The `retry_after` of a rate limited request error in `error`, if any.
fn rate_limited(error: &anyhow::Error) -> Option<Option<Duration>> {
    error.chain().find_map(|error| {
        if let Some(error) = error.downcast_ref::<AsRequestError>() {
            error.is_rate_limited().then(|| error.retry_after())
        } else if let Some(error) = error.downcast_ref::<DsRequestError>() {
            error.is_rate_limited().then(|| error.retry_after())
        } else if let Some(error) = error.downcast_ref::<QsRequestError>() {
            error.is_rate_limited().then(|| error.retry_after())
        } else {
            None
        }
    })
}

impl From<sqlx::Error> for OutboundServiceRunError {
    fn from(error: sqlx::Error) -> Self {
        Self::Recoverable(error.into())
    }
}

/// Permanent server errors (e.g. group not found) are fatal, other rejections
/// are recoverable.
impl From<DsRequestError> for OutboundServiceRunError {
    fn from(error: DsRequestError) -> Self {
        if error.is_rate_limited() {
            Self::RateLimited {
                retry_after: error.retry_after(),
            }
        } else if error.is_network_error() {
            Self::NetworkError
        } else if error.is_not_found() {
            Self::Fatal(error.into())
        } else {
            Self::Recoverable(error.into())
        }
    }
}

/// Protocol and validation errors are fatal, other server errors are
/// recoverable.
impl From<QsRequestError> for OutboundServiceRunError {
    fn from(error: QsRequestError) -> Self {
        if error.is_rate_limited() {
            Self::RateLimited {
                retry_after: error.retry_after(),
            }
        } else if error.is_network_error() {
            Self::NetworkError
        } else if error.is_unsupported_version() || !matches!(error, QsRequestError::Tonic(_)) {
            Self::Fatal(error.into())
        } else {
            Self::Recoverable(error.into())
        }
    }
}

/// A job that is blocked or whose target is gone will not succeed on retry,
/// so these are fatal like domain errors.
impl<E> From<JobError<E>> for OutboundServiceRunError
where
    E: std::error::Error + Send + Sync + 'static,
{
    fn from(error: JobError<E>) -> Self {
        match error {
            JobError::NetworkError => Self::NetworkError,
            JobError::RateLimited { retry_after } => Self::RateLimited { retry_after },
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

impl RunResultExt for Result<(), OutboundServiceRunError> {
    fn or_abort(self, task: &'static str) -> Result<(), WorkAborted> {
        match self {
            Ok(()) => Ok(()),
            Err(
                OutboundServiceRunError::Fatal(error) | OutboundServiceRunError::Recoverable(error),
            ) => {
                error!(%error, task, "Outbound service task failed");
                Ok(())
            }
            Err(OutboundServiceRunError::NetworkError) => {
                info!(
                    task,
                    "Network appears unavailable, aborting outbound service run"
                );
                Err(WorkAborted::Interrupted)
            }
            Err(OutboundServiceRunError::RateLimited { retry_after }) => {
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

    use airprotos::common::v1::{
        DeviceLimitReachedDetail, StatusDetails, StatusDetailsCode, status_details,
    };
    use tonic::{Code, Status};

    use super::*;

    #[test]
    fn ds_errors_are_classified() {
        let error = DsRequestError::Tonic(Status::resource_exhausted("Too Many Requests!"));
        assert_matches!(
            OutboundServiceRunError::from(error),
            OutboundServiceRunError::RateLimited { retry_after: None }
        );

        let error = DsRequestError::Tonic(Status::unavailable("server stopped"));
        assert_matches!(
            OutboundServiceRunError::from(error),
            OutboundServiceRunError::NetworkError
        );

        let error = DsRequestError::Tonic(Status::not_found("group not found"));
        assert_matches!(
            OutboundServiceRunError::from(error),
            OutboundServiceRunError::Fatal(_)
        );

        let error = DsRequestError::Tonic(
            StatusDetails {
                code: StatusDetailsCode::DeviceLimitReached.into(),
                detail: Some(status_details::Detail::DeviceLimitReached(
                    DeviceLimitReachedDetail { max_devices: 2 },
                )),
            }
            .to_status(Code::ResourceExhausted, "max devices exceeded"),
        );
        assert_matches!(
            OutboundServiceRunError::from(error),
            OutboundServiceRunError::Recoverable(_)
        );
    }

    #[test]
    fn qs_errors_are_classified() {
        let error = QsRequestError::Tonic(Status::resource_exhausted("Too Many Requests!"));
        assert_matches!(
            OutboundServiceRunError::from(error),
            OutboundServiceRunError::RateLimited { retry_after: None }
        );

        let error = QsRequestError::Tonic(Status::unavailable("server stopped"));
        assert_matches!(
            OutboundServiceRunError::from(error),
            OutboundServiceRunError::NetworkError
        );

        let error = QsRequestError::UnexpectedResponse;
        assert_matches!(
            OutboundServiceRunError::from(error),
            OutboundServiceRunError::Fatal(_)
        );

        let error = QsRequestError::Tonic(Status::internal("boom"));
        assert_matches!(
            OutboundServiceRunError::from(error),
            OutboundServiceRunError::Recoverable(_)
        );
    }

    #[test]
    fn rate_limits_are_found_in_anyhow_chains() {
        let mut status = Status::resource_exhausted("Too Many Requests!");
        status
            .metadata_mut()
            .insert("retry-after", "3".parse().unwrap());
        let error = anyhow::Error::from(AsRequestError::Tonic(status)).context("refreshing");
        assert_matches!(
            OutboundServiceRunError::from(error),
            OutboundServiceRunError::RateLimited { retry_after: Some(retry_after) }
                if retry_after == Duration::from_secs(3)
        );

        let error = anyhow::Error::from(QsRequestError::Tonic(Status::internal("boom")));
        assert_matches!(
            OutboundServiceRunError::from(error),
            OutboundServiceRunError::Recoverable(_)
        );
    }
}
