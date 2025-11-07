// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use mls_assist::memory_provider::Codec;
use serde::{Serialize, de::DeserializeOwned};

use crate::codec::PersistenceCodec;

pub(super) struct Json;

impl Json {
    pub(crate) fn to_writer<W: std::io::Write, T: Serialize>(
        value: &T,
        writer: &mut W,
    ) -> Result<(), serde_json::Error> {
        serde_json::to_writer(writer, value)
    }
}

impl super::Codec for Json {
    type Error = serde_json::Error;

    fn to_vec<T>(value: &T) -> Result<Vec<u8>, Self::Error>
    where
        T: Sized + Serialize,
    {
        serde_json::to_vec(value)
    }

    fn from_slice<T>(bytes: &[u8]) -> Result<T, Self::Error>
    where
        T: DeserializeOwned,
    {
        serde_json::from_slice(bytes)
    }
}

fn run_for_all_versions<F>(f: F)
where
    F: Fn(super::PersistenceCodec),
{
    for version in 0..u8::MAX {
        let Ok(codec) = super::PersistenceCodec::try_from(version) else {
            return;
        };
        f(codec)
    }
}

#[test]
fn serde_json() {
    let value = 42;
    let serialized = Json::to_vec(&value).unwrap();
    let deserialized: i32 = Json::from_slice(&serialized).unwrap();
    assert_eq!(value, deserialized);
}

/// Test that all codecs can serialize and deserialize a simple value
#[test]
fn functional_correctness() {
    let value = 42;
    let functional_correctness_inner = |codec: super::PersistenceCodec| {
        println!("Testing codec: {codec:?}");
        let serialized = codec.serialize(&value).unwrap();
        // The first byte should be the codec version
        assert!(serialized.first().copied() == Some(codec as u8));
        // The rest of the bytes should be the serialized value
        let deserialized: i32 = codec.deserialize(&serialized[1..]).unwrap();
        assert_eq!(value, deserialized);
    };
    run_for_all_versions(functional_correctness_inner);

    // For good measure, check that the public API works as well
    let serialized = PersistenceCodec::to_vec(&value).unwrap();
    let deserialized: i32 = PersistenceCodec::from_slice(&serialized).unwrap();
    assert_eq!(value, deserialized);
}

/// Test that the default codec can deserialize serialized values from all codec
/// versions
#[test]
fn default_codec_deserialization() {
    let value = 42;
    let default_codec_deserialization_inner = |codec: super::PersistenceCodec| {
        let serialized = codec.serialize(&value).unwrap();
        let deserialized: i32 = super::PersistenceCodec::from_slice(&serialized).unwrap();
        assert_eq!(value, deserialized);
    };

    run_for_all_versions(default_codec_deserialization_inner);
}
