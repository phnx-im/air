// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::fmt;

use serde::{Deserialize, Serialize};

/// Identifier of a system message that all devices of a user derive on their
/// own from an event.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct EventId(#[serde(with = "serde_bytes")] [u8; 32]);

impl fmt::Debug for EventId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("EventId")
            .field(&hex::encode(self.0))
            .finish()
    }
}

impl EventId {
    pub fn from_slice(bytes: &[u8]) -> Result<Self, std::array::TryFromSliceError> {
        bytes.try_into().map(Self)
    }

    pub fn as_slice(&self) -> &[u8] {
        self.0.as_slice()
    }
}

impl From<[u8; 32]> for EventId {
    fn from(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }
}

mod sqlx_impls {
    use sqlx::{
        Database, Decode, Sqlite, Type,
        encode::{Encode, IsNull},
        error::BoxDynError,
    };

    use super::*;

    impl Type<Sqlite> for EventId {
        fn type_info() -> <Sqlite as Database>::TypeInfo {
            <Vec<u8> as Type<Sqlite>>::type_info()
        }
    }

    impl<'q> Encode<'q, Sqlite> for &'q EventId {
        fn encode_by_ref(
            &self,
            buf: &mut <Sqlite as Database>::ArgumentBuffer,
        ) -> Result<IsNull, BoxDynError> {
            Encode::<Sqlite>::encode(self.0.as_slice(), buf)
        }
    }

    impl Decode<'_, Sqlite> for EventId {
        fn decode(value: <Sqlite as Database>::ValueRef<'_>) -> Result<Self, BoxDynError> {
            let bytes: &[u8] = Decode::<Sqlite>::decode(value)?;
            Ok(Self(bytes.try_into()?))
        }
    }
}
