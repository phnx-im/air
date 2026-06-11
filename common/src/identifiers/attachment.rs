// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::str::FromStr;

use displaydoc::Display;
use thiserror::Error;
use url::Url;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, Eq, PartialEq, PartialOrd, Ord, Hash)]
pub struct AttachmentId {
    pub uuid: Uuid,
}

impl AttachmentId {
    pub fn new(uuid: Uuid) -> Self {
        Self { uuid }
    }

    pub fn from_url(url: &Url) -> Result<Self, AttachmentIdParseError> {
        if url.scheme() != "air" {
            return Err(AttachmentIdParseError::InvalidScheme);
        }
        let suffix = url
            .path()
            .strip_prefix("/attachment/")
            .ok_or(AttachmentIdParseError::InvalidPrefix)?;
        let uuid = suffix
            .parse()
            .map_err(|_| AttachmentIdParseError::InvalidUuid)?;
        Ok(Self { uuid })
    }

    pub fn uuid(&self) -> Uuid {
        self.uuid
    }
}

impl FromStr for AttachmentId {
    type Err = AttachmentIdParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::from_url(&s.parse()?)
    }
}

#[derive(Debug, Display, Error)]
pub enum AttachmentIdParseError {
    /// {0}
    Url(#[from] url::ParseError),
    /// The UUID is invalid
    InvalidUuid,
    /// The URL scheme is invalid
    InvalidScheme,
    /// The URL prefix is invalid
    InvalidPrefix,
}

mod sqlx_impls {
    use sqlx::{Database, Decode, Encode, Sqlite, Type, encode::IsNull, error::BoxDynError};

    use super::*;

    impl Type<Sqlite> for AttachmentId {
        fn type_info() -> <Sqlite as Database>::TypeInfo {
            <Uuid as Type<Sqlite>>::type_info()
        }
    }

    impl<'q> Encode<'q, Sqlite> for AttachmentId {
        fn encode_by_ref(
            &self,
            buf: &mut <Sqlite as Database>::ArgumentBuffer,
        ) -> Result<IsNull, BoxDynError> {
            Encode::<Sqlite>::encode_by_ref(&self.uuid, buf)
        }
    }

    impl<'r> Decode<'r, Sqlite> for AttachmentId {
        fn decode(value: <Sqlite as Database>::ValueRef<'r>) -> Result<Self, BoxDynError> {
            let id: Uuid = Decode::<Sqlite>::decode(value)?;
            Ok(Self::new(id))
        }
    }
}

#[cfg(test)]
mod test {
    use uuid::Uuid;

    #[test]
    fn from_url() {
        let url = "air:///attachment/b6a42a7a-62fa-4c10-acfb-6124d80aae09?width=1920&height=1080"
            .parse()
            .unwrap();
        let attachment_id = super::AttachmentId::from_url(&url).unwrap();
        assert_eq!(
            attachment_id.uuid,
            Uuid::parse_str("b6a42a7a-62fa-4c10-acfb-6124d80aae09").unwrap()
        );
    }
}
