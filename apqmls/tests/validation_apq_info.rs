// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use apqmls::{
    ApqCiphersuite, ApqMlsGroup,
    authentication::{ApqCredentialWithKey, ApqSignatureKeyPair, ApqSigner},
    extension::{APQMLS_COMPONENT_ID, ApqInfo, PqtMode},
    validation::{
        ApqValidationError, Session, validate_apq_group_info, validate_apq_session,
        validate_apq_session_at_construction,
    },
};
use openmls::{
    component::{ComponentId, ComponentType},
    group::{GroupContext, GroupEpoch, GroupId, MlsGroup},
    prelude::{
        AppDataDictionary, AppDataDictionaryExtension, Capabilities, Ciphersuite,
        CredentialWithKey, Extension, ExtensionType, Extensions, MlsMessageBodyIn, MlsMessageIn,
        OpenMlsProvider, group_info::VerifiableGroupInfo,
    },
};
use openmls_rust_crypto::OpenMlsRustCrypto;
use openmls_traits::signatures::Signer;
use tls_codec::{Deserialize as _, Serialize as _};

const MODE: PqtMode = PqtMode::ConfAndAuth;

fn t_group_id() -> GroupId {
    GroupId::from_slice(b"t_group")
}

fn pq_group_id() -> GroupId {
    GroupId::from_slice(b"pq_group")
}

fn ciphersuite() -> ApqCiphersuite {
    ApqCiphersuite::default_pq_conf_and_auth()
}

fn valid_info() -> ApqInfo {
    ApqInfo {
        t_session_group_id: t_group_id(),
        pq_session_group_id: pq_group_id(),
        mode: MODE,
        t_cipher_suite: ciphersuite().t_ciphersuite(),
        pq_cipher_suite: ciphersuite().pq_ciphersuite(),
        t_epoch: GroupEpoch::from(0),
        pq_epoch: GroupEpoch::from(0),
    }
}

/// Builds the two MLS groups directly instead of going through the APQ group builder, so that the
/// [`ApqInfo`] in each group context can be chosen freely.
struct Fixture {
    provider: OpenMlsRustCrypto,
    signer: ApqSignatureKeyPair,
    credential: ApqCredentialWithKey,
}

impl Fixture {
    fn new() -> Self {
        let signer = ApqSignatureKeyPair::new(ciphersuite().into()).unwrap();
        let credential = ApqCredentialWithKey::new(b"Alice", &signer);
        Self {
            provider: OpenMlsRustCrypto::default(),
            signer,
            credential,
        }
    }

    fn t_group(&self, info: Option<&ApqInfo>) -> MlsGroup {
        self.build(
            t_group_id(),
            ciphersuite().t_ciphersuite(),
            extensions(info),
            self.signer.t_signer(),
            self.credential.t_credential.clone(),
        )
    }

    fn pq_group(&self, info: Option<&ApqInfo>) -> MlsGroup {
        self.build(
            pq_group_id(),
            ciphersuite().pq_ciphersuite(),
            extensions(info),
            self.signer.pq_signer(),
            self.credential.pq_credential.clone(),
        )
    }

    fn build(
        &self,
        group_id: GroupId,
        group_ciphersuite: Ciphersuite,
        group_context_extensions: Extensions<GroupContext>,
        signer: &impl Signer,
        credential: CredentialWithKey,
    ) -> MlsGroup {
        let capabilities = Capabilities::new(
            None,
            Some(&[
                ciphersuite().t_ciphersuite(),
                ciphersuite().pq_ciphersuite(),
            ]),
            Some(&[ExtensionType::AppDataDictionary]),
            None,
            None,
        );
        MlsGroup::builder()
            .ciphersuite(group_ciphersuite)
            .with_group_id(group_id)
            .with_capabilities(capabilities)
            .with_group_context_extensions(group_context_extensions)
            .build(&self.provider, signer, credential)
            .unwrap()
    }
}

/// Advances `group` by one epoch, leaving its group context extensions and thus
/// its APQInfo untouched. That is what a PARTIAL commit does to the T session.
fn advance(fixture: &Fixture, group: &mut MlsGroup, signer: &impl Signer) {
    let provider = &fixture.provider;
    group
        .commit_builder()
        .force_self_update(true)
        .load_psks(provider.storage())
        .unwrap()
        .build(provider.rand(), provider.crypto(), signer, |_| true)
        .unwrap()
        .stage_commit(provider)
        .unwrap();
    group.merge_pending_commit(provider).unwrap();
}

fn extensions(info: Option<&ApqInfo>) -> Extensions<GroupContext> {
    let mut dictionary = AppDataDictionary::new();
    if let Some(info) = info {
        dictionary.insert(
            ComponentId::from(ComponentType::AppComponents),
            vec![APQMLS_COMPONENT_ID].tls_serialize_detached().unwrap(),
        );
        dictionary.insert(APQMLS_COMPONENT_ID, info.tls_serialize_detached().unwrap());
    }
    Extensions::from_vec(vec![Extension::AppDataDictionary(
        AppDataDictionaryExtension::new(dictionary),
    )])
    .unwrap()
}

/// The `GroupInfo` a joiner would receive for `group`.
fn group_info(
    group: &MlsGroup,
    crypto: &OpenMlsRustCrypto,
    signer: &impl Signer,
) -> VerifiableGroupInfo {
    let message = group
        .export_group_info(crypto.crypto(), signer, false)
        .unwrap();
    let MlsMessageBodyIn::GroupInfo(group_info) =
        MlsMessageIn::tls_deserialize_exact(message.tls_serialize_detached().unwrap())
            .unwrap()
            .extract()
    else {
        panic!("expected a group info");
    };
    group_info
}

/// Runs the external-joiner validation over the `GroupInfo`s of a pair of groups
/// carrying the given APQInfo.
fn validate_group_info_pair(
    t_info: Option<&ApqInfo>,
    pq_info: Option<&ApqInfo>,
) -> Result<ApqInfo, ApqValidationError> {
    let fixture = Fixture::new();
    let t_group = fixture.t_group(t_info);
    let pq_group = fixture.pq_group(pq_info);
    validate_apq_group_info(
        &group_info(&t_group, &fixture.provider, fixture.signer.t_signer()),
        &group_info(&pq_group, &fixture.provider, fixture.signer.pq_signer()),
    )
}

/// Each call gets its own storage, so the two group IDs are always free.
fn validate_pair(
    t_info: Option<&ApqInfo>,
    pq_info: Option<&ApqInfo>,
) -> Result<ApqInfo, ApqValidationError> {
    let fixture = Fixture::new();
    validate_apq_session(&fixture.t_group(t_info), &fixture.pq_group(pq_info))
}

#[test]
fn matching_apq_info_is_accepted() {
    let info = valid_info();
    assert_eq!(validate_pair(Some(&info), Some(&info)).unwrap(), info);
}

#[test]
fn built_group_is_valid() {
    let fixture = Fixture::new();
    let group = ApqMlsGroup::builder()
        .with_group_ids(t_group_id(), pq_group_id())
        .set_mode(MODE)
        .build(
            &fixture.provider,
            &fixture.signer,
            fixture.credential.clone(),
        )
        .unwrap();

    let info = validate_apq_session(&group.t_group, group.pq_group()).unwrap();
    assert_eq!(info.mode, MODE);
    assert_eq!(&info.t_session_group_id, group.t_group.group_id());
    assert_eq!(&info.pq_session_group_id, group.pq_group().group_id());
}

#[test]
fn missing_apq_info_is_rejected() {
    let info = valid_info();
    assert!(matches!(
        validate_pair(None, Some(&info)),
        Err(ApqValidationError::MissingApqInfo(Session::T))
    ));
    assert!(matches!(
        validate_pair(Some(&info), None),
        Err(ApqValidationError::MissingApqInfo(Session::Pq))
    ));
}

#[test]
fn mismatched_apq_info_is_rejected() {
    let info = valid_info();
    let mut other_info = valid_info();
    other_info.pq_epoch = GroupEpoch::from(1);

    assert!(matches!(
        validate_pair(Some(&info), Some(&other_info)),
        Err(ApqValidationError::ApqInfoMismatch)
    ));
}

#[test]
fn foreign_group_id_is_rejected() {
    let mut info = valid_info();
    info.t_session_group_id = GroupId::from_slice(b"other");
    assert!(matches!(
        validate_pair(Some(&info), Some(&info)),
        Err(ApqValidationError::GroupIdMismatch(Session::T))
    ));

    let mut info = valid_info();
    info.pq_session_group_id = GroupId::from_slice(b"other");
    assert!(matches!(
        validate_pair(Some(&info), Some(&info)),
        Err(ApqValidationError::GroupIdMismatch(Session::Pq))
    ));
}

#[test]
fn ciphersuite_not_matching_the_group_is_rejected() {
    let mut info = valid_info();
    info.t_cipher_suite = Ciphersuite::MLS_128_DHKEMP256_AES128GCM_SHA256_P256;
    assert!(matches!(
        validate_pair(Some(&info), Some(&info)),
        Err(ApqValidationError::CiphersuiteMismatch(Session::T))
    ));

    let mut info = valid_info();
    info.pq_cipher_suite = Ciphersuite::MLS_192_MLKEM1024_AES256GCM_SHA384_P384;
    assert!(matches!(
        validate_pair(Some(&info), Some(&info)),
        Err(ApqValidationError::CiphersuiteMismatch(Session::Pq))
    ));
}

#[test]
fn mode_not_matching_the_ciphersuites_is_rejected() {
    let mut info = valid_info();
    info.mode = PqtMode::ConfOnly;
    assert!(matches!(
        validate_pair(Some(&info), Some(&info)),
        Err(ApqValidationError::ModeMismatch { .. })
    ));
}

#[test]
fn a_pq_epoch_that_does_not_match_the_group_is_rejected() {
    let mut info = valid_info();
    info.pq_epoch = GroupEpoch::from(9);
    assert!(matches!(
        validate_pair(Some(&info), Some(&info)),
        Err(ApqValidationError::EpochMismatch(Session::Pq))
    ));

    // The same in the other direction: a PQ group ahead of what APQInfo names.
    let info = valid_info();
    let fixture = Fixture::new();
    let t_group = fixture.t_group(Some(&info));
    let mut pq_group = fixture.pq_group(Some(&info));
    advance(&fixture, &mut pq_group, fixture.signer.pq_signer());
    assert!(matches!(
        validate_apq_session(&t_group, &pq_group),
        Err(ApqValidationError::EpochMismatch(Session::Pq))
    ));
}

#[test]
fn a_t_epoch_ahead_of_the_group_is_rejected() {
    let mut info = valid_info();
    info.t_epoch = GroupEpoch::from(7);
    assert!(matches!(
        validate_pair(Some(&info), Some(&info)),
        Err(ApqValidationError::TEpochAhead { .. })
    ));
}

#[test]
fn a_stale_t_epoch_is_only_rejected_at_construction() {
    let info = valid_info();
    let fixture = Fixture::new();
    let mut t_group = fixture.t_group(Some(&info));
    let pq_group = fixture.pq_group(Some(&info));
    advance(&fixture, &mut t_group, fixture.signer.t_signer());

    validate_apq_session(&t_group, &pq_group).unwrap();

    assert!(matches!(
        validate_apq_session_at_construction(&t_group, &pq_group, |_, _| true),
        Err(ApqValidationError::EpochMismatch(Session::T))
    ));
}

#[test]
fn an_external_joiner_accepts_a_valid_group_info_pair() {
    let info = valid_info();
    assert_eq!(
        validate_group_info_pair(Some(&info), Some(&info)).unwrap(),
        info
    );
}

#[test]
fn an_external_joiner_rejects_a_missing_or_mismatched_apq_info() {
    let info = valid_info();
    assert!(matches!(
        validate_group_info_pair(None, Some(&info)),
        Err(ApqValidationError::MissingApqInfo(Session::T))
    ));
    assert!(matches!(
        validate_group_info_pair(Some(&info), None),
        Err(ApqValidationError::MissingApqInfo(Session::Pq))
    ));

    let mut other = valid_info();
    other.pq_epoch = GroupEpoch::from(1);
    assert!(matches!(
        validate_group_info_pair(Some(&info), Some(&other)),
        Err(ApqValidationError::ApqInfoMismatch)
    ));
}

#[test]
fn an_external_joiner_rejects_group_id_and_ciphersuite_mismatches() {
    let mut info = valid_info();
    info.pq_session_group_id = GroupId::from_slice(b"other");
    assert!(matches!(
        validate_group_info_pair(Some(&info), Some(&info)),
        Err(ApqValidationError::GroupIdMismatch(Session::Pq))
    ));

    let mut info = valid_info();
    info.t_cipher_suite = Ciphersuite::MLS_128_DHKEMP256_AES128GCM_SHA256_P256;
    assert!(matches!(
        validate_group_info_pair(Some(&info), Some(&info)),
        Err(ApqValidationError::CiphersuiteMismatch(Session::T))
    ));
}

#[test]
fn an_external_joiner_rejects_an_invalid_mode_and_ciphersuite_combination() {
    let mut info = valid_info();
    info.mode = PqtMode::ConfOnly;
    assert!(matches!(
        validate_group_info_pair(Some(&info), Some(&info)),
        Err(ApqValidationError::ModeMismatch { .. })
    ));
}

#[test]
fn an_external_joiner_checks_the_apq_info_epochs() {
    let info = valid_info();
    let fixture = Fixture::new();
    let mut t_group = fixture.t_group(Some(&info));
    let pq_group = fixture.pq_group(Some(&info));
    advance(&fixture, &mut t_group, fixture.signer.t_signer());
    validate_apq_group_info(
        &group_info(&t_group, &fixture.provider, fixture.signer.t_signer()),
        &group_info(&pq_group, &fixture.provider, fixture.signer.pq_signer()),
    )
    .unwrap();

    let mut info = valid_info();
    info.t_epoch = GroupEpoch::from(12);
    assert!(matches!(
        validate_group_info_pair(Some(&info), Some(&info)),
        Err(ApqValidationError::TEpochAhead { .. })
    ));

    let mut info = valid_info();
    info.pq_epoch = GroupEpoch::from(12);
    assert!(matches!(
        validate_group_info_pair(Some(&info), Some(&info)),
        Err(ApqValidationError::EpochMismatch(Session::Pq))
    ));
}
