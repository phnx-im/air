// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use apqmls::{
    ApqMlsGroup,
    extension::{APQMLS_COMPONENT_ID, PqtMode},
    messages::{ApqMlsMessageIn, ApqRatchetTreeIn, ApqWelcome},
};
use openmls::{
    component::{ComponentId, ComponentType},
    group::{GroupContext, GroupId, MlsGroupJoinConfig},
    prelude::{
        AppDataDictionary, AppDataDictionaryExtension, Extension, Extensions, OpenMlsProvider,
    },
};
use openmls_rust_crypto::OpenMlsRustCrypto;
use tls_codec::Deserialize as _;

use crate::utils::{assert_groups_eq, client::Client};

mod utils;

const MODE: PqtMode = PqtMode::ConfAndAuth;

fn build_group(
    t_extensions: Option<Extensions<GroupContext>>,
    pq_extensions: Option<Extensions<GroupContext>>,
) -> (Client<OpenMlsRustCrypto>, ApqMlsGroup) {
    let ciphersuite = MODE.default_ciphersuite();
    let alice = Client::new("Alice", ciphersuite.into(), OpenMlsRustCrypto::default());

    let mut builder = ApqMlsGroup::builder()
        .with_group_ids(
            GroupId::random(alice.provider.rand()),
            GroupId::from_slice(b"test_pq_group"),
        )
        .set_mode(MODE);
    if t_extensions.is_some() || pq_extensions.is_some() {
        builder = builder
            .with_group_context_extensions(
                t_extensions.unwrap_or_default(),
                pq_extensions.unwrap_or_default(),
            )
            .unwrap();
    }
    let group = builder
        .build(
            &alice.provider,
            &alice.signer,
            alice.credential_with_key.clone(),
        )
        .unwrap();
    (alice, group)
}

fn dictionary_with(component_id: ComponentId) -> Extensions<GroupContext> {
    let mut dictionary = AppDataDictionary::new();
    dictionary.insert(component_id, Vec::new());
    Extensions::from_vec(vec![Extension::AppDataDictionary(
        AppDataDictionaryExtension::new(dictionary),
    )])
    .unwrap()
}

fn context_dictionary(context: &GroupContext) -> &AppDataDictionary {
    context
        .extensions()
        .app_data_dictionary()
        .expect("group context has no app data dictionary")
        .dictionary()
}

fn app_components(dictionary: &AppDataDictionary) -> Vec<ComponentId> {
    let bytes = dictionary
        .get(&ComponentId::from(ComponentType::AppComponents))
        .expect("no AppComponents entry");
    Vec::tls_deserialize_exact(bytes).unwrap()
}

/// Default behavior (no caller dictionary): both group contexts advertise the APQMLS component and
/// carry the ApqInfo entry; SafeAAD is not required.
#[test]
fn default_dictionary() {
    let (_, group) = build_group(None, None);
    for context in [
        group.t_group.export_group_context(),
        group.pq_group().export_group_context(),
    ] {
        let dictionary = context_dictionary(context);
        assert!(app_components(dictionary).contains(&APQMLS_COMPONENT_ID));
        assert!(dictionary.contains(&APQMLS_COMPONENT_ID));
        assert!(!context.safe_aad_required());
    }
}

/// A caller-provided dictionary entry survives the build and is merged with the APQMLS entries
/// instead of being clobbered.
#[test]
fn caller_dictionary_is_merged() {
    let safe_aad = ComponentId::from(ComponentType::SafeAad);
    let (_, group) = build_group(
        Some(dictionary_with(safe_aad)),
        Some(dictionary_with(safe_aad)),
    );
    for context in [
        group.t_group.export_group_context(),
        group.pq_group().export_group_context(),
    ] {
        let dictionary = context_dictionary(context);
        // Caller entry survived ...
        assert!(context.safe_aad_required());
        // ... and the APQMLS entries are still there.
        assert!(app_components(dictionary).contains(&APQMLS_COMPONENT_ID));
        assert!(dictionary.contains(&APQMLS_COMPONENT_ID));
    }
}

/// A caller-provided AppComponents list is extended, not overwritten.
#[test]
fn caller_app_components_are_merged() {
    const CALLER_COMPONENT_ID: ComponentId = 0x9000;
    let mut dictionary = AppDataDictionary::new();
    dictionary.insert(
        ComponentId::from(ComponentType::AppComponents),
        tls_codec::Serialize::tls_serialize_detached(&[CALLER_COMPONENT_ID].as_slice()).unwrap(),
    );
    let extensions = Extensions::from_vec(vec![Extension::AppDataDictionary(
        AppDataDictionaryExtension::new(dictionary),
    )])
    .unwrap();

    let (_, group) = build_group(Some(extensions), None);

    let components = app_components(context_dictionary(group.t_group.export_group_context()));
    assert!(components.contains(&CALLER_COMPONENT_ID));
    assert!(components.contains(&APQMLS_COMPONENT_ID));
}

/// Extensions are per-group: a dictionary provided only for the T group does not leak into the PQ
/// group.
#[test]
fn asymmetric_extensions() {
    let safe_aad = ComponentId::from(ComponentType::SafeAad);
    let (_, group) = build_group(Some(dictionary_with(safe_aad)), None);

    assert!(group.t_group.export_group_context().safe_aad_required());
    assert!(!group.pq_group().export_group_context().safe_aad_required());
    // Both still carry the APQMLS entries.
    for context in [
        group.t_group.export_group_context(),
        group.pq_group().export_group_context(),
    ] {
        assert!(dictionary_contains_apq_info(context));
    }
}

/// The caller entry survives a commit: the commit-path dictionary updater must produce only the
/// ApqInfo delta, leaving foreign entries in place.
#[test]
fn caller_entry_survives_commit() {
    let safe_aad = ComponentId::from(ComponentType::SafeAad);
    let (alice, mut group) = build_group(
        Some(dictionary_with(safe_aad)),
        Some(dictionary_with(safe_aad)),
    );

    group
        .commit_builder()
        .finalize(&alice.provider, &alice.signer, |_| true, |_| true)
        .unwrap();
    group.merge_pending_commit(&alice.provider).unwrap();

    for context in [
        group.t_group.export_group_context(),
        group.pq_group().export_group_context(),
    ] {
        assert!(context.safe_aad_required());
        assert!(dictionary_contains_apq_info(context));
    }
}

fn dictionary_contains_apq_info(context: &GroupContext) -> bool {
    context_dictionary(context).contains(&APQMLS_COMPONENT_ID)
}

/// Adds Bob to Alice's group; returns the welcome and the ratchet tree.
fn add_member(
    alice: &Client<OpenMlsRustCrypto>,
    alice_group: &mut ApqMlsGroup,
    bob: &Client<OpenMlsRustCrypto>,
) -> (ApqWelcome, ApqRatchetTreeIn) {
    let key_package = bob.generate_key_package(MODE.default_ciphersuite());
    let bundle = alice_group
        .commit_builder()
        .propose_adds([key_package])
        .finalize(&alice.provider, &alice.signer, |_| true, |_| true)
        .unwrap();
    alice_group.merge_pending_commit(&alice.provider).unwrap();
    let ratchet_tree = alice_group.export_ratchet_tree();
    (bundle.into_welcome().unwrap(), ratchet_tree.into())
}

fn assert_joiner_inherited_dictionary(alice_group: &ApqMlsGroup, bob_group: &ApqMlsGroup) {
    for (alice_context, bob_context) in [
        (
            alice_group.t_group.export_group_context(),
            bob_group.t_group.export_group_context(),
        ),
        (
            alice_group.pq_group().export_group_context(),
            bob_group.pq_group().export_group_context(),
        ),
    ] {
        assert!(bob_context.safe_aad_required());
        assert_eq!(
            context_dictionary(alice_context),
            context_dictionary(bob_context)
        );
    }
}

/// A joiner via `ApqMlsGroup::new_from_welcome` inherits the creator's merged dictionary (incl. the
/// SafeAad entry) through the welcome.
#[test]
fn joiner_inherits_dictionary_via_new_from_welcome() {
    let safe_aad = ComponentId::from(ComponentType::SafeAad);
    let (alice, mut alice_group) = build_group(
        Some(dictionary_with(safe_aad)),
        Some(dictionary_with(safe_aad)),
    );

    let bob = Client::new(
        "Bob",
        MODE.default_ciphersuite().into(),
        OpenMlsRustCrypto::default(),
    );
    let (welcome, ratchet_tree) = add_member(&alice, &mut alice_group, &bob);

    let mut bob_group = ApqMlsGroup::new_from_welcome(
        &bob.provider,
        &MlsGroupJoinConfig::default(),
        welcome,
        Some(ratchet_tree),
    )
    .unwrap();

    assert_joiner_inherited_dictionary(&alice_group, &bob_group);
    assert_group_operational(&alice, &mut alice_group, &bob, &mut bob_group);
}

/// Verifies the joined group is fully operational, beyond context contents: shared group state,
/// correct membership, and commits flowing in both directions after the join.
fn assert_group_operational(
    alice: &Client<OpenMlsRustCrypto>,
    alice_group: &mut ApqMlsGroup,
    bob: &Client<OpenMlsRustCrypto>,
    bob_group: &mut ApqMlsGroup,
) {
    // Same groups and epochs on both sides.
    assert_eq!(alice_group.t_group.group_id(), bob_group.t_group.group_id());
    assert_eq!(
        alice_group.pq_group().group_id(),
        bob_group.pq_group().group_id()
    );
    assert_eq!(alice_group.t_group.epoch(), bob_group.t_group.epoch());
    assert_eq!(alice_group.pq_group().epoch(), bob_group.pq_group().epoch());

    // Both members are present in both views.
    for group in [&*alice_group, &*bob_group] {
        assert_eq!(group.t_group.members().count(), 2);
        assert_eq!(group.pq_group().members().count(), 2);
    }

    // Epoch secrets agree (this is what proves the joiner's PSK derivation between the PQ and T
    // joins actually worked).
    assert_groups_eq(alice_group, bob_group);

    // Bob commits a self-update; Alice processes and merges it.
    commit_and_process(bob, bob_group, alice, alice_group);
    // And the other direction.
    commit_and_process(alice, alice_group, bob, bob_group);

    // The SafeAad requirement survived both post-join commits.
    assert!(bob_group.t_group.export_group_context().safe_aad_required());
}

/// `committer` creates and merges a self-update commit; `processor` processes and merges it.
/// Asserts both views agree afterwards.
fn commit_and_process(
    committer: &Client<OpenMlsRustCrypto>,
    committer_group: &mut ApqMlsGroup,
    processor: &Client<OpenMlsRustCrypto>,
    processor_group: &mut ApqMlsGroup,
) {
    let bundle = committer_group
        .commit_builder()
        .force_self_update(true)
        .finalize(&committer.provider, &committer.signer, |_| true, |_| true)
        .unwrap();
    committer_group
        .merge_pending_commit(&committer.provider)
        .unwrap();

    let protocol_message = ApqMlsMessageIn::try_from(bundle.commit)
        .unwrap()
        .into_protocol_message()
        .unwrap();
    let processed = processor_group
        .process_message(&processor.provider, protocol_message, |cred1, cred2| {
            cred1 == cred2
        })
        .unwrap();
    processor_group
        .merge_staged_commit(&processor.provider, processed.into_staged_commit().unwrap())
        .unwrap();

    assert_groups_eq(committer_group, processor_group);
}
