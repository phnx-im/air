// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::errors::{QueueError, StorageError};

use super::network_provider::NetworkProvider;
use aircommon::crypto::errors::{DecryptionError, EncryptionError, KeyGenerationError};
use thiserror::Error;
use tracing::error;

// === DS API errors ===

/// Error fetching a message from the QS.
#[derive(Error, Debug)]
pub enum QsEnqueueError<N: NetworkProvider> {
    /// Unseal error
    #[error(transparent)]
    UnsealError(#[from] DecryptionError),
    /// An error occurred enqueueing in a fan out queue
    #[error(transparent)]
    EnqueueError(#[from] EnqueueError),
    /// An error occurred while sending a message to the network
    #[error("An error occurred while sending a message to the network")]
    NetworkError(N::NetworkError),
    /// Storage provider error
    #[error("Storage provider error")]
    StorageError,
    /// Unrecoverable implementation error
    #[error("Library Error")]
    LibraryError,
    /// Invalid response
    #[error("Invalid response")]
    InvalidResponse,
}

/// Error enqueuing a fanned-out message.
#[derive(Error, Debug, PartialEq, Eq, Clone)]
pub enum EnqueueError {
    /// Unrecoverable implementation error
    #[error("Library Error")]
    LibraryError, // E.g. an error while encoding a message before enqueuing it.
    /// Error in the underlying storage provider
    #[error("Error in the underlying storage provider")]
    Storage,
    /// Client not found
    #[error("Client not found")]
    ClientNotFound,
}

impl From<StorageError> for EnqueueError {
    fn from(error: StorageError) -> Self {
        error!(%error, "Failed to enqueue message");
        EnqueueError::Storage
    }
}

impl From<sqlx::Error> for EnqueueError {
    fn from(error: sqlx::Error) -> Self {
        error!(%error, "Failed to enqueue message");
        EnqueueError::Storage
    }
}

impl From<QueueError> for EnqueueError {
    fn from(error: QueueError) -> Self {
        error!(%error, "Failed to enqueue message");
        EnqueueError::Storage
    }
}

impl From<EncryptionError> for EnqueueError {
    fn from(error: EncryptionError) -> Self {
        error!(%error, "Failed to enqueue message");
        EnqueueError::LibraryError
    }
}

// === Internal errors ===

#[derive(Debug, Error)]
pub(super) enum GenerateAndStoreError {
    #[error("Error generating signature keypair")]
    KeyGenerationError(#[from] KeyGenerationError),
    #[error("Error storing key")]
    StorageError(#[from] StorageError),
}
