// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::time::Duration;

use airapiclient::{ClassifyRequestError, RequestErrorKind};

#[derive(Debug, thiserror::Error)]
#[error("{cause:?}: {error}")]
pub(crate) struct Recoverable {
    pub(crate) cause: RecoverableCause,
    #[source]
    pub(crate) error: anyhow::Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RecoverableCause {
    Server,
    WrongEpoch,
    Busy,
    Deferred,
}

impl Recoverable {
    pub(crate) fn server(error: impl Into<anyhow::Error>) -> Self {
        Self {
            cause: RecoverableCause::Server,
            error: error.into(),
        }
    }

    pub(crate) fn wrong_epoch(error: impl Into<anyhow::Error>) -> Self {
        Self {
            cause: RecoverableCause::WrongEpoch,
            error: error.into(),
        }
    }

    pub(crate) fn busy(error: impl Into<anyhow::Error>) -> Self {
        Self {
            cause: RecoverableCause::Busy,
            error: error.into(),
        }
    }

    pub(crate) fn deferred(error: impl Into<anyhow::Error>) -> Self {
        Self {
            cause: RecoverableCause::Deferred,
            error: error.into(),
        }
    }
}

/// A failed request, classified the same way for jobs and the outbound service.
#[derive(Debug)]
pub(crate) enum RequestFailure {
    RateLimited {
        retry_after: Option<Duration>,
        error: anyhow::Error,
    },
    Network(anyhow::Error),
    NotFound(anyhow::Error),
    Recoverable(Recoverable),
    Fatal(anyhow::Error),
}

impl RequestFailure {
    /// Server errors are recoverable, everything the server refused for good is fatal.
    pub(crate) fn classify(
        error: impl ClassifyRequestError + std::error::Error + Send + Sync + 'static,
    ) -> Self {
        match error.kind() {
            RequestErrorKind::RateLimited { retry_after } => Self::RateLimited {
                retry_after,
                error: error.into(),
            },
            RequestErrorKind::Network => Self::Network(error.into()),
            RequestErrorKind::NotFound => Self::NotFound(error.into()),
            RequestErrorKind::ServerError => Self::Recoverable(Recoverable::server(error)),
            RequestErrorKind::Rejected => Self::Fatal(error.into()),
        }
    }
}
