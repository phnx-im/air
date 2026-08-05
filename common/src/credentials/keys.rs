// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::ops::Deref;

use apqmls::authentication::ApqSigner;
use mls_assist::{
    openmls::prelude::{BasicCredential, Credential, SignatureScheme},
    openmls_traits::signatures::{Signer, SignerError},
};
use serde::{Deserialize, Serialize};
use sqlx::{Database, Decode, Encode, Sqlite, Type, encode::IsNull, error::BoxDynError};
use tls_codec::{Serialize as _, TlsDeserializeBytes, TlsSerialize, TlsSize};
use uuid::Uuid;

use crate::{
    codec::{self, PersistenceCodec},
    crypto::{
        RawKey,
        errors::KeyGenerationError,
        signatures::{DEFAULT_SIGNATURE_SCHEME, private_keys::Convertible, signable::Signature},
    },
};

use super::{AsCredential, AsIntermediateCredential, RoomPolicyIdentity, SelfGroupCredential};

use crate::crypto::signatures::private_keys::{SigningKey, VerifyingKey};

use thiserror::Error;

use super::UserCredential;

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
    credential: UserCredential,             // known to other users and the server
}

impl TryFrom<&UserCredential> for Credential {
    type Error = tls_codec::Error;

    fn try_from(value: &UserCredential) -> Result<Self, Self::Error> {
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
        buf: &mut <Sqlite as Database>::ArgumentBuffer,
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
    /// Pair a signing key with a matching credential and validates it.
    pub fn from_prelim_key(
        prelim_key: PreliminaryClientSigningKey,
        credential: UserCredential,
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

    pub fn credential(&self) -> &UserCredential {
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

impl ApqSigner for ClientSigningKey {
    type TSigner = Self;
    type PqSigner = Self;

    fn t_signer(&self) -> &Self {
        self
    }

    fn pq_signer(&self) -> &Self {
        self
    }
}

/// Signing key for a client's leaf in its user's self-group.
///
/// Minted per device. Unlike [`ClientSigningKey`], the paired [`SelfGroupCredential`] carries
/// no key material. The DS verifies request envelopes from self-group members against the
/// leaf's public signature key, so the key type matches [`ClientKeyType`].
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SelfGroupSigningKey {
    signing_key: SigningKey<ClientKeyType>,
    credential: SelfGroupCredential,
}

impl SelfGroupSigningKey {
    pub fn generate(client_id: Uuid) -> Result<Self, KeyGenerationError> {
        Ok(Self {
            signing_key: SigningKey::generate()?,
            credential: SelfGroupCredential::new(client_id),
        })
    }

    pub fn credential(&self) -> &SelfGroupCredential {
        &self.credential
    }
}

impl Deref for SelfGroupSigningKey {
    type Target = SigningKey<ClientKeyType>;

    fn deref(&self) -> &Self::Target {
        &self.signing_key
    }
}

impl Type<Sqlite> for SelfGroupSigningKey {
    fn type_info() -> <Sqlite as Database>::TypeInfo {
        <Vec<u8> as Type<Sqlite>>::type_info()
    }
}

impl<'q> Encode<'q, Sqlite> for SelfGroupSigningKey {
    fn encode_by_ref(
        &self,
        buf: &mut <Sqlite as Database>::ArgumentBuffer,
    ) -> Result<IsNull, BoxDynError> {
        let bytes = PersistenceCodec::to_vec(self)?;
        Encode::<Sqlite>::encode(bytes, buf)
    }
}

impl<'r> Decode<'r, Sqlite> for SelfGroupSigningKey {
    fn decode(value: <Sqlite as Database>::ValueRef<'r>) -> Result<Self, BoxDynError> {
        let bytes: &[u8] = Decode::<Sqlite>::decode(value)?;
        let value = PersistenceCodec::from_slice(bytes)?;
        Ok(value)
    }
}

impl Signer for SelfGroupSigningKey {
    fn sign(&self, payload: &[u8]) -> Result<Vec<u8>, SignerError> {
        self.signing_key
            .sign(payload)
            .map_err(|_| SignerError::SigningError)
            .map(|s| s.into_bytes())
    }

    fn signature_scheme(&self) -> SignatureScheme {
        DEFAULT_SIGNATURE_SCHEME
    }
}

impl ApqSigner for SelfGroupSigningKey {
    type TSigner = Self;
    type PqSigner = Self;

    fn t_signer(&self) -> &Self {
        self
    }

    fn pq_signer(&self) -> &Self {
        self
    }
}

/// The signing key for the local client's leaf in a group.
///
/// Regular groups use the user-level [`ClientSigningKey`]. The self-group uses the per-device
/// [`SelfGroupSigningKey`].
#[derive(Clone, Debug)]
pub enum LeafSigningKey {
    User(ClientSigningKey),
    SelfGroup(SelfGroupSigningKey),
}

#[derive(Debug, Error)]
pub enum LeafSigningKeyError {
    #[error(transparent)]
    Tls(#[from] tls_codec::Error),
    #[error(transparent)]
    Codec(#[from] codec::Error),
}

impl Deref for LeafSigningKey {
    type Target = SigningKey<ClientKeyType>;

    fn deref(&self) -> &Self::Target {
        match self {
            LeafSigningKey::User(key) => key,
            LeafSigningKey::SelfGroup(key) => key,
        }
    }
}

impl LeafSigningKey {
    /// The MLS credential to place in the leaf.
    pub fn mls_credential(&self) -> Result<Credential, LeafSigningKeyError> {
        match self {
            LeafSigningKey::User(key) => Ok(Credential::try_from(key.credential())?),
            LeafSigningKey::SelfGroup(key) => Ok(key.credential().to_credential()?),
        }
    }

    /// Room-policy identity of the leaf's owner, see
    /// [`LeafCredential::room_policy_identity`](super::LeafCredential::room_policy_identity).
    pub fn room_policy_identity(&self) -> RoomPolicyIdentity {
        match self {
            LeafSigningKey::User(key) => {
                RoomPolicyIdentity::User(key.credential().user_id().clone())
            }
            LeafSigningKey::SelfGroup(key) => {
                RoomPolicyIdentity::Client(key.credential().client_id())
            }
        }
    }
}

impl Signer for LeafSigningKey {
    fn sign(&self, payload: &[u8]) -> Result<Vec<u8>, SignerError> {
        match self {
            LeafSigningKey::User(key) => key.sign(payload),
            LeafSigningKey::SelfGroup(key) => key.sign(payload),
        }
    }

    fn signature_scheme(&self) -> SignatureScheme {
        match self {
            LeafSigningKey::User(key) => key.signature_scheme(),
            LeafSigningKey::SelfGroup(key) => key.signature_scheme(),
        }
    }
}

impl ApqSigner for LeafSigningKey {
    type TSigner = Self;
    type PqSigner = Self;

    fn t_signer(&self) -> &Self {
        self
    }

    fn pq_signer(&self) -> &Self {
        self
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
pub struct UsernameKeyType;

impl RawKey for UsernameKeyType {}

pub type UsernameSigningKey = SigningKey<UsernameKeyType>;

pub type UsernameVerifyingKey = VerifyingKey<UsernameKeyType>;

pub type UsernameSignature = Signature<UsernameKeyType>;

#[cfg(test)]
mod test {
    use mls_assist::openmls_traits::signatures::Signer;
    use uuid::Uuid;

    use crate::crypto::signatures::{
        DEFAULT_SIGNATURE_SCHEME, private_keys::VerifyingKeyBehaviour,
    };

    use super::SelfGroupSigningKey;

    #[test]
    fn self_group_signing_key_round_trips_client_id() {
        let client_id = Uuid::new_v4();
        let signing_key = SelfGroupSigningKey::generate(client_id).unwrap();
        assert_eq!(signing_key.credential().client_id(), client_id);
    }

    #[test]
    fn self_group_signing_key_signs_and_verifies() {
        let signing_key = SelfGroupSigningKey::generate(Uuid::new_v4()).unwrap();
        assert_eq!(signing_key.signature_scheme(), DEFAULT_SIGNATURE_SCHEME);

        let payload = b"payload to sign";
        let signature = signing_key.sign(payload).unwrap();
        signing_key
            .verifying_key()
            .verify(payload, &signature)
            .unwrap();
    }
}
