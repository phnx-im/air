// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use serde::{Serialize, de::DeserializeOwned};
use sqlx::{Database, Decode, Encode, Type, encode::IsNull, error::BoxDynError};

use super::PersistenceCodec;

pub struct BlobEncoded<T: Serialize>(pub T);

impl<DB: Database, T: Serialize> Type<DB> for BlobEncoded<T>
where
    Vec<u8>: Type<DB>,
{
    fn type_info() -> <DB as Database>::TypeInfo {
        Vec::<u8>::type_info()
    }
}

impl<'q, DB: Database, T: Serialize> Encode<'q, DB> for BlobEncoded<T>
where
    Vec<u8>: Encode<'q, DB>,
{
    fn encode_by_ref(
        &self,
        buf: &mut <DB as Database>::ArgumentBuffer<'q>,
    ) -> Result<IsNull, BoxDynError> {
        let bytes = PersistenceCodec::to_vec(&self.0)?;
        Encode::<DB>::encode(&bytes, buf)
    }
}

#[derive(Debug)]
pub struct BlobDecoded<T>(pub T);

impl<T> BlobDecoded<T> {
    pub fn into_inner(self) -> T {
        self.0
    }
}

impl<DB: Database, T: DeserializeOwned> Type<DB> for BlobDecoded<T>
where
    Vec<u8>: Type<DB>,
{
    fn type_info() -> <DB as Database>::TypeInfo {
        Vec::<u8>::type_info()
    }
}

impl<'q, DB: Database, T: DeserializeOwned> Decode<'q, DB> for BlobDecoded<T>
where
    for<'a> &'a [u8]: Decode<'a, DB>,
{
    fn decode(value: <DB as Database>::ValueRef<'q>) -> Result<Self, BoxDynError> {
        let bytes: &[u8] = Decode::<DB>::decode(value)?;
        PersistenceCodec::from_slice(bytes)
            .map(BlobDecoded)
            .map_err(From::from)
    }
}
