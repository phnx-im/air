// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::marker::PhantomData;

use sha2::{Digest, Sha256};
use tls_codec::{Serialize, TlsDeserializeBytes, TlsSerialize, TlsSize};

use crate::{crypto::Labeled, identifiers::TlsStr};

pub type HashAlg = Sha256;
pub const HASH_SIZE: usize = 32;

#[derive(Debug, TlsSize, TlsSerialize)]
struct LabeledHashPayload<'a, T: Serialize> {
    operation: TlsStr<'a>,
    label: TlsStr<'a>,
    payload: T,
}

impl<T: Serialize + Labeled> LabeledHashPayload<'_, T> {
    fn new(payload: T) -> Self {
        Self {
            operation: TlsStr("Air Hash"),
            label: TlsStr(T::LABEL),
            payload,
        }
    }
}

#[derive(serde::Serialize, serde::Deserialize, TlsSize, TlsSerialize, TlsDeserializeBytes)]
#[serde(transparent)]
pub struct Hash<T: Labeled + Sized> {
    #[serde(with = "serde_bytes")]
    bytes: [u8; HASH_SIZE],
    _marker: PhantomData<T>,
}

impl<T: Labeled + Sized> Hash<T> {
    #[cfg(any(feature = "test_utils", test))]
    pub fn new_for_test(mut bytes: Vec<u8>) -> Self {
        // Pad until the length is exactly HASH_SIZE
        if bytes.len() < HASH_SIZE {
            bytes.resize(HASH_SIZE, 0);
        }
        // Ensure the length is exactly HASH_SIZE
        assert_eq!(
            bytes.len(),
            HASH_SIZE,
            "Hash must be exactly {HASH_SIZE} bytes long",
        );
        let mut hash_bytes = [0u8; HASH_SIZE];
        hash_bytes.copy_from_slice(&bytes[..HASH_SIZE]);
        Self {
            bytes: hash_bytes,
            _marker: PhantomData,
        }
    }

    pub fn as_bytes(&self) -> &[u8; HASH_SIZE] {
        &self.bytes
    }

    pub fn into_bytes(&self) -> [u8; HASH_SIZE] {
        self.bytes
    }

    pub fn from_bytes(bytes: [u8; HASH_SIZE]) -> Self {
        Self {
            bytes,
            _marker: PhantomData,
        }
    }
}

/// Trait for types that can be hashed
///
/// Default implementation requires the type (and references to it) to implement
/// `tls_codec::Serialize` and `Labeled`.
#[allow(private_bounds)]
pub trait Hashable: HashableHelper {
    /// Returns the hash of the data as a byte array.
    fn hash(&self) -> Hash<Self> {
        HashableHelper::hash(self)
    }
}

trait HashableHelper: Labeled + Sized {
    /// Returns the hash of the data as a byte array.
    fn hash(&self) -> Hash<Self>;
}

impl<T: Labeled + Sized> HashableHelper for T
where
    for<'a> &'a T: Serialize,
{
    fn hash(&self) -> Hash<T> {
        let mut hasher = HashAlg::new();
        let serialization_result = LabeledHashPayload::new(self).tls_serialize(&mut hasher);
        debug_assert!(serialization_result.is_ok(), "Serialization failed");
        let serialized_labeled_payload = serialization_result.unwrap_or_default();

        let hash_bytes = hasher.finalize();
        Hash {
            bytes: hash_bytes.into(),
            _marker: PhantomData,
        }
    }
}

mod trait_impls {
    use sqlx::{Database, Encode, Type};

    use super::*;

    impl<T: Labeled> std::fmt::Debug for Hash<T> {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "Hash<{}>({:?})", T::LABEL, self.bytes)
        }
    }

    impl<T: Labeled> PartialEq for Hash<T> {
        fn eq(&self, other: &Self) -> bool {
            self.bytes == other.bytes
        }
    }

    impl<T: Labeled> Eq for Hash<T> {}

    impl<T: Labeled> std::hash::Hash for Hash<T> {
        fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
            self.bytes.hash(state);
        }
    }

    impl<T: Labeled> Clone for Hash<T> {
        fn clone(&self) -> Self {
            *self
        }
    }

    impl<T: Labeled> Copy for Hash<T> {}

    impl<T: Labeled, DB: Database> Type<DB> for Hash<T>
    where
        Vec<u8>: Type<DB>,
    {
        fn type_info() -> <DB as Database>::TypeInfo {
            <Vec<u8> as Type<DB>>::type_info()
        }
    }

    impl<'a, T: Labeled, DB: Database> Encode<'a, DB> for Hash<T>
    where
        Vec<u8>: Encode<'a, DB>,
    {
        fn encode_by_ref(
            &self,
            buf: &mut <DB as Database>::ArgumentBuffer<'a>,
        ) -> Result<sqlx::encode::IsNull, sqlx::error::BoxDynError> {
            let bytes = self.as_bytes().to_vec();
            Encode::<DB>::encode(&bytes, buf)
        }
    }

    impl<'a, T: Labeled, DB: Database> sqlx::Decode<'a, DB> for Hash<T>
    where
        &'a [u8]: sqlx::Decode<'a, DB>,
    {
        fn decode(value: <DB as Database>::ValueRef<'a>) -> Result<Self, sqlx::error::BoxDynError> {
            let byte_slice = <&[u8]>::decode(value)?;
            let bytes: [u8; HASH_SIZE] = byte_slice
                .try_into()
                .map_err(|_| sqlx::error::BoxDynError::from("Invalid hash length"))?;
            Ok(Self {
                bytes,
                _marker: PhantomData,
            })
        }
    }
}
