// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! This module defines types and functions for handling Pre-Shared Keys (PSKs)
//! in APQMLS.

use openmls::{
    group::{
        MlsGroup, PendingSafeExportSecretError, ProcessedMessageSafeExportSecretError,
        SafeExportSecretError,
    },
    prelude::{Ciphersuite, CryptoError},
    schedule::{PreSharedKeyId, Psk, errors::PskError, psk::ApplicationPsk},
    storage::OpenMlsProvider,
};
use openmls_traits::storage::StorageProvider as _;
use thiserror::Error;

use crate::{extension::APQMLS_COMPONENT_ID, secret::Secret};

/// Error while handling PSKs in APQMLS.
#[derive(Debug, Error)]
pub enum ApqPskError<StorageError> {
    #[error(transparent)]
    ExportFromGroup(#[from] SafeExportSecretError<StorageError>),
    #[error(transparent)]
    ExportFromProcessed(#[from] ProcessedMessageSafeExportSecretError),
    #[error(transparent)]
    ExportFromPending(#[from] PendingSafeExportSecretError<StorageError>),
    #[error("Error deriving PSK ID: {0}")]
    DerivingPskId(#[from] CryptoError),
    #[error("OpenMLS PSK error: {0}")]
    Psk(#[from] PskError),
    #[error("Error serializing PSK ID: {0}")]
    SerializingPskId(#[from] tls_codec::Error),
}

/// <https://datatracker.ietf.org/doc/html/draft-ietf-mls-combiner#name-key-schedule>
pub(crate) fn derive_and_store_psk<
    Provider: openmls::storage::OpenMlsProvider,
    const FROM_PENDING: bool,
>(
    provider: &Provider,
    group: &mut MlsGroup,
    t_ciphersuite: Ciphersuite,
) -> Result<PreSharedKeyId, ApqPskError<Provider::StorageError>> {
    let apq_exporter: Secret = if FROM_PENDING {
        group
            .safe_export_secret_from_pending(
                provider.crypto(),
                provider.storage(),
                APQMLS_COMPONENT_ID,
            )?
            .into()
    } else {
        group
            .safe_export_secret(provider.crypto(), provider.storage(), APQMLS_COMPONENT_ID)?
            .into()
    };

    let apq_psk_id = apq_exporter.derive_secret(provider.crypto(), t_ciphersuite, "psk_id")?;
    let apq_psk = apq_exporter.derive_secret(provider.crypto(), t_ciphersuite, "psk")?;
    drop(apq_exporter); // Zeroize the secret

    let psk = Psk::Application(ApplicationPsk::new(
        APQMLS_COMPONENT_ID,
        apq_psk_id.as_slice().into(),
    ));

    let id = PreSharedKeyId::new(t_ciphersuite, provider.rand(), psk)?;
    store_psk(provider, id, apq_psk.as_slice())
}

pub(crate) fn store_psk<Provider: OpenMlsProvider>(
    provider: &Provider,
    psk_id: PreSharedKeyId,
    psk: &[u8],
) -> Result<PreSharedKeyId, ApqPskError<Provider::StorageError>> {
    // Delete any existing PSK with the same ID.
    provider
        .storage()
        .delete_psk::<Psk>(psk_id.psk())
        .map_err(|_| ApqPskError::Psk(PskError::Storage))?;
    psk_id.store(provider, psk)?;
    Ok(psk_id)
}
