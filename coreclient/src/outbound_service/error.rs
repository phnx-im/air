// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use airapiclient::ds_api::DsRequestError;

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

/// Errors that occur while running the outbound service. Fatal errors will
/// cause just the current task to be skipped, while network errors will cause
/// the entire run to be skipped (i.e. no further tasks will be executed until
/// the next run).
#[derive(Debug, thiserror::Error)]
pub(super) enum OutboundServiceRunError {
    #[error("Network error, skipping remaining outbound service tasks for this run")]
    NetworkError,
    #[error("Fatal error: {0}")]
    Fatal(anyhow::Error),
}

impl From<anyhow::Error> for OutboundServiceRunError {
    fn from(error: anyhow::Error) -> Self {
        Self::Fatal(error)
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
