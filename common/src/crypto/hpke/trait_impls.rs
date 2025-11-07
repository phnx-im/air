// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use sqlx::{
    Database, Postgres, Sqlite, Type,
    decode::Decode,
    encode::{Encode, IsNull},
    error::BoxDynError,
    postgres::{
        PgArgumentBuffer, PgHasArrayType, PgTypeInfo, PgValueRef,
        types::{PgRecordDecoder, PgRecordEncoder},
    },
    sqlite::SqliteTypeInfo,
};

use crate::{codec::PersistenceCodec, crypto::secrets::SecretBytes};

use super::{DecryptionKey, EncryptionKey};

impl<'q, KT, DB: Database> Encode<'q, DB> for EncryptionKey<KT>
where
    Vec<u8>: Encode<'q, DB>,
{
    fn encode_by_ref(
        &self,
        buf: &mut <DB as Database>::ArgumentBuffer<'q>,
    ) -> Result<IsNull, BoxDynError> {
        <Vec<u8> as Encode<'q, DB>>::encode_by_ref(&self.key, buf)
    }
}

impl<'r, KT, DB: Database> Decode<'r, DB> for EncryptionKey<KT>
where
    Vec<u8>: Decode<'r, DB>,
{
    fn decode(value: <DB as Database>::ValueRef<'r>) -> Result<Self, BoxDynError> {
        <Vec<u8> as Decode<'r, DB>>::decode(value).map(Self::new)
    }
}

impl<KT, DB: Database> Type<DB> for EncryptionKey<KT>
where
    Vec<u8>: Type<DB>,
{
    fn type_info() -> DB::TypeInfo {
        <Vec<u8> as Type<DB>>::type_info()
    }
}

impl<KT> PgHasArrayType for EncryptionKey<KT>
where
    Vec<u8>: PgHasArrayType,
{
    fn array_type_info() -> PgTypeInfo {
        <Vec<u8> as PgHasArrayType>::array_type_info()
    }
}

impl<KT> Encode<'_, Postgres> for DecryptionKey<KT> {
    fn encode_by_ref(&self, buf: &mut PgArgumentBuffer) -> Result<IsNull, BoxDynError> {
        let mut encoder = PgRecordEncoder::new(buf);
        encoder.encode(&self.decryption_key)?;
        encoder.encode(&self.encryption_key)?;
        encoder.finish();
        Result::Ok(IsNull::No)
    }
}

impl<'r, KT> Decode<'r, Postgres> for DecryptionKey<KT> {
    fn decode(value: PgValueRef<'r>) -> Result<Self, BoxDynError> {
        let mut decoder = PgRecordDecoder::new(value)?;
        let decryption_key = decoder.try_decode::<SecretBytes>()?;
        let encryption_key = decoder.try_decode::<EncryptionKey<KT>>()?;
        Result::Ok(DecryptionKey {
            decryption_key,
            encryption_key,
        })
    }
}

impl<KT> Type<Postgres> for DecryptionKey<KT> {
    fn type_info() -> PgTypeInfo {
        PgTypeInfo::with_name("decryption_key_data")
    }
}

impl<KT> PgHasArrayType for DecryptionKey<KT> {
    fn array_type_info() -> PgTypeInfo {
        PgTypeInfo::array_of("decryption_key_data")
    }
}

impl<KT> Type<Sqlite> for DecryptionKey<KT> {
    fn type_info() -> SqliteTypeInfo {
        <Vec<u8> as Type<Sqlite>>::type_info()
    }
}

impl<KT> Encode<'_, Sqlite> for DecryptionKey<KT> {
    fn encode_by_ref(
        &self,
        buf: &mut <Sqlite as Database>::ArgumentBuffer<'_>,
    ) -> Result<IsNull, BoxDynError> {
        let bytes = PersistenceCodec::to_vec(self).map_err(BoxDynError::from)?;
        Encode::<Sqlite>::encode(bytes, buf)
    }
}

impl<'r, KT> Decode<'r, Sqlite> for DecryptionKey<KT> {
    fn decode(value: <Sqlite as Database>::ValueRef<'r>) -> Result<Self, BoxDynError> {
        let bytes: &[u8] = Decode::<Sqlite>::decode(value)?;
        PersistenceCodec::from_slice(bytes).map_err(BoxDynError::from)
    }
}

impl<KT> tls_codec::Size for EncryptionKey<KT> {
    fn tls_serialized_len(&self) -> usize {
        self.key.tls_serialized_len()
    }
}

impl<KT> tls_codec::Serialize for EncryptionKey<KT> {
    fn tls_serialize<W: std::io::Write>(&self, writer: &mut W) -> Result<usize, tls_codec::Error> {
        self.key.tls_serialize(writer)
    }
}

impl<KT> tls_codec::DeserializeBytes for EncryptionKey<KT> {
    fn tls_deserialize_bytes(bytes: &[u8]) -> Result<(Self, &[u8]), tls_codec::Error> {
        let (key, remaining) = Vec::<u8>::tls_deserialize_bytes(bytes)?;
        Ok((Self::new(key), remaining))
    }
}

impl<KT> PartialEq for EncryptionKey<KT> {
    fn eq(&self, other: &Self) -> bool {
        self.key == other.key
    }
}

impl<KT> Eq for EncryptionKey<KT> {}

impl<KT> Clone for EncryptionKey<KT> {
    fn clone(&self) -> Self {
        Self::new(self.key.clone())
    }
}

impl<KT> Clone for DecryptionKey<KT> {
    fn clone(&self) -> Self {
        Self {
            decryption_key: self.decryption_key.clone(),
            encryption_key: self.encryption_key.clone(),
        }
    }
}
