// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use openmls::{
    group::{
        MlsGroup, MlsGroupJoinConfig, ProcessedWelcome, StagedWelcome,
        WelcomeError as OpenMlsWelcomeError,
    },
    prelude::{Ciphersuite, Credential},
    schedule::PreSharedKeyId,
    storage::OpenMlsProvider,
};
use thiserror::Error;

use crate::{
    ApqMlsGroup,
    messages::{ApqRatchetTreeIn, ApqWelcome},
    psk::{ApqPskError, derive_and_store_psk},
    validation::{ApqValidationError, validate_apq_session_at_construction, validate_welcome_psk},
};

/// Errors that can occur when creating a new [`ApqMlsGroup`] from a welcome
/// message.
#[derive(Debug, Error)]
pub enum WelcomeError<StorageError> {
    #[error("Failed to process welcome message: {0}")]
    Processing(#[from] OpenMlsWelcomeError<StorageError>),
    #[error(transparent)]
    Psk(#[from] ApqPskError<StorageError>),
    #[error(transparent)]
    Validation(#[from] ApqValidationError),
}

/// Derives the combiner PSK from the freshly joined PQ group and stores it, so
/// that the T Welcome can be processed.
///
/// Returns the [`PreSharedKeyId`] of the stored PSK. Callers that drive the two
/// joins themselves must pass it to
/// [`crate::validation::validate_welcome_psk`] before they join the T session.
pub fn derive_and_store_join_psk<Provider: OpenMlsProvider>(
    provider: &Provider,
    pq_group: &mut MlsGroup,
    t_ciphersuite: Ciphersuite,
) -> Result<PreSharedKeyId, WelcomeError<Provider::StorageError>> {
    let psk_id = derive_and_store_psk::<_, false>(provider, pq_group, t_ciphersuite)?;
    Ok(psk_id)
}

impl ApqMlsGroup {
    /// Creates a new [`ApqMlsGroup`] from a welcome message.
    // TODO: Split into sans-io friendly parts.
    pub fn new_from_welcome<Provider: OpenMlsProvider>(
        provider: &Provider,
        mls_group_config: &MlsGroupJoinConfig,
        welcome: ApqWelcome,
        ratchet_tree: Option<ApqRatchetTreeIn>,
        credential_equivalence: impl Fn(&Credential, &Credential) -> bool,
    ) -> Result<Self, WelcomeError<Provider::StorageError>> {
        let (t_ratchet_tree, pq_ratchet_tree) = match ratchet_tree {
            Some(r) => (Some(r.t_ratchet_tree), Some(r.pq_ratchet_tree)),
            None => (None, None),
        };
        let mut pq_group = StagedWelcome::new_from_welcome(
            provider,
            mls_group_config,
            welcome.pq_welcome,
            pq_ratchet_tree,
        )?
        .into_group(provider)?;

        let t_ciphersuite = welcome.t_welcome.ciphersuite();

        // The PQ group is already persisted, so any failure from here on must
        // roll it back. Otherwise we leave a half-joined group in storage.
        let t_result = (|| {
            let psk_id = derive_and_store_join_psk(provider, &mut pq_group, t_ciphersuite)?;
            let processed_t_welcome =
                ProcessedWelcome::new_from_welcome(provider, mls_group_config, welcome.t_welcome)?;
            validate_welcome_psk(&processed_t_welcome, &psk_id)?;
            processed_t_welcome
                .into_staged_welcome(provider, t_ratchet_tree)?
                .into_group(provider)
                .map_err(WelcomeError::from)
        })();
        let mut t_group = match t_result {
            Ok(t_group) => t_group,
            Err(error) => {
                let _ = pq_group.delete(provider.storage());
                return Err(error);
            }
        };

        validate_or_delete(
            provider,
            &mut t_group,
            &mut pq_group,
            credential_equivalence,
        )?;

        Ok(Self { t_group, pq_group })
    }
}

/// Validates the joined session, deleting both groups if it is rejected.
fn validate_or_delete<Provider: OpenMlsProvider>(
    provider: &Provider,
    t_group: &mut MlsGroup,
    pq_group: &mut MlsGroup,
    credential_equivalence: impl Fn(&Credential, &Credential) -> bool,
) -> Result<(), WelcomeError<Provider::StorageError>> {
    if let Err(error) =
        validate_apq_session_at_construction(t_group, pq_group, credential_equivalence)
    {
        let _ = t_group.delete(provider.storage());
        let _ = pq_group.delete(provider.storage());
        return Err(error.into());
    }
    Ok(())
}
