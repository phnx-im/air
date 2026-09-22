// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use apqmls::{
    ApqMlsGroup,
    authentication::ApqSigner,
    extension::{APQMLS_COMPONENT_ID, ApqInfo, ApqInfoUpdate, ApqInfoUpdateError, PqtMode},
    external_commit_builder::ApqExternalCommitBuilderError,
    messages::{
        ApqKeyPackage, ApqMlsMessageIn, ApqProtocolMessage, ApqRatchetTreeIn, ApqWelcome,
        VerifiableApqGroupInfo,
    },
    processing::{
        ApqProcessMessageError, ApqProcessMessageValidationError, ApqProcessPublicMessageError,
    },
    psk::derive_and_store_commit_psk,
    public_group::ApqPublicGroup,
    validation::{ApqValidationError, Session},
    welcome::WelcomeError,
};
use openmls::{
    component::ComponentData,
    group::{
        CommitMessageBundle, GroupEpoch, GroupId, MlsGroup, MlsGroupCreateConfig,
        MlsGroupJoinConfig, PURE_PLAINTEXT_WIRE_FORMAT_POLICY, ProposalStore,
    },
    prelude::{
        AppDataUpdateProposal, Ciphersuite, Credential, KeyPackage, MlsMessageBodyIn, MlsMessageIn,
        MlsMessageOut, OpenMlsProvider, PreSharedKeyProposal, Proposal, PublicGroup,
    },
    schedule::PreSharedKeyId,
};
use openmls_rust_crypto::OpenMlsRustCrypto;
use openmls_traits::signatures::Signer;
use tls_codec::{Deserialize as _, Serialize as _};

use crate::utils::{assert_groups_eq, client::Client};

mod utils;

/// The mode the test groups run in. The immutability tests flip it, so the
/// tampered value must differ from this one.
const MODE: PqtMode = PqtMode::ConfOnly;

fn compare_credentials(cred1: &Credential, cred2: &Credential) -> bool {
    cred1 == cred2
}

fn new_client(identity: &str) -> Client<OpenMlsRustCrypto> {
    Client::new(
        identity,
        MODE.default_ciphersuite().into(),
        OpenMlsRustCrypto::default(),
    )
}

fn join_config() -> MlsGroupJoinConfig {
    MlsGroupJoinConfig::builder()
        .wire_format_policy(PURE_PLAINTEXT_WIRE_FORMAT_POLICY)
        .build()
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

struct TwoMembers {
    alice: Client<OpenMlsRustCrypto>,
    bob: Client<OpenMlsRustCrypto>,
    alice_group: ApqMlsGroup,
    bob_group: ApqMlsGroup,
}

fn two_member_group() -> TwoMembers {
    let alice = new_client("Alice");
    let bob = new_client("Bob");
    let mut alice_group = create_group(&alice);

    let key_package = bob.generate_key_package(MODE.default_ciphersuite());
    let bundle = alice_group
        .commit_builder()
        .propose_adds([key_package])
        .finalize(&alice.provider, &alice.signer, |_| true, |_| true)
        .unwrap();
    alice_group.merge_pending_commit(&alice.provider).unwrap();
    let bob_group = ApqMlsGroup::new_from_welcome(
        &bob.provider,
        &join_config(),
        bundle.into_welcome().unwrap(),
        Some(alice_group.export_ratchet_tree().into()),
        compare_credentials,
    )
    .unwrap();

    TwoMembers {
        alice,
        bob,
        alice_group,
        bob_group,
    }
}

fn protocol_message(t_commit: MlsMessageOut, pq_commit: MlsMessageOut) -> ApqProtocolMessage {
    let convert = |message: MlsMessageOut| {
        MlsMessageIn::tls_deserialize_exact(message.tls_serialize_detached().unwrap())
            .unwrap()
            .try_into_protocol_message()
            .unwrap()
    };
    ApqProtocolMessage::new(convert(t_commit), convert(pq_commit))
}

fn update_proposal(update: ApqInfoUpdate) -> AppDataUpdateProposal {
    AppDataUpdateProposal::update(
        APQMLS_COMPONENT_ID,
        update.tls_serialize_detached().unwrap(),
    )
}

/// The APQInfo entry in the group context app data dictionary of a leg.
fn apq_entry(group: &MlsGroup) -> Vec<u8> {
    group
        .export_group_context()
        .extensions()
        .app_data_dictionary()
        .expect("group context has no app data dictionary")
        .dictionary()
        .get(&APQMLS_COMPONENT_ID)
        .expect("group context has no APQInfo entry")
        .to_vec()
}

/// The APQInfo dictionary entry a handcrafted commit writes.
#[derive(Clone, Copy)]
enum Entry<'a> {
    Set(&'a [u8]),
    Remove,
}

/// Commits in a single leg, with the given proposals and the given APQInfo
/// dictionary entry. Bypasses the APQ commit builder, so tests can send
/// payloads and entries the builder would never produce.
fn leg_commit(
    provider: &OpenMlsRustCrypto,
    group: &mut MlsGroup,
    signer: &impl Signer,
    adds: &[KeyPackage],
    proposals: &[AppDataUpdateProposal],
    extra_proposals: &[Proposal],
    entry: Entry<'_>,
) -> CommitMessageBundle {
    let mut builder = group
        .commit_builder()
        .force_self_update(true)
        .propose_adds(adds.to_vec());
    for proposal in proposals {
        builder = builder.add_proposal(Proposal::AppDataUpdate(Box::new(proposal.clone())));
    }
    for proposal in extra_proposals {
        builder = builder.add_proposal(proposal.clone());
    }
    let mut builder = builder
        .load_psks(provider.storage())
        .unwrap()
        .create_group_info(true);
    let mut updater = builder.app_data_dictionary_updater();
    match entry {
        Entry::Set(entry) => updater.set(ComponentData::from_parts(
            APQMLS_COMPONENT_ID,
            entry.to_vec().into(),
        )),
        Entry::Remove => updater.remove(&APQMLS_COMPONENT_ID),
    }
    let changes = updater.changes();
    builder.with_app_data_dictionary_updates(changes);
    builder
        .build(provider.rand(), provider.crypto(), signer, |_| true)
        .unwrap()
        .stage_commit(provider)
        .unwrap()
}

/// The value stored under [`foreign_apq_psk_id`].
const FOREIGN_PSK: &[u8] = &[0u8; 32];

/// An application PSK for the APQ component that is not the one the new PQ
/// epoch yields, e.g. a stale one from an earlier epoch.
fn foreign_apq_psk_id(t_ciphersuite: Ciphersuite) -> PreSharedKeyId {
    PreSharedKeyId::application(
        APQMLS_COMPONENT_ID,
        b"not the apq psk id".to_vec(),
        vec![0u8; t_ciphersuite.hash_length()],
    )
}

/// Which PreSharedKey proposal the T leg of a handcrafted commit carries.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ApqPsk {
    /// The PSK exported from the new PQ epoch, as a conformant sender includes
    /// it.
    Derived,
    /// No PreSharedKey proposal at all.
    Omit,
    /// An application PSK for the APQ component that is not the derived one.
    Foreign,
    /// The derived PSK plus a second APQ PSK, which the draft does not allow.
    DerivedAndForeign,
    /// Same, in the other order.
    ForeignAndDerived,
}

/// Same as [`leg_commit`], for both legs of `group`.
///
/// The PQ leg is committed first, so that the PSK the T leg announces is the
/// one the new PQ epoch yields, as it is in a real FULL commit.
fn handcrafted_bundles(
    client: &Client<OpenMlsRustCrypto>,
    group: ApqMlsGroup,
    adds: &[ApqKeyPackage],
    proposals: &[AppDataUpdateProposal],
    entry: Entry<'_>,
    psk: ApqPsk,
) -> (ApqMlsGroup, CommitMessageBundle, CommitMessageBundle) {
    let (mut t_group, mut pq_group) = group.into_groups();
    let t_ciphersuite = t_group.ciphersuite();
    let t_adds: Vec<_> = adds.iter().map(|kp| kp.t_key_package().clone()).collect();
    let pq_adds: Vec<_> = adds.iter().map(|kp| kp.pq_key_package().clone()).collect();
    let pq_bundle = leg_commit(
        &client.provider,
        &mut pq_group,
        client.signer.pq_signer(),
        &pq_adds,
        proposals,
        &[],
        entry,
    );
    let derived = |pq_group: &mut MlsGroup| {
        derive_and_store_commit_psk(&client.provider, pq_group, t_ciphersuite).unwrap()
    };
    let foreign = || {
        let psk_id = foreign_apq_psk_id(t_ciphersuite);
        psk_id.store(&client.provider, FOREIGN_PSK).unwrap();
        psk_id
    };
    let psk_ids = match psk {
        ApqPsk::Derived => vec![derived(&mut pq_group)],
        ApqPsk::Omit => Vec::new(),
        ApqPsk::Foreign => vec![foreign()],
        ApqPsk::DerivedAndForeign => vec![derived(&mut pq_group), foreign()],
        ApqPsk::ForeignAndDerived => {
            let foreign = foreign();
            vec![foreign, derived(&mut pq_group)]
        }
    };
    let psk_proposals: Vec<Proposal> = psk_ids
        .into_iter()
        .map(|psk_id| Proposal::PreSharedKey(Box::new(PreSharedKeyProposal::new(psk_id))))
        .collect();
    let t_bundle = leg_commit(
        &client.provider,
        &mut t_group,
        client.signer.t_signer(),
        &t_adds,
        proposals,
        &psk_proposals,
        entry,
    );
    (
        ApqMlsGroup::from_groups(t_group, pq_group),
        t_bundle,
        pq_bundle,
    )
}

/// The commit pair of [`handcrafted_bundles`], for tests that add no members.
fn handcrafted_commit(
    client: &Client<OpenMlsRustCrypto>,
    group: ApqMlsGroup,
    proposals: &[AppDataUpdateProposal],
    entry: Entry<'_>,
    psk: ApqPsk,
) -> (ApqMlsGroup, ApqProtocolMessage) {
    let (group, t_bundle, pq_bundle) =
        handcrafted_bundles(client, group, &[], proposals, entry, psk);
    (
        group,
        protocol_message(t_bundle.into_commit(), pq_bundle.into_commit()),
    )
}

/// The APQInfo the next full commit in `group` must carry.
fn next_apq_info(group: &ApqMlsGroup) -> ApqInfo {
    let mut apq_info = group.apq_info().unwrap();
    apq_info.t_epoch = GroupEpoch::from(group.t_epoch().as_u64() + 1);
    apq_info.pq_epoch = GroupEpoch::from(group.pq_epoch().as_u64() + 1);
    apq_info
}

/// Builds the DS view of both legs from the group infos and ratchet trees.
fn ds_group(
    client: &Client<OpenMlsRustCrypto>,
    group: &ApqMlsGroup,
    ds_provider: &OpenMlsRustCrypto,
) -> ApqPublicGroup {
    let (t_message, pq_message) = group_info_halves(client, group);
    let MlsMessageBodyIn::GroupInfo(t_group_info) = roundtrip(t_message).extract() else {
        panic!("expected a group info");
    };
    let MlsMessageBodyIn::GroupInfo(pq_group_info) = roundtrip(pq_message).extract() else {
        panic!("expected a group info");
    };
    let (t_ratchet_tree, pq_ratchet_tree) =
        ApqRatchetTreeIn::from(group.export_ratchet_tree()).split();
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
    ApqPublicGroup::from_groups(t_public_group, pq_public_group)
}

/// [`Result::unwrap_err`] without a `Debug` bound on the success type.
fn expect_error<T, E>(result: Result<T, E>) -> E {
    match result {
        Ok(_) => panic!("expected an error"),
        Err(error) => error,
    }
}

fn roundtrip(message: MlsMessageOut) -> MlsMessageIn {
    MlsMessageIn::tls_deserialize_exact(message.tls_serialize_detached().unwrap()).unwrap()
}

fn group_info_halves(
    client: &Client<OpenMlsRustCrypto>,
    group: &ApqMlsGroup,
) -> (MlsMessageOut, MlsMessageOut) {
    group
        .export_group_info(client.provider.crypto(), &client.signer, false)
        .unwrap()
        .split()
}

/// Assembles the two halves the DS serves to an external joiner, which allows
/// combining halves from unrelated groups.
fn verifiable_group_info(
    t_message: MlsMessageOut,
    pq_message: MlsMessageOut,
) -> VerifiableApqGroupInfo {
    let mut bytes = t_message.tls_serialize_detached().unwrap();
    bytes.extend(pq_message.tls_serialize_detached().unwrap());
    ApqMlsMessageIn::tls_deserialize_exact(bytes)
        .unwrap()
        .into_verifiable_group_info()
        .unwrap()
}

#[test]
fn full_update_agrees_between_sender_and_receiver() {
    let TwoMembers {
        alice,
        bob,
        mut alice_group,
        mut bob_group,
    } = two_member_group();

    let expected = next_apq_info(&alice_group);
    let bundle = alice_group
        .commit_builder()
        .force_self_update(true)
        .finalize(&alice.provider, &alice.signer, |_| true, |_| true)
        .unwrap();
    alice_group.merge_pending_commit(&alice.provider).unwrap();

    let message = ApqMlsMessageIn::try_from(bundle.commit)
        .unwrap()
        .into_protocol_message()
        .unwrap();
    let processed = bob_group
        .process_message(&bob.provider, message, compare_credentials)
        .unwrap();
    bob_group
        .merge_staged_commit(&bob.provider, processed.into_staged_commit().unwrap())
        .unwrap();

    assert_eq!(alice_group.apq_info(), Some(expected.clone()));
    assert_eq!(bob_group.apq_info(), Some(expected));

    // The dictionary entries must be byte-identical, otherwise the group
    // contexts and thus the transcript hashes diverge.
    let entry = apq_entry(&alice_group.t_group);
    for group in [&alice_group, &bob_group] {
        assert_eq!(apq_entry(&group.t_group), entry);
        assert_eq!(apq_entry(group.pq_group()), entry);
    }
    assert_groups_eq(&mut alice_group, &mut bob_group);
}

#[test]
fn epoch_only_updates_are_accepted() {
    let TwoMembers {
        alice,
        bob,
        alice_group,
        mut bob_group,
    } = two_member_group();

    let expected = next_apq_info(&alice_group);
    let proposals = [
        update_proposal(ApqInfoUpdate::NewTEpoch(expected.t_epoch)),
        update_proposal(ApqInfoUpdate::NewPqEpoch(expected.pq_epoch)),
    ];
    let entry = expected.tls_serialize_detached().unwrap();
    let (mut alice_group, message) = handcrafted_commit(
        &alice,
        alice_group,
        &proposals,
        Entry::Set(&entry),
        ApqPsk::Derived,
    );
    alice_group.merge_pending_commit(&alice.provider).unwrap();

    let processed = bob_group
        .process_message(&bob.provider, message, compare_credentials)
        .unwrap();
    bob_group
        .merge_staged_commit(&bob.provider, processed.into_staged_commit().unwrap())
        .unwrap();

    assert_eq!(alice_group.apq_info(), Some(expected.clone()));
    assert_eq!(bob_group.apq_info(), Some(expected));
    assert_eq!(apq_entry(&bob_group.t_group), entry);
    assert_groups_eq(&mut alice_group, &mut bob_group);
}

#[test]
fn malformed_update_payload_is_rejected() {
    let TwoMembers {
        alice,
        bob,
        alice_group,
        mut bob_group,
    } = two_member_group();

    let entry = next_apq_info(&alice_group)
        .tls_serialize_detached()
        .unwrap();
    let proposals = [AppDataUpdateProposal::update(
        APQMLS_COMPONENT_ID,
        vec![0xff],
    )];
    let (_alice_group, message) = handcrafted_commit(
        &alice,
        alice_group,
        &proposals,
        Entry::Set(&entry),
        ApqPsk::Derived,
    );

    let error =
        expect_error(bob_group.process_message(&bob.provider, message, compare_credentials));
    assert!(matches!(
        error,
        ApqProcessMessageError::ApqInfoUpdate(ApqInfoUpdateError::MalformedUpdate(_))
    ));
}

/// A full update that tampers with one immutable APQInfo field. The new epochs
/// are correct, so the immutability check is the only one that can reject the
/// commit.
fn tampered_commit(
    alice: &Client<OpenMlsRustCrypto>,
    alice_group: ApqMlsGroup,
    tamper: impl FnOnce(&mut ApqInfo),
) -> (ApqMlsGroup, ApqProtocolMessage) {
    let mut tampered = next_apq_info(&alice_group);
    tamper(&mut tampered);
    let entry = tampered.tls_serialize_detached().unwrap();
    let proposals = [update_proposal(ApqInfoUpdate::FullUpdate(tampered))];
    handcrafted_commit(
        alice,
        alice_group,
        &proposals,
        Entry::Set(&entry),
        ApqPsk::Derived,
    )
}

fn mode_downgrade_commit(
    alice: &Client<OpenMlsRustCrypto>,
    alice_group: ApqMlsGroup,
) -> (ApqMlsGroup, ApqProtocolMessage) {
    tampered_commit(alice, alice_group, |info| info.mode = PqtMode::ConfAndAuth)
}

#[test]
fn mode_change_is_rejected_by_member() {
    let TwoMembers {
        alice,
        bob,
        alice_group,
        mut bob_group,
    } = two_member_group();

    let (_alice_group, message) = mode_downgrade_commit(&alice, alice_group);

    let error =
        expect_error(bob_group.process_message(&bob.provider, message, compare_credentials));
    assert!(matches!(
        error,
        ApqProcessMessageError::Validation(
            ApqProcessMessageValidationError::ImmutableApqInfoModified
        )
    ));
}

#[test]
fn mode_change_is_rejected_by_public_group() {
    let TwoMembers {
        alice, alice_group, ..
    } = two_member_group();

    let ds_provider = OpenMlsRustCrypto::default();
    let mut ds_group = ds_group(&alice, &alice_group, &ds_provider);
    let (_alice_group, message) = mode_downgrade_commit(&alice, alice_group);

    let error = expect_error(ds_group.as_mut().process_message(
        ds_provider.crypto(),
        message,
        compare_credentials,
    ));
    assert_eq!(
        error,
        ApqProcessPublicMessageError::Validation(
            ApqProcessMessageValidationError::ImmutableApqInfoModified
        )
    );
}

#[test]
fn group_id_and_ciphersuite_changes_are_rejected() {
    let tampers: [fn(&mut ApqInfo); 4] = [
        |info| info.t_session_group_id = GroupId::from_slice(b"other t group"),
        |info| info.pq_session_group_id = GroupId::from_slice(b"other pq group"),
        |info| info.t_cipher_suite = Ciphersuite::MLS_128_DHKEMP256_AES128GCM_SHA256_P256,
        |info| info.pq_cipher_suite = Ciphersuite::MLS_128_MLKEM768_AES256GCM_SHA384_Ed25519,
    ];

    for tamper in tampers {
        let TwoMembers {
            alice,
            bob,
            alice_group,
            mut bob_group,
        } = two_member_group();

        let (_alice_group, message) = tampered_commit(&alice, alice_group, tamper);

        let error =
            expect_error(bob_group.process_message(&bob.provider, message, compare_credentials));
        assert!(
            matches!(
                error,
                ApqProcessMessageError::Validation(
                    ApqProcessMessageValidationError::InvalidApqInfo
                )
            ),
            "unexpected error: {error:?}"
        );
    }
}

#[test]
fn a_full_commit_without_the_apq_psk_is_rejected_by_member() {
    let TwoMembers {
        alice,
        bob,
        alice_group,
        mut bob_group,
    } = two_member_group();

    let entry = next_apq_info(&alice_group)
        .tls_serialize_detached()
        .unwrap();
    let proposals = [update_proposal(ApqInfoUpdate::FullUpdate(next_apq_info(
        &alice_group,
    )))];
    let (_alice_group, message) = handcrafted_commit(
        &alice,
        alice_group,
        &proposals,
        Entry::Set(&entry),
        ApqPsk::Omit,
    );

    let error =
        expect_error(bob_group.process_message(&bob.provider, message, compare_credentials));
    assert!(matches!(
        error,
        ApqProcessMessageError::Validation(ApqProcessMessageValidationError::MissingApqPsk)
    ));
}

#[test]
fn a_full_commit_with_a_foreign_apq_psk_is_rejected_by_member() {
    let TwoMembers {
        alice,
        bob,
        alice_group,
        mut bob_group,
    } = two_member_group();

    let entry = next_apq_info(&alice_group)
        .tls_serialize_detached()
        .unwrap();
    let proposals = [update_proposal(ApqInfoUpdate::FullUpdate(next_apq_info(
        &alice_group,
    )))];

    // Bob holds the PSK too, so the T commit itself is processable. Only the
    // comparison against the PSK the new PQ epoch yields can catch this.
    foreign_apq_psk_id(alice_group.t_group.ciphersuite())
        .store(&bob.provider, FOREIGN_PSK)
        .unwrap();

    let (_alice_group, message) = handcrafted_commit(
        &alice,
        alice_group,
        &proposals,
        Entry::Set(&entry),
        ApqPsk::Foreign,
    );

    let error =
        expect_error(bob_group.process_message(&bob.provider, message, compare_credentials));
    assert!(matches!(
        error,
        ApqProcessMessageError::Validation(ApqProcessMessageValidationError::ApqPskMismatch)
    ));
}

#[test]
fn a_commit_builder_commit_with_a_group_info_is_accepted() {
    let TwoMembers {
        alice,
        bob,
        mut alice_group,
        mut bob_group,
    } = two_member_group();

    let ds_provider = OpenMlsRustCrypto::default();
    let mut ds_group = ds_group(&alice, &alice_group, &ds_provider);

    let bundle = alice_group
        .commit_builder()
        .force_self_update(true)
        .create_group_info(true)
        .finalize(&alice.provider, &alice.signer, |_| true, |_| true)
        .unwrap();
    assert!(bundle.group_info.is_some());
    alice_group.merge_pending_commit(&alice.provider).unwrap();

    let (t_commit, pq_commit) = bundle.commit.split();
    ds_group
        .as_mut()
        .process_message(
            ds_provider.crypto(),
            protocol_message(t_commit.clone(), pq_commit.clone()),
            compare_credentials,
        )
        .unwrap();

    let processed = bob_group
        .process_message(
            &bob.provider,
            protocol_message(t_commit, pq_commit),
            compare_credentials,
        )
        .unwrap();
    bob_group
        .merge_staged_commit(&bob.provider, processed.into_staged_commit().unwrap())
        .unwrap();
    assert_groups_eq(&mut alice_group, &mut bob_group);
}

#[test]
fn a_full_commit_with_two_apq_psks_is_rejected() {
    for psk in [ApqPsk::DerivedAndForeign, ApqPsk::ForeignAndDerived] {
        let TwoMembers {
            alice,
            bob,
            alice_group,
            mut bob_group,
        } = two_member_group();

        let entry = next_apq_info(&alice_group)
            .tls_serialize_detached()
            .unwrap();
        let proposals = [update_proposal(ApqInfoUpdate::FullUpdate(next_apq_info(
            &alice_group,
        )))];
        foreign_apq_psk_id(alice_group.t_group.ciphersuite())
            .store(&bob.provider, FOREIGN_PSK)
            .unwrap();

        let (_alice_group, message) =
            handcrafted_commit(&alice, alice_group, &proposals, Entry::Set(&entry), psk);

        let error =
            expect_error(bob_group.process_message(&bob.provider, message, compare_credentials));
        assert!(
            matches!(
                error,
                ApqProcessMessageError::Validation(
                    ApqProcessMessageValidationError::DuplicateApqPsk
                )
            ),
            "unexpected error: {error:?}"
        );
    }
}

#[test]
fn a_welcome_without_the_apq_psk_is_rejected() {
    let alice = new_client("Alice");
    let bob = new_client("Bob");
    let alice_group = create_group(&alice);

    let entry = next_apq_info(&alice_group)
        .tls_serialize_detached()
        .unwrap();
    let proposals = [update_proposal(ApqInfoUpdate::FullUpdate(next_apq_info(
        &alice_group,
    )))];
    let key_package = bob.generate_key_package(MODE.default_ciphersuite());
    let (mut alice_group, t_bundle, pq_bundle) = handcrafted_bundles(
        &alice,
        alice_group,
        &[key_package],
        &proposals,
        Entry::Set(&entry),
        ApqPsk::Omit,
    );
    alice_group.merge_pending_commit(&alice.provider).unwrap();

    let welcome = ApqWelcome::new(
        t_bundle.into_welcome().unwrap(),
        pq_bundle.into_welcome().unwrap(),
    );
    let result = ApqMlsGroup::new_from_welcome(
        &bob.provider,
        &join_config(),
        welcome,
        Some(alice_group.export_ratchet_tree().into()),
        compare_credentials,
    );
    assert!(matches!(
        expect_error(result),
        WelcomeError::Validation(ApqValidationError::MissingApqPsk)
    ));
}

#[test]
fn a_full_commit_without_the_apq_psk_is_rejected_by_public_group() {
    let TwoMembers {
        alice, alice_group, ..
    } = two_member_group();

    let ds_provider = OpenMlsRustCrypto::default();
    let mut ds_group = ds_group(&alice, &alice_group, &ds_provider);

    let entry = next_apq_info(&alice_group)
        .tls_serialize_detached()
        .unwrap();
    let proposals = [update_proposal(ApqInfoUpdate::FullUpdate(next_apq_info(
        &alice_group,
    )))];
    let (_alice_group, message) = handcrafted_commit(
        &alice,
        alice_group,
        &proposals,
        Entry::Set(&entry),
        ApqPsk::Omit,
    );

    let error = expect_error(ds_group.as_mut().process_message(
        ds_provider.crypto(),
        message,
        compare_credentials,
    ));
    assert_eq!(
        error,
        ApqProcessPublicMessageError::Validation(ApqProcessMessageValidationError::MissingApqPsk)
    );
}

#[test]
fn a_removed_apq_info_is_rejected_by_public_group() {
    let TwoMembers {
        alice, alice_group, ..
    } = two_member_group();

    let ds_provider = OpenMlsRustCrypto::default();
    let mut ds_group = ds_group(&alice, &alice_group, &ds_provider);

    let proposals = [AppDataUpdateProposal::remove(APQMLS_COMPONENT_ID)];
    let (_alice_group, message) = handcrafted_commit(
        &alice,
        alice_group,
        &proposals,
        Entry::Remove,
        ApqPsk::Derived,
    );

    let error = expect_error(ds_group.as_mut().process_message(
        ds_provider.crypto(),
        message,
        compare_credentials,
    ));
    assert_eq!(
        error,
        ApqProcessPublicMessageError::Validation(ApqProcessMessageValidationError::MissingApqInfo)
    );
}

#[test]
fn a_wrong_epoch_is_rejected_by_public_group() {
    let TwoMembers {
        alice, alice_group, ..
    } = two_member_group();

    let ds_provider = OpenMlsRustCrypto::default();
    let mut ds_group = ds_group(&alice, &alice_group, &ds_provider);

    let (_alice_group, message) = tampered_commit(&alice, alice_group, |info| {
        info.pq_epoch = GroupEpoch::from(42)
    });

    let error = expect_error(ds_group.as_mut().process_message(
        ds_provider.crypto(),
        message,
        compare_credentials,
    ));
    assert_eq!(
        error,
        ApqProcessPublicMessageError::Validation(ApqProcessMessageValidationError::InvalidApqInfo)
    );
}

#[test]
fn a_malformed_update_payload_is_rejected_by_public_group() {
    let TwoMembers {
        alice, alice_group, ..
    } = two_member_group();

    let ds_provider = OpenMlsRustCrypto::default();
    let mut ds_group = ds_group(&alice, &alice_group, &ds_provider);

    let entry = next_apq_info(&alice_group)
        .tls_serialize_detached()
        .unwrap();
    let proposals = [AppDataUpdateProposal::update(
        APQMLS_COMPONENT_ID,
        vec![0xff],
    )];
    let (_alice_group, message) = handcrafted_commit(
        &alice,
        alice_group,
        &proposals,
        Entry::Set(&entry),
        ApqPsk::Derived,
    );

    let error = expect_error(ds_group.as_mut().process_message(
        ds_provider.crypto(),
        message,
        compare_credentials,
    ));
    assert!(matches!(
        error,
        ApqProcessPublicMessageError::ApqInfoUpdate(ApqInfoUpdateError::MalformedUpdate(_))
    ));
}

#[test]
fn external_join_requires_matching_apq_info() {
    let alice = new_client("Alice");
    let bob = new_client("Bob");
    let charlie = new_client("Charlie");
    let alice_group = create_group(&alice);
    let charlie_group = create_group(&charlie);

    // T half from Alice's group, PQ half from Charlie's unrelated group.
    let (t_message, _) = group_info_halves(&alice, &alice_group);
    let (_, pq_message) = group_info_halves(&charlie, &charlie_group);
    let ratchet_tree = ApqRatchetTreeIn::new(
        alice_group.t_group.export_ratchet_tree().into(),
        charlie_group.pq_group().export_ratchet_tree().into(),
    );

    let result = ApqMlsGroup::external_commit_builder()
        .with_ratchet_tree(ratchet_tree)
        .with_config(join_config())
        .build(
            &bob.provider,
            &bob.signer,
            bob.credential_with_key.clone(),
            verifiable_group_info(t_message, pq_message),
            compare_credentials,
        );
    assert!(matches!(
        result,
        Err(ApqExternalCommitBuilderError::Validation(
            ApqValidationError::ApqInfoMismatch
        ))
    ));
}

#[test]
fn external_join_requires_apq_info_in_the_pq_group_info() {
    let alice = new_client("Alice");
    let bob = new_client("Bob");
    let alice_group = create_group(&alice);

    // A plain MLS group has no APQInfo in its group context.
    let plain_group = MlsGroup::new_with_group_id(
        &alice.provider,
        alice.signer.pq_signer(),
        &MlsGroupCreateConfig::default(),
        GroupId::from_slice(b"plain_pq_group"),
        alice.credential_with_key.pq_credential.clone(),
    )
    .unwrap();
    let pq_message = plain_group
        .export_group_info(alice.provider.crypto(), alice.signer.pq_signer(), false)
        .unwrap();
    let (t_message, _) = group_info_halves(&alice, &alice_group);
    let ratchet_tree = ApqRatchetTreeIn::new(
        alice_group.t_group.export_ratchet_tree().into(),
        plain_group.export_ratchet_tree().into(),
    );

    let result = ApqMlsGroup::external_commit_builder()
        .with_ratchet_tree(ratchet_tree)
        .with_config(join_config())
        .build(
            &bob.provider,
            &bob.signer,
            bob.credential_with_key.clone(),
            verifiable_group_info(t_message, pq_message),
            compare_credentials,
        );
    assert!(matches!(
        result,
        Err(ApqExternalCommitBuilderError::Validation(
            ApqValidationError::MissingApqInfo(Session::Pq)
        ))
    ));
}
