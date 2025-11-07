// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use mls_assist::{
    openmls::{
        key_packages::InitKey,
        prelude::{HpkeAeadType, HpkeConfig, HpkeKdfType, HpkeKemType, HpkePublicKey},
    },
    openmls_rust_crypto::OpenMlsRustCrypto,
    openmls_traits::{
        OpenMlsProvider,
        crypto::OpenMlsCrypto,
        random::OpenMlsRand,
        types::{HpkeCiphertext, HpkePrivateKey},
    },
};
use serde::{Deserialize, Serialize};
use tls_codec::Serialize as TlsSerializeTrait;
use tracing::error;

use crate::identifiers::{ClientConfig, SealedClientReference};

use super::{
    RawKey,
    errors::{DecryptionError, EncryptionError, RandomnessError},
    secrets::SecretBytes,
};

pub mod trait_impls;

#[derive(Debug, Serialize, Deserialize)]
#[serde(transparent)]
pub struct EncryptionKey<KT> {
    #[serde(with = "serde_bytes")]
    key: Vec<u8>,
    _type: std::marker::PhantomData<KT>,
}

pub const HPKE_CONFIG: HpkeConfig = HpkeConfig(
    HpkeKemType::DhKem25519,
    HpkeKdfType::HkdfSha256,
    HpkeAeadType::AesGcm256,
);

impl<KT> EncryptionKey<KT> {
    fn new(bytes: Vec<u8>) -> Self {
        Self {
            key: bytes,
            _type: std::marker::PhantomData,
        }
    }

    /// Encrypt the given plaintext using this key.
    pub(crate) fn encrypt(&self, info: &[u8], aad: &[u8], plain_txt: &[u8]) -> HpkeCiphertext {
        let rust_crypto = OpenMlsRustCrypto::default();
        rust_crypto
            .crypto()
            .hpke_seal(HPKE_CONFIG, &self.key, info, aad, plain_txt)
            // TODO: get rid of unwrap
            .unwrap()
    }

    #[cfg(any(test, feature = "test_utils"))]
    pub fn new_for_test(bytes: Vec<u8>) -> Self {
        Self::new(bytes)
    }
}

impl<KT: RawKey> EncryptionKey<KT> {
    pub fn into_bytes(self) -> Vec<u8> {
        self.key
    }

    pub fn from_bytes(bytes: Vec<u8>) -> Self {
        Self {
            key: bytes,
            _type: std::marker::PhantomData,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DecryptionKey<KT> {
    decryption_key: SecretBytes,
    #[serde(bound = "")]
    encryption_key: EncryptionKey<KT>,
}

#[cfg(any(test, feature = "test_utils"))]
impl<KT> PartialEq for DecryptionKey<KT> {
    fn eq(&self, other: &Self) -> bool {
        let decryption_key: &[u8] = self.decryption_key.as_ref();
        let other_decryption_key: &[u8] = other.decryption_key.as_ref();
        decryption_key == other_decryption_key && self.encryption_key == other.encryption_key
    }
}

#[cfg(any(test, feature = "test_utils"))]
impl<KT> Eq for DecryptionKey<KT> {}

impl<KT> DecryptionKey<KT> {
    pub fn new(decryption_key: HpkePrivateKey, encryption_key: EncryptionKey<KT>) -> Self {
        Self {
            decryption_key: decryption_key.as_ref().to_vec().into(),
            encryption_key,
        }
    }

    pub fn decrypt(
        &self,
        info: &[u8],
        aad: &[u8],
        ct: &HpkeCiphertext,
    ) -> Result<Vec<u8>, DecryptionError> {
        let rust_crypto = OpenMlsRustCrypto::default();
        rust_crypto
            .crypto()
            .hpke_open(HPKE_CONFIG, ct, &self.decryption_key, info, aad)
            .map_err(|_| DecryptionError::DecryptionError)
    }

    pub fn generate() -> Result<Self, RandomnessError> {
        let provider = OpenMlsRustCrypto::default();
        let key_seed = provider
            .rand()
            .random_array::<32>()
            .map_err(|_| RandomnessError::InsufficientRandomness)?;
        let keypair = provider
            .crypto()
            .derive_hpke_keypair(HPKE_CONFIG, &key_seed)
            .map_err(|_| RandomnessError::InsufficientRandomness)?;
        Ok(Self::new(
            keypair.private,
            EncryptionKey::new(keypair.public),
        ))
    }

    pub fn encryption_key(&self) -> &EncryptionKey<KT> {
        &self.encryption_key
    }
}

// TODO: We might want to properly fix AAD and Info at some point.
pub trait HpkeEncryptable<KT, HpkeCiphertextType: From<HpkeCiphertext>>:
    tls_codec::Serialize
{
    /// Encrypts the data with the given encryption key.
    fn encrypt(
        &self,
        encryption_key: &EncryptionKey<KT>,
        info: &[u8],
        aad: &[u8],
    ) -> HpkeCiphertextType {
        // Hiding a LibraryError behind an empty vec.
        let plain_txt = self.tls_serialize_detached().unwrap_or_default();
        encryption_key.encrypt(info, aad, &plain_txt).into()
    }
}

pub trait HpkeDecryptable<KT, HpkeCiphertextType: AsRef<HpkeCiphertext>>:
    tls_codec::DeserializeBytes + Sized
{
    fn decrypt(
        ct: HpkeCiphertextType,
        decryption_key: &DecryptionKey<KT>,
        info: &[u8],
        aad: &[u8],
    ) -> Result<Self, DecryptionError> {
        let plaintext = decryption_key.decrypt(info, aad, ct.as_ref())?;
        Self::tls_deserialize_exact_bytes(&plaintext).map_err(|e| {
            error!(%e, "Error deserializing decrypted data");
            DecryptionError::DeserializationError
        })
    }
}

pub struct JoinerInfoKeyType;
pub type JoinerInfoEncryptionKey = EncryptionKey<JoinerInfoKeyType>;

// We need this From trait, because we have to work with the hpke init key from
// the KeyPackage, which we get as HpkePublicKey from OpenMLS.
impl From<HpkePublicKey> for JoinerInfoEncryptionKey {
    fn from(value: HpkePublicKey) -> Self {
        Self::new(value.as_slice().to_vec())
    }
}

impl From<InitKey> for JoinerInfoEncryptionKey {
    fn from(value: InitKey) -> Self {
        Self::new(value.as_slice().to_vec())
    }
}

pub type JoinerInfoDecryptionKey = DecryptionKey<JoinerInfoKeyType>;

// We need this From trait, because we have to work with the hpke init key from
// the KeyPackage, which we get as HpkePrivateKey and HpkePublicKey from
// OpenMLS.
impl From<(HpkePrivateKey, InitKey)> for JoinerInfoDecryptionKey {
    fn from((sk, init_key): (HpkePrivateKey, InitKey)) -> Self {
        let vec: Vec<u8> = init_key.key().as_slice().to_vec();
        DecryptionKey::new(sk, EncryptionKey::new(vec))
    }
}

#[derive(Debug)]
pub struct ClientIdKeyType;
pub type ClientIdEncryptionKey = EncryptionKey<ClientIdKeyType>;

impl RawKey for ClientIdKeyType {}

impl ClientIdEncryptionKey {
    pub fn seal_client_config(
        &self,
        client_config: ClientConfig,
    ) -> Result<SealedClientReference, EncryptionError> {
        let bytes = client_config
            .tls_serialize_detached()
            .map_err(|_| EncryptionError::SerializationError)?;
        let ciphertext = self.encrypt(&[], &[], &bytes);
        Ok(SealedClientReference { ciphertext })
    }
}

pub type ClientIdDecryptionKey = DecryptionKey<ClientIdKeyType>;
