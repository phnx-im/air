// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::fmt;

use displaydoc::Display;
use mimi_content::{MimiContent, cbor::Value, content_container::ExtensionName};
use mls_assist::openmls::group::GroupId;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tls_codec::Serialize as _;

use crate::identifiers::UserId;

/// Private extension naming the scheme that derives a message's Mimi ID.
///
/// The scheme is part of the message so that every client computes the same ID for it, no
/// matter when the client receives it. Messages without the extension use the derivation of
/// draft-ietf-mimi-content-07, which is what clients predating the extension compute for every
/// message. The extension is therefore only sent once every client understands it.
pub const MIMI_ID_SCHEME_EXTENSION: ExtensionName = ExtensionName::Number(-2);

/// Scheme 1: the derivation of draft-ietf-mimi-content-09 over the TLS-serialized user ID and
/// the raw group ID, with 16-bit length prefixes.
pub const MIMI_ID_SCHEME_LENGTH_PREFIXED: i64 = 1;

/// Message Identifier calculated from the Group ID, sender's User ID and Mimi Content.
///
/// The identifier is stable between different devices and is used to identify distributed
/// messages.
///
/// See <https://www.ietf.org/archive/id/draft-ietf-mimi-content-09.html#section-3.3>
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct MimiId(#[serde(with = "serde_bytes")] [u8; 32]);

impl fmt::Debug for MimiId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("MimiId").field(&hex::encode(self.0)).finish()
    }
}

impl MimiId {
    /// Calculate the Mimi ID from the group ID, sender's User ID and Mimi Content.
    ///
    /// The content's [`MIMI_ID_SCHEME_EXTENSION`] selects the derivation. Fails if the sender or
    /// content does not serialize, or if the content names a scheme this client does not know.
    pub fn calculate(
        group_id: &GroupId,
        sender: &UserId,
        content: &MimiContent,
    ) -> Result<Self, MimiIdCalculationError> {
        let user_id_bytes = sender
            .tls_serialize_detached()
            .map_err(|_| MimiIdCalculationError::UserIdSerializationFailed)?;
        let bytes = match content.extensions.get(&MIMI_ID_SCHEME_EXTENSION) {
            None => content
                .message_id_draft07(user_id_bytes.as_slice(), group_id.as_slice())
                .map_err(|_| MimiIdCalculationError::ContentSerializationFailed)?,
            Some(Value::Int(MIMI_ID_SCHEME_LENGTH_PREFIXED)) => content
                .message_id(user_id_bytes.as_slice(), group_id.as_slice())
                .map_err(|_| MimiIdCalculationError::ContentSerializationFailed)?,
            Some(_) => return Err(MimiIdCalculationError::UnknownScheme),
        };
        bytes
            .try_into()
            .map(Self)
            .map_err(|_| MimiIdCalculationError::InvalidIdLength)
    }

    pub fn from_slice(bytes: &[u8]) -> Result<Self, MimiIdCalculationError> {
        bytes
            .try_into()
            .map(Self)
            .map_err(|_| MimiIdCalculationError::InvalidIdLength)
    }

    pub fn as_slice(&self) -> &[u8] {
        self.0.as_slice()
    }

    pub fn into_inner(self) -> [u8; 32] {
        self.0
    }
}

impl From<[u8; 32]> for MimiId {
    fn from(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }
}

impl AsRef<[u8; 32]> for MimiId {
    fn as_ref(&self) -> &[u8; 32] {
        &self.0
    }
}

#[derive(Debug, Error, Display)]
pub enum MimiIdCalculationError {
    /// The Mimi Content failed to serialize.
    ContentSerializationFailed,
    /// The sender's User ID failed to serialize.
    UserIdSerializationFailed,
    /// Invalid ID length
    InvalidIdLength,
    /// The content names a Mimi ID scheme this client does not know.
    UnknownScheme,
}

mod sqlx_impls {
    use sqlx::{
        Database, Decode, Sqlite, Type,
        encode::{Encode, IsNull},
        error::BoxDynError,
    };

    use super::*;

    impl Type<Sqlite> for MimiId {
        fn type_info() -> <Sqlite as Database>::TypeInfo {
            <Vec<u8> as Type<Sqlite>>::type_info()
        }
    }

    impl<'q> Encode<'q, Sqlite> for &'q MimiId {
        fn encode_by_ref(
            &self,
            buf: &mut <Sqlite as Database>::ArgumentBuffer,
        ) -> Result<IsNull, BoxDynError> {
            Encode::<Sqlite>::encode(self.0.as_slice(), buf)
        }
    }

    impl Decode<'_, Sqlite> for MimiId {
        fn decode(value: <Sqlite as Database>::ValueRef<'_>) -> Result<Self, BoxDynError> {
            let bytes: &[u8] = Decode::<Sqlite>::decode(value)?;
            Ok(Self(bytes.try_into()?))
        }
    }
}

#[cfg(test)]
mod tests {
    use mimi_content::{Disposition, NestedPart};
    use sha2::{Digest, Sha256};

    use super::*;

    fn sender() -> UserId {
        UserId::random("example.com".parse().unwrap())
    }

    fn group_id() -> GroupId {
        GroupId::from_slice(b"room")
    }

    fn sample_content(salt: Vec<u8>) -> MimiContent {
        MimiContent {
            salt,
            nested_part: NestedPart::SinglePart {
                disposition: Disposition::Render,
                language: String::new(),
                content_type: "text/markdown".to_owned(),
                content: b"hello".to_vec(),
            },
            ..Default::default()
        }
    }

    fn marked(mut content: MimiContent) -> MimiContent {
        content.extensions.insert(
            MIMI_ID_SCHEME_EXTENSION,
            Value::Int(MIMI_ID_SCHEME_LENGTH_PREFIXED),
        );
        content
    }

    /// Unmarked content keeps the draft-07 derivation, including content with an empty salt as
    /// older attachment messages have.
    #[test]
    fn unmarked_content_uses_legacy_derivation() {
        let sender = sender();
        let sender_bytes = sender.tls_serialize_detached().unwrap();
        for salt in [vec![7; 16], Vec::new()] {
            let content = sample_content(salt);

            let mut hasher = Sha256::new();
            hasher.update(&sender_bytes);
            hasher.update(group_id().as_slice());
            hasher.update(content.serialize().unwrap());
            hasher.update(&content.salt);
            let expected = [&[0x01][..], &hasher.finalize()[..31]].concat();

            let id = MimiId::calculate(&group_id(), &sender, &content).unwrap();
            assert_eq!(id.as_slice(), expected);
        }
    }

    /// Frozen wire bytes of the message shapes Air sends, with the legacy IDs computed by an
    /// independent encoder. Guards two things at once: that re-serialization reproduces the
    /// bytes, and that unmarked content keeps the ID it had before the scheme marker existed.
    #[test]
    fn legacy_ids_of_frozen_messages_are_stable() {
        const SENDER: &[u8] = b"sender-bytes";
        const ROOM: &[u8] = b"room-bytes";
        let fixtures = [
            (
                "markdown",
                "8750000102030405060708090a0b0c0d0e0ff640f6f6a0850160016d746578742f6d61726b646f776e4c48656c6c6f20776f726c6421",
                "011177d81278d36a24c82272c5d6eeda54467e980907f2dc80de11bc32ce10d8",
            ),
            (
                "reply",
                "8750000102030405060708090a0b0c0d0e0ff640f658200100000000000000000000000000000000000000000000000000000000000000a0850160016d746578742f6d61726b646f776e424869",
                "01648ea25c908388f623a9256ab7836ac365a44fbc0f641f79b59e3949416b15",
            ),
            (
                "empty salt attachment",
                "8740f640f6f6a08506600302828f06600269696d6167652f706e6778476169723a2f2f2f6174746163686d656e742f30313933613264342d313233342d373030302d383030302d3030303030303030303030303f77696474683d34266865696768743d3400187802582000000000000000000000000000000000000000000000000000000000000000004c000000000000000000000000400158200000000000000000000000000000000000000000000000000000000000000000606970686f746f2e706e67850860016d746578742f626c757268617368581c4c454856366e574232796b3870796f306164522a2e376b434d646e6a",
                "01f81322ee5cf5218e141c4e7e5eb79dbb76df3ce4c2bf6c736f90b52c9037b7",
            ),
            (
                "self group deletion",
                "8750000102030405060708090a0b0c0d0e0ff640f6f6a120a10282582001000000000000000000000000000000000000000000000000000000000000005820010202020202020202020202020202020202020202020202020202020202020283006000",
                "0160703d11dddf4cc56b60196618e41888b75d9b19b10f088efdf78371e262e9",
            ),
        ];

        for (name, bytes, expected_id) in fixtures {
            let bytes = hex::decode(bytes).unwrap();
            let content = MimiContent::deserialize(&bytes).unwrap();
            assert_eq!(content.serialize().unwrap(), bytes, "{name}");
            let id = content.message_id_draft07(SENDER, ROOM).unwrap();
            assert_eq!(hex::encode(id), expected_id, "{name}");
        }
    }

    #[test]
    fn marked_content_uses_length_prefixed_derivation() {
        let sender = sender();
        let content = marked(sample_content(vec![7; 16]));

        let expected = content
            .message_id(
                &sender.tls_serialize_detached().unwrap(),
                group_id().as_slice(),
            )
            .unwrap();
        let id = MimiId::calculate(&group_id(), &sender, &content).unwrap();
        assert_eq!(id.as_slice(), expected);

        let unmarked =
            MimiId::calculate(&group_id(), &sender, &sample_content(vec![7; 16])).unwrap();
        assert_ne!(id, unmarked);
    }

    #[test]
    fn unknown_scheme_is_rejected() {
        let mut content = sample_content(vec![7; 16]);
        content
            .extensions
            .insert(MIMI_ID_SCHEME_EXTENSION, Value::Int(2));
        let err = MimiId::calculate(&group_id(), &sender(), &content).unwrap_err();
        assert!(matches!(err, MimiIdCalculationError::UnknownScheme));
    }
}
