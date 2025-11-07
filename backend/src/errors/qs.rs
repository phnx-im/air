// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use thiserror::Error;
use tonic::Status;

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
    /// Error deleteing user record
    #[error("Error deleteing user record")]
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
