// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use openmls::{
    component::ComponentData,
    group::{
        CommitMessageBundle, CreateCommitError, ExternalCommitBuilder, ExternalCommitBuilderError,
        ExternalCommitBuilderFinalizeError, GroupEpoch, LeafNodeLifetimePolicy, MlsGroup,
        MlsGroupJoinConfig,
    },
    prelude::{
        AppDataUpdateProposal, CredentialWithKey, LeafNodeParameters, PreSharedKeyProposal,
        PublicMessageIn, RatchetTreeIn, group_info::VerifiableGroupInfo,
    },
    storage::OpenMlsProvider,
};
use openmls_traits::signatures::Signer;
use tap::Pipe;
use thiserror::Error;

use crate::{
    ApqCiphersuite, ApqMlsGroup,
    authentication::{ApqCredentialWithKey, ApqSigner},
    commit_builder::ApqCommitMessageBundle,
    extension::{
        APQMLS_COMPONENT_ID, ApqInfo, ensure_extension_support, ensure_leaf_node_component_support,
    },
    key_package::ensure_ciphersuite_support,
    messages::{ApqProposalIn, ApqRatchetTreeIn, VerifiableApqGroupInfo},
    psk::{ApqPskError, derive_and_store_psk},
};

impl ApqMlsGroup {
    /// Build an external commit into an existing APQMLS group, to join it as a new member (external
    /// join) or rejoin it (resync).
    ///
    /// Mirrors [`openmls::group::MlsGroup::external_commit_builder`].
    pub fn external_commit_builder() -> ApqExternalCommitBuilder {
        Default::default()
    }
}

// Implementation detail: this builder is not split in 2 stages as the OpenMLS external commit
// builder is done; OpenMLS boundary is group built -> commit is not yet built. This does not map
// onto the combiner, because there are two legs T and PQ. When the T leg reaches its `Initial`
// state, the PQ leg must be already fully built.
#[derive(Default)]
pub struct ApqExternalCommitBuilder {
    ratchet_tree: Option<ApqRatchetTreeIn>,
    proposals: Vec<ApqProposalIn>,
    config: MlsGroupJoinConfig,
    validate_lifetimes: LeafNodeLifetimePolicy,
    aad: Vec<u8>,
    /// (T, PQ) leaf node parameters if any were provided.
    leaf_node_parameters: Option<(LeafNodeParameters, LeafNodeParameters)>,
    create_group_info: bool,
    t_psk_proposals: Vec<PreSharedKeyProposal>,
}

impl ApqExternalCommitBuilder {
    /// Creates a new [`ApqExternalCommitBuilder`] with default values.
    pub fn new() -> Self {
        Default::default()
    }

    pub fn with_ratchet_tree(mut self, ratchet_tree: ApqRatchetTreeIn) -> Self {
        self.ratchet_tree = Some(ratchet_tree);
        self
    }

    pub fn with_config(mut self, config: MlsGroupJoinConfig) -> Self {
        self.config = config;
        self
    }

    pub fn with_aad(mut self, aad: Vec<u8>) -> Self {
        self.aad = aad;
        self
    }

    pub fn skip_lifetime_validation(mut self) -> Self {
        self.validate_lifetimes = LeafNodeLifetimePolicy::Skip;
        self
    }

    pub fn with_proposals(mut self, proposals: Vec<ApqProposalIn>) -> Self {
        self.proposals = proposals;
        self
    }

    pub fn leaf_node_parameters(mut self, t: LeafNodeParameters, pq: LeafNodeParameters) -> Self {
        self.leaf_node_parameters = Some((t, pq));
        self
    }

    /// Add connection-offer PSK to the T group.
    pub fn add_t_psk_proposal(mut self, proposal: PreSharedKeyProposal) -> Self {
        self.t_psk_proposals.push(proposal);
        self
    }

    pub fn create_group_info(mut self, create_group_info: bool) -> Self {
        self.create_group_info = create_group_info;
        self
    }

    pub fn build<S: ApqSigner, Provider: OpenMlsProvider>(
        self,
        provider: &Provider,
        signer: &S,
        credential_with_key: ApqCredentialWithKey,
        group_info: VerifiableApqGroupInfo,
    ) -> Result<
        (ApqMlsGroup, ApqCommitMessageBundle),
        ApqExternalCommitBuilderError<Provider::StorageError>,
    > {
        let Self {
            ratchet_tree,
            proposals,
            config,
            validate_lifetimes,
            aad,
            leaf_node_parameters,
            create_group_info,
            t_psk_proposals,
        } = self;

        let VerifiableApqGroupInfo {
            t_group_info,
            pq_group_info,
        } = group_info;

        let (t_proposals, pq_proposals): (Vec<_>, Vec<_>) = proposals
            .into_iter()
            .map(
                |ApqProposalIn {
                     t_proposal,
                     pq_proposal,
                 }| { (t_proposal, pq_proposal) },
            )
            .unzip();

        let (t_ratchet_tree, pq_ratchet_tree) = ratchet_tree.map(ApqRatchetTreeIn::split).unzip();

        // Increase the epoch in the apq info component.
        let mut apq_info = ApqInfo::from_extensions(t_group_info.group_context().extensions())?
            .ok_or(ApqExternalCommitBuilderError::MissingApqInfo)?;
        apq_info.set_epoch(
            GroupEpoch::from(t_group_info.epoch().as_u64() + 1),
            GroupEpoch::from(pq_group_info.epoch().as_u64() + 1),
        );
        let component_data = apq_info.to_component_data()?;
        let app_data_update_proposal =
            AppDataUpdateProposal::update(APQMLS_COMPONENT_ID, component_data.data());

        // Leaf node parameters
        let apq_ciphersuite =
            ApqCiphersuite::new(t_group_info.ciphersuite(), pq_group_info.ciphersuite());

        let (t_ln_parameters, pq_ln_parameters) = leaf_node_parameters.unwrap_or_default();
        let t_ln_parameters = ensure_leaf_node_parameters(&t_ln_parameters, apq_ciphersuite)?;
        let pq_ln_parameters = ensure_leaf_node_parameters(&pq_ln_parameters, apq_ciphersuite)?;

        // PQ leg
        let (mut pq_group, pq_bundle) = build_and_finalize_leg(
            provider,
            pq_proposals,
            pq_ratchet_tree,
            config.clone(),
            validate_lifetimes,
            aad.clone(),
            pq_group_info,
            credential_with_key.pq_credential,
            pq_ln_parameters,
            app_data_update_proposal.clone(),
            component_data.clone(),
            Vec::new(),
            create_group_info,
            signer.pq_signer(),
        )?;

        // From here on the PQ leg is already merged and persisted (there is no staged-but-unmerged
        // state for external commits), so any failure must roll it back: otherwise we leave an
        // orphaned, already-advanced PQ group with no matching T counterpart in storage.
        let t_result = (|| {
            // Derive the combiner PSK from the realized PQ group
            //
            // FROM_PENDING = false, because the commit is already merged by finalize
            let psk_proposal = derive_and_store_psk::<_, false>(
                provider,
                &mut pq_group,
                t_group_info.ciphersuite(),
            )?
            .pipe(PreSharedKeyProposal::new);

            // T leg

            // PSKs:
            //
            // * `psk_proposal` is the combiner PSK stored by the previous step;
            // * `t_psk_proposals` are connection-offer PSKs that must be stored by the caller.
            let t_psk_proposals = std::iter::once(psk_proposal)
                .chain(t_psk_proposals)
                .collect();

            build_and_finalize_leg(
                provider,
                t_proposals,
                t_ratchet_tree,
                config,
                validate_lifetimes,
                aad,
                t_group_info,
                credential_with_key.t_credential,
                t_ln_parameters,
                app_data_update_proposal,
                component_data,
                t_psk_proposals,
                create_group_info,
                signer.t_signer(),
            )
        })();
        let (t_group, t_bundle) = match t_result {
            Ok(result) => result,
            Err(err) => {
                let _ = pq_group.delete(provider.storage());
                return Err(err);
            }
        };

        Ok((
            ApqMlsGroup::from_groups(t_group, pq_group),
            ApqCommitMessageBundle::from_bundles(t_bundle, pq_bundle),
        ))
    }
}

/// Builds and finalizes the external commit for a single leg (T or PQ).
#[allow(clippy::too_many_arguments)]
fn build_and_finalize_leg<Provider: OpenMlsProvider>(
    provider: &Provider,
    proposals: Vec<PublicMessageIn>,
    ratchet_tree: Option<RatchetTreeIn>,
    config: MlsGroupJoinConfig,
    validate_lifetimes: LeafNodeLifetimePolicy,
    aad: Vec<u8>,
    group_info: VerifiableGroupInfo,
    credential_with_key: CredentialWithKey,
    leaf_node_parameters: LeafNodeParameters,
    app_data_update_proposal: AppDataUpdateProposal,
    component_data: ComponentData,
    psk_proposals: Vec<PreSharedKeyProposal>,
    create_group_info: bool,
    signer: &impl Signer,
) -> Result<(MlsGroup, CommitMessageBundle), ApqExternalCommitBuilderError<Provider::StorageError>>
{
    let mut external_builder = ExternalCommitBuilder::new()
        .with_proposals(proposals)
        .with_config(config)
        .with_aad(aad);
    if let Some(tree) = ratchet_tree {
        external_builder = external_builder.with_ratchet_tree(tree);
    }
    if let LeafNodeLifetimePolicy::Skip = validate_lifetimes {
        external_builder = external_builder.skip_lifetime_validation();
    }
    let mut commit_builder = external_builder
        .build_group(provider, group_info, credential_with_key)?
        .leaf_node_parameters(leaf_node_parameters)
        .add_app_data_update_proposal(app_data_update_proposal)
        .add_psk_proposals(psk_proposals)
        .load_psks(provider.storage())?
        .create_group_info(create_group_info);
    let mut updater = commit_builder.app_data_dictionary_updater();
    updater.set(component_data);
    commit_builder.with_app_data_dictionary_updates(updater.changes());
    commit_builder
        .build(provider.rand(), provider.crypto(), signer, |_| true)?
        .finalize(provider)
        .map_err(Into::into)
}

/// Ensures extensions supports for APQ and ciphersuite in the leaf node parameters.
fn ensure_leaf_node_parameters(
    params: &LeafNodeParameters,
    apq_ciphersuite: ApqCiphersuite,
) -> Result<LeafNodeParameters, tls_codec::Error> {
    let t_capabilities = params
        .capabilities()
        .cloned()
        .unwrap_or_default()
        .pipe(ensure_extension_support)?
        .pipe(|c| ensure_ciphersuite_support(c, apq_ciphersuite))?;
    let ln_extensions = params
        .extensions()
        .cloned()
        .unwrap_or_default()
        .pipe(ensure_leaf_node_component_support)?;
    let mut builder = LeafNodeParameters::builder()
        .with_capabilities(t_capabilities)
        .with_extensions(ln_extensions);
    if let Some(credential_with_key) = params.credential_with_key() {
        builder = builder.with_credential_with_key(credential_with_key.clone());
    };
    Ok(builder.build())
}

/// Errors that can occur when creating a new [`ApqMlsGroup`] from an external commit.
#[derive(Debug, Error)]
pub enum ApqExternalCommitBuilderError<StorageError> {
    #[error(transparent)]
    ExternalCommit(#[from] ExternalCommitBuilderError<StorageError>),
    #[error(transparent)]
    BuildCommit(#[from] CreateCommitError),
    #[error(transparent)]
    Finalize(#[from] ExternalCommitBuilderFinalizeError<StorageError>),
    #[error(transparent)]
    Psk(#[from] ApqPskError<StorageError>),
    /// Missing required ApqInfo in group-info extensions
    #[error("Missing required ApqInfo in group-info extensions")]
    MissingApqInfo,
    /// Malformed extension
    #[error("Malformed extension")]
    MalformedExtension(#[from] tls_codec::Error),
}
