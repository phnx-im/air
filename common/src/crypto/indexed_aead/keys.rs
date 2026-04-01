// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::marker::PhantomData;

use crate::{
    LibraryError,
    crypto::{
        RawKey,
        aead::{
            AEAD_KEY_SIZE, AeadEncryptable, AeadDecryptable, AeadKey,
            keys::{EncryptedUserProfileKey, EncryptedUserProfileKeyCtype, IdentityLinkWrapperKey},
        },
        errors::{DecryptionError, EncryptionError, RandomnessError},
        kdf::{KDF_KEY_SIZE, KdfDerivable, KdfKey},
        secrets::Secret,
    },
    identifiers::UserId,
};
use serde::{Deserialize, Serialize};
use tls_codec::{TlsSerialize, TlsSize};
use tracing::error;

mod trait_impls;

/// Marker trait for indexed keys
pub trait IndexedKeyType {
    type DerivationContext<'a>: tls_codec::Serialize + Clone;

    // The `LABEL` constant is used to identify the key type in the database.
    const LABEL: &'static str;
}

/// Marker trait for keys that can be randomly generated
pub trait RandomlyGeneratable {}

// Dummy wrapper type to avoid orphan problem.
#[derive(Default)]
pub struct KeyTypeInstance<KT: IndexedKeyType>(PhantomData<KT>);

impl<KT: IndexedKeyType> KeyTypeInstance<KT> {
    pub fn new() -> Self {
        Self(PhantomData)
    }
}

/// A wrapper type for secrets that are associated with a specific key type.
#[derive(Debug, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TypedSecret<KT, ST, const SIZE: usize> {
    secret: Secret<SIZE>,
    _type: PhantomData<(KT, ST)>,
}

#[derive(Debug)]
pub struct BaseSecretType;
#[derive(Debug)]
pub struct KeySecretType;
#[derive(Debug)]
pub struct IndexSecretType;

/// A base secret is meant to derive a key and an index for the key type `KT`.
pub type BaseSecret<KT> = TypedSecret<KT, BaseSecretType, KDF_KEY_SIZE>;
/// A key is derived from the base secret. Other traits like the `AeadKey` trait
/// can be implemented to allow these keys to be used.
pub type Key<KT> = TypedSecret<KT, KeySecretType, AEAD_KEY_SIZE>;
/// An index is derived from the base secret. It is used to identify the key
/// of the same key type `KT` derived from the same [`BaseSecret`].
pub type Index<KT> = TypedSecret<KT, IndexSecretType, AEAD_KEY_SIZE>;

impl<KT: RawKey> Key<KT> {
    pub fn from_bytes(bytes: [u8; AEAD_KEY_SIZE]) -> Self {
        Self {
            secret: Secret::from(bytes),
            _type: PhantomData,
        }
    }

    pub fn into_bytes(self) -> [u8; AEAD_KEY_SIZE] {
        self.secret.into_secret()
    }
}

pub trait RawIndex {}

impl<KT: RawIndex> Index<KT> {
    pub fn from_bytes(bytes: [u8; AEAD_KEY_SIZE]) -> Self {
        Self {
            secret: Secret::from(bytes),
            _type: PhantomData,
        }
    }

    pub fn into_bytes(self) -> [u8; AEAD_KEY_SIZE] {
        self.secret.into_secret()
    }
}

impl<KT> BaseSecret<KT> {
    pub fn random() -> Result<Self, RandomnessError> {
        let value = Secret::<KDF_KEY_SIZE>::random()?;
        Ok(Self {
            secret: value,
            _type: PhantomData,
        })
    }
}

impl<KT> AsRef<Secret<KDF_KEY_SIZE>> for BaseSecret<KT> {
    fn as_ref(&self) -> &Secret<KDF_KEY_SIZE> {
        &self.secret
    }
}

impl<KT> KdfKey for BaseSecret<KT> {
    const ADDITIONAL_LABEL: &'static str = "indexed key base secret";
}

impl<KT, ST, const LENGTH: usize> From<Secret<LENGTH>> for TypedSecret<KT, ST, LENGTH> {
    fn from(value: Secret<LENGTH>) -> Self {
        Self {
            secret: value,
            _type: PhantomData,
        }
    }
}

impl<KT> Index<KT> {
    #[cfg(any(test, feature = "test_utils"))]
    pub fn dummy() -> Self {
        Self {
            secret: Secret::random().unwrap(),
            _type: PhantomData,
        }
    }
}

impl<KT: RandomlyGeneratable> Key<KT> {
    pub fn random() -> Result<Self, RandomnessError> {
        let value = Secret::<AEAD_KEY_SIZE>::random()?;
        Ok(Self {
            secret: value,
            _type: PhantomData,
        })
    }
}

impl<KT> AsRef<Secret<AEAD_KEY_SIZE>> for Key<KT> {
    fn as_ref(&self) -> &Secret<AEAD_KEY_SIZE> {
        &self.secret
    }
}

#[derive(TlsSerialize, TlsSize)]
struct DerivationContext<'a, KT: IndexedKeyType> {
    context: KT::DerivationContext<'a>,
    key_type_instance: KeyTypeInstance<KT>,
}

impl<KT: IndexedKeyType> KdfDerivable<BaseSecret<KT>, DerivationContext<'_, KT>, AEAD_KEY_SIZE>
    for Key<KT>
{
    const LABEL: &'static str = "key";
}

impl<KT: IndexedKeyType> KdfDerivable<BaseSecret<KT>, DerivationContext<'_, KT>, AEAD_KEY_SIZE>
    for Index<KT>
{
    const LABEL: &'static str = "index";
}

/// An [`IndexedAeadKey`] is an indexed key that can be derive from a base
/// secret. It implements the `AeadKey` trait.
#[derive(Serialize, Deserialize, Debug)]
pub struct IndexedAeadKey<KT> {
    base_secret: BaseSecret<KT>,
    key: Key<KT>,
    index: Index<KT>,
}

impl<KT: IndexedKeyType> IndexedAeadKey<KT> {
    pub fn from_parts(base_secret: BaseSecret<KT>, key: Key<KT>, index: Index<KT>) -> Self {
        Self {
            base_secret,
            key,
            index,
        }
    }

    pub fn from_base_secret(
        base_secret: BaseSecret<KT>,
        context: KT::DerivationContext<'_>,
    ) -> Result<Self, LibraryError> {
        let derive_context = DerivationContext {
            context,
            key_type_instance: KeyTypeInstance::<KT>::new(),
        };
        let key =
            <Key<KT> as KdfDerivable<_, _, AEAD_KEY_SIZE>>::derive(&base_secret, &derive_context)?;
        let index = Index::derive(&base_secret, &derive_context)?;
        Ok(Self {
            base_secret,
            key,
            index,
        })
    }

    pub fn base_secret(&self) -> &BaseSecret<KT> {
        &self.base_secret
    }

    pub fn index(&self) -> &Index<KT> {
        &self.index
    }

    pub fn key(&self) -> &Key<KT> {
        &self.key
    }
}

impl<KT> AsRef<Secret<AEAD_KEY_SIZE>> for IndexedAeadKey<KT> {
    fn as_ref(&self) -> &Secret<AEAD_KEY_SIZE> {
        &self.key.secret
    }
}

impl<KT> AeadKey for IndexedAeadKey<KT> {}

// User profile key

#[derive(Debug, Serialize, Deserialize)]
pub struct UserProfileKeyType;

impl IndexedKeyType for UserProfileKeyType {
    type DerivationContext<'a> = &'a UserId;

    const LABEL: &'static str = "user_profile_key";
}

pub type UserProfileKeyIndex = Index<UserProfileKeyType>;

impl RawIndex for UserProfileKeyType {}

pub type UserProfileBaseSecret = BaseSecret<UserProfileKeyType>;

pub type UserProfileKey = IndexedAeadKey<UserProfileKeyType>;

impl UserProfileKey {
    pub fn random(user_id: &UserId) -> Result<Self, RandomnessError> {
        let base_secret = BaseSecret::random()?;
        Self::from_base_secret(base_secret, user_id).map_err(|e| {
            error!(error = %e, "Key derivation error");
            RandomnessError::InsufficientRandomness
        })
    }

    pub fn encrypt(
        &self,
        wrapper_key: &IdentityLinkWrapperKey,
        user_id: &UserId,
    ) -> Result<EncryptedUserProfileKey, EncryptionError> {
        self.base_secret.encrypt_with_aad(wrapper_key, user_id)
    }

    pub fn decrypt(
        wrapper_key: &IdentityLinkWrapperKey,
        encrypted_key: &EncryptedUserProfileKey,
        user_id: &UserId,
    ) -> Result<Self, DecryptionError> {
        let base_secret = BaseSecret::decrypt_with_aad(wrapper_key, encrypted_key, user_id)?;
        Self::from_base_secret(base_secret, user_id).map_err(|e| {
            error!(error = %e, "Key derivation error");
            DecryptionError::DecryptionError
        })
    }
}

impl AeadEncryptable<IdentityLinkWrapperKey, EncryptedUserProfileKeyCtype>
    for UserProfileBaseSecret
{
}
impl AeadDecryptable<IdentityLinkWrapperKey, EncryptedUserProfileKeyCtype>
    for UserProfileBaseSecret
{
}
