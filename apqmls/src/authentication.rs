// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! This module defines types and traits for handling authentication in APQMLS.

use openmls::prelude::{
    BasicCredential, CredentialWithKey, CryptoError, SignaturePublicKey, SignatureScheme,
};
use openmls_basic_credential::SignatureKeyPair;
use openmls_traits::{
    signatures::Signer,
    storage::{self, CURRENT_VERSION},
};
use serde::{Deserialize, Serialize};
use tap::Pipe;
use tls_codec::{Serialize as _, TlsDeserialize, TlsSerialize, TlsSize};

use crate::ApqCiphersuite;

/// The combined verifying key of a [`ApqSigner`].
#[derive(Debug, Clone, PartialEq, Eq, TlsSize, TlsSerialize, TlsDeserialize)]
pub struct ApqVerifyingKey {
    pub t_verifying_key: SignaturePublicKey,
    pub pq_verifying_key: SignaturePublicKey,
}

impl ApqVerifyingKey {
    pub fn to_bytes(&self) -> Vec<u8> {
        // We unwrap here, because we know that public keys are not going to be
        // too long for the TLS codec to handle.
        self.tls_serialize_detached().unwrap()
    }
}

/// A combined credential with key for use in APQMLS.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApqCredentialWithKey {
    pub t_credential: CredentialWithKey,
    pub pq_credential: CredentialWithKey,
}

impl ApqCredentialWithKey {
    pub fn new(identity: &[u8], keypair: &ApqSignatureKeyPair) -> Self {
        let t_credential = BasicCredential::new(identity.to_vec());
        let pq_credential = BasicCredential::new(identity.to_vec());

        Self {
            t_credential: CredentialWithKey {
                credential: t_credential.into(),
                signature_key: keypair.t_signer.public().into(),
            },
            pq_credential: CredentialWithKey {
                credential: pq_credential.into(),
                signature_key: keypair.pq_signer.public().into(),
            },
        }
    }
}

/// A trait for types that can sign messages in APQMLS.
pub trait ApqSigner {
    type TSigner: Signer;
    type PqSigner: Signer;

    fn t_signer(&self) -> &Self::TSigner;
    fn pq_signer(&self) -> &Self::PqSigner;
}

/// The signature scheme of a [`ApqSigner`].
#[derive(Debug, Clone, Copy)]
pub struct ApqSignatureScheme {
    pub t_signature_scheme: SignatureScheme,
    pub pq_signature_scheme: SignatureScheme,
}

impl From<ApqCiphersuite> for ApqSignatureScheme {
    fn from(ciphersuite: ApqCiphersuite) -> Self {
        Self {
            t_signature_scheme: ciphersuite.t_ciphersuite.signature_algorithm(),
            pq_signature_scheme: ciphersuite.pq_ciphersuite.signature_algorithm(),
        }
    }
}

/// A combined signature key pair for use in APQMLS.
#[derive(Debug, Clone, TlsSize, TlsSerialize, TlsDeserialize, Serialize, Deserialize)]
pub struct ApqSignatureKeyPair {
    t_signer: SignatureKeyPair,
    pq_signer: SignatureKeyPair,
}

impl ApqSignatureKeyPair {
    pub fn new(signature_scheme: ApqSignatureScheme) -> Result<Self, CryptoError> {
        let t_signer = SignatureKeyPair::new(signature_scheme.t_signature_scheme)?;
        let pq_signer = SignatureKeyPair::new(signature_scheme.pq_signature_scheme)?;
        Self {
            t_signer,
            pq_signer,
        }
        .pipe(Ok)
    }

    pub fn id(&self) -> ApqStorageId {
        ApqStorageId {
            t_signature_scheme: self.t_signer.signature_scheme(),
            t_verifying_key: self.t_signer.public().to_vec(),
            pq_signature_scheme: self.pq_signer.signature_scheme(),
            pq_verifying_key: self.pq_signer.public().to_vec(),
        }
    }
}

impl ApqSigner for ApqSignatureKeyPair {
    type TSigner = SignatureKeyPair;
    type PqSigner = SignatureKeyPair;

    fn t_signer(&self) -> &SignatureKeyPair {
        &self.t_signer
    }

    fn pq_signer(&self) -> &SignatureKeyPair {
        &self.pq_signer
    }
}

/// The storage ID for a [`ApqSignatureKeyPair`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApqStorageId {
    t_signature_scheme: SignatureScheme,
    t_verifying_key: Vec<u8>,
    pq_signature_scheme: SignatureScheme,
    pq_verifying_key: Vec<u8>,
}

// Implement key traits for the storage id
impl storage::Key<CURRENT_VERSION> for ApqStorageId {}
impl storage::traits::SignaturePublicKey<CURRENT_VERSION> for ApqStorageId {}

// Implement entity trait for the signature key pair
impl storage::Entity<CURRENT_VERSION> for ApqSignatureKeyPair {}
impl storage::traits::SignatureKeyPair<CURRENT_VERSION> for ApqSignatureKeyPair {}
