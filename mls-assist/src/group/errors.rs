// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use openmls::group::PublicProcessMessageError;
use openmls_traits::{
    public_storage::PublicStorageProvider as PublicStorageProviderTrait, storage::CURRENT_VERSION,
};
use thiserror::Error;

#[cfg(doc)]
use openmls::prelude::{GroupContext, ProcessedMessage, group_info::GroupInfo};

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
    /// Invalid group info signature.
    #[error("Invalid group info signature.")]
    InvalidGroupInfoSignature,
    /// Invalid group info message.
    #[error("Invalid group info message.")]
    InvalidGroupInfoMessage,
    /// See [`ProcessMessageError`] for more details.
    #[error(transparent)]
    ProcessMessageError(#[from] PublicProcessMessageError),
    /// Unknown sender.
    #[error("Unknown sender.")]
    UnknownSender,
    /// [`GroupContext`] is inconsistent between [`ProcessedMessage`] and [`GroupInfo`].
    #[error("[`GroupContext`] is inconsistent between [`ProcessedMessage`] and [`GroupInfo`].")]
    InconsistentGroupContext,
}

#[derive(Error, Debug, PartialEq, Clone)]
pub enum LibraryError {
    /// See [`LibraryError`] for more details.
    #[error("Error in the implementation of this Library.")]
    LibraryError,
    #[error(transparent)]
    OpenMlsLibraryError(#[from] openmls::prelude::LibraryError),
}
