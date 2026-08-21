// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use apqmls::{
    ApqCiphersuite, ApqMlsGroup,
    authentication::{ApqCredentialWithKey, ApqSignatureKeyPair},
    extension::PqtMode,
    validation::{ApqValidationError, implied_mode, validate_ciphersuites},
};
use openmls::{group::GroupId, prelude::Ciphersuite};
use openmls_rust_crypto::OpenMlsRustCrypto;

const T: Ciphersuite = Ciphersuite::MLS_128_DHKEMX25519_AES128GCM_SHA256_Ed25519;
const T_P384: Ciphersuite = Ciphersuite::MLS_256_DHKEMP384_AES256GCM_SHA384_P384;
/// Pure ML-KEM-1024 KEM with a classical P384 signature.
const PQ_CONF_ONLY: Ciphersuite = Ciphersuite::MLS_192_MLKEM1024_AES256GCM_SHA384_P384;
/// Pure ML-KEM-1024 KEM with an ML-DSA-87 signature.
const PQ_CONF_AND_AUTH: Ciphersuite = Ciphersuite::MLS_256_MLKEM1024_AES256GCM_SHA384_MLDSA87;
/// X-Wing combines ML-KEM-768 with X25519, so its KEM is not pure PQ.
const PQ_HYBRID_KEM: Ciphersuite = Ciphersuite::MLS_128_MLKEM768X25519_AES256GCM_SHA384_Ed25519;

#[test]
fn defaults_are_valid() {
    validate_ciphersuites(ApqCiphersuite::default_pq_conf(), PqtMode::ConfOnly).unwrap();
    validate_ciphersuites(
        ApqCiphersuite::default_pq_conf_and_auth(),
        PqtMode::ConfAndAuth,
    )
    .unwrap();
}

#[test]
fn classical_signature_in_pq_session_is_conf_only() {
    validate_ciphersuites(ApqCiphersuite::new(T, PQ_CONF_ONLY), PqtMode::ConfOnly).unwrap();
    assert_eq!(implied_mode(PQ_CONF_ONLY), PqtMode::ConfOnly);
    assert_eq!(implied_mode(PQ_CONF_AND_AUTH), PqtMode::ConfAndAuth);
}

#[test]
fn duplicate_ciphersuites_are_rejected() {
    assert!(matches!(
        validate_ciphersuites(ApqCiphersuite::new(T, T), PqtMode::ConfOnly),
        Err(ApqValidationError::DuplicateCiphersuite(T))
    ));
}

#[test]
fn two_traditional_ciphersuites_are_rejected() {
    assert!(matches!(
        validate_ciphersuites(ApqCiphersuite::new(T, T_P384), PqtMode::ConfOnly),
        Err(ApqValidationError::InvalidPqKem(_))
    ));
}

#[test]
fn hybrid_kem_in_pq_session_is_rejected() {
    assert!(matches!(
        validate_ciphersuites(ApqCiphersuite::new(T, PQ_HYBRID_KEM), PqtMode::ConfOnly),
        Err(ApqValidationError::InvalidPqKem(_))
    ));
}

#[test]
fn pq_ciphersuite_in_t_session_is_rejected() {
    assert!(matches!(
        validate_ciphersuites(
            ApqCiphersuite::new(PQ_CONF_ONLY, PQ_CONF_AND_AUTH),
            PqtMode::ConfAndAuth
        ),
        Err(ApqValidationError::NonClassicalTKem(_))
    ));
}

#[test]
fn conf_and_auth_with_classical_pq_signature_is_rejected() {
    assert!(matches!(
        validate_ciphersuites(ApqCiphersuite::new(T, PQ_CONF_ONLY), PqtMode::ConfAndAuth),
        Err(ApqValidationError::ModeMismatch { .. })
    ));
}

#[test]
fn conf_only_with_pq_signature_is_rejected() {
    assert!(matches!(
        validate_ciphersuites(ApqCiphersuite::new(T, PQ_CONF_AND_AUTH), PqtMode::ConfOnly),
        Err(ApqValidationError::ModeMismatch { .. })
    ));
}

#[test]
fn group_builder_rejects_invalid_ciphersuites() {
    let provider = OpenMlsRustCrypto::default();
    let ciphersuite = ApqCiphersuite::new(T, T);
    let signer = ApqSignatureKeyPair::new(ciphersuite.into()).unwrap();
    let credential = ApqCredentialWithKey::new(b"Alice", &signer);

    let error = ApqMlsGroup::builder()
        .with_group_ids(GroupId::from_slice(b"t"), GroupId::from_slice(b"pq"))
        .with_ciphersuite(ciphersuite)
        .build(&provider, &signer, credential)
        .expect_err("built a group with duplicate ciphersuites");

    assert!(error.to_string().contains("same ciphersuite"), "{error}");
}
