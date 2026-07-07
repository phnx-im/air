// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use apqmls::{
    ApqMlsGroup,
    authentication::ApqSigner,
    commit_builder::ApqCommitMessageBundle,
    extension::PqtMode,
    external_commit_builder::ApqExternalCommitBuilderError,
    messages::{
        ApqMlsMessageIn, ApqMlsMessageOut, ApqProposalIn, ApqProtocolMessage, ApqRatchetTreeIn,
        VerifiableApqGroupInfo,
    },
    processing::ApqProcessedMessage,
    public_group::ApqPublicGroup,
};
use openmls::{
    group::{
        GroupId, MlsGroup, MlsGroupCreateConfig, MlsGroupJoinConfig,
        PURE_PLAINTEXT_WIRE_FORMAT_POLICY, ProposalStore,
    },
    prelude::{
        Capabilities, Credential, LeafNodeIndex, LeafNodeParameters, MlsMessageBodyIn,
        MlsMessageIn, MlsMessageOut, OpenMlsProvider, PreSharedKeyProposal,
        ProcessedMessageContent, ProposalType, PublicGroup, PublicMessageIn,
    },
    schedule::PreSharedKeyId,
};
use openmls_rust_crypto::OpenMlsRustCrypto;
use tls_codec::{Deserialize as _, Serialize as _};

use crate::utils::{assert_groups_eq, client::Client};

mod utils;

const TEST_MODES: [PqtMode; 2] = [PqtMode::ConfAndAuth, PqtMode::ConfOnly];

fn compare_credentials(cred1: &Credential, cred2: &Credential) -> bool {
    cred1 == cred2
}

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

fn test_capabilities() -> Capabilities {
    Capabilities::new(None, None, None, Some(&[ProposalType::SelfRemove]), None)
}

fn create_group(client: &Client<OpenMlsRustCrypto>, mode: PqtMode) -> ApqMlsGroup {
    ApqMlsGroup::builder()
        .with_group_ids(
            GroupId::random(client.provider.rand()),
            GroupId::from_slice(b"test_pq_group"),
        )
        .with_wire_format_policy(PURE_PLAINTEXT_WIRE_FORMAT_POLICY)
        .with_capabilities(test_capabilities())
        .set_mode(mode)
        .build(
            &client.provider,
            &client.signer,
            client.credential_with_key.clone(),
        )
        .unwrap()
}

/// Adds members via an Add commit + Welcome and returns their groups.
fn add_members(
    adder: &Client<OpenMlsRustCrypto>,
    adder_group: &mut ApqMlsGroup,
    joiners: &[&Client<OpenMlsRustCrypto>],
    mode: PqtMode,
) -> Vec<ApqMlsGroup> {
    let key_packages: Vec<_> = joiners
        .iter()
        .map(|joiner| {
            joiner.generate_key_package_with_capabilities(
                mode.default_ciphersuite(),
                test_capabilities(),
            )
        })
        .collect();
    let bundle = adder_group
        .commit_builder()
        .propose_adds(key_packages)
        .finalize(&adder.provider, &adder.signer, |_| true, |_| true)
        .unwrap();
    adder_group.merge_pending_commit(&adder.provider).unwrap();
    let welcome = bundle.into_welcome().unwrap();
    joiners
        .iter()
        .map(|joiner| {
            ApqMlsGroup::new_from_welcome(
                &joiner.provider,
                &join_config(),
                welcome.clone(),
                Some(adder_group.export_ratchet_tree().into()),
            )
            .unwrap()
        })
        .collect()
}

/// Exports what the DS would serve to an external joiner.
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

fn into_protocol_message(message: ApqMlsMessageOut) -> ApqProtocolMessage {
    ApqMlsMessageIn::try_from(message)
        .unwrap()
        .into_protocol_message()
        .unwrap()
}

/// Simulates the wire: serializes an outgoing message and deserializes it as an incoming one.
fn roundtrip(message: MlsMessageOut) -> MlsMessageIn {
    MlsMessageIn::tls_deserialize_exact(message.tls_serialize_detached().unwrap()).unwrap()
}

fn public_message_in(message: MlsMessageOut) -> PublicMessageIn {
    let MlsMessageBodyIn::PublicMessage(public_message) = roundtrip(message).extract() else {
        panic!("expected a public message");
    };
    public_message
}

fn external_join(
    client: &Client<OpenMlsRustCrypto>,
    group_info: VerifiableApqGroupInfo,
    ratchet_tree: ApqRatchetTreeIn,
) -> (ApqMlsGroup, ApqCommitMessageBundle) {
    let (group, bundle) = ApqMlsGroup::external_commit_builder()
        .with_ratchet_tree(ratchet_tree)
        .with_config(join_config())
        .create_group_info(true)
        .build(
            &client.provider,
            &client.signer,
            client.credential_with_key.clone(),
            group_info,
        )
        .unwrap();
    assert!(bundle.group_info.is_some());
    (group, bundle)
}

fn process_commit(
    client: &Client<OpenMlsRustCrypto>,
    group: &mut ApqMlsGroup,
    message: ApqMlsMessageOut,
) -> ApqProcessedMessage {
    group
        .process_message(
            &client.provider,
            into_protocol_message(message),
            compare_credentials,
        )
        .unwrap()
}

fn process_and_merge(
    client: &Client<OpenMlsRustCrypto>,
    group: &mut ApqMlsGroup,
    message: ApqMlsMessageOut,
) {
    let staged_commit = process_commit(client, group, message)
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

#[test]
fn external_join_roundtrip() {
    for mode in TEST_MODES {
        let alice = new_client("Alice", mode);
        let bob = new_client("Bob", mode);
        let mut alice_group = create_group(&alice, mode);

        let (group_info, ratchet_tree) = export_join_info(&alice, &alice_group);
        let (mut bob_group, bundle) = external_join(&bob, group_info, ratchet_tree);

        process_and_merge(&alice, &mut alice_group, bundle.commit);
        assert_groups_eq(&mut alice_group, &mut bob_group);

        // Bidirectional application messages, so a one-sided PSK bug can't
        // pass.
        let message = send_t_message(&alice, &mut alice_group, b"hello bob");
        assert_eq!(
            receive_t_message(&bob, &mut bob_group, message),
            b"hello bob"
        );
        let message = send_t_message(&bob, &mut bob_group, b"hello alice");
        assert_eq!(
            receive_t_message(&alice, &mut alice_group, message),
            b"hello alice"
        );
    }
}

#[test]
fn resync() {
    for mode in TEST_MODES {
        let alice = new_client("Alice", mode);
        let bob = new_client("Bob", mode);
        let mut alice_group = create_group(&alice, mode);
        let old_bob_group = add_members(&alice, &mut alice_group, &[&bob], mode)
            .pop()
            .unwrap();
        let old_bob_leaf = old_bob_group.t_group.own_leaf_index();

        // Bob rejoins via an external commit.
        let (group_info, ratchet_tree) = export_join_info(&alice, &alice_group);
        let (mut new_bob_group, bundle) = external_join(&bob, group_info, ratchet_tree);

        // The commit removes Bob's old leaf in both groups.
        let staged_commit = process_commit(&alice, &mut alice_group, bundle.commit)
            .into_staged_commit()
            .unwrap();
        assert!(
            staged_commit
                .t_staged_commit
                .remove_proposals()
                .any(|p| p.remove_proposal().removed() == old_bob_leaf)
        );
        assert!(
            staged_commit
                .pq_staged_commit
                .remove_proposals()
                .any(|p| p.remove_proposal().removed() == old_bob_leaf)
        );
        alice_group
            .merge_staged_commit(&alice.provider, staged_commit)
            .unwrap();

        assert_groups_eq(&mut alice_group, &mut new_bob_group);
        assert_eq!(alice_group.t_group.members().count(), 2);
        assert_eq!(alice_group.pq_group().members().count(), 2);

        // T/PQ PSK material stays aligned: a follow-up full commit from Alice processes cleanly at
        // Bob.
        let bundle = alice_group
            .commit_builder()
            .force_self_update(true)
            .finalize(&alice.provider, &alice.signer, |_| true, |_| true)
            .unwrap();
        alice_group.merge_pending_commit(&alice.provider).unwrap();
        process_and_merge(&bob, &mut new_bob_group, bundle.commit);
        assert_groups_eq(&mut alice_group, &mut new_bob_group);
    }
}

#[test]
fn ds_accepts_external_commit() {
    for mode in TEST_MODES {
        let alice = new_client("Alice", mode);
        let bob = new_client("Bob", mode);
        let alice_group = create_group(&alice, mode);

        // The DS builds its public view of both groups from the group infos and ratchet trees.
        let ds_provider = OpenMlsRustCrypto::default();
        let (t_message, pq_message) = alice_group
            .export_group_info(alice.provider.crypto(), &alice.signer, false)
            .unwrap()
            .split();
        let MlsMessageBodyIn::GroupInfo(t_group_info) = roundtrip(t_message).extract() else {
            panic!("expected a group info");
        };
        let MlsMessageBodyIn::GroupInfo(pq_group_info) = roundtrip(pq_message).extract() else {
            panic!("expected a group info");
        };
        let (t_ratchet_tree, pq_ratchet_tree) =
            ApqRatchetTreeIn::from(alice_group.export_ratchet_tree()).split();
        let (t_public_group, _) = PublicGroup::from_external(
            ds_provider.crypto(),
            ds_provider.storage(),
            t_ratchet_tree,
            t_group_info,
            ProposalStore::default(),
        )
        .unwrap();
        let (pq_public_group, _) = PublicGroup::from_external(
            ds_provider.crypto(),
            ds_provider.storage(),
            pq_ratchet_tree,
            pq_group_info,
            ProposalStore::default(),
        )
        .unwrap();
        let mut ds_group = ApqPublicGroup::from_groups(t_public_group, pq_public_group);

        let (group_info, ratchet_tree) = export_join_info(&alice, &alice_group);
        let (_bob_group, bundle) = external_join(&bob, group_info, ratchet_tree);

        // Direct regression for the DS-side checks: both epochs bumped, both commits carry ApqInfo,
        // dictionary update applied.
        let staged_commit = ds_group
            .as_mut()
            .process_message(
                ds_provider.crypto(),
                into_protocol_message(bundle.commit),
                compare_credentials,
            )
            .unwrap()
            .into_staged_commit()
            .unwrap();
        ds_group
            .as_mut()
            .merge_staged_commit(ds_provider.storage(), staged_commit)
            .unwrap();
    }
}

#[test]
fn missing_apq_info() {
    let mode = PqtMode::ConfAndAuth;
    let alice = new_client("Alice", mode);
    let bob = new_client("Bob", mode);

    // Two plain MLS groups without the ApqInfo component in their group context. The builder must
    // fail before building any group state.
    let create_config = MlsGroupCreateConfig::default();
    let plain_group = |group_id: &[u8]| {
        MlsGroup::new_with_group_id(
            &alice.provider,
            alice.signer.t_signer(),
            &create_config,
            GroupId::from_slice(group_id),
            alice.credential_with_key.t_credential.clone(),
        )
        .unwrap()
    };
    let t_group = plain_group(b"plain_t_group");
    let pq_group = plain_group(b"plain_pq_group");

    let export_group_info = |group: &MlsGroup| {
        group
            .export_group_info(alice.provider.crypto(), alice.signer.t_signer(), false)
            .unwrap()
    };
    let mut group_info_bytes = export_group_info(&t_group)
        .tls_serialize_detached()
        .unwrap();
    group_info_bytes.extend(
        export_group_info(&pq_group)
            .tls_serialize_detached()
            .unwrap(),
    );
    let group_info = ApqMlsMessageIn::tls_deserialize_exact(group_info_bytes)
        .unwrap()
        .into_verifiable_group_info()
        .unwrap();
    let ratchet_tree = ApqRatchetTreeIn::new(
        t_group.export_ratchet_tree().into(),
        pq_group.export_ratchet_tree().into(),
    );

    let result = ApqMlsGroup::external_commit_builder()
        .with_ratchet_tree(ratchet_tree)
        .with_config(join_config())
        .build(
            &bob.provider,
            &bob.signer,
            bob.credential_with_key.clone(),
            group_info,
        );
    assert!(matches!(
        result,
        Err(ApqExternalCommitBuilderError::MissingApqInfo)
    ));
}

#[test]
fn psk_continuity() {
    for mode in TEST_MODES {
        let alice = new_client("Alice", mode);
        let bob = new_client("Bob", mode);
        let mut alice_group = create_group(&alice, mode);

        let (group_info, ratchet_tree) = export_join_info(&alice, &alice_group);
        let (mut bob_group, bundle) = external_join(&bob, group_info, ratchet_tree);
        process_and_merge(&alice, &mut alice_group, bundle.commit);

        // An existing member runs a normal APQ membership change and Bob processes it: the
        // external-joined state is a valid basis for later combiner operations.
        let carol = new_client("Carol", mode);
        let bundle = alice_group
            .commit_builder()
            .propose_adds([carol.generate_key_package(mode.default_ciphersuite())])
            .finalize(&alice.provider, &alice.signer, |_| true, |_| true)
            .unwrap();
        alice_group.merge_pending_commit(&alice.provider).unwrap();
        process_and_merge(&bob, &mut bob_group, bundle.commit);
        assert_groups_eq(&mut alice_group, &mut bob_group);

        // And the other way around: Bob commits, Alice processes.
        let bundle = bob_group
            .commit_builder()
            .force_self_update(true)
            .finalize(&bob.provider, &bob.signer, |_| true, |_| true)
            .unwrap();
        bob_group.merge_pending_commit(&bob.provider).unwrap();
        process_and_merge(&alice, &mut alice_group, bundle.commit);
        assert_groups_eq(&mut alice_group, &mut bob_group);
    }
}

#[test]
fn parked_self_remove_via_with_proposals() {
    for mode in TEST_MODES {
        let alice = new_client("Alice", mode);
        let bob = new_client("Bob", mode);
        let carol = new_client("Carol", mode);
        let mut alice_group = create_group(&alice, mode);
        let mut groups = add_members(&alice, &mut alice_group, &[&bob, &carol], mode);
        let carol_group = groups.pop().unwrap();

        // Carol parks a self-remove proposal in both groups.
        let (mut carol_t_group, mut carol_pq_group) = carol_group.into_groups();
        let t_proposal = carol_t_group
            .leave_group_via_self_remove(&carol.provider, carol.signer.t_signer())
            .unwrap();
        let pq_proposal = carol_pq_group
            .leave_group_via_self_remove(&carol.provider, carol.signer.pq_signer())
            .unwrap();

        // Alice processes and stores the parked proposals per leg, so she can resolve them by
        // reference from Bob's external commit.
        let store_parked_proposal = |group: &mut MlsGroup, message: MlsMessageOut| {
            let protocol_message = roundtrip(message).try_into_protocol_message().unwrap();
            let processed_message = group
                .process_message(&alice.provider, protocol_message)
                .unwrap();
            let ProcessedMessageContent::ProposalMessage(proposal) =
                processed_message.into_content()
            else {
                panic!("expected a proposal");
            };
            group
                .store_pending_proposal(alice.provider.storage(), *proposal)
                .unwrap();
        };
        let (mut alice_t_group, mut alice_pq_group) = alice_group.into_groups();
        store_parked_proposal(&mut alice_t_group, t_proposal.clone());
        store_parked_proposal(&mut alice_pq_group, pq_proposal.clone());
        let mut alice_group = ApqMlsGroup::from_groups(alice_t_group, alice_pq_group);

        // Bob resyncs, feeding the parked proposals through the builder.
        let parked_proposal = ApqProposalIn::new(
            public_message_in(t_proposal),
            public_message_in(pq_proposal),
        );
        // Bob's new leaf must also advertise SelfRemove support, since the commit covers a
        // SelfRemove proposal.
        let leaf_node_parameters = LeafNodeParameters::builder()
            .with_capabilities(test_capabilities())
            .build();
        let (group_info, ratchet_tree) = export_join_info(&alice, &alice_group);
        let (mut new_bob_group, bundle) = ApqMlsGroup::external_commit_builder()
            .with_ratchet_tree(ratchet_tree)
            .with_config(join_config())
            .with_proposals(vec![parked_proposal])
            .leaf_node_parameters(leaf_node_parameters.clone(), leaf_node_parameters)
            .build(
                &bob.provider,
                &bob.signer,
                bob.credential_with_key.clone(),
                group_info,
            )
            .unwrap();

        process_and_merge(&alice, &mut alice_group, bundle.commit);
        assert_groups_eq(&mut alice_group, &mut new_bob_group);

        // Carol is gone from *both* groups (catches T/PQ half mis-routing); Bob's old leaf was
        // replaced, so two members remain.
        let carol_t_credential = &carol.credential_with_key.t_credential.credential;
        let carol_pq_credential = &carol.credential_with_key.pq_credential.credential;
        for group in [&alice_group, &new_bob_group] {
            assert_eq!(group.t_group.members().count(), 2);
            assert_eq!(group.pq_group().members().count(), 2);
            assert!(
                group
                    .t_group
                    .members()
                    .all(|member| member.credential != *carol_t_credential)
            );
            assert!(
                group
                    .pq_group()
                    .members()
                    .all(|member| member.credential != *carol_pq_credential)
            );
        }
    }
}

#[test]
fn fresh_join_vs_resync_leaf_placement() {
    for mode in TEST_MODES {
        let alice = new_client("Alice", mode);
        let bob = new_client("Bob", mode);
        let mut alice_group = create_group(&alice, mode);

        // Fresh join: Bob lands at the leftmost blank leaf, no removes.
        let (group_info, ratchet_tree) = export_join_info(&alice, &alice_group);
        let (bob_group, bundle) = external_join(&bob, group_info, ratchet_tree);
        assert_eq!(bob_group.t_group.own_leaf_index(), LeafNodeIndex::new(1));
        assert_eq!(bob_group.pq_group().own_leaf_index(), LeafNodeIndex::new(1));

        let staged_commit = process_commit(&alice, &mut alice_group, bundle.commit)
            .into_staged_commit()
            .unwrap();
        assert_eq!(staged_commit.t_staged_commit.remove_proposals().count(), 0);
        assert_eq!(staged_commit.pq_staged_commit.remove_proposals().count(), 0);
        alice_group
            .merge_staged_commit(&alice.provider, staged_commit)
            .unwrap();

        // Resync: the old leaf is removed and Bob is re-added in its place.
        let (group_info, ratchet_tree) = export_join_info(&alice, &alice_group);
        let (mut new_bob_group, bundle) = external_join(&bob, group_info, ratchet_tree);

        let staged_commit = process_commit(&alice, &mut alice_group, bundle.commit)
            .into_staged_commit()
            .unwrap();
        let removed = |staged: &openmls::group::StagedCommit| {
            staged
                .remove_proposals()
                .map(|p| p.remove_proposal().removed())
                .collect::<Vec<_>>()
        };
        assert_eq!(
            removed(&staged_commit.t_staged_commit),
            vec![LeafNodeIndex::new(1)]
        );
        assert_eq!(
            removed(&staged_commit.pq_staged_commit),
            vec![LeafNodeIndex::new(1)]
        );
        alice_group
            .merge_staged_commit(&alice.provider, staged_commit)
            .unwrap();

        assert_eq!(
            new_bob_group.t_group.own_leaf_index(),
            LeafNodeIndex::new(1)
        );
        assert_eq!(
            new_bob_group.pq_group().own_leaf_index(),
            LeafNodeIndex::new(1)
        );
        assert_eq!(alice_group.t_group.members().count(), 2);
        assert_groups_eq(&mut alice_group, &mut new_bob_group);
    }
}

#[test]
fn no_group_info() {
    let mode = PqtMode::ConfAndAuth;
    let alice = new_client("Alice", mode);
    let bob = new_client("Bob", mode);
    let alice_group = create_group(&alice, mode);

    let (group_info, ratchet_tree) = export_join_info(&alice, &alice_group);
    let (_bob_group, bundle) = ApqMlsGroup::external_commit_builder()
        .with_ratchet_tree(ratchet_tree)
        .with_config(join_config())
        .create_group_info(false)
        .build(
            &bob.provider,
            &bob.signer,
            bob.credential_with_key.clone(),
            group_info,
        )
        .unwrap();
    assert!(bundle.group_info.is_none());
}

#[test]
fn aad_roundtrip() {
    let mode = PqtMode::ConfAndAuth;
    let alice = new_client("Alice", mode);
    let bob = new_client("Bob", mode);
    let mut alice_group = create_group(&alice, mode);

    let (group_info, ratchet_tree) = export_join_info(&alice, &alice_group);
    let (_bob_group, bundle) = ApqMlsGroup::external_commit_builder()
        .with_ratchet_tree(ratchet_tree)
        .with_config(join_config())
        .with_aad(b"apq test aad".to_vec())
        .build(
            &bob.provider,
            &bob.signer,
            bob.credential_with_key.clone(),
            group_info,
        )
        .unwrap();

    let processed_message = process_commit(&alice, &mut alice_group, bundle.commit);
    assert_eq!(processed_message.t_message.aad(), b"apq test aad");
    assert_eq!(processed_message.pq_message.aad(), b"apq test aad");
}

#[test]
fn t_leg_failure_does_not_leave_orphaned_pq_group() {
    let mode = PqtMode::ConfAndAuth;
    let alice = new_client("Alice", mode);
    let bob = new_client("Bob", mode);
    let alice_group = create_group(&alice, mode);

    let (group_info, ratchet_tree) = export_join_info(&alice, &alice_group);
    let pq_group_id = GroupId::from_slice(b"test_pq_group");

    // References a PSK that was never stored. The PQ leg has nothing to do with this proposal and
    // succeeds; the T leg's `load_psks` then fails, but only after the PQ leg has already been
    // merged and persisted.
    let bogus_psk = PreSharedKeyProposal::new(PreSharedKeyId::external(
        b"unregistered".to_vec(),
        b"nonce".to_vec(),
    ));

    let result = ApqMlsGroup::external_commit_builder()
        .with_ratchet_tree(ratchet_tree)
        .with_config(join_config())
        .add_t_psk_proposal(bogus_psk)
        .build(
            &bob.provider,
            &bob.signer,
            bob.credential_with_key.clone(),
            group_info,
        );
    assert!(result.is_err());

    // The already-merged PQ leg must be rolled back: no dangling group state.
    assert!(
        MlsGroup::load(bob.provider.storage(), &pq_group_id)
            .unwrap()
            .is_none()
    );
}
