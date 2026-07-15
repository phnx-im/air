// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use thiserror::Error;
use tonic::Status;
use tracing::error;

use crate::{errors::StorageError, qs::staged_key_package::StageKeyPackageError};

// === Client ===

#[derive(Debug, Error)]
pub(crate) enum QsCreateClientRecordError {
    /// Unrecoverable implementation error
    #[error("Library Error")]
    LibraryError,
    /// Error creating client record
    #[error("Error creating user record")]
    StorageError,
}

impl From<QsCreateClientRecordError> for Status {
    fn from(e: QsCreateClientRecordError) -> Self {
        let msg = e.to_string();
        match e {
            QsCreateClientRecordError::LibraryError | QsCreateClientRecordError::StorageError => {
                Status::internal(msg)
            }
        }
    }
}

#[derive(Debug, Error)]
pub(crate) enum QsUpdateClientRecordError {
    /// Client not found
    #[error("Client not found")]
    UnknownClient,
    /// Error creating client record
    #[error("Error creating user record")]
    StorageError,
}

impl From<QsUpdateClientRecordError> for Status {
    fn from(e: QsUpdateClientRecordError) -> Self {
        let msg = e.to_string();
        match e {
            QsUpdateClientRecordError::UnknownClient => Status::not_found(msg),
            QsUpdateClientRecordError::StorageError => Status::internal(msg),
        }
    }
}

// === User ===

#[derive(Debug, Error)]
pub(crate) enum QsCreateUserError {
    /// Error creating client record
    #[error("Error creating user record")]
    StorageError,
}

#[derive(Debug, Error)]
pub(crate) enum QsUpdateUserError {
    /// User not found
    #[error("User not found")]
    UnknownUser,
    /// Error updating user record
    #[error("Error updating user record")]
    StorageError,
}

#[derive(Debug, Error)]
pub(crate) enum QsDeleteUserError {
    /// Error deleting user record
    #[error("Error deleting user record")]
    StorageError,
}

// === Key Packages ===

#[derive(Debug, Error)]
pub(crate) enum QsPublishKeyPackagesError {
    /// Error publishing key packages
    #[error("Error publishing key packages")]
    StorageError,
    /// Invalid KeyPackage
    #[error("Invalid KeyPackage")]
    InvalidKeyPackage,
}

impl From<StorageError> for QsPublishKeyPackagesError {
    fn from(error: StorageError) -> Self {
        error!(%error, "Error publishing key packages");
        Self::StorageError
    }
}

impl From<sqlx::Error> for QsPublishKeyPackagesError {
    fn from(error: sqlx::Error) -> Self {
        error!(%error, "Error publishing key packages");
        Self::StorageError
    }
}

impl From<QsPublishKeyPackagesError> for Status {
    fn from(e: QsPublishKeyPackagesError) -> Self {
        let msg = e.to_string();
        match e {
            QsPublishKeyPackagesError::StorageError => Status::internal(msg),
            QsPublishKeyPackagesError::InvalidKeyPackage => Status::invalid_argument(msg),
        }
    }
}

#[derive(Debug, Error)]
pub(crate) enum QsStageKeyPackagesError {
    #[error("Empty batch")]
    EmptyBatch,
    #[error("Invalid KeyPackage TLS")]
    InvalidKeyPackageTls,
    #[error("Invalid KeyPackage")]
    InvalidKeyPackage,
    #[error("Unknown client")]
    UnknownClient,
    #[error(transparent)]
    Sqlx(#[from] sqlx::Error),
    #[error(transparent)]
    Stage(#[from] StageKeyPackageError),
    #[error(transparent)]
    Codec(#[from] aircommon::codec::Error),
}

impl From<QsStageKeyPackagesError> for Status {
    fn from(error: QsStageKeyPackagesError) -> Self {
        match error {
            QsStageKeyPackagesError::EmptyBatch
            | QsStageKeyPackagesError::InvalidKeyPackageTls
            | QsStageKeyPackagesError::InvalidKeyPackage
            | QsStageKeyPackagesError::UnknownClient
            | QsStageKeyPackagesError::Stage(
                StageKeyPackageError::MissingKeyPackage | StageKeyPackageError::KeyPackageMismatch,
            ) => Status::invalid_argument(error.to_string()),
            QsStageKeyPackagesError::Sqlx(error)
            | QsStageKeyPackagesError::Stage(StageKeyPackageError::Sqlx(error)) => {
                error!(%error, "Failed to stage key packages");
                Status::internal("Storage error")
            }
            QsStageKeyPackagesError::Codec(error) => {
                error!(%error, "Failed to encode key package");
                Status::internal("Storage error")
            }
        }
    }
}

#[derive(Debug, Error)]
pub(crate) enum QsKeyPackageError {
    /// Error retrieving user key packages
    #[error("Error retrieving user key packages")]
    StorageError,
}

impl From<QsKeyPackageError> for Status {
    fn from(e: QsKeyPackageError) -> Self {
        let msg = e.to_string();
        match e {
            QsKeyPackageError::StorageError => Status::internal(msg),
        }
    }
}

#[derive(Debug, Error)]
pub(crate) enum QsEncryptionKeyError {
    /// Library error
    #[error("Library Error")]
    LibraryError,
    /// Error retrieving encryption key
    #[error("Error retrieving encryption key")]
    StorageError,
}

impl From<QsEncryptionKeyError> for Status {
    fn from(e: QsEncryptionKeyError) -> Self {
        let msg = e.to_string();
        match e {
            QsEncryptionKeyError::LibraryError | QsEncryptionKeyError::StorageError => {
                Status::internal(msg)
            }
        }
    }
}
