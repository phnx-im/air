// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use serde::{Serialize, de::DeserializeOwned};
use thiserror::Error;

use super::Codec;

#[derive(Debug)]
pub(super) struct Cbor;

impl Cbor {
    pub(crate) fn to_writer<T: Serialize, W: std::io::Write>(
        value: T,
        writer: W,
    ) -> Result<(), minicbor_serde::error::EncodeError<std::io::Error>> {
        let writer = minicbor::encode::write::Writer::new(writer);
        let mut serializer = minicbor_serde::Serializer::new(writer);
        value.serialize(&mut serializer)?;
        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum CborError {
    #[error(transparent)]
    Serialization(#[from] minicbor_serde::error::EncodeError<std::convert::Infallible>),
    #[error(transparent)]
    Deserialization(#[from] minicbor_serde::error::DecodeError),
}

impl Codec for Cbor {
    type Error = CborError;

    fn to_vec<T>(value: &T) -> Result<Vec<u8>, Self::Error>
    where
        T: Sized + Serialize,
    {
        Ok(minicbor_serde::to_vec(value)?)
    }

    fn from_slice<T>(bytes: &[u8]) -> Result<T, Self::Error>
    where
        T: DeserializeOwned,
    {
        Ok(minicbor_serde::from_slice(bytes)?)
    }
}
