// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::marker::PhantomData;

use crate::crypto::secrets::Secret;
use sqlx::{
    Database, Decode, Encode, Sqlite, Type, encode::IsNull, error::BoxDynError,
    sqlite::SqliteTypeInfo,
};

use super::{BaseSecret, Index, IndexedAeadKey, IndexedKeyType, Key, KeyTypeInstance, TypedSecret};

impl<'q, KT: IndexedKeyType> Encode<'q, Sqlite> for KeyTypeInstance<KT> {
    fn encode_by_ref(
        &self,
        buf: &mut <Sqlite as Database>::ArgumentBuffer,
    ) -> Result<IsNull, BoxDynError> {
        <&str as Encode<Sqlite>>::encode_by_ref(&KT::LABEL, buf)
    }
}

impl<'r, KT: IndexedKeyType> Decode<'r, Sqlite> for KeyTypeInstance<KT> {
    fn decode(value: <Sqlite as Database>::ValueRef<'r>) -> Result<Self, BoxDynError> {
        let label: &str = Decode::<Sqlite>::decode(value)?;
        if label != KT::LABEL {
            return Err(BoxDynError::from(format!(
                "Invalid key type label: expected {}, got {}",
                KT::LABEL,
                label
            )));
        }
        Ok(Self(PhantomData))
    }
}

impl<KT: IndexedKeyType> tls_codec::Size for KeyTypeInstance<KT> {
    fn tls_serialized_len(&self) -> usize {
        KT::LABEL.len()
    }
}

impl<KT: IndexedKeyType> tls_codec::Serialize for KeyTypeInstance<KT> {
    fn tls_serialize<W: std::io::Write>(&self, writer: &mut W) -> Result<usize, tls_codec::Error> {
        let label = KT::LABEL.as_bytes();
        let written = writer.write(label)?;
        Ok(written)
    }
}

impl<KT: IndexedKeyType> Type<Sqlite> for KeyTypeInstance<KT> {
    fn type_info() -> SqliteTypeInfo {
        <&str as Type<Sqlite>>::type_info()
    }
}

impl<'q, KT, ST, const SIZE: usize, DB: Database> Encode<'q, DB> for TypedSecret<KT, ST, SIZE>
where
    Box<[u8]>: Encode<'q, DB>,
{
    fn encode_by_ref(
        &self,
        buf: &mut <DB as Database>::ArgumentBuffer,
    ) -> Result<IsNull, BoxDynError> {
        self.secret.encode_by_ref(buf)
    }
}

impl<KT, ST, const SIZE: usize, DB: Database> Type<DB> for TypedSecret<KT, ST, SIZE>
where
    Secret<SIZE>: Type<DB>,
{
    fn type_info() -> <DB as Database>::TypeInfo {
        <Secret<SIZE> as Type<DB>>::type_info()
    }
}

impl<'r, KT, ST, const SIZE: usize, DB: Database> Decode<'r, DB> for TypedSecret<KT, ST, SIZE>
where
    &'r [u8]: Decode<'r, DB>,
{
    fn decode(value: <DB as Database>::ValueRef<'r>) -> Result<Self, BoxDynError> {
        let secret: Secret<SIZE> = Decode::<DB>::decode(value)?;
        Ok(Self {
            secret,
            _type: PhantomData,
        })
    }
}

impl<KT, ST, const SIZE: usize> tls_codec::Size for TypedSecret<KT, ST, SIZE> {
    fn tls_serialized_len(&self) -> usize {
        self.secret.tls_serialized_len()
    }
}

impl<KT, ST, const SIZE: usize> tls_codec::Serialize for TypedSecret<KT, ST, SIZE> {
    fn tls_serialize<W: std::io::Write>(&self, writer: &mut W) -> Result<usize, tls_codec::Error> {
        self.secret.tls_serialize(writer)
    }
}

impl<KT, ST, const SIZE: usize> tls_codec::DeserializeBytes for TypedSecret<KT, ST, SIZE> {
    fn tls_deserialize_bytes(bytes: &[u8]) -> Result<(Self, &[u8]), tls_codec::Error> {
        let (secret, remaining) = Secret::<SIZE>::tls_deserialize_bytes(bytes)?;
        Ok((
            Self {
                secret,
                _type: PhantomData,
            },
            remaining,
        ))
    }
}

impl<KT, ST, const SIZE: usize> Clone for TypedSecret<KT, ST, SIZE> {
    fn clone(&self) -> Self {
        Self {
            secret: self.secret.clone(),
            _type: PhantomData,
        }
    }
}

impl<KT, ST, const SIZE: usize> PartialEq for TypedSecret<KT, ST, SIZE> {
    fn eq(&self, other: &Self) -> bool {
        self.secret == other.secret
    }
}

impl<KT, ST, const SIZE: usize> Eq for TypedSecret<KT, ST, SIZE> {}

impl<KT> tls_codec::Size for IndexedAeadKey<KT> {
    fn tls_serialized_len(&self) -> usize {
        self.base_secret.tls_serialized_len()
            + self.key.tls_serialized_len()
            + self.index.tls_serialized_len()
    }
}

impl<KT> tls_codec::Serialize for IndexedAeadKey<KT> {
    fn tls_serialize<W: std::io::Write>(&self, writer: &mut W) -> Result<usize, tls_codec::Error> {
        let mut written = self.base_secret.tls_serialize(writer)?;
        written += self.key.tls_serialize(writer)?;
        written += self.index.tls_serialize(writer)?;
        Ok(written)
    }
}

impl<KT> tls_codec::DeserializeBytes for IndexedAeadKey<KT> {
    fn tls_deserialize_bytes(bytes: &[u8]) -> Result<(Self, &[u8]), tls_codec::Error> {
        let (base_secret, bytes) = BaseSecret::<KT>::tls_deserialize_bytes(bytes)?;
        let (key, bytes) = Key::<KT>::tls_deserialize_bytes(bytes)?;
        let (index, bytes) = Index::<KT>::tls_deserialize_bytes(bytes)?;
        Ok((
            Self {
                base_secret,
                key,
                index,
            },
            bytes,
        ))
    }
}

impl<KT> Clone for IndexedAeadKey<KT> {
    fn clone(&self) -> Self {
        Self {
            base_secret: self.base_secret.clone(),
            key: self.key.clone(),
            index: self.index.clone(),
        }
    }
}
impl<KT> PartialEq for IndexedAeadKey<KT> {
    fn eq(&self, other: &Self) -> bool {
        self.base_secret == other.base_secret && self.key == other.key && self.index == other.index
    }
}
impl<KT> Eq for IndexedAeadKey<KT> {}
