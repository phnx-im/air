// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::ops::Deref;

use mls_assist::{
    openmls::prelude::{BasicCredential, Credential, SignatureScheme},
    openmls_traits::signatures::{Signer, SignerError},
};
use serde::{Deserialize, Serialize};
use sqlx::{Database, Decode, Encode, Sqlite, Type, encode::IsNull, error::BoxDynError};
use tls_codec::{Serialize as _, TlsDeserializeBytes, TlsSerialize, TlsSize};

use crate::{
    codec::PersistenceCodec,
    crypto::{
        RawKey,
        signatures::{private_keys::Convertible, signable::Signature},
    },
};

use super::{AsCredential, AsIntermediateCredential};

use crate::crypto::signatures::private_keys::{SigningKey, VerifyingKey};

use thiserror::Error;

use super::ClientCredential;

#[derive(Debug)]
pub struct AsIntermediateKeyType;

pub type AsIntermediateSignature = Signature<AsIntermediateKeyType>;

impl RawKey for AsIntermediateKeyType {}

#[derive(Clone, Serialize, Deserialize)]
pub struct AsIntermediateSigningKey {
    signing_key: SigningKey<AsIntermediateKeyType>,
    credential: AsIntermediateCredential,
}

impl Deref for AsIntermediateSigningKey {
    type Target = SigningKey<AsIntermediateKeyType>;

    fn deref(&self) -> &Self::Target {
        &self.signing_key
    }
}

impl Convertible<AsIntermediateKeyType> for PreliminaryAsKeyType {}

impl AsIntermediateSigningKey {
    pub fn from_prelim_key(
        prelim_key: PreliminaryAsIntermediateSigningKey,
        credential: AsIntermediateCredential,
    ) -> Result<Self, SigningKeyCreationError> {
        let prelim_key = prelim_key.convert();
        if prelim_key.verifying_key() != credential.verifying_key() {
            return Err(SigningKeyCreationError::PublicKeyMismatch);
        }
        Ok(Self {
            signing_key: prelim_key,
            credential,
        })
    }

    pub fn credential(&self) -> &AsIntermediateCredential {
        &self.credential
    }

    pub fn into_credential(self) -> AsIntermediateCredential {
        self.credential
    }
}

#[derive(Debug, Error)]
pub enum SigningKeyCreationError {
    #[error("Public key mismatch")]
    PublicKeyMismatch,
}

#[derive(Debug)]
pub struct AsKeyType;

impl RawKey for AsKeyType {}

pub type AsSignature = Signature<AsKeyType>;

#[derive(Debug, Serialize, Deserialize)]
pub struct AsSigningKey {
    signing_key: SigningKey<AsKeyType>,
    credential: AsCredential,
}

impl Deref for AsSigningKey {
    type Target = SigningKey<AsKeyType>;

    fn deref(&self) -> &Self::Target {
        &self.signing_key
    }
}

impl AsSigningKey {
    pub fn from_private_key_and_credential(
        private_key: SigningKey<AsKeyType>,
        credential: AsCredential,
    ) -> Self {
        Self {
            signing_key: private_key,
            credential,
        }
    }

    pub fn credential(&self) -> &AsCredential {
        &self.credential
    }

    pub fn into_credential(self) -> AsCredential {
        self.credential
    }
}

pub type AsVerifyingKey = VerifyingKey<AsKeyType>;

pub type AsIntermediateVerifyingKey = VerifyingKey<AsIntermediateKeyType>;

#[derive(Debug)]
pub struct ClientKeyType;

pub type ClientSignature = Signature<ClientKeyType>;

impl RawKey for ClientKeyType {}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ClientSigningKey {
    signing_key: SigningKey<ClientKeyType>, // private
    credential: ClientCredential,           // known to other users and the server
}

impl TryFrom<&ClientCredential> for Credential {
    type Error = tls_codec::Error;

    fn try_from(value: &ClientCredential) -> Result<Self, Self::Error> {
        let basic_credential = BasicCredential::new(value.tls_serialize_detached()?);
        Ok(basic_credential.into())
    }
}

impl Type<Sqlite> for ClientSigningKey {
    fn type_info() -> <Sqlite as Database>::TypeInfo {
        <Vec<u8> as Type<Sqlite>>::type_info()
    }
}

impl<'q> Encode<'q, Sqlite> for ClientSigningKey {
    fn encode_by_ref(
        &self,
        buf: &mut <Sqlite as Database>::ArgumentBuffer<'q>,
    ) -> Result<IsNull, BoxDynError> {
        let bytes = PersistenceCodec::to_vec(self)?;
        Encode::<Sqlite>::encode(bytes, buf)
    }
}

impl<'r> Decode<'r, Sqlite> for ClientSigningKey {
    fn decode(value: <Sqlite as Database>::ValueRef<'r>) -> Result<Self, BoxDynError> {
        let bytes: &[u8] = Decode::<Sqlite>::decode(value)?;
        let value = PersistenceCodec::from_slice(bytes)?;
        Ok(value)
    }
}

impl Deref for ClientSigningKey {
    type Target = SigningKey<ClientKeyType>;

    fn deref(&self) -> &Self::Target {
        &self.signing_key
    }
}

impl Convertible<ClientKeyType> for PreliminaryClientKeyType {}

impl ClientSigningKey {
    pub fn from_prelim_key(
        prelim_key: PreliminaryClientSigningKey,
        credential: ClientCredential,
    ) -> Result<Self, SigningKeyCreationError> {
        let prelim_key = prelim_key.convert();
        if prelim_key.verifying_key() != credential.verifying_key() {
            return Err(SigningKeyCreationError::PublicKeyMismatch);
        }
        Ok(Self {
            signing_key: prelim_key,
            credential,
        })
    }

    pub fn credential(&self) -> &ClientCredential {
        &self.credential
    }
}

pub type ClientVerifyingKey = VerifyingKey<ClientKeyType>;

impl RawKey for ClientVerifyingKey {}

impl Signer for ClientSigningKey {
    fn sign(&self, payload: &[u8]) -> Result<Vec<u8>, SignerError> {
        self.signing_key
            .sign(payload)
            .map_err(|_| SignerError::SigningError)
            .map(|s| s.into_bytes())
    }

    fn signature_scheme(&self) -> SignatureScheme {
        self.credential.signature_scheme()
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct PreliminaryClientKeyType;
pub type PreliminaryClientSigningKey = SigningKey<PreliminaryClientKeyType>;

#[derive(Debug, Clone, TlsDeserializeBytes, TlsSerialize, TlsSize, Serialize, Deserialize)]
pub struct PreliminaryAsKeyType;
pub type PreliminaryAsIntermediateSigningKey = SigningKey<PreliminaryAsKeyType>;

pub type PreliminaryAsIntermediateVerifyingKey = VerifyingKey<PreliminaryAsKeyType>;

#[derive(Debug)]
pub struct HandleKeyType;

impl RawKey for HandleKeyType {}

pub type HandleSigningKey = SigningKey<HandleKeyType>;

pub type HandleVerifyingKey = VerifyingKey<HandleKeyType>;

pub type HandleSignature = Signature<HandleKeyType>;
