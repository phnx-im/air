// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use cbor::Cbor;
use error::CodecError;
use mls_assist::memory_provider::Codec;
use serde::{Serialize, de::DeserializeOwned};

mod cbor;
mod error;
mod persistence;
#[cfg(test)]
mod tests;

pub use error::Error;
pub use persistence::{BlobDecoded, BlobEncoded};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
#[non_exhaustive]
pub enum PersistenceCodec {
    #[cfg(test)]
    OlderTestVersion = 0,
    #[default]
    V1 = 1,
}

impl TryFrom<u8> for PersistenceCodec {
    type Error = Error;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            #[cfg(test)]
            0 => Ok(PersistenceCodec::OlderTestVersion),
            1 => Ok(PersistenceCodec::V1),
            _ => Err(Error::UnknownCodecVersion),
        }
    }
}

impl PersistenceCodec {
    fn serialize_to_writer<T: Serialize>(
        &self,
        value: &T,
        writer: &mut impl std::io::Write,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
        // The first byte is always the codec version
        writer.write_all(&[*self as u8])?;
        match self {
            #[cfg(test)]
            PersistenceCodec::OlderTestVersion => tests::Json::to_writer(value, writer)?,
            PersistenceCodec::V1 => Cbor::to_writer(value, writer)?,
        }
        Ok(())
    }

    fn serialize<T: Sized + Serialize>(
        &self,
        value: &T,
    ) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync + 'static>> {
        let mut buf = Vec::new();
        self.serialize_to_writer(value, &mut buf)?;
        Ok(buf)
    }

    fn deserialize<T: DeserializeOwned>(
        &self,
        bytes: &[u8],
    ) -> Result<T, Box<dyn std::error::Error + Send + Sync + 'static>> {
        let res = match self {
            #[cfg(test)]
            PersistenceCodec::OlderTestVersion => tests::Json::from_slice(bytes)?,
            PersistenceCodec::V1 => Cbor::from_slice(bytes)?,
        };
        Ok(res)
    }

    pub fn to_vec<T>(value: &T) -> Result<Vec<u8>, Error>
    where
        T: Sized + Serialize,
    {
        let codec_version = PersistenceCodec::default();
        let res = codec_version.serialize(value).map_err(|error| CodecError {
            codec_version,
            error,
        })?;
        Ok(res)
    }

    pub fn from_slice<T>(bytes: &[u8]) -> Result<T, Error>
    where
        T: DeserializeOwned,
    {
        let codec_version_byte = bytes.first().ok_or(Error::EmptyInputSlice)?;
        let codec_version = PersistenceCodec::try_from(*codec_version_byte)?;
        codec_version.deserialize(&bytes[1..]).map_err(|error| {
            CodecError {
                codec_version,
                error,
            }
            .into()
        })
    }
}

impl Codec for PersistenceCodec {
    type Error = Error;

    fn to_vec<T>(value: &T) -> Result<Vec<u8>, Self::Error>
    where
        T: Sized + Serialize,
    {
        Self::to_vec(value)
    }

    fn from_slice<T>(bytes: &[u8]) -> Result<T, Self::Error>
    where
        T: DeserializeOwned,
    {
        Self::from_slice(bytes)
    }
}
