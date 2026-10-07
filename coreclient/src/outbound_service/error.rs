// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::time::Duration;

use airapiclient::{
    ApiClientInitError, ClassifyRequestError, RequestErrorKind, as_api::AsRequestError,
    ds_api::DsRequestError, qs_api::QsRequestError,
};
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
        .is_some_and(|error| !matches!(error.kind(), RequestErrorKind::Network))
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
    Recoverable(anyhow::Error),
    #[error("Fatal error: {0}")]
    Fatal(anyhow::Error),
}

impl OutboundServiceError {
    /// Reports this error as fatal, unless the anyhow chain contains a rate
    /// limited or network request error. Neither is specific to the item, so
    /// they are reported as such instead of dropping the item.
    pub(crate) fn fatal(error: impl Into<anyhow::Error>) -> Self {
        let error = error.into();
        transient_request_error(&error).unwrap_or_else(|| Self::Fatal(error))
    }

    /// Reports this error as recoverable, unless the anyhow chain contains a
    /// rate limited request error.
    pub(crate) fn recoverable(error: impl Into<anyhow::Error>) -> Self {
        let error = error.into();
        match transient_request_error(&error) {
            Some(rate_limited @ Self::RateLimited { .. }) => rate_limited,
            _ => Self::Recoverable(error),
        }
    }
}

/// See [`OutboundServiceError::recoverable`].
// TODO: remove me
impl From<anyhow::Error> for OutboundServiceError {
    fn from(error: anyhow::Error) -> Self {
        Self::recoverable(error)
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
///
/// TODO(gabriel): remove this abomination by making sure we bubble up this
/// correctly from all callsites.
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
    fn from(error: sqlx::Error) -> Self {
        Self::Recoverable(error.into())
    }
}

impl From<DsRequestError> for OutboundServiceError {
    fn from(error: DsRequestError) -> Self {
        // The queue resolves a wrong epoch, so a later attempt may succeed
        if error.is_wrong_epoch() {
            return Self::Recoverable(error.into());
        }
        Self::from_request_error(error)
    }
}

impl From<QsRequestError> for OutboundServiceError {
    fn from(error: QsRequestError) -> Self {
        // Retrying with the same client version will not help
        if error.is_unsupported_version() {
            return Self::Fatal(error.into());
        }
        Self::from_request_error(error)
    }
}

impl OutboundServiceError {
    /// Server errors are recoverable, everything the server refused for good
    /// is fatal.
    fn from_request_error(
        error: impl ClassifyRequestError + std::error::Error + Send + Sync + 'static,
    ) -> Self {
        match error.kind() {
            RequestErrorKind::RateLimited { retry_after } => Self::RateLimited { retry_after },
            RequestErrorKind::Network => Self::NetworkError,
            RequestErrorKind::ServerError => Self::Recoverable(error.into()),
            RequestErrorKind::NotFound | RequestErrorKind::Rejected => Self::Fatal(error.into()),
        }
    }
}

/// A job that is blocked or whose target is gone will not succeed on retry,
/// so these are fatal like domain errors.
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
            Err(OutboundServiceError::Fatal(error) | OutboundServiceError::Recoverable(error)) => {
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

    #[test]
    fn rate_limits_are_found_in_anyhow_chains() {
        let mut status = Status::resource_exhausted("Too Many Requests!");
        status
            .metadata_mut()
            .insert("retry-after", "3".parse().unwrap());
        let error = anyhow::Error::from(AsRequestError::Tonic(status)).context("refreshing");
        assert_matches!(
            OutboundServiceError::from(error),
            OutboundServiceError::RateLimited { retry_after: Some(retry_after) }
                if retry_after == Duration::from_secs(3)
        );

        let error = anyhow::Error::from(QsRequestError::Tonic(Status::internal("boom")));
        assert_matches!(
            OutboundServiceError::from(error),
            OutboundServiceError::Recoverable(_)
        );
    }

    #[test]
    fn only_fatal_reports_network_errors_from_anyhow_chains() {
        let network = || {
            anyhow::Error::from(AsRequestError::Tonic(Status::unavailable("down")))
                .context("verifying credentials")
        };
        assert_matches!(
            OutboundServiceError::fatal(network()),
            OutboundServiceError::NetworkError
        );
        assert_matches!(
            OutboundServiceError::recoverable(network()),
            OutboundServiceError::Recoverable(_)
        );
        assert_matches!(
            OutboundServiceError::fatal(anyhow::anyhow!("invalid group state")),
            OutboundServiceError::Fatal(_)
        );
    }
}
