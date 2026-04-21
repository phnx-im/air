// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::marker::PhantomData;

use sqlx::{Postgres, postgres::PgHasArrayType};

use super::{AeadCiphertext, Ciphertext};

impl<CT> sqlx::Type<Postgres> for Ciphertext<CT> {
    fn type_info() -> <Postgres as sqlx::Database>::TypeInfo {
        <AeadCiphertext as sqlx::Type<Postgres>>::type_info()
    }
}

impl<CT> PgHasArrayType for Ciphertext<CT> {
    fn array_type_info() -> <Postgres as sqlx::Database>::TypeInfo {
        <AeadCiphertext as PgHasArrayType>::array_type_info()
    }
}

impl<CT> sqlx::Encode<'_, Postgres> for Ciphertext<CT> {
    fn encode_by_ref(
        &self,
        buf: &mut <Postgres as sqlx::Database>::ArgumentBuffer<'_>,
    ) -> Result<sqlx::encode::IsNull, sqlx::error::BoxDynError> {
        sqlx::Encode::<Postgres>::encode(&self.ct, buf)
    }
}

impl<CT> sqlx::Decode<'_, Postgres> for Ciphertext<CT> {
    fn decode(
        value: <Postgres as sqlx::Database>::ValueRef<'_>,
    ) -> Result<Self, sqlx::error::BoxDynError> {
        let aead_ciphertext: AeadCiphertext = sqlx::Decode::<Postgres>::decode(value)?;
        Ok(Self {
            ct: aead_ciphertext,
            pd: PhantomData,
        })
    }
}

impl<CT> tls_codec::Size for Ciphertext<CT> {
    fn tls_serialized_len(&self) -> usize {
        self.ct.tls_serialized_len()
    }
}

impl<CT> tls_codec::Serialize for Ciphertext<CT> {
    fn tls_serialize<W: std::io::Write>(&self, writer: &mut W) -> Result<usize, tls_codec::Error> {
        self.ct.tls_serialize(writer)
    }
}

impl<CT> tls_codec::DeserializeBytes for Ciphertext<CT> {
    fn tls_deserialize_bytes(bytes: &[u8]) -> Result<(Self, &[u8]), tls_codec::Error> {
        let (aead_ciphertext, rest) = AeadCiphertext::tls_deserialize_bytes(bytes)?;
        Ok((
            Self {
                ct: aead_ciphertext,
                pd: PhantomData,
            },
            rest,
        ))
    }
}

impl<CT> Clone for Ciphertext<CT> {
    fn clone(&self) -> Self {
        Self {
            ct: self.ct.clone(),
            pd: PhantomData,
        }
    }
}
impl<CT> PartialEq for Ciphertext<CT> {
    fn eq(&self, other: &Self) -> bool {
        self.ct == other.ct
    }
}
impl<CT> Eq for Ciphertext<CT> {}
