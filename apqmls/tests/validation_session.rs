// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Tests for the checks that only apply to a path constructing a paired APQ
//! session: matching epochs and consistent membership.

use apqmls::{
    ApqCiphersuite, ApqMlsGroup,
    authentication::ApqSigner,
    extension::PqtMode,
    validation::{
        ApqValidationError, Session, validate_apq_session, validate_apq_session_at_construction,
        validate_membership,
    },
};
use openmls::{
    group::{GroupId, MlsGroup, PURE_PLAINTEXT_WIRE_FORMAT_POLICY},
    prelude::{Ciphersuite, Credential, LeafNodeIndex, OpenMlsProvider},
};
use openmls_rust_crypto::OpenMlsRustCrypto;
use openmls_traits::signatures::Signer;

use crate::utils::client::Client;

mod utils;

const MODE: PqtMode = PqtMode::ConfOnly;

/// A ciphersuite pair whose two legs share a signature algorithm, so that a
/// member can use one signing key in both.
const SHARED_SIGNATURE_CIPHERSUITE: ApqCiphersuite = ApqCiphersuite::new(
    Ciphersuite::MLS_128_DHKEMX25519_AES128GCM_SHA256_Ed25519,
    Ciphersuite::MLS_128_MLKEM768_AES256GCM_SHA384_Ed25519,
);

/// The two legs of a real deployment carry different credentials, so tests use
/// the same "anything goes" predicate the client does.
fn any_credential(_: &Credential, _: &Credential) -> bool {
    true
}

fn new_client(identity: &str) -> Client<OpenMlsRustCrypto> {
    Client::new(
        identity,
        MODE.default_ciphersuite().into(),
        OpenMlsRustCrypto::default(),
    )
}

fn create_group(client: &Client<OpenMlsRustCrypto>) -> ApqMlsGroup {
    ApqMlsGroup::builder()
        .with_group_ids(
            GroupId::random(client.provider.rand()),
            GroupId::random(client.provider.rand()),
        )
        .with_wire_format_policy(PURE_PLAINTEXT_WIRE_FORMAT_POLICY)
        .set_mode(MODE)
        .build(
            &client.provider,
            &client.signer,
            client.credential_with_key.clone(),
        )
        .unwrap()
}

fn shared_signature_client(identity: &str) -> Client<OpenMlsRustCrypto> {
    Client::new(
        identity,
        SHARED_SIGNATURE_CIPHERSUITE.into(),
        OpenMlsRustCrypto::default(),
    )
}

fn shared_signature_group(client: &Client<OpenMlsRustCrypto>) -> ApqMlsGroup {
    ApqMlsGroup::builder()
        .with_group_ids(
            GroupId::random(client.provider.rand()),
            GroupId::random(client.provider.rand()),
        )
        .with_wire_format_policy(PURE_PLAINTEXT_WIRE_FORMAT_POLICY)
        .with_ciphersuite(SHARED_SIGNATURE_CIPHERSUITE)
        .set_mode(MODE)
        .build(
            &client.provider,
            &client.signer,
            client.credential_with_key.clone(),
        )
        .unwrap()
}

/// Which leg of the session a single-leg operation applies to.
#[derive(Clone, Copy)]
enum Leg {
    T,
    Pq,
}

/// Commits in one leg only, bypassing the APQ commit builder so that the two
/// legs can be driven out of step.
fn commit_in_leg(
    client: &Client<OpenMlsRustCrypto>,
    group: ApqMlsGroup,
    leg: Leg,
    stage: impl FnOnce(
        openmls::group::CommitBuilder<'_, openmls::group::Initial>,
    ) -> openmls::group::CommitBuilder<'_, openmls::group::Initial>,
) -> ApqMlsGroup {
    let (mut t_group, mut pq_group) = group.into_groups();
    match leg {
        Leg::T => commit_and_merge(client, &mut t_group, client.signer.t_signer(), stage),
        Leg::Pq => commit_and_merge(client, &mut pq_group, client.signer.pq_signer(), stage),
    }
    ApqMlsGroup::from_groups(t_group, pq_group)
}

fn commit_and_merge(
    client: &Client<OpenMlsRustCrypto>,
    group: &mut MlsGroup,
    signer: &impl Signer,
    stage: impl FnOnce(
        openmls::group::CommitBuilder<'_, openmls::group::Initial>,
    ) -> openmls::group::CommitBuilder<'_, openmls::group::Initial>,
) {
    stage(group.commit_builder().force_self_update(true))
        .load_psks(client.provider.storage())
        .unwrap()
        .build(
            client.provider.rand(),
            client.provider.crypto(),
            signer,
            |_| true,
        )
        .unwrap()
        .stage_commit(&client.provider)
        .unwrap();
    group.merge_pending_commit(&client.provider).unwrap();
}

/// Adds `joiners` to both legs through the APQ commit builder.
fn add_members(
    adder: &Client<OpenMlsRustCrypto>,
    group: &mut ApqMlsGroup,
    joiners: &[&Client<OpenMlsRustCrypto>],
) {
    for joiner in joiners {
        let key_package = joiner.generate_key_package(MODE.default_ciphersuite());
        group
            .commit_builder()
            .propose_adds([key_package])
            .finalize(&adder.provider, &adder.signer, |_| true, |_| true)
            .unwrap();
        group.merge_pending_commit(&adder.provider).unwrap();
    }
}

#[test]
fn a_freshly_built_group_satisfies_the_construction_checks() {
    let alice = new_client("Alice");
    let group = create_group(&alice);

    let apq_info =
        validate_apq_session_at_construction(&group.t_group, group.pq_group(), any_credential)
            .unwrap();
    assert_eq!(apq_info.t_epoch.as_u64(), 0);
    assert_eq!(apq_info.pq_epoch.as_u64(), 0);
}

#[test]
fn a_stale_t_epoch_is_only_rejected_at_construction() {
    let alice = new_client("Alice");
    let group = commit_in_leg(&alice, create_group(&alice), Leg::T, |builder| builder);

    // Mid-session this is fine.
    validate_apq_session(&group.t_group, group.pq_group()).unwrap();

    // On a construction path it is not.
    assert!(matches!(
        validate_apq_session_at_construction(&group.t_group, group.pq_group(), any_credential),
        Err(ApqValidationError::EpochMismatch(Session::T))
    ));
}

#[test]
fn a_stale_pq_epoch_is_rejected_at_construction() {
    let alice = new_client("Alice");
    let group = commit_in_leg(&alice, create_group(&alice), Leg::Pq, |builder| builder);

    assert!(matches!(
        validate_apq_session_at_construction(&group.t_group, group.pq_group(), any_credential),
        Err(ApqValidationError::EpochMismatch(Session::Pq))
    ));
}

#[test]
fn matching_membership_is_accepted() {
    let alice = new_client("Alice");
    let bob = new_client("Bob");
    let mut group = create_group(&alice);
    add_members(&alice, &mut group, &[&bob]);

    validate_membership(&group.t_group, group.pq_group(), any_credential).unwrap();
}

#[test]
fn divergent_membership_is_rejected() {
    let alice = new_client("Alice");
    let bob = new_client("Bob");
    let group = create_group(&alice);

    // Add Bob to the T leg only, so the two legs disagree on membership.
    let key_package = bob.generate_key_package(MODE.default_ciphersuite());
    let t_key_package = key_package.t_key_package().clone();
    let group = commit_in_leg(&alice, group, Leg::T, |builder| {
        builder.propose_adds([t_key_package])
    });

    assert!(matches!(
        validate_membership(&group.t_group, group.pq_group(), any_credential),
        Err(ApqValidationError::MemberCountMismatch {
            t_members: 2,
            pq_members: 1,
        })
    ));
}

#[test]
fn credentials_are_compared_through_the_predicate() {
    let alice = new_client("Alice");
    let group = create_group(&alice);

    // The harness gives both legs the same credential, so equality holds here.
    validate_membership(&group.t_group, group.pq_group(), |a, b| a == b).unwrap();

    // A predicate that never matches must reject the very first leaf.
    assert!(matches!(
        validate_membership(&group.t_group, group.pq_group(), |_, _| false),
        Err(ApqValidationError::CredentialMismatch(0))
    ));
}

#[test]
fn the_predicate_decides_what_counts_as_the_same_member() {
    let alice = shared_signature_client("Alice");
    let impostor = shared_signature_client("Alice");
    let alice_group = shared_signature_group(&alice);
    let impostor_group = shared_signature_group(&impostor);

    // Splicing the impostor's PQ leg onto Alice's T leg keeps the leaf count and
    // the occupancy, so the structural checks pass either way.
    validate_membership(
        &alice_group.t_group,
        impostor_group.pq_group(),
        any_credential,
    )
    .unwrap();

    assert!(matches!(
        validate_membership(&alice_group.t_group, impostor_group.pq_group(), |_, _| {
            false
        }),
        Err(ApqValidationError::CredentialMismatch(0))
    ));
}

#[test]
fn a_blank_leaf_in_one_leg_is_rejected() {
    let alice = new_client("Alice");
    let bob = new_client("Bob");
    let charlie = new_client("Charlie");
    let mut group = create_group(&alice);
    add_members(&alice, &mut group, &[&bob, &charlie]);

    // Remove Bob from the PQ leg and Charlie from the T leg. Both legs then
    // have two members, but they sit at different leaves.
    let group = commit_in_leg(&alice, group, Leg::Pq, |builder| {
        builder.propose_removals([LeafNodeIndex::new(1)])
    });
    let group = commit_in_leg(&alice, group, Leg::T, |builder| {
        builder.propose_removals([LeafNodeIndex::new(2)])
    });

    assert!(matches!(
        validate_membership(&group.t_group, group.pq_group(), any_credential),
        Err(ApqValidationError::LeafOccupancyMismatch(_))
    ));
}
