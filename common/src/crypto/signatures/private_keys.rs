// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::marker::PhantomData;

use mls_assist::{
    openmls::prelude::SignaturePublicKey,
    openmls_rust_crypto::OpenMlsRustCrypto,
    openmls_traits::{OpenMlsProvider, crypto::OpenMlsCrypto},
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    LibraryError,
    crypto::{RawKey, errors::KeyGenerationError, secrets::SecretBytes},
};

use super::{DEFAULT_SIGNATURE_SCHEME, signable::Signature};

/// A key that can be used to verify signatures. It should be parameterized by a
/// unique key type to ensure type safety.
#[derive(Debug, Serialize, Deserialize)]
#[serde(transparent)]
pub struct VerifyingKey<KT> {
    #[serde(with = "serde_bytes")]
    pub(super) key: Vec<u8>,
    _type: PhantomData<KT>,
}

/// A reference to a verifying key. This is used to avoid copying the key
/// unnecessarily. It should be parameterized by a unique key type to ensure
/// type safety.
pub struct VerifyingKeyRef<'a, KT> {
    key: &'a [u8],
    _type: PhantomData<KT>,
}

// We need these traits to interop the MLS leaf keys.
impl<'a, KT> From<&'a SignaturePublicKey> for VerifyingKeyRef<'a, KT> {
    fn from(pk: &'a SignaturePublicKey) -> Self {
        Self::from_slice(pk.as_slice())
    }
}

impl<KT> From<VerifyingKey<KT>> for SignaturePublicKey {
    fn from(pk: VerifyingKey<KT>) -> Self {
        SignaturePublicKey::from(pk.key)
    }
}

trait AsSlice {
    fn as_slice(&self) -> &[u8];
}

impl<KT: RawKey> VerifyingKey<KT> {
    pub fn into_bytes(self) -> Vec<u8> {
        self.key
    }

    pub fn from_bytes(bytes: Vec<u8>) -> Self {
        Self {
            key: bytes,
            _type: PhantomData,
        }
    }
}

#[allow(private_bounds)]
pub trait VerifyingKeyBehaviour: AsSlice {
    /// Verify the given signature with the given payload. Returns an error if the
    /// verification fails or if the signature does not have the right length.
    fn verify(&self, payload: &[u8], signature: &[u8]) -> Result<(), SignatureVerificationError> {
        let rust_crypto = OpenMlsRustCrypto::default();
        rust_crypto
            .crypto()
            .verify_signature(
                DEFAULT_SIGNATURE_SCHEME,
                payload,
                self.as_slice(),
                signature,
            )
            .map_err(|_| SignatureVerificationError::VerificationFailure)
    }
}

impl<KT> VerifyingKeyBehaviour for &VerifyingKey<KT> {}
impl<KT> VerifyingKeyBehaviour for VerifyingKeyRef<'_, KT> {}

impl<KT> AsSlice for &VerifyingKey<KT> {
    fn as_slice(&self) -> &[u8] {
        &self.key
    }
}

impl<KT> AsSlice for VerifyingKeyRef<'_, KT> {
    fn as_slice(&self) -> &[u8] {
        self.key
    }
}

impl<'a, KT> VerifyingKeyRef<'a, KT> {
    fn from_slice(key: &'a [u8]) -> Self {
        Self {
            key,
            _type: PhantomData,
        }
    }
}

impl<KT> VerifyingKey<KT> {
    pub(super) fn new(bytes: Vec<u8>) -> Self {
        Self {
            key: bytes,
            _type: PhantomData,
        }
    }

    #[cfg(any(test, feature = "test_utils"))]
    pub fn new_for_test(value: Vec<u8>) -> Self {
        Self::new(value)
    }

    pub fn as_slice(&self) -> &[u8] {
        &self.key
    }

    pub fn as_ref(&self) -> VerifyingKeyRef<'_, KT> {
        VerifyingKeyRef {
            key: &self.key,
            _type: PhantomData,
        }
    }
}

impl<KT> PartialEq for VerifyingKeyRef<'_, KT> {
    fn eq(&self, other: &Self) -> bool {
        self.key == other.key
    }
}

/// This is the key that is used to sign messages. It also contains the public
/// key of the same type. This struct should be parameterized by a unique key
/// type to ensure type safety.
#[derive(Debug, Serialize, Deserialize)]
pub struct SigningKey<KT> {
    pub(super) signing_key: SecretBytes,
    #[serde(bound = "")]
    pub(super) verifying_key: VerifyingKey<KT>,
}

impl<KT> Clone for SigningKey<KT> {
    fn clone(&self) -> Self {
        Self {
            signing_key: self.signing_key.clone(),
            verifying_key: self.verifying_key.clone(),
        }
    }
}

impl<KT> SigningKey<KT> {
    /// Generate a new signing key.
    pub fn generate() -> Result<SigningKey<KT>, KeyGenerationError> {
        let (private_key, public_key) = OpenMlsRustCrypto::default()
            .crypto()
            .signature_key_gen(DEFAULT_SIGNATURE_SCHEME)
            .map_err(|_| KeyGenerationError::KeypairGeneration)?;
        Ok(Self {
            signing_key: SecretBytes::from(private_key),
            verifying_key: VerifyingKey::new(public_key),
        })
    }

    pub fn verifying_key(&self) -> &VerifyingKey<KT> {
        &self.verifying_key
    }

    /// Sign the given payload with this signing key.
    pub(crate) fn sign(&self, payload: &[u8]) -> Result<Signature<KT>, LibraryError> {
        let rust_crypto = OpenMlsRustCrypto::default();
        rust_crypto
            .crypto()
            .sign(DEFAULT_SIGNATURE_SCHEME, payload, &self.signing_key)
            .map_err(|_| LibraryError)
            .map(Signature::from_bytes)
    }
}

/// Marker trait that allows the conversion between the implementer and the
/// `Target` key type.
pub trait Convertible<Target> {}

impl<Source> VerifyingKey<Source> {
    pub fn convert<Target>(self) -> VerifyingKey<Target>
    where
        Source: Convertible<Target>,
    {
        VerifyingKey::new(self.key)
    }
}

impl<Source> SigningKey<Source> {
    pub fn convert<Target>(self) -> SigningKey<Target>
    where
        Source: Convertible<Target>,
    {
        SigningKey {
            signing_key: self.signing_key,
            verifying_key: VerifyingKey::new(self.verifying_key.key),
        }
    }
}

/// Error verifying signature.
#[derive(Error, Debug)]
pub enum SignatureVerificationError {
    /// Could not verify this signature with the given payload.
    #[error("Could not verify this signature with the given payload.")]
    VerificationFailure,
    /// Unrecoverable implementation error
    #[error(transparent)]
    LibraryError(#[from] LibraryError),
}

pub const SIGNATURE_PUBLIC_KEY_SIZE: usize = 32;
