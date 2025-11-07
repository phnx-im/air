// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::crypto::errors::DecryptionError;
use mls_assist::messages::AssistedMessageError;
use openmls::group::{CreateMessageError, MlsGroupStateError, ProcessMessageError};
use thiserror::Error;

#[derive(Error, Debug)]
#[allow(clippy::enum_variant_names)]
pub enum GroupOperationError {
    #[error(transparent)]
    MlsGroupStateError(#[from] MlsGroupStateError),
    #[error(transparent)]
    CreateMessageError(#[from] CreateMessageError),
    #[error(transparent)]
    ProcessMessageError(#[from] ProcessMessageError<sqlx::Error>),
    #[error("Missing key package in key store")]
    MissingKeyPackage,
    #[error(transparent)]
    JoinerInfoDecryptionError(#[from] DecryptionError),
    #[error(transparent)]
    TlsCodecError(#[from] tls_codec::Error),
    #[error(transparent)]
    AssistedMessageError(#[from] AssistedMessageError),
    #[error(transparent)]
    MimiContentError(#[from] mimi_content::Error),
}
