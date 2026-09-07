// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Entry points for a virtual client's sibling emulator clients to join an
//! [`ApqMlsGroup`] another sibling either created or joined via an external
//! commit. Both entry points reconstruct the sibling's state from the shared
//! emulation epoch, so no secret travels between the emulator clients.

use openmls::{
    components::vc_derivation_info::EpochId,
    group::{MlsGroup, MlsGroupJoinConfig, VcExternalCommitJoinError, VcGroupCreationJoinError},
    prelude::{ProtocolMessage, RatchetTreeIn, group_info::VerifiableGroupInfo},
    storage::OpenMlsProvider,
};
use thiserror::Error;

use crate::{
    ApqMlsGroup,
    extension::{ApqInfo, ApqInfoUpdateError},
    messages::{ApqRatchetTreeIn, VerifiableApqGroupInfo},
    processing::compute_app_data_updates,
    psk::{ApqPskError, derive_and_store_psk},
};

impl ApqMlsGroup {
    /// Reconstructs an [`ApqMlsGroup`] a sibling emulator client created with
    /// [`crate::group_builder::GroupBuilder::vc_emulation`].
    ///
    /// `group_info` describes both halves at epoch 0. The T half is joined
    /// first, then the PQ half, matching the order in which the creating
    /// sibling consumed the emulation epoch's `key_package` generations.
    ///
    /// No PSK is stored here. Like the creating sibling's build, this join
    /// produces the epoch-0 state, and the combiner PSK for the T half is
    /// derived when the first commit is processed.
    ///
    /// The provider's storage should span the whole call in one transaction.
    pub fn vc_join_at_creation<Provider: OpenMlsProvider>(
        provider: &Provider,
        join_config: &MlsGroupJoinConfig,
        group_info: VerifiableApqGroupInfo,
        ratchet_tree: Option<ApqRatchetTreeIn>,
        epoch_id: EpochId,
    ) -> Result<ApqMlsGroup, VcCreationJoinError<Provider::StorageError>> {
        validate_group_info_linkage(&group_info)?;

        let VerifiableApqGroupInfo {
            t_group_info,
            pq_group_info,
        } = group_info;
        let (t_ratchet_tree, pq_ratchet_tree) = ratchet_tree.map(ApqRatchetTreeIn::split).unzip();

        let mut t_group = MlsGroup::vc_join_at_creation(
            provider,
            join_config,
            t_group_info,
            t_ratchet_tree,
            epoch_id.clone(),
        )?;
        // The T half is already persisted, so a failing PQ half must roll it back.
        let pq_group = match MlsGroup::vc_join_at_creation(
            provider,
            join_config,
            pq_group_info,
            pq_ratchet_tree,
            epoch_id,
        ) {
            Ok(pq_group) => pq_group,
            Err(err) => {
                let _ = t_group.delete(provider.storage());
                return Err(err.into());
            }
        };

        Ok(ApqMlsGroup::from_groups(t_group, pq_group))
    }

    /// Reconstructs an [`ApqMlsGroup`] a sibling emulator client joined with
    /// [`crate::external_commit_builder::ApqExternalCommitBuilder`], by
    /// processing that sibling's external commits.
    ///
    /// `group_info` describes both halves at the epoch *before* the external
    /// commits. The PQ half is joined first, because the T commit references
    /// the combiner PSK derived from the joined PQ half.
    ///
    /// Application PSKs referenced by the T commit, for example connection-offer
    /// PSKs, must be stored in the provider before calling this. The combiner
    /// PSK is the only PSK this function stores itself.
    ///
    /// The provider's storage should span the whole call in one transaction.
    pub fn vc_join_via_sibling_external_commit<Provider: OpenMlsProvider>(
        provider: &Provider,
        join_config: &MlsGroupJoinConfig,
        group_info: VerifiableApqGroupInfo,
        ratchet_tree: Option<ApqRatchetTreeIn>,
        t_external_commit: impl Into<ProtocolMessage>,
        pq_external_commit: impl Into<ProtocolMessage>,
        epoch_id: EpochId,
    ) -> Result<ApqMlsGroup, VcSiblingExternalCommitJoinError<Provider::StorageError>> {
        validate_group_info_linkage(&group_info)?;

        let VerifiableApqGroupInfo {
            t_group_info,
            pq_group_info,
        } = group_info;
        let t_ciphersuite = t_group_info.ciphersuite();
        let (t_ratchet_tree, pq_ratchet_tree) = ratchet_tree.map(ApqRatchetTreeIn::split).unzip();

        let mut pq_group = process_sibling_external_commit(
            provider,
            join_config,
            pq_group_info,
            pq_ratchet_tree,
            pq_external_commit,
            epoch_id.clone(),
        )?;

        // From here on the PQ half is merged and persisted, so any failure must roll it back:
        // otherwise we leave an orphaned PQ group with no matching T counterpart in storage.
        let t_result = (|| {
            // Derive the combiner PSK from the joined PQ half, so the T commit's PSK proposal
            // resolves from storage.
            //
            // FROM_PENDING = false, because the PQ commit is already merged by the join.
            derive_and_store_psk::<_, false>(provider, &mut pq_group, t_ciphersuite)?;

            process_sibling_external_commit(
                provider,
                join_config,
                t_group_info,
                t_ratchet_tree,
                t_external_commit,
                epoch_id,
            )
        })();
        let t_group = match t_result {
            Ok(t_group) => t_group,
            Err(err) => {
                let _ = pq_group.delete(provider.storage());
                return Err(err);
            }
        };

        Ok(ApqMlsGroup::from_groups(t_group, pq_group))
    }
}

/// Joins one half by processing the sibling's external commit, resolving the
/// commit's AppDataUpdate proposals like regular processing does.
fn process_sibling_external_commit<Provider: OpenMlsProvider>(
    provider: &Provider,
    join_config: &MlsGroupJoinConfig,
    group_info: VerifiableGroupInfo,
    ratchet_tree: Option<RatchetTreeIn>,
    external_commit: impl Into<ProtocolMessage>,
    epoch_id: EpochId,
) -> Result<MlsGroup, VcSiblingExternalCommitJoinError<Provider::StorageError>> {
    let mut builder = MlsGroup::vc_external_commit_join_builder().with_config(join_config.clone());
    if let Some(ratchet_tree) = ratchet_tree {
        builder = builder.with_ratchet_tree(ratchet_tree);
    }
    let mut staged = builder.process_commit(provider, group_info, external_commit, epoch_id)?;
    let updates = compute_app_data_updates(
        staged.app_data_dictionary_updater(),
        staged.app_data_update_proposals(),
    )?;
    staged.with_app_data_dictionary_updates(updates);
    staged.into_group(provider).map_err(Into::into)
}

/// Checks that the two group infos describe the two halves of the same APQMLS
/// group before any group state is created.
fn validate_group_info_linkage(
    group_info: &VerifiableApqGroupInfo,
) -> Result<(), ApqGroupInfoLinkageError> {
    let t_info = apq_info(&group_info.t_group_info)?;
    let pq_info = apq_info(&group_info.pq_group_info)?;

    if t_info != pq_info {
        return Err(ApqGroupInfoLinkageError::ApqInfoMismatch);
    }
    if t_info.t_session_group_id != *group_info.t_group_info.group_id()
        || t_info.pq_session_group_id != *group_info.pq_group_info.group_id()
    {
        return Err(ApqGroupInfoLinkageError::GroupIdMismatch);
    }
    if t_info.t_cipher_suite != group_info.t_group_info.ciphersuite()
        || t_info.pq_cipher_suite != group_info.pq_group_info.ciphersuite()
    {
        return Err(ApqGroupInfoLinkageError::CiphersuiteMismatch);
    }
    Ok(())
}

fn apq_info(group_info: &VerifiableGroupInfo) -> Result<ApqInfo, ApqGroupInfoLinkageError> {
    ApqInfo::from_extensions(group_info.group_context().extensions())?
        .ok_or(ApqGroupInfoLinkageError::MissingApqInfo)
}

/// Errors that can occur when the group infos of the two halves do not describe
/// the same APQMLS group.
#[derive(Debug, Error)]
pub enum ApqGroupInfoLinkageError {
    /// Missing required ApqInfo in group-info extensions
    #[error("Missing required ApqInfo in group-info extensions")]
    MissingApqInfo,
    /// Malformed extension
    #[error("Malformed extension")]
    MalformedExtension(#[from] tls_codec::Error),
    /// The two group infos carry different ApqInfo
    #[error("The two group infos carry different ApqInfo")]
    ApqInfoMismatch,
    /// The ApqInfo group IDs do not match the group infos
    #[error("The ApqInfo group IDs do not match the group infos")]
    GroupIdMismatch,
    /// The ApqInfo ciphersuites do not match the group infos
    #[error("The ApqInfo ciphersuites do not match the group infos")]
    CiphersuiteMismatch,
}

/// Errors that can occur when reconstructing an [`ApqMlsGroup`] created by a
/// sibling emulator client.
#[derive(Debug, Error)]
pub enum VcCreationJoinError<StorageError> {
    #[error(transparent)]
    Join(#[from] VcGroupCreationJoinError<StorageError>),
    #[error(transparent)]
    Linkage(#[from] ApqGroupInfoLinkageError),
}

/// Errors that can occur when reconstructing an [`ApqMlsGroup`] a sibling
/// emulator client joined via an external commit.
#[derive(Debug, Error)]
pub enum VcSiblingExternalCommitJoinError<StorageError> {
    #[error(transparent)]
    Join(#[from] VcExternalCommitJoinError<StorageError>),
    #[error(transparent)]
    ApqInfoUpdate(#[from] ApqInfoUpdateError),
    #[error(transparent)]
    Psk(#[from] ApqPskError<StorageError>),
    #[error(transparent)]
    Linkage(#[from] ApqGroupInfoLinkageError),
}
