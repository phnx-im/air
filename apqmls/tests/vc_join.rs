// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use apqmls::{
    ApqMlsGroup,
    authentication::{ApqSignatureKeyPair, ApqSigner},
    commit_builder::ApqCommitMessageBundle,
    extension::{APQMLS_COMPONENT_ID, ApqInfo, PqtMode},
    messages::{ApqMlsMessageIn, ApqMlsMessageOut, ApqRatchetTreeIn, VerifiableApqGroupInfo},
    vc_join::{ApqGroupInfoLinkageError, VcCreationJoinError, VcSiblingExternalCommitJoinError},
};
use openmls::{
    component::{ComponentId, ComponentType},
    components::vc_derivation_info::{EpochId, VC_COMPONENT_ID},
    group::{
        GroupContext, GroupEpoch, GroupId, MlsGroup, MlsGroupCreateConfig, MlsGroupJoinConfig,
        PURE_PLAINTEXT_WIRE_FORMAT_POLICY, StagedWelcome, VcExternalCommitJoinError,
        VcGroupCreationJoinError,
    },
    prelude::{
        AppDataDictionary, AppDataDictionaryExtension, Capabilities, Ciphersuite, Credential,
        Extension, ExtensionType, Extensions, KeyPackage, LeafNode, LeafNodeParameters,
        MlsMessageBodyIn, MlsMessageIn, MlsMessageOut, OpenMlsProvider, PreSharedKeyProposal,
        ProcessedMessageContent, ProtocolMessage, group_info::VerifiableGroupInfo,
    },
    schedule::{
        PreSharedKeyId,
        psk::{ExternalPsk, Psk},
    },
};
use openmls_rust_crypto::OpenMlsRustCrypto;
use tls_codec::{Deserialize as _, Serialize as _};

use crate::utils::{assert_groups_eq, client::Client};

mod utils;

const TEST_MODES: [PqtMode; 2] = [PqtMode::ConfAndAuth, PqtMode::ConfOnly];

fn new_client(identity: &str, mode: PqtMode) -> Client<OpenMlsRustCrypto> {
    Client::new(
        identity,
        mode.default_ciphersuite().into(),
        OpenMlsRustCrypto::default(),
    )
}

fn join_config() -> MlsGroupJoinConfig {
    MlsGroupJoinConfig::builder()
        .wire_format_policy(PURE_PLAINTEXT_WIRE_FORMAT_POLICY)
        .build()
}

/// A leaf sending virtual-client operations must declare `AppDataDictionary`.
fn vc_capabilities() -> Capabilities {
    Capabilities::builder()
        .extensions(vec![ExtensionType::AppDataDictionary])
        .build()
}

/// The leaf-node extensions a virtual client's leaf must carry: an
/// `AppComponents` entry listing the virtual-clients component, which is what
/// makes the receiver surface the derivation info.
fn vc_leaf_extensions() -> Extensions<LeafNode> {
    let components: Vec<ComponentId> = vec![VC_COMPONENT_ID, APQMLS_COMPONENT_ID];
    let mut dictionary = AppDataDictionary::new();
    dictionary.insert(
        ComponentId::from(ComponentType::AppComponents),
        components.tls_serialize_detached().unwrap(),
    );
    Extensions::from_vec(vec![Extension::AppDataDictionary(
        AppDataDictionaryExtension::new(dictionary),
    )])
    .unwrap()
}

/// Sets up two emulator clients of one virtual client: `provider_a` founds the
/// emulation group, `provider_b` joins it via welcome, and both register the
/// same emulation epoch. The emulation group is a plain MLS group, so it runs
/// on the traditional ciphersuite.
fn setup_sibling_emulation_epoch(
    ciphersuite: Ciphersuite,
    provider_a: &OpenMlsRustCrypto,
    provider_b: &OpenMlsRustCrypto,
) -> EpochId {
    let emulator_config = MlsGroupCreateConfig::builder()
        .ciphersuite(ciphersuite)
        .wire_format_policy(PURE_PLAINTEXT_WIRE_FORMAT_POLICY)
        .capabilities(vc_capabilities())
        .with_leaf_node_extensions(vc_leaf_extensions())
        .unwrap()
        .build();

    let (credential_a, signer_a) = openmls::prelude::test_utils::new_credential(
        provider_a,
        b"AliceEmulatorA",
        ciphersuite.signature_algorithm(),
    );
    let mut emulator_a =
        MlsGroup::new(provider_a, &signer_a, &emulator_config, credential_a).unwrap();

    let (credential_b, signer_b) = openmls::prelude::test_utils::new_credential(
        provider_b,
        b"AliceEmulatorB",
        ciphersuite.signature_algorithm(),
    );
    let key_package_b = KeyPackage::builder()
        .leaf_node_capabilities(vc_capabilities())
        .leaf_node_extensions(vc_leaf_extensions())
        .build(ciphersuite, provider_b, &signer_b, credential_b)
        .unwrap()
        .key_package()
        .to_owned();
    let (_commit, welcome, _group_info) = emulator_a
        .add_members(provider_a, &signer_a, &[key_package_b])
        .unwrap();
    emulator_a.merge_pending_commit(provider_a).unwrap();
    let mut emulator_b = StagedWelcome::new_from_welcome(
        provider_b,
        &join_config(),
        welcome.into_welcome().unwrap(),
        Some(emulator_a.export_ratchet_tree().into()),
    )
    .unwrap()
    .into_group(provider_b)
    .unwrap();

    let epoch_id = emulator_a
        .register_vc_emulation_epoch(provider_a.crypto(), provider_a.storage())
        .unwrap();
    let sibling_epoch_id = emulator_b
        .register_vc_emulation_epoch(provider_b.crypto(), provider_b.storage())
        .unwrap();
    assert_eq!(
        epoch_id, sibling_epoch_id,
        "siblings must derive the same EpochId"
    );
    epoch_id
}

/// One virtual client with two emulator clients: separate providers, one shared
/// signing identity, one shared emulation epoch.
struct VirtualClient {
    a: Client<OpenMlsRustCrypto>,
    b: Client<OpenMlsRustCrypto>,
    epoch_id: EpochId,
}

fn new_virtual_client(identity: &str, mode: PqtMode) -> VirtualClient {
    let ciphersuite = mode.default_ciphersuite();
    let provider_a = OpenMlsRustCrypto::default();
    let provider_b = OpenMlsRustCrypto::default();
    let epoch_id =
        setup_sibling_emulation_epoch(ciphersuite.t_ciphersuite(), &provider_a, &provider_b);

    // Both emulator clients hold the shared signing identity, so either can sign for the shared
    // leaf.
    let signer = ApqSignatureKeyPair::new(ciphersuite.into()).unwrap();
    for provider in [&provider_a, &provider_b] {
        signer.t_signer().store(provider.storage()).unwrap();
        signer.pq_signer().store(provider.storage()).unwrap();
    }

    VirtualClient {
        a: Client::with_signer(identity, signer.clone(), provider_a),
        b: Client::with_signer(identity, signer, provider_b),
        epoch_id,
    }
}

/// A plain APQ group, created by a client that is not a virtual client.
fn create_group(client: &Client<OpenMlsRustCrypto>, mode: PqtMode) -> ApqMlsGroup {
    ApqMlsGroup::builder()
        .set_mode(mode)
        .with_wire_format_policy(PURE_PLAINTEXT_WIRE_FORMAT_POLICY)
        .build(
            &client.provider,
            &client.signer,
            client.credential_with_key.clone(),
        )
        .unwrap()
}

/// An APQ group founded on the shared virtual-client leaf.
fn create_vc_group(
    client: &Client<OpenMlsRustCrypto>,
    mode: PqtMode,
    epoch_id: EpochId,
) -> ApqMlsGroup {
    ApqMlsGroup::builder()
        .set_mode(mode)
        .with_wire_format_policy(PURE_PLAINTEXT_WIRE_FORMAT_POLICY)
        .with_capabilities(vc_capabilities())
        .with_leaf_node_extensions(vc_leaf_extensions(), vc_leaf_extensions())
        .unwrap()
        .vc_emulation(epoch_id)
        .build(
            &client.provider,
            &client.signer,
            client.credential_with_key.clone(),
        )
        .unwrap()
}

/// Exports what the DS would serve to a joiner.
fn export_join_info(
    client: &Client<OpenMlsRustCrypto>,
    group: &ApqMlsGroup,
) -> (VerifiableApqGroupInfo, ApqRatchetTreeIn) {
    let message = group
        .export_group_info(client.provider.crypto(), &client.signer, false)
        .unwrap();
    let group_info = ApqMlsMessageIn::try_from(message)
        .unwrap()
        .into_verifiable_group_info()
        .unwrap();
    (group_info, group.export_ratchet_tree().into())
}

/// The two halves' group infos separately, so tests can recombine them.
fn export_half_group_infos(
    client: &Client<OpenMlsRustCrypto>,
    group: &ApqMlsGroup,
) -> (VerifiableGroupInfo, VerifiableGroupInfo) {
    let (t_message, pq_message) = group
        .export_group_info(client.provider.crypto(), &client.signer, false)
        .unwrap()
        .split();
    (
        verifiable_group_info(t_message),
        verifiable_group_info(pq_message),
    )
}

fn verifiable_group_info(message: MlsMessageOut) -> VerifiableGroupInfo {
    let MlsMessageBodyIn::GroupInfo(group_info) = roundtrip(message).extract() else {
        panic!("expected a group info");
    };
    group_info
}

/// An external join on the shared virtual-client leaf, optionally carrying
/// application PSK proposals in the T commit.
fn external_vc_join(
    client: &Client<OpenMlsRustCrypto>,
    epoch_id: &EpochId,
    group_info: VerifiableApqGroupInfo,
    ratchet_tree: ApqRatchetTreeIn,
    t_psk_proposals: Vec<PreSharedKeyProposal>,
) -> (ApqMlsGroup, ApqCommitMessageBundle) {
    let leaf_node_parameters = LeafNodeParameters::builder()
        .with_capabilities(vc_capabilities())
        .with_extensions(vc_leaf_extensions())
        .build();
    let mut builder = ApqMlsGroup::external_commit_builder()
        .with_ratchet_tree(ratchet_tree)
        .with_config(join_config())
        .leaf_node_parameters(leaf_node_parameters.clone(), leaf_node_parameters)
        .vc_emulation(epoch_id.clone());
    for proposal in t_psk_proposals {
        builder = builder.add_t_psk_proposal(proposal);
    }
    builder
        .build(
            &client.provider,
            &client.signer,
            client.credential_with_key.clone(),
            group_info,
        )
        .unwrap()
}

/// Simulates the wire: serializes an outgoing message and deserializes it as an incoming one.
fn roundtrip(message: MlsMessageOut) -> MlsMessageIn {
    MlsMessageIn::tls_deserialize_exact(message.tls_serialize_detached().unwrap()).unwrap()
}

fn protocol_messages(message: ApqMlsMessageOut) -> (ProtocolMessage, ProtocolMessage) {
    let (t_message, pq_message) = message.split();
    (
        roundtrip(t_message).try_into_protocol_message().unwrap(),
        roundtrip(pq_message).try_into_protocol_message().unwrap(),
    )
}

fn compare_credentials(cred1: &Credential, cred2: &Credential) -> bool {
    cred1 == cred2
}

fn process_and_merge(
    client: &Client<OpenMlsRustCrypto>,
    group: &mut ApqMlsGroup,
    message: ApqMlsMessageOut,
) {
    let protocol_message = ApqMlsMessageIn::try_from(message)
        .unwrap()
        .into_protocol_message()
        .unwrap();
    let staged_commit = group
        .process_message(&client.provider, protocol_message, compare_credentials)
        .unwrap()
        .into_staged_commit()
        .unwrap();
    group
        .merge_staged_commit(&client.provider, staged_commit)
        .unwrap();
}

/// Application messages are only sent in the T group.
fn send_t_message(
    sender: &Client<OpenMlsRustCrypto>,
    group: &mut ApqMlsGroup,
    payload: &[u8],
) -> MlsMessageOut {
    group
        .t_group
        .create_message(&sender.provider, sender.signer.t_signer(), payload)
        .unwrap()
}

fn receive_t_message(
    receiver: &Client<OpenMlsRustCrypto>,
    group: &mut ApqMlsGroup,
    message: MlsMessageOut,
) -> Vec<u8> {
    let protocol_message = roundtrip(message).try_into_protocol_message().unwrap();
    let processed_message = group
        .t_group
        .process_message(&receiver.provider, protocol_message)
        .unwrap();
    let ProcessedMessageContent::ApplicationMessage(application_message) =
        processed_message.into_content()
    else {
        panic!("expected an application message");
    };
    application_message.into_bytes()
}

fn assert_same_leaf(group1: &ApqMlsGroup, group2: &ApqMlsGroup) {
    assert_eq!(
        group1.t_group.own_leaf_index(),
        group2.t_group.own_leaf_index(),
        "the siblings must share the T leaf"
    );
    assert_eq!(
        group1.pq_group().own_leaf_index(),
        group2.pq_group().own_leaf_index(),
        "the siblings must share the PQ leaf"
    );
    assert_eq!(group1.t_epoch(), group2.t_epoch());
    assert_eq!(group1.pq_epoch(), group2.pq_epoch());
}

/// An application PSK, as a connection offer would carry it. The same PSK value
/// is stored in every provider, each under its own nonce, because PSK storage is
/// keyed on the value.
fn store_application_psk<'a>(
    providers: impl IntoIterator<Item = &'a OpenMlsRustCrypto>,
    ciphersuite: Ciphersuite,
    psk_id: &[u8],
    psk: &[u8],
) -> Vec<PreSharedKeyId> {
    providers
        .into_iter()
        .map(|provider| {
            let id = PreSharedKeyId::new(
                ciphersuite,
                provider.rand(),
                Psk::External(ExternalPsk::new(psk_id.to_vec())),
            )
            .unwrap();
            id.store(provider, psk).unwrap();
            id
        })
        .collect()
}

#[test]
fn sibling_joins_group_created_by_virtual_client() {
    for mode in TEST_MODES {
        let alice = new_virtual_client("Alice (VC)", mode);
        let bob = new_client("Bob", mode);

        let mut alice_a_group = create_vc_group(&alice.a, mode, alice.epoch_id.clone());
        let (group_info, ratchet_tree) = export_join_info(&alice.a, &alice_a_group);

        let mut alice_b_group = ApqMlsGroup::vc_join_at_creation(
            &alice.b.provider,
            &join_config(),
            group_info,
            Some(ratchet_tree),
            alice.epoch_id.clone(),
        )
        .unwrap();

        assert_groups_eq(&mut alice_a_group, &mut alice_b_group);
        assert_same_leaf(&alice_a_group, &alice_b_group);

        // The reconstructed state is functional: the sibling adds Bob and exchanges an application
        // message with him.
        let bundle = alice_b_group
            .commit_builder()
            .propose_adds([bob.generate_key_package(mode.default_ciphersuite())])
            .vc_emulation(alice.epoch_id.clone())
            .finalize(&alice.b.provider, &alice.b.signer, |_| true, |_| true)
            .unwrap();
        alice_b_group
            .merge_pending_commit(&alice.b.provider)
            .unwrap();
        let mut bob_group = ApqMlsGroup::new_from_welcome(
            &bob.provider,
            &join_config(),
            bundle.into_welcome().unwrap(),
            Some(alice_b_group.export_ratchet_tree().into()),
        )
        .unwrap();

        assert_groups_eq(&mut alice_b_group, &mut bob_group);
        let message = send_t_message(&alice.b, &mut alice_b_group, b"hello bob");
        assert_eq!(
            receive_t_message(&bob, &mut bob_group, message),
            b"hello bob"
        );
    }
}

#[test]
fn sibling_joins_group_via_external_commit_of_virtual_client() {
    for mode in TEST_MODES {
        let alice = new_virtual_client("Alice (VC)", mode);
        let bob = new_client("Bob", mode);
        let mut bob_group = create_group(&bob, mode);

        // Two copies of the pre-commit join info: one is consumed by the external commit, the other
        // lets the sibling rebuild the pre-commit public group.
        let (group_info, ratchet_tree) = export_join_info(&bob, &bob_group);
        let (sibling_group_info, sibling_ratchet_tree) = export_join_info(&bob, &bob_group);

        let (mut alice_a_group, bundle) = external_vc_join(
            &alice.a,
            &alice.epoch_id,
            group_info,
            ratchet_tree,
            Vec::new(),
        );
        process_and_merge(&bob, &mut bob_group, bundle.commit.clone());

        let (t_commit, pq_commit) = protocol_messages(bundle.commit);
        let mut alice_b_group = ApqMlsGroup::vc_join_via_sibling_external_commit(
            &alice.b.provider,
            &join_config(),
            sibling_group_info,
            Some(sibling_ratchet_tree),
            t_commit,
            pq_commit,
            alice.epoch_id.clone(),
        )
        .unwrap();

        assert_groups_eq(&mut alice_a_group, &mut alice_b_group);
        assert_same_leaf(&alice_a_group, &alice_b_group);

        // The sibling is a working member: Bob decrypts its application message.
        let message = send_t_message(&alice.b, &mut alice_b_group, b"hello from the sibling");
        assert_eq!(
            receive_t_message(&bob, &mut bob_group, message),
            b"hello from the sibling"
        );
    }
}

#[test]
fn sibling_join_resolves_application_psk_of_external_commit() {
    let mode = PqtMode::ConfAndAuth;
    let alice = new_virtual_client("Alice (VC)", mode);
    let bob = new_client("Bob", mode);
    let mut bob_group = create_group(&bob, mode);

    // The PSK is known to the committing sibling, to the joining sibling and to Bob, who has to
    // resolve it when processing the commit.
    let psk_ids = store_application_psk(
        [&alice.a.provider, &alice.b.provider, &bob.provider],
        mode.default_ciphersuite().t_ciphersuite(),
        b"connection offer",
        b"connection offer psk",
    );
    let committer_psk_id = psk_ids[0].clone();
    assert_ne!(
        committer_psk_id.psk_nonce(),
        psk_ids[1].psk_nonce(),
        "the sibling must resolve the PSK stored under its own nonce"
    );

    let (group_info, ratchet_tree) = export_join_info(&bob, &bob_group);
    let (sibling_group_info, sibling_ratchet_tree) = export_join_info(&bob, &bob_group);

    let (mut alice_a_group, bundle) = external_vc_join(
        &alice.a,
        &alice.epoch_id,
        group_info,
        ratchet_tree,
        vec![PreSharedKeyProposal::new(committer_psk_id)],
    );
    process_and_merge(&bob, &mut bob_group, bundle.commit.clone());

    let (t_commit, pq_commit) = protocol_messages(bundle.commit);
    let mut alice_b_group = ApqMlsGroup::vc_join_via_sibling_external_commit(
        &alice.b.provider,
        &join_config(),
        sibling_group_info,
        Some(sibling_ratchet_tree),
        t_commit,
        pq_commit,
        alice.epoch_id.clone(),
    )
    .unwrap();

    assert_groups_eq(&mut alice_a_group, &mut alice_b_group);
    assert_same_leaf(&alice_a_group, &alice_b_group);
}

#[test]
fn creation_join_with_foreign_epoch_id_fails() {
    let mode = PqtMode::ConfAndAuth;
    let alice = new_virtual_client("Alice (VC)", mode);
    let foreign = new_virtual_client("Foreign (VC)", mode);

    let alice_a_group = create_vc_group(&alice.a, mode, alice.epoch_id.clone());
    let (group_info, ratchet_tree) = export_join_info(&alice.a, &alice_a_group);

    let result = ApqMlsGroup::vc_join_at_creation(
        &alice.b.provider,
        &join_config(),
        group_info,
        Some(ratchet_tree),
        foreign.epoch_id.clone(),
    );
    assert!(matches!(
        result,
        Err(VcCreationJoinError::Join(
            VcGroupCreationJoinError::EpochIdMismatch
        ))
    ));

    // Neither half was created.
    assert!(
        ApqMlsGroup::load(alice.b.provider.storage(), &alice_a_group.group_id())
            .unwrap()
            .is_none()
    );
}

#[test]
fn sibling_external_commit_join_with_foreign_epoch_id_fails() {
    let mode = PqtMode::ConfAndAuth;
    let alice = new_virtual_client("Alice (VC)", mode);
    let foreign = new_virtual_client("Foreign (VC)", mode);
    let bob = new_client("Bob", mode);
    let bob_group = create_group(&bob, mode);

    let (group_info, ratchet_tree) = export_join_info(&bob, &bob_group);
    let (sibling_group_info, sibling_ratchet_tree) = export_join_info(&bob, &bob_group);

    let (_alice_a_group, bundle) = external_vc_join(
        &alice.a,
        &alice.epoch_id,
        group_info,
        ratchet_tree,
        Vec::new(),
    );

    let (t_commit, pq_commit) = protocol_messages(bundle.commit);
    let result = ApqMlsGroup::vc_join_via_sibling_external_commit(
        &alice.b.provider,
        &join_config(),
        sibling_group_info,
        Some(sibling_ratchet_tree),
        t_commit,
        pq_commit,
        foreign.epoch_id.clone(),
    );
    assert!(matches!(
        result,
        Err(VcSiblingExternalCommitJoinError::Join(
            VcExternalCommitJoinError::EpochIdMismatch
        ))
    ));

    // The PQ half is attempted first and fails, so neither half is left in storage.
    assert!(
        ApqMlsGroup::load(alice.b.provider.storage(), &bob_group.group_id())
            .unwrap()
            .is_none()
    );
}

#[test]
fn creation_join_rolls_back_t_half_when_pq_half_fails() {
    let mode = PqtMode::ConfAndAuth;
    let alice = new_virtual_client("Alice (VC)", mode);

    let alice_a_group = create_vc_group(&alice.a, mode, alice.epoch_id.clone());
    let (group_info, ratchet_tree) = export_join_info(&alice.a, &alice_a_group);

    // The PQ half is served the T half's ratchet tree. The linkage check only looks at the group
    // infos and passes, and the T half joins and persists, so the PQ half is the first to fail, on
    // the leaf signatures of the foreign tree.
    let (t_ratchet_tree, _pq_ratchet_tree) = ratchet_tree.split();
    let ratchet_tree = ApqRatchetTreeIn::new(t_ratchet_tree.clone(), t_ratchet_tree);

    let result = ApqMlsGroup::vc_join_at_creation(
        &alice.b.provider,
        &join_config(),
        group_info,
        Some(ratchet_tree),
        alice.epoch_id.clone(),
    );
    assert!(result.is_err());

    assert!(
        MlsGroup::load(alice.b.provider.storage(), alice_a_group.t_group.group_id())
            .unwrap()
            .is_none()
    );
}

#[test]
fn sibling_external_commit_join_rolls_back_pq_half_when_t_half_fails() {
    let mode = PqtMode::ConfAndAuth;
    let alice = new_virtual_client("Alice (VC)", mode);
    let bob = new_client("Bob", mode);
    let mut bob_group = create_group(&bob, mode);

    // The application PSK is only stored by the committing sibling and by Bob, so the joining
    // sibling cannot resolve it. The PQ half has nothing to do with the proposal and is joined and
    // merged first, so the T half is the first to fail.
    let psk_ids = store_application_psk(
        [&alice.a.provider, &bob.provider],
        mode.default_ciphersuite().t_ciphersuite(),
        b"connection offer",
        b"connection offer psk",
    );

    let (group_info, ratchet_tree) = export_join_info(&bob, &bob_group);
    let (sibling_group_info, sibling_ratchet_tree) = export_join_info(&bob, &bob_group);

    let (_alice_a_group, bundle) = external_vc_join(
        &alice.a,
        &alice.epoch_id,
        group_info,
        ratchet_tree,
        vec![PreSharedKeyProposal::new(psk_ids[0].clone())],
    );
    process_and_merge(&bob, &mut bob_group, bundle.commit.clone());

    let (t_commit, pq_commit) = protocol_messages(bundle.commit);
    let result = ApqMlsGroup::vc_join_via_sibling_external_commit(
        &alice.b.provider,
        &join_config(),
        sibling_group_info,
        Some(sibling_ratchet_tree),
        t_commit,
        pq_commit,
        alice.epoch_id.clone(),
    );
    assert!(result.is_err());

    assert!(
        MlsGroup::load(alice.b.provider.storage(), bob_group.pq_group().group_id())
            .unwrap()
            .is_none()
    );
}

#[test]
fn creation_join_with_group_infos_of_different_groups_fails() {
    let mode = PqtMode::ConfAndAuth;
    let alice = new_virtual_client("Alice (VC)", mode);
    let bob = new_client("Bob", mode);

    let first_group = create_group(&bob, mode);
    let second_group = create_group(&bob, mode);
    let (t_group_info, _) = export_half_group_infos(&bob, &first_group);
    let (_, pq_group_info) = export_half_group_infos(&bob, &second_group);

    let result = ApqMlsGroup::vc_join_at_creation(
        &alice.b.provider,
        &join_config(),
        VerifiableApqGroupInfo::new(t_group_info, pq_group_info),
        None,
        alice.epoch_id.clone(),
    );
    assert!(matches!(
        result,
        Err(VcCreationJoinError::Linkage(
            ApqGroupInfoLinkageError::ApqInfoMismatch
        ))
    ));
}

/// Swapping the two halves' group infos leaves their ApqInfo equal, so the
/// group-ID check is the first one to fail.
#[test]
fn creation_join_with_swapped_group_infos_fails() {
    let mode = PqtMode::ConfAndAuth;
    let alice = new_virtual_client("Alice (VC)", mode);
    let bob = new_client("Bob", mode);

    let bob_group = create_group(&bob, mode);
    let (t_group_info, pq_group_info) = export_half_group_infos(&bob, &bob_group);

    let result = ApqMlsGroup::vc_join_at_creation(
        &alice.b.provider,
        &join_config(),
        VerifiableApqGroupInfo::new(pq_group_info, t_group_info),
        None,
        alice.epoch_id.clone(),
    );
    assert!(matches!(
        result,
        Err(VcCreationJoinError::Linkage(
            ApqGroupInfoLinkageError::GroupIdMismatch
        ))
    ));
}

#[test]
fn creation_join_with_mismatched_ciphersuite_fails() {
    let mode = PqtMode::ConfAndAuth;
    let alice = new_virtual_client("Alice (VC)", mode);
    let bob = new_client("Bob", mode);
    let ciphersuite = mode.default_ciphersuite();

    // A real APQ group always carries an ApqInfo that agrees with the ciphersuites of its halves, so
    // the mismatch is built from two plain MLS groups on the traditional ciphersuite that carry a
    // hand-made ApqInfo. It names their group IDs, so the ApqInfo and group-ID checks pass, but it
    // claims the post-quantum ciphersuite for the PQ half.
    let t_group_id = GroupId::from_slice(b"plain_t_group");
    let pq_group_id = GroupId::from_slice(b"plain_pq_group");
    let apq_info = ApqInfo {
        t_session_group_id: t_group_id.clone(),
        pq_session_group_id: pq_group_id.clone(),
        mode,
        t_cipher_suite: ciphersuite.t_ciphersuite(),
        pq_cipher_suite: ciphersuite.pq_ciphersuite(),
        t_epoch: GroupEpoch::from(0),
        pq_epoch: GroupEpoch::from(0),
    };
    let mut dictionary = AppDataDictionary::new();
    dictionary.insert(
        APQMLS_COMPONENT_ID,
        apq_info.tls_serialize_detached().unwrap(),
    );
    let extensions = Extensions::<GroupContext>::from_vec(vec![Extension::AppDataDictionary(
        AppDataDictionaryExtension::new(dictionary),
    )])
    .unwrap();

    let create_config = MlsGroupCreateConfig::builder()
        .ciphersuite(ciphersuite.t_ciphersuite())
        .capabilities(vc_capabilities())
        .with_group_context_extensions(extensions)
        .build();
    let plain_group = |group_id: GroupId| {
        MlsGroup::new_with_group_id(
            &bob.provider,
            bob.signer.t_signer(),
            &create_config,
            group_id,
            bob.credential_with_key.t_credential.clone(),
        )
        .unwrap()
    };
    let t_group = plain_group(t_group_id);
    let pq_group = plain_group(pq_group_id);

    let export_group_info = |group: &MlsGroup| {
        verifiable_group_info(
            group
                .export_group_info(bob.provider.crypto(), bob.signer.t_signer(), false)
                .unwrap(),
        )
    };
    let result = ApqMlsGroup::vc_join_at_creation(
        &alice.b.provider,
        &join_config(),
        VerifiableApqGroupInfo::new(export_group_info(&t_group), export_group_info(&pq_group)),
        None,
        alice.epoch_id.clone(),
    );
    assert!(matches!(
        result,
        Err(VcCreationJoinError::Linkage(
            ApqGroupInfoLinkageError::CiphersuiteMismatch
        ))
    ));
}
