// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use apqmls::{
    ApqMlsGroup, authentication::ApqSigner, extension::PqtMode, messages::ApqMlsMessageIn,
};
use openmls::{
    group::{GroupId, MlsGroup, MlsGroupJoinConfig},
    prelude::{
        Capabilities, Credential, Extension, ExtensionType, Extensions, LeafNodeIndex,
        LeafNodeParameters, MlsMessageIn, OpenMlsProvider, ProcessedMessageContent,
        UnknownExtension,
    },
};
use openmls_rust_crypto::OpenMlsRustCrypto;
use tls_codec::{Deserialize as _, Serialize};

use crate::utils::{assert_groups_eq, client::Client};

mod utils;

fn compare_credentials(cred1: &Credential, cred2: &Credential) -> bool {
    cred1 == cred2
}

fn join_group_helper(mode: PqtMode) -> JoinedGroup {
    let ciphersuite = mode.default_ciphersuite();
    let alice = Client::new("Alice", ciphersuite.into(), OpenMlsRustCrypto::default());
    let bob = Client::new("Bob", ciphersuite.into(), OpenMlsRustCrypto::default());

    // Create a new ApqMlsGroup for Alice
    let mut alice_group = ApqMlsGroup::builder()
        .with_group_ids(
            GroupId::random(alice.provider.rand()),
            GroupId::from_slice(b"test_pq_group"),
        )
        .set_mode(mode)
        .build(
            &alice.provider,
            &alice.signer,
            alice.credential_with_key.clone(),
        )
        .unwrap();

    // Generate KeyPackages for Bob
    let key_package = bob.generate_key_package(ciphersuite);

    // Alice proposes to add Bob's KeyPackages
    let commit_bundle = alice_group
        .commit_builder()
        .propose_adds([key_package])
        .finalize(&alice.provider, &alice.signer, |_| true, |_| true)
        .unwrap();

    alice_group.merge_pending_commit(&alice.provider).unwrap();

    let ratchet_tree = alice_group.export_ratchet_tree();

    // Bob joins Alice's group
    let welcome = commit_bundle.into_welcome().unwrap();
    let mut bob_group = ApqMlsGroup::new_from_welcome(
        &bob.provider,
        &MlsGroupJoinConfig::default(),
        welcome,
        Some(ratchet_tree.into()),
        compare_credentials,
    )
    .unwrap();

    assert_groups_eq(&mut alice_group, &mut bob_group);

    JoinedGroup {
        alice,
        bob,
        alice_group,
        bob_group,
    }
}

fn update_group_helper(group: JoinedGroup) -> JoinedGroup {
    let JoinedGroup {
        mut alice_group,
        mut bob_group,
        alice,
        bob,
    } = group;

    // Alice does an update
    let alice_commit_bundle = alice_group
        .commit_builder()
        .force_self_update(true)
        .finalize(&alice.provider, &alice.signer, |_| true, |_| true)
        .unwrap();
    alice_group.merge_pending_commit(&alice.provider).unwrap();

    let message_in = ApqMlsMessageIn::try_from(alice_commit_bundle.commit).unwrap();
    let protocol_message = message_in.into_protocol_message().unwrap();

    // Bob processes Alice's update
    let processed_message = bob_group
        .process_message(&bob.provider, protocol_message, compare_credentials)
        .unwrap();
    bob_group
        .merge_staged_commit(
            &bob.provider,
            processed_message.into_staged_commit().unwrap(),
        )
        .unwrap();

    assert_groups_eq(&mut alice_group, &mut bob_group);

    JoinedGroup {
        alice,
        bob,
        alice_group,
        bob_group,
    }
}

struct JoinedGroup {
    alice: Client<OpenMlsRustCrypto>,
    bob: Client<OpenMlsRustCrypto>,
    alice_group: ApqMlsGroup,
    bob_group: ApqMlsGroup,
}

const TEST_MODE: [PqtMode; 2] = [PqtMode::ConfAndAuth, PqtMode::ConfOnly];

const MARKER_EXTENSION_TYPE: u16 = 0xff00;

// The commit builder adds the APQMLS-specific support, the marker extension must be advertised
// here.
fn marker_capabilities() -> Capabilities {
    Capabilities::new(
        None,
        None,
        Some(&[ExtensionType::Unknown(MARKER_EXTENSION_TYPE)]),
        None,
        None,
    )
}

fn marker_parameters(data: &[u8]) -> LeafNodeParameters {
    let extension = Extension::Unknown(MARKER_EXTENSION_TYPE, UnknownExtension(data.to_vec()));
    LeafNodeParameters::builder()
        .with_capabilities(marker_capabilities())
        .with_extensions(Extensions::from_vec(vec![extension]).unwrap())
        .build()
}

fn marker(group: &MlsGroup) -> Option<&[u8]> {
    group
        .own_leaf_node()
        .unwrap()
        .extensions()
        .unknown(MARKER_EXTENSION_TYPE)
        .map(|extension| extension.0.as_slice())
}

fn self_update_with_leaf_node_parameters(
    alice: &Client<OpenMlsRustCrypto>,
    bob: &Client<OpenMlsRustCrypto>,
    alice_group: &mut ApqMlsGroup,
    bob_group: &mut ApqMlsGroup,
    t_params: LeafNodeParameters,
    pq_params: LeafNodeParameters,
) {
    let alice_commit_bundle = alice_group
        .commit_builder()
        .force_self_update(true)
        .leaf_node_parameters(t_params, pq_params)
        .finalize(&alice.provider, &alice.signer, |_| true, |_| true)
        .unwrap();
    alice_group.merge_pending_commit(&alice.provider).unwrap();

    // Bob processes Alice's update
    let message_in = ApqMlsMessageIn::try_from(alice_commit_bundle.commit).unwrap();
    let protocol_message = message_in.into_protocol_message().unwrap();
    let processed_message = bob_group
        .process_message(&bob.provider, protocol_message, compare_credentials)
        .unwrap();
    bob_group
        .merge_staged_commit(
            &bob.provider,
            processed_message.into_staged_commit().unwrap(),
        )
        .unwrap();

    assert_groups_eq(alice_group, bob_group);
}

#[test]
fn join_group() {
    for mode in TEST_MODE {
        join_group_helper(mode);
    }
}

#[test]
fn update_group() {
    for ciphersuite in TEST_MODE {
        let joined_group = join_group_helper(ciphersuite);
        update_group_helper(joined_group);
    }
}

/// The T and PQ leaf node parameters must each be applied to their own group,
/// not swapped or overwritten by one another.
#[test]
fn update_with_leaf_node_parameters() {
    for mode in TEST_MODE {
        let JoinedGroup {
            alice,
            bob,
            mut alice_group,
            mut bob_group,
        } = join_group_helper(mode);

        // Alice does an update with distinguishable leaf node parameters
        self_update_with_leaf_node_parameters(
            &alice,
            &bob,
            &mut alice_group,
            &mut bob_group,
            marker_parameters(b"t"),
            marker_parameters(b"pq"),
        );

        // Each group's new leaf carries its own parameters
        assert_eq!(marker(&alice_group.t_group), Some(b"t".as_slice()));
        assert_eq!(marker(alice_group.pq_group()), Some(b"pq".as_slice()));
    }
}

// LeafNodeParameters with extensions None means "keep the current leaf
// extensions" in OpenMLS. The commit builder must not turn that into a
// replacement of the leaf extensions.
#[test]
fn update_without_leaf_extensions_keeps_leaf_extensions() {
    for mode in TEST_MODE {
        let JoinedGroup {
            alice,
            bob,
            mut alice_group,
            mut bob_group,
        } = join_group_helper(mode);

        // Alice sets the marker extension on both leaves
        self_update_with_leaf_node_parameters(
            &alice,
            &bob,
            &mut alice_group,
            &mut bob_group,
            marker_parameters(b"t"),
            marker_parameters(b"pq"),
        );

        // Alice self-updates again, only touching capabilities this time
        let capabilities_only_params = LeafNodeParameters::builder()
            .with_capabilities(marker_capabilities())
            .build();
        self_update_with_leaf_node_parameters(
            &alice,
            &bob,
            &mut alice_group,
            &mut bob_group,
            capabilities_only_params.clone(),
            capabilities_only_params,
        );

        // The marker extension set by the previous update must survive
        assert_eq!(marker(&alice_group.t_group), Some(b"t".as_slice()));
        assert_eq!(marker(alice_group.pq_group()), Some(b"pq".as_slice()));
    }
}

#[test]
fn remove_from_group() {
    for ciphersuite in TEST_MODE {
        let JoinedGroup {
            mut bob_group, bob, ..
        } = join_group_helper(ciphersuite);

        // Bob removes Alice
        let _bob_commit_bundle = bob_group
            .commit_builder()
            .propose_removals(std::iter::once(LeafNodeIndex::new(0)))
            .finalize(&bob.provider, &bob.signer, |_| true, |_| true)
            .unwrap();
        bob_group.merge_pending_commit(&bob.provider).unwrap();
    }
}

#[test]
fn t_only_update() {
    for ciphersuite in TEST_MODE {
        let JoinedGroup {
            alice,
            bob,
            mut alice_group,
            mut bob_group,
        } = join_group_helper(ciphersuite);

        // Alice does a T-only update
        let alice_commit_bundle = alice_group
            .t_group
            .commit_builder()
            .force_self_update(true)
            .load_psks(alice.provider.storage())
            .unwrap()
            .build(
                alice.provider.rand(),
                alice.provider.crypto(),
                alice.signer.t_signer(),
                |_| true,
            )
            .unwrap()
            .stage_commit(&alice.provider)
            .unwrap();

        alice_group
            .t_group
            .merge_pending_commit(&alice.provider)
            .unwrap();

        // Bob processes Alice's T-only update
        let commit = MlsMessageIn::tls_deserialize_exact(
            alice_commit_bundle
                .into_commit()
                .tls_serialize_detached()
                .unwrap(),
        )
        .unwrap()
        .try_into_protocol_message()
        .unwrap();

        let processed_message = bob_group
            .t_group
            .process_message(&bob.provider, commit)
            .unwrap();
        let ProcessedMessageContent::StagedCommitMessage(processed_message) =
            processed_message.into_content()
        else {
            panic!("Expected a staged commit message");
        };

        bob_group
            .t_group
            .merge_staged_commit(&bob.provider, *processed_message)
            .unwrap();

        assert_groups_eq(&mut alice_group, &mut bob_group);

        // Do an APQMLS update to make sure everything still works
        let joined_group = JoinedGroup {
            alice,
            bob,
            alice_group,
            bob_group,
        };

        update_group_helper(joined_group);
    }
}
