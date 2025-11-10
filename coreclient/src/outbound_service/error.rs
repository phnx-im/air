// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

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
