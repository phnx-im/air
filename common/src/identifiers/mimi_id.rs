// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::fmt;

use displaydoc::Display;
use mimi_content::MimiContent;
use mls_assist::openmls::group::GroupId;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tls_codec::Serialize as _;

use crate::identifiers::UserId;

/// Message Identifier calculated from the Group ID, sender's User ID and Mimi Content.
///
/// The identifier is stable between different devices and is used to identify distributed
/// messages.
///
/// See <https://www.ietf.org/archive/id/draft-ietf-mimi-content-06.html#section-3.3>
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
    /// Returns `None` if the Mimi ID cannot be calculated. This might happens if the sender or
    /// content fails to serialize.
    pub fn calculate(
        group_id: &GroupId,
        sender: &UserId,
        content: &MimiContent,
    ) -> Result<Self, MimiIdCalculationError> {
        let user_id_bytes = sender
            .tls_serialize_detached()
            .map_err(|_| MimiIdCalculationError::UserIdSerializationFailed)?;
        let bytes = content
            .message_id(user_id_bytes.as_slice(), group_id.as_slice())
            .map_err(|_| MimiIdCalculationError::ContentSerializationFailed)?;
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
            buf: &mut <Sqlite as Database>::ArgumentBuffer<'q>,
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
