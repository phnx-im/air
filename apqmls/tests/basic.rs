// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use apqmls::{
    ApqMlsGroup, authentication::ApqSigner, extension::PqtMode, messages::ApqMlsMessageIn,
};
use openmls::{
    group::{GroupId, MlsGroupJoinConfig},
    prelude::{Credential, LeafNodeIndex, MlsMessageIn, OpenMlsProvider, ProcessedMessageContent},
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
