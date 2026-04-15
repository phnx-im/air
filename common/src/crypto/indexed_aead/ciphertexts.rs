// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::crypto::{
    aead::{AeadDecryptable, AeadEncryptable, Ciphertext},
    errors::{DecryptionError, EncryptionError},
};

use super::keys::{Index, IndexedAeadKey, IndexedKeyType};

/// A ciphertext that contains an index of the [`IndexedAeadKey`] used to
/// encrypt it.
#[derive(Debug, Serialize, Deserialize)]
pub struct IndexedCiphertext<KT, CT> {
    #[serde(bound = "")]
    key_index: Index<KT>,
    #[serde(bound = "")]
    ciphertext: Ciphertext<CT>,
}

impl<KT, CT> IndexedCiphertext<KT, CT> {
    pub fn from_parts(key_index: Index<KT>, ciphertext: Ciphertext<CT>) -> Self {
        Self {
            key_index,
            ciphertext,
        }
    }

    pub fn into_parts(self) -> (Index<KT>, Ciphertext<CT>) {
        (self.key_index, self.ciphertext)
    }

    /// Returns the index of the key used to encrypt this ciphertext.
    pub fn key_index(&self) -> &Index<KT> {
        &self.key_index
    }

    #[cfg(any(test, feature = "test_utils"))]
    pub fn dummy() -> Self {
        Self {
            key_index: Index::dummy(),
            ciphertext: Ciphertext::dummy(),
        }
    }
}

/// This trait allows payloads to be encrypted with an indexed key. The
/// resulting [`IndexedCiphertext`] contains the index of the key used to
/// encrypt it.
pub trait IndexEncryptable<KT: IndexedKeyType, CT>:
    AeadEncryptable<IndexedAeadKey<KT>, CT>
{
    fn encrypt_with_index(
        &self,
        key: &IndexedAeadKey<KT>,
    ) -> Result<IndexedCiphertext<KT, CT>, IndexEncryptionError> {
        let ciphertext = self.encrypt(key)?;
        let indexed_ciphertext = IndexedCiphertext {
            key_index: key.index().clone(),
            ciphertext,
        };
        Ok(indexed_ciphertext)
    }
}

/// This trait allows payloads to be decrypted with an indexed key. Decryption
/// will fail if the key index in the ciphertext does not match the key index of
/// the provided key.
pub trait IndexDecryptable<KT: IndexedKeyType, CT>:
    AeadDecryptable<IndexedAeadKey<KT>, CT>
{
    fn decrypt_with_index(
        key: &IndexedAeadKey<KT>,
        ciphertext: &IndexedCiphertext<KT, CT>,
    ) -> Result<Self, IndexDecryptionError> {
        if &ciphertext.key_index != key.index() {
            return Err(IndexDecryptionError::InvalidKeyIndex);
        }
        let plaintext = Self::decrypt(key, &ciphertext.ciphertext)?;
        Ok(plaintext)
    }
}

#[derive(Error, Debug)]
pub enum IndexEncryptionError {
    /// Encryption error
    #[error(transparent)]
    EncryptionError(#[from] EncryptionError),
    /// Invalid key index
    #[error("Invalid key index")]
    InvalidKeyIndex,
}

#[derive(Error, Debug)]
pub enum IndexDecryptionError {
    /// Encryption error
    #[error(transparent)]
    DecryptionError(#[from] DecryptionError),
    /// Invalid key index
    #[error("Invalid key index")]
    InvalidKeyIndex,
}

mod trait_impls {
    use sqlx::{
        Decode, Encode, Postgres, Type,
        encode::IsNull,
        error::BoxDynError,
        postgres::{
            PgTypeInfo, PgValueRef,
            types::{PgRecordDecoder, PgRecordEncoder},
        },
    };

    use super::*;

    impl<KT, CT> tls_codec::Size for IndexedCiphertext<KT, CT> {
        fn tls_serialized_len(&self) -> usize {
            self.key_index.tls_serialized_len() + self.ciphertext.tls_serialized_len()
        }
    }

    impl<KT, CT> tls_codec::Serialize for IndexedCiphertext<KT, CT> {
        fn tls_serialize<W: std::io::Write>(
            &self,
            writer: &mut W,
        ) -> Result<usize, tls_codec::Error> {
            let mut written = self.key_index.tls_serialize(writer)?;
            written += self.ciphertext.tls_serialize(writer)?;
            Ok(written)
        }
    }

    impl<KT, CT> tls_codec::DeserializeBytes for IndexedCiphertext<KT, CT> {
        fn tls_deserialize_bytes(bytes: &[u8]) -> Result<(Self, &[u8]), tls_codec::Error> {
            let (key_index, bytes) = Index::<KT>::tls_deserialize_bytes(bytes)?;
            let (ciphertext, bytes) = Ciphertext::<CT>::tls_deserialize_bytes(bytes)?;
            Ok((
                Self {
                    key_index,
                    ciphertext,
                },
                bytes,
            ))
        }
    }

    impl<KT, CT> Clone for IndexedCiphertext<KT, CT> {
        fn clone(&self) -> Self {
            Self {
                key_index: self.key_index.clone(),
                ciphertext: self.ciphertext.clone(),
            }
        }
    }

    impl<KT, CT> PartialEq for IndexedCiphertext<KT, CT> {
        fn eq(&self, other: &Self) -> bool {
            self.key_index == other.key_index && self.ciphertext == other.ciphertext
        }
    }

    impl<KT, CT> Eq for IndexedCiphertext<KT, CT> {}

    impl<KT, CT> Encode<'_, Postgres> for IndexedCiphertext<KT, CT> {
        fn encode_by_ref(
            &self,
            buf: &mut <Postgres as sqlx::Database>::ArgumentBuffer<'_>,
        ) -> Result<IsNull, BoxDynError> {
            let mut encoder = PgRecordEncoder::new(buf);
            encoder.encode(&self.ciphertext)?;
            encoder.encode(&self.key_index)?;
            encoder.finish();
            Result::Ok(IsNull::No)
        }
    }

    impl<KT, CT> Decode<'_, Postgres> for IndexedCiphertext<KT, CT> {
        fn decode(value: PgValueRef<'_>) -> Result<Self, BoxDynError> {
            let mut decoder = PgRecordDecoder::new(value)?;
            let ciphertext = decoder.try_decode::<Ciphertext<CT>>()?;
            let key_index = decoder.try_decode::<Index<KT>>()?;
            Result::Ok(IndexedCiphertext {
                key_index,
                ciphertext,
            })
        }
    }

    impl<KT, CT> Type<Postgres> for IndexedCiphertext<KT, CT> {
        fn type_info() -> PgTypeInfo {
            PgTypeInfo::with_name("indexed_ciphertext")
        }
    }
}
