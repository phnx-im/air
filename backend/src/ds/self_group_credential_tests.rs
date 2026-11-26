// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! DS checks of signed self-group credentials on every path a leaf enters or changes.

use aircommon::{
    credentials::{
        SelfGroupCredential,
        keys::{SelfGroupSigningKey, UserSigningKey},
        test_utils::{malformed_self_group_credential, other_key_of, random_user_signing_key},
    },
    messages::{
        client_ds::{AddUsersInfo, GroupOperationParams},
        welcome_attribution_info::EncryptedWelcomeAttributionInfo,
    },
    mls_group_config::{default_mls_group_join_config, self_group_leaf_node_capabilities},
};
use airprotos::{convert::TryRefInto, delivery_service::v1::UserCredential as UserCredentialProto};
use apqmls::{
    ApqCiphersuite, ApqMlsGroup,
    authentication::ApqCredentialWithKey,
    external_commit_builder::ApqExternalCommitBuilder,
    messages::{ApqKeyPackage, ApqRatchetTreeIn, VerifiableApqGroupInfo},
};
use mls_assist::{
    messages::AssistedWelcome,
    openmls::{
        group::ExternalCommitBuilder,
        prelude::{
            Ciphersuite, Credential, CredentialType, CredentialWithKey, Extensions, KeyPackage,
            LeafNodeIndex, LeafNodeParameters, MlsGroup, MlsMessageOut, OpenMlsProvider,
            PURE_PLAINTEXT_WIRE_FORMAT_POLICY, Welcome,
        },
    },
    openmls_rust_crypto::OpenMlsRustCrypto,
};
use tonic::{Code, Status};
use uuid::Uuid;

use crate::errors::{GroupOperationError, ResyncClientError};

use super::{
    create_group::creator_credential,
    delete_group::DeleteKind,
    group_operation::{ProcessedApqGroupOperation, ProcessedGroupOperation},
    group_state::DsGroupState,
    resync::ResyncOutcome,
    test_utils::{
        FailingAs, StoredKeys, add_joiner_profiles, apq_assisted, assert_authenticated, assisted,
        authentication_cases, ds_group_state, group_context_extensions, group_operation_aad,
        qs_reference_extension, verifiable_group_info,
    },
    user_credentials::Unauthenticated,
};

const MAX_DEVICES: u32 = 5;
const CIPHERSUITE: Ciphersuite = Ciphersuite::MLS_128_DHKEMX25519_AES128GCM_SHA256_Ed25519;

struct Device {
    provider: OpenMlsRustCrypto,
    key: SelfGroupSigningKey,
}

impl Device {
    fn new() -> Self {
        Self {
            provider: OpenMlsRustCrypto::default(),
            key: SelfGroupSigningKey::generate(Uuid::new_v4()).unwrap(),
        }
    }

    fn client_id(&self) -> Uuid {
        self.key.credential().client_id()
    }

    fn signature_key(&self) -> &[u8] {
        self.key.verifying_key().as_slice()
    }

    fn unsigned(&self) -> Credential {
        self.key.credential().to_credential().unwrap()
    }

    fn signed_by(&self, user: &UserSigningKey) -> Credential {
        SelfGroupCredential::signed(self.client_id(), self.signature_key(), user)
            .unwrap()
            .to_credential()
            .unwrap()
    }

    fn signed_for_another_leaf_key(&self, user: &UserSigningKey) -> Credential {
        SelfGroupCredential::signed(self.client_id(), &[7; 32], user)
            .unwrap()
            .to_credential()
            .unwrap()
    }

    fn malformed(&self) -> Credential {
        malformed_self_group_credential(self.client_id())
    }

    fn with_key(&self, credential: Credential) -> CredentialWithKey {
        CredentialWithKey {
            credential,
            signature_key: self.signature_key().into(),
        }
    }
}

fn new_group(creator: &Device, credential: Credential) -> MlsGroup {
    MlsGroup::builder()
        .ciphersuite(CIPHERSUITE)
        .with_wire_format_policy(PURE_PLAINTEXT_WIRE_FORMAT_POLICY)
        .with_capabilities(self_group_leaf_node_capabilities())
        .with_group_context_extensions(group_context_extensions(true, None))
        .build(
            &creator.provider,
            &creator.key,
            creator.with_key(credential),
        )
        .unwrap()
}

fn key_package(device: &Device, credential: Credential) -> KeyPackage {
    KeyPackage::builder()
        .leaf_node_capabilities(self_group_leaf_node_capabilities())
        .leaf_node_extensions(Extensions::single(qs_reference_extension()).unwrap())
        .build(
            CIPHERSUITE,
            &device.provider,
            &device.key,
            device.with_key(credential),
        )
        .unwrap()
        .key_package()
        .clone()
}

/// A self group as its creator sees it, and its DS state.
///
/// The DS state is built from the group as it is, so a test can start from leaves the DS
/// wouldn't let in.
struct SelfGroup {
    creator: Device,
    group: MlsGroup,
    state: DsGroupState,
}

impl SelfGroup {
    /// A self group of a new creator device whose leaf carries the credential that
    /// `creator_credential` builds for it.
    fn new(creator_credential: impl FnOnce(&Device) -> Credential) -> Self {
        Self::with_joiners(creator_credential, Vec::new())
    }

    fn with_joiners(
        creator_credential: impl FnOnce(&Device) -> Credential,
        joiners: Vec<(&Device, Credential)>,
    ) -> Self {
        let creator = Device::new();
        let mut group = new_group(&creator, creator_credential(&creator));
        let key_packages: Vec<_> = joiners
            .into_iter()
            .map(|(device, credential)| key_package(device, credential))
            .collect();
        if !key_packages.is_empty() {
            group
                .commit_builder()
                .propose_adds(key_packages)
                .load_psks(creator.provider.storage())
                .unwrap()
                .build(
                    creator.provider.rand(),
                    creator.provider.crypto(),
                    &creator.key,
                    |_| true,
                )
                .unwrap()
                .stage_commit(&creator.provider)
                .unwrap();
            group.merge_pending_commit(&creator.provider).unwrap();
        }
        let mut state = ds_group_state(&creator.provider, &group, &creator.key);
        add_joiner_profiles(&mut state);
        Self {
            creator,
            group,
            state,
        }
    }

    /// Stages a commit of the creator with an update path and returns it with its Welcome and
    /// group info.
    ///
    /// With `credential`, the update path replaces the creator's credential.
    fn stage_commit(
        &mut self,
        adds: Vec<KeyPackage>,
        removals: Vec<LeafNodeIndex>,
        credential: Option<Credential>,
    ) -> (MlsMessageOut, Option<Welcome>, MlsMessageOut) {
        let mut builder = self
            .group
            .commit_builder()
            .force_self_update(true)
            .propose_adds(adds)
            .propose_removals(removals);
        if let Some(credential) = credential {
            builder = builder.leaf_node_parameters(
                LeafNodeParameters::builder()
                    .with_credential_with_key(self.creator.with_key(credential))
                    .build(),
            );
        }
        let (commit, welcome, group_info) = builder
            .load_psks(self.creator.provider.storage())
            .unwrap()
            .create_group_info(true)
            .build(
                self.creator.provider.rand(),
                self.creator.provider.crypto(),
                &self.creator.key,
                |_| true,
            )
            .unwrap()
            .stage_commit(&self.creator.provider)
            .unwrap()
            .into_contents();
        (commit, welcome, group_info.unwrap().into())
    }

    /// Sends a commit of the creator with an update path through the group operation path, and
    /// merges it if the DS accepts it.
    ///
    /// With `credential`, the update path replaces the creator's credential.
    async fn group_operation(
        &mut self,
        adds: Vec<KeyPackage>,
        removals: Vec<LeafNodeIndex>,
        credential: Option<Credential>,
    ) -> Result<Unauthenticated<ProcessedGroupOperation>, GroupOperationError> {
        let added = adds.len();
        self.group.set_aad(group_operation_aad(added));
        let (commit, welcome, group_info) = self.stage_commit(adds, removals, credential);
        let add_users_info_option = welcome.map(|welcome| AddUsersInfo {
            welcome: AssistedWelcome { welcome },
            encrypted_welcome_attribution_infos: (0..added)
                .map(|_| EncryptedWelcomeAttributionInfo::dummy())
                .collect(),
        });
        let params = GroupOperationParams {
            commit: assisted(commit, group_info),
            add_users_info_option,
        };
        let result = self
            .state
            .process_group_operation(params, MAX_DEVICES)
            .await;
        if result.is_ok() {
            self.group
                .merge_pending_commit(&self.creator.provider)
                .unwrap();
        } else {
            self.group
                .clear_pending_commit(self.creator.provider.storage())
                .unwrap();
        }
        result
    }

    async fn add(
        &mut self,
        device: &Device,
        credential: Credential,
    ) -> Result<Unauthenticated<ProcessedGroupOperation>, GroupOperationError> {
        self.group_operation(vec![key_package(device, credential)], Vec::new(), None)
            .await
    }

    async fn update(
        &mut self,
        credential: Credential,
    ) -> Result<Unauthenticated<ProcessedGroupOperation>, GroupOperationError> {
        self.group_operation(Vec::new(), Vec::new(), Some(credential))
            .await
    }

    async fn remove(
        &mut self,
        index: u32,
    ) -> Result<Unauthenticated<ProcessedGroupOperation>, GroupOperationError> {
        self.group_operation(Vec::new(), vec![LeafNodeIndex::new(index)], None)
            .await
    }

    /// Rejoins `device`, which holds the leaf at `index`, by an external commit with
    /// `credential`.
    fn resync(
        &mut self,
        device: &Device,
        index: u32,
        credential: Credential,
    ) -> Result<Unauthenticated<ResyncOutcome>, ResyncClientError> {
        let group_info = self
            .group
            .export_group_info(self.creator.provider.crypto(), &self.creator.key, false)
            .unwrap();
        let leaf_node_parameters = LeafNodeParameters::builder()
            .with_capabilities(self_group_leaf_node_capabilities())
            .with_extensions(Extensions::single(qs_reference_extension()).unwrap())
            .build();
        // The builder removes the leaf with the same signature key.
        let (_joined, bundle) = ExternalCommitBuilder::new()
            .with_config(default_mls_group_join_config())
            .with_ratchet_tree(self.group.export_ratchet_tree().into())
            .build_group(
                &device.provider,
                verifiable_group_info(group_info),
                device.with_key(credential),
            )
            .unwrap()
            .leaf_node_parameters(leaf_node_parameters)
            .load_psks(device.provider.storage())
            .unwrap()
            .create_group_info(true)
            .build(
                device.provider.rand(),
                device.provider.crypto(),
                &device.key,
                |_| true,
            )
            .unwrap()
            .finalize(&device.provider)
            .unwrap();
        let (commit, _welcome, group_info) = bundle.into_contents();
        self.state.resync_client(
            assisted(commit, group_info.unwrap().into()),
            LeafNodeIndex::new(index),
        )
    }
}

/// Both legs sign with Ed25519, so that the self-group signing key signs both.
fn apq_ciphersuite() -> ApqCiphersuite {
    ApqCiphersuite::new(
        CIPHERSUITE,
        Ciphersuite::MLS_128_MLKEM768_AES256GCM_SHA384_Ed25519,
    )
}

impl Device {
    /// The T leaf carries `credential`, the PQ leaf an empty one, as on clients.
    fn apq_with_key(&self, credential: Credential) -> ApqCredentialWithKey {
        ApqCredentialWithKey {
            t_credential: self.with_key(credential),
            pq_credential: self.with_key(Credential::new(CredentialType::Basic, Vec::new())),
        }
    }
}

/// An APQ self group as its creator sees it, and the DS state of both legs.
struct ApqSelfGroup {
    creator: Device,
    group: ApqMlsGroup,
    t_state: DsGroupState,
    pq_state: DsGroupState,
}

impl ApqSelfGroup {
    fn with_joiners(
        creator_credential: impl FnOnce(&Device) -> Credential,
        joiners: Vec<(&Device, Credential)>,
    ) -> Self {
        let creator = Device::new();
        let mut group = ApqMlsGroup::builder()
            .with_ciphersuite(apq_ciphersuite())
            .with_wire_format_policy(PURE_PLAINTEXT_WIRE_FORMAT_POLICY)
            .with_capabilities(self_group_leaf_node_capabilities())
            .with_group_context_extensions(
                group_context_extensions(true, None),
                Extensions::empty(),
            )
            .unwrap()
            .build(
                &creator.provider,
                &creator.key,
                creator.apq_with_key(creator_credential(&creator)),
            )
            .unwrap();
        let key_packages: Vec<_> = joiners
            .into_iter()
            .map(|(device, credential)| {
                ApqKeyPackage::builder()
                    .leaf_node_capabilities(self_group_leaf_node_capabilities())
                    .leaf_node_extensions(Extensions::single(qs_reference_extension()).unwrap())
                    .build(
                        &device.provider,
                        apq_ciphersuite(),
                        &device.key,
                        device.apq_with_key(credential),
                    )
                    .unwrap()
                    .into_key_package()
            })
            .collect();
        group
            .commit_builder()
            .propose_adds(key_packages)
            .finalize(&creator.provider, &creator.key, |_| true, |_| true)
            .unwrap();
        group.merge_pending_commit(&creator.provider).unwrap();
        // The DS records joiners on the T leg only.
        let mut t_state = ds_group_state(&creator.provider, &group.t_group, &creator.key);
        add_joiner_profiles(&mut t_state);
        let pq_state = ds_group_state(&creator.provider, group.pq_group(), &creator.key);
        Self {
            creator,
            group,
            t_state,
            pq_state,
        }
    }

    /// Sends an APQ self update of the creator whose T leaf carries `credential` through the
    /// group operation path.
    fn update(
        &mut self,
        credential: Credential,
    ) -> Result<Unauthenticated<ProcessedApqGroupOperation>, GroupOperationError> {
        self.group.t_group.set_aad(group_operation_aad(0));
        let t_parameters = LeafNodeParameters::builder()
            .with_credential_with_key(self.creator.with_key(credential))
            .build();
        let bundle = self
            .group
            .commit_builder()
            .force_self_update(true)
            .leaf_node_parameters(t_parameters, LeafNodeParameters::builder().build())
            .create_group_info(true)
            .finalize(
                &self.creator.provider,
                &self.creator.key,
                |_| true,
                |_| true,
            )
            .unwrap();
        DsGroupState::process_apq_group_operation(
            &mut self.t_state,
            &mut self.pq_state,
            apq_assisted(bundle),
            None,
            None,
            MAX_DEVICES,
        )
    }

    /// Rejoins `device`, which holds the leaves at `index`, by an APQ external commit whose T
    /// leaf carries `credential`.
    fn resync(
        &mut self,
        device: &Device,
        index: u32,
        credential: Credential,
    ) -> Result<Unauthenticated<ResyncOutcome>, ResyncClientError> {
        let (t_group_info, pq_group_info) = self
            .group
            .export_group_info(self.creator.provider.crypto(), &self.creator.key, false)
            .unwrap()
            .split();
        let group_info = VerifiableApqGroupInfo::new(
            verifiable_group_info(t_group_info),
            verifiable_group_info(pq_group_info),
        );
        let ratchet_tree = ApqRatchetTreeIn::new(
            self.group.t_group.export_ratchet_tree().into(),
            self.group.pq_group().export_ratchet_tree().into(),
        );
        let leaf_node_parameters = LeafNodeParameters::builder()
            .with_capabilities(self_group_leaf_node_capabilities())
            .with_extensions(Extensions::single(qs_reference_extension()).unwrap())
            .build();
        // The builder removes the leaves with the same signature key.
        let (_joined, bundle) = ApqExternalCommitBuilder::new()
            .with_ratchet_tree(ratchet_tree)
            .with_config(default_mls_group_join_config())
            .leaf_node_parameters(leaf_node_parameters.clone(), leaf_node_parameters)
            .create_group_info(true)
            .build(
                &device.provider,
                &device.key,
                device.apq_with_key(credential),
                group_info,
                |_, _| true,
            )
            .unwrap();
        DsGroupState::apq_resync_client(
            &mut self.t_state,
            &mut self.pq_state,
            apq_assisted(bundle),
            LeafNodeIndex::new(index),
        )
    }
}

/// Returns the client IDs of the signed self-group credentials `outcome` holds back.
fn held_back<T>(outcome: Unauthenticated<T>) -> Vec<Uuid> {
    outcome.into_parts().1.self_group_client_ids()
}

/// Builders of credentials that the DS rejects for a device in a self group whose signed leaves
/// are signed with the given user signing key.
const INVALID_CREDENTIALS: [fn(&Device, &UserSigningKey) -> Credential; 3] = [
    Device::signed_for_another_leaf_key,
    |device, _| device.signed_by(&random_user_signing_key()),
    |device, _| device.malformed(),
];

#[tokio::test]
async fn an_add_holds_back_a_signed_credential_for_the_as() {
    let alice = random_user_signing_key();
    let mut group = SelfGroup::new(|creator| creator.signed_by(&alice));
    let joiner = Device::new();

    // The creator's update path carries its signed credential as well.
    let outcome = group.add(&joiner, joiner.signed_by(&alice)).await.unwrap();
    assert_eq!(
        held_back(outcome),
        [group.creator.client_id(), joiner.client_id()]
    );
}

#[tokio::test]
async fn an_add_is_authenticated_against_the_key_the_as_stores() {
    let alice = random_user_signing_key();
    for (stored, signer, accepted) in authentication_cases(&alice) {
        let mut group = SelfGroup::new(Device::unsigned);
        let joiner = Device::new();
        let result = group
            .add(&joiner, joiner.signed_by(&signer))
            .await
            .unwrap()
            .authenticate(&stored)
            .await;
        assert_authenticated(result, accepted);
    }
}

#[tokio::test]
async fn an_add_with_an_invalid_credential_is_rejected() {
    let alice = random_user_signing_key();
    for invalid in INVALID_CREDENTIALS {
        let mut group = SelfGroup::new(|creator| creator.signed_by(&alice));
        let joiner = Device::new();
        let result = group.add(&joiner, invalid(&joiner, &alice)).await;
        assert!(matches!(result, Err(GroupOperationError::InvalidMessage)));
    }
}

#[tokio::test]
async fn adds_of_two_users_in_one_commit_are_rejected() {
    let mut group = SelfGroup::new(Device::unsigned);
    let first = Device::new();
    let second = Device::new();

    let result = group
        .group_operation(
            vec![
                key_package(&first, first.signed_by(&random_user_signing_key())),
                key_package(&second, second.signed_by(&random_user_signing_key())),
            ],
            Vec::new(),
            None,
        )
        .await;
    assert!(matches!(result, Err(GroupOperationError::InvalidMessage)));
}

#[tokio::test]
async fn an_update_path_holds_back_a_signed_credential_for_the_as() {
    let mut group = SelfGroup::new(Device::unsigned);
    let signed = group.creator.signed_by(&random_user_signing_key());

    let outcome = group.update(signed).await.unwrap();
    assert_eq!(held_back(outcome), [group.creator.client_id()]);
}

#[tokio::test]
async fn an_update_path_is_authenticated_against_the_key_the_as_stores() {
    let alice = random_user_signing_key();
    for (stored, signer, accepted) in authentication_cases(&alice) {
        let mut group = SelfGroup::new(Device::unsigned);
        let signed = group.creator.signed_by(&signer);
        let result = group
            .update(signed)
            .await
            .unwrap()
            .authenticate(&stored)
            .await;
        assert_authenticated(result, accepted);
    }
}

#[tokio::test]
async fn an_update_path_with_an_invalid_credential_is_rejected() {
    let alice = random_user_signing_key();
    let sibling = Device::new();
    for invalid in INVALID_CREDENTIALS {
        let mut group = SelfGroup::with_joiners(
            Device::unsigned,
            vec![(&sibling, sibling.signed_by(&alice))],
        );
        let credential = invalid(&group.creator, &alice);
        let result = group.update(credential).await;
        assert!(matches!(result, Err(GroupOperationError::InvalidMessage)));
    }
}

#[tokio::test]
async fn the_same_user_rule_skips_unsigned_leaves_and_the_replaced_leaf() {
    let bob = random_user_signing_key();
    let stored = StoredKeys::of(&[&bob]);
    let sibling = Device::new();
    let creator_credentials: [fn(&Device) -> Credential; 2] = [
        // No signed leaf in the group.
        Device::unsigned,
        // The replaced leaf is the only signed one.
        |creator| creator.signed_by(&random_user_signing_key()),
    ];
    for creator_credential in creator_credentials {
        let mut group =
            SelfGroup::with_joiners(creator_credential, vec![(&sibling, sibling.unsigned())]);
        let signed = group.creator.signed_by(&bob);
        let outcome = group.update(signed).await.unwrap();
        outcome.authenticate(&stored).await.unwrap();
    }
}

#[tokio::test]
async fn a_resync_holds_back_a_signed_credential_for_the_as() {
    let alice = random_user_signing_key();
    let sibling = Device::new();
    let mut group = SelfGroup::with_joiners(
        |creator| creator.signed_by(&alice),
        vec![(&sibling, sibling.unsigned())],
    );

    let outcome = group
        .resync(&sibling, 1, sibling.signed_by(&alice))
        .unwrap();
    assert_eq!(held_back(outcome), [sibling.client_id()]);
}

#[tokio::test]
async fn a_resync_is_authenticated_against_the_key_the_as_stores() {
    let alice = random_user_signing_key();
    for (stored, signer, accepted) in authentication_cases(&alice) {
        let sibling = Device::new();
        let mut group =
            SelfGroup::with_joiners(Device::unsigned, vec![(&sibling, sibling.unsigned())]);
        let result = group
            .resync(&sibling, 1, sibling.signed_by(&signer))
            .unwrap()
            .authenticate(&stored)
            .await;
        assert_authenticated(result, accepted);
    }
}

#[tokio::test]
async fn a_resync_with_an_invalid_credential_is_rejected() {
    let alice = random_user_signing_key();
    let sibling = Device::new();
    for invalid in INVALID_CREDENTIALS {
        let mut group = SelfGroup::with_joiners(
            |creator| creator.signed_by(&alice),
            vec![(&sibling, sibling.unsigned())],
        );
        let result = group.resync(&sibling, 1, invalid(&sibling, &alice));
        assert!(matches!(result, Err(ResyncClientError::InvalidMessage)));
    }
}

#[tokio::test]
async fn a_resync_may_replace_the_only_signed_leaf() {
    let sibling = Device::new();
    let mut group = SelfGroup::with_joiners(
        Device::unsigned,
        vec![(&sibling, sibling.signed_by(&random_user_signing_key()))],
    );

    let bob = random_user_signing_key();
    group
        .resync(&sibling, 1, sibling.signed_by(&bob))
        .unwrap()
        .authenticate(&StoredKeys::of(&[&bob]))
        .await
        .unwrap();
}

#[tokio::test]
async fn unsigned_credentials_are_accepted_without_an_as_lookup() {
    let sibling = Device::new();
    let joiner = Device::new();
    let mut group = SelfGroup::with_joiners(Device::unsigned, vec![(&sibling, sibling.unsigned())]);
    let unsigned = group.creator.unsigned();

    let outcomes = [
        group.add(&joiner, joiner.unsigned()).await.unwrap(),
        group.update(unsigned).await.unwrap(),
    ];
    for outcome in outcomes {
        outcome.authenticate(&FailingAs).await.unwrap();
    }
    group
        .resync(&sibling, 1, sibling.unsigned())
        .unwrap()
        .authenticate(&FailingAs)
        .await
        .unwrap();
}

#[tokio::test]
async fn a_leaf_with_an_invalid_credential_can_be_removed() {
    let alice = random_user_signing_key();
    let malformed = Device::new();
    let unverifiable = Device::new();
    let mut group = SelfGroup::with_joiners(
        |creator| creator.signed_by(&alice),
        vec![
            (&malformed, malformed.malformed()),
            (&unverifiable, unverifiable.signed_by(&other_key_of(&alice))),
        ],
    );

    for index in [1, 2] {
        group
            .remove(index)
            .await
            .unwrap()
            .authenticate(&StoredKeys::of(&[&alice]))
            .await
            .unwrap();
    }
}

#[test]
fn a_delete_from_a_signed_leaf_passes() {
    let sibling = Device::new();
    let mut group = SelfGroup::with_joiners(
        |creator| creator.signed_by(&random_user_signing_key()),
        vec![(&sibling, sibling.signed_by(&random_user_signing_key()))],
    );

    let (commit, _welcome, group_info) =
        group.stage_commit(Vec::new(), vec![LeafNodeIndex::new(1)], None);
    // Deletion takes no AS connector, so it can't look up a key.
    group
        .state
        .delete_group(assisted(commit, group_info), DeleteKind::Member)
        .unwrap();
}

/// Runs the creator check of all creation paths on a self group whose creator leaf carries
/// `credential`, with the creator credential of `creator`.
fn create(device: &Device, credential: Credential, creator: &UserSigningKey) -> Result<(), Status> {
    let group = new_group(device, credential);
    let state = ds_group_state(&device.provider, &group, &device.key);
    let provided: UserCredentialProto = creator.credential().try_ref_into().unwrap();
    creator_credential(state.group(), Some(&provided)).map(|_| ())
}

#[test]
fn creation_checks_the_creator_leaf_against_the_creator_credential() {
    let alice = random_user_signing_key();
    let device = Device::new();
    let cases = [
        (device.signed_by(&alice), true),
        (device.unsigned(), true),
        // The signature doesn't verify with the creator's key.
        (device.signed_by(&other_key_of(&alice)), false),
        (device.signed_by(&random_user_signing_key()), false),
    ];
    for (credential, accepted) in cases {
        assert_authenticated(create(&device, credential, &alice), accepted);
    }
}

#[test]
fn creation_rejects_a_creator_leaf_with_a_malformed_credential() {
    let device = Device::new();
    let status = create(&device, device.malformed(), &random_user_signing_key()).unwrap_err();
    assert_eq!(status.code(), Code::InvalidArgument);
}

#[test]
fn an_apq_update_path_checks_the_self_group_credential() {
    let alice = random_user_signing_key();
    let sibling = Device::new();
    let new_group = || {
        ApqSelfGroup::with_joiners(
            Device::unsigned,
            vec![(&sibling, sibling.signed_by(&alice))],
        )
    };

    let mut group = new_group();
    let signed = group.creator.signed_by(&alice);
    assert_eq!(
        held_back(group.update(signed).unwrap()),
        [group.creator.client_id()]
    );

    for invalid in INVALID_CREDENTIALS {
        let mut group = new_group();
        let credential = invalid(&group.creator, &alice);
        assert!(matches!(
            group.update(credential),
            Err(GroupOperationError::InvalidMessage)
        ));
    }
}

#[test]
fn an_apq_resync_checks_the_self_group_credential() {
    let alice = random_user_signing_key();
    let sibling = Device::new();
    let new_group = || {
        ApqSelfGroup::with_joiners(
            |creator| creator.signed_by(&alice),
            vec![(&sibling, sibling.unsigned())],
        )
    };

    let outcome = new_group()
        .resync(&sibling, 1, sibling.signed_by(&alice))
        .unwrap();
    assert_eq!(held_back(outcome), [sibling.client_id()]);

    for invalid in INVALID_CREDENTIALS {
        let result = new_group().resync(&sibling, 1, invalid(&sibling, &alice));
        assert!(matches!(result, Err(ResyncClientError::InvalidMessage)));
    }
}
