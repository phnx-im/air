// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use apqmls::processing::ApqProcessPublicMessageError;
use openmls::group::PublicProcessMessageError;
use openmls_traits::{
    public_storage::PublicStorageProvider as PublicStorageProviderTrait, storage::CURRENT_VERSION,
};
use thiserror::Error;

use crate::group::process::GroupInfoValidationError;

pub type StorageError<Provider> =
    <Provider as PublicStorageProviderTrait<CURRENT_VERSION>>::PublicError;

/// Process message error
#[derive(Error, Debug, PartialEq, Clone)]
pub enum ProcessAssistedMessageError {
    /// Invalid assisted message.
    #[error("Invalid assisted message.")]
    InvalidAssistedMessage,
    /// See [`LibraryError`] for more details.
    #[error(transparent)]
    LibraryError(#[from] LibraryError),
    /// See [`ProcessMessageError`] for more details.
    #[error(transparent)]
    ProcessMessageError(#[from] PublicProcessMessageError),
    #[error(transparent)]
    GroupInfoValidation(#[from] GroupInfoValidationError),
}

/// Process message error
#[derive(Error, Debug, PartialEq, Clone)]
pub enum ProcessApqAssistedMessageError {
    /// Invalid assisted message.
    #[error("Invalid assisted message.")]
    InvalidAssistedMessage,
    /// See [`LibraryError`] for more details.
    #[error(transparent)]
    LibraryError(#[from] LibraryError),
    /// See [`ProcessMessageError`] for more details.
    #[error(transparent)]
    ProcessMessageError(#[from] ApqProcessPublicMessageError),
    #[error(transparent)]
    GroupInfoValidation(#[from] GroupInfoValidationError),
}

#[derive(Error, Debug, PartialEq, Clone)]
pub enum LibraryError {
    /// See [`LibraryError`] for more details.
    #[error("Error in the implementation of this Library.")]
    LibraryError,
    #[error(transparent)]
    OpenMlsLibraryError(#[from] openmls::prelude::LibraryError),
}
