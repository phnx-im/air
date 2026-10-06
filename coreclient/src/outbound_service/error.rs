// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::time::Duration;

use airapiclient::ds_api::DsRequestError;
use tracing::error;

use crate::outbound_service::WorkAborted;

/// Classifies a DS API error as fatal or recoverable.
///
/// Permanent server errors (e.g. group not found) are fatal — retrying will
/// never succeed. Transport/availability errors are recoverable.
pub(crate) fn classify_ds_error(error: DsRequestError) -> OutboundServiceError {
    if error.is_not_found() {
        OutboundServiceError::fatal(error)
    } else {
        OutboundServiceError::recoverable(error)
    }
}

pub(crate) fn is_ds_not_found_error(error: &anyhow::Error) -> bool {
    error
        .downcast_ref::<DsRequestError>()
        .is_some_and(DsRequestError::is_not_found)
}

/// Whether the DS rate limited a request.
pub(crate) fn is_ds_rate_limited_error(error: &anyhow::Error) -> bool {
    error
        .downcast_ref::<DsRequestError>()
        .is_some_and(DsRequestError::is_rate_limited)
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

/// Errors that occur while running the outbound service. Fatal errors will
/// cause just the current task to be skipped, while network errors will cause
/// the entire run to be skipped (i.e. no further tasks will be executed until
/// the next run).
#[derive(Debug, thiserror::Error)]
pub(super) enum OutboundServiceRunError {
    #[error("Network error, skipping remaining outbound service tasks for this run")]
    NetworkError,
    #[error("Rate limited, skipping remaining outbound service tasks for this run")]
    RateLimited { retry_after: Option<Duration> },
    #[error("Fatal error: {0}")]
    Fatal(anyhow::Error),
}

impl From<anyhow::Error> for OutboundServiceRunError {
    fn from(error: anyhow::Error) -> Self {
        Self::Fatal(error)
    }
}

impl From<sqlx::Error> for OutboundServiceRunError {
    fn from(error: sqlx::Error) -> Self {
        Self::Fatal(error.into())
    }
}

impl From<DsRequestError> for OutboundServiceRunError {
    fn from(error: DsRequestError) -> Self {
        if error.is_rate_limited() {
            Self::RateLimited {
                retry_after: error.retry_after(),
            }
        } else if error.is_network_error() {
            Self::NetworkError
        } else {
            Self::Fatal(error.into())
        }
    }
}

pub(super) trait RunResultExt {
    /// Logs fatal errors and continues, aborts the run otherwise.
    fn or_abort(self, task: &str) -> Result<(), WorkAborted>;
}

impl RunResultExt for Result<(), OutboundServiceRunError> {
    fn or_abort(self, task: &'static str) -> Result<(), WorkAborted> {
        match self {
            Ok(()) => Ok(()),
            Err(OutboundServiceRunError::Fatal(error)) => {
                error!(%error, task, "Outbound service task failed");
                Ok(())
            }
            Err(OutboundServiceRunError::NetworkError) => Err(WorkAborted::Interrupted),
            Err(OutboundServiceRunError::RateLimited { retry_after }) => {
                Err(WorkAborted::BackOff { retry_after })
            }
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum OutboundServiceError {
    #[error("Fatal error: {0}")]
    Fatal(anyhow::Error),
    #[error("Recoverable error: {0}")]
    Recoverable(anyhow::Error),
}

impl OutboundServiceError {
    pub(crate) fn fatal(error: impl Into<anyhow::Error>) -> Self {
        Self::Fatal(error.into())
    }

    pub(crate) fn recoverable(error: impl Into<anyhow::Error>) -> Self {
        Self::Recoverable(error.into())
    }
}

#[cfg(test)]
mod tests {
    use airprotos::common::v1::{
        DeviceLimitReachedDetail, StatusDetails, StatusDetailsCode, status_details,
    };
    use anyhow::Context;
    use tonic::{Code, Status};

    use super::*;

    #[test]
    fn rate_limited_ds_errors_are_detected() {
        let rate_limited: anyhow::Error =
            DsRequestError::Tonic(Status::resource_exhausted("Too Many Requests!")).into();
        assert!(is_ds_rate_limited_error(&rate_limited));

        let with_context = Err::<(), _>(DsRequestError::Tonic(Status::resource_exhausted(
            "Too Many Requests!",
        )))
        .context("failed to send reaction")
        .unwrap_err();
        assert!(is_ds_rate_limited_error(&with_context));

        let unavailable: anyhow::Error =
            DsRequestError::Tonic(Status::unavailable("server stopped")).into();
        assert!(!is_ds_rate_limited_error(&unavailable));

        let device_limit: anyhow::Error = DsRequestError::Tonic(
            StatusDetails {
                code: StatusDetailsCode::DeviceLimitReached.into(),
                detail: Some(status_details::Detail::DeviceLimitReached(
                    DeviceLimitReachedDetail { max_devices: 2 },
                )),
            }
            .to_status(Code::ResourceExhausted, "max devices exceeded"),
        )
        .into();
        assert!(!is_ds_rate_limited_error(&device_limit));
    }
}
