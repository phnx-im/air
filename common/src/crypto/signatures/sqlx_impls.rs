// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! SQLx traits for signing keys.
//!
//! The `Type` derive macro couldn't be used because the sqlx(type_name) macro doesn't
//! support generic types.

use sqlx::{
    Database, Decode, Encode, Postgres, Type,
    encode::IsNull,
    error::BoxDynError,
    postgres::{
        PgArgumentBuffer, PgHasArrayType, PgTypeInfo, PgValueRef,
        types::{PgRecordDecoder, PgRecordEncoder},
    },
};

use crate::crypto::secrets::SecretBytes;

use super::private_keys::{SigningKey, VerifyingKey};

impl<KT, DB: Database> Type<DB> for VerifyingKey<KT>
where
    Vec<u8>: Type<DB>,
{
    fn type_info() -> <DB as Database>::TypeInfo {
        <Vec<u8> as Type<DB>>::type_info()
    }
}

impl<'a, KT, DB: Database> Encode<'a, DB> for VerifyingKey<KT>
where
    Vec<u8>: Encode<'a, DB>,
{
    fn encode_by_ref(
        &self,
        buf: &mut <DB as Database>::ArgumentBuffer,
    ) -> Result<IsNull, BoxDynError> {
        <Vec<u8> as Encode<DB>>::encode_by_ref(&self.key, buf)
    }
}

impl<'r, KT, DB: Database> Decode<'r, DB> for VerifyingKey<KT>
where
    Vec<u8>: Decode<'r, DB>,
{
    fn decode(value: <DB as Database>::ValueRef<'r>) -> Result<Self, BoxDynError> {
        <Vec<u8> as Decode<DB>>::decode(value).map(Self::new)
    }
}

impl<KT> Encode<'_, Postgres> for SigningKey<KT> {
    fn encode_by_ref(&self, buf: &mut PgArgumentBuffer) -> Result<IsNull, BoxDynError> {
        let mut encoder = PgRecordEncoder::new(buf);
        encoder.encode(&self.signing_key)?;
        encoder.encode(&self.verifying_key)?;
        encoder.finish();
        Result::Ok(IsNull::No)
    }
}

impl<'r, KT> Decode<'r, Postgres> for SigningKey<KT> {
    fn decode(value: PgValueRef<'r>) -> Result<Self, BoxDynError> {
        let mut decoder = PgRecordDecoder::new(value)?;
        let signing_key = decoder.try_decode::<SecretBytes>()?;
        let verifying_key = decoder.try_decode::<VerifyingKey<KT>>()?;
        Result::Ok(SigningKey {
            signing_key,
            verifying_key,
        })
    }
}

impl<KT> Type<Postgres> for SigningKey<KT> {
    fn type_info() -> PgTypeInfo {
        PgTypeInfo::with_name("signing_key_data")
    }
}

impl<KT> PgHasArrayType for SigningKey<KT> {
    fn array_type_info() -> PgTypeInfo {
        PgTypeInfo::array_of("signing_key_data")
    }
}

impl<KT> tls_codec::Size for VerifyingKey<KT> {
    fn tls_serialized_len(&self) -> usize {
        self.key.tls_serialized_len()
    }
}

impl<KT> tls_codec::Serialize for VerifyingKey<KT> {
    fn tls_serialize<W: std::io::Write>(&self, writer: &mut W) -> Result<usize, tls_codec::Error> {
        self.key.tls_serialize(writer)
    }
}

impl<KT> tls_codec::DeserializeBytes for VerifyingKey<KT> {
    fn tls_deserialize_bytes(bytes: &[u8]) -> Result<(Self, &[u8]), tls_codec::Error> {
        let (key, remaining) = Vec::<u8>::tls_deserialize_bytes(bytes)?;
        Ok((Self::new(key), remaining))
    }
}

impl<KT> Clone for VerifyingKey<KT> {
    fn clone(&self) -> Self {
        Self::new(self.key.clone())
    }
}

impl<KT> PartialEq for VerifyingKey<KT> {
    fn eq(&self, other: &Self) -> bool {
        self.key == other.key
    }
}

impl<KT> Eq for VerifyingKey<KT> {}
