// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::str::FromStr;

use displaydoc::Display;
use thiserror::Error;
use url::Url;
use uuid::Uuid;

/// Uniquely identifies an attachment on DS
#[derive(Debug, Clone, Copy, Eq, PartialEq, PartialOrd, Ord, Hash)]
pub struct RemoteAttachmentId {
    uuid: Uuid,
}

impl RemoteAttachmentId {
    pub fn new(uuid: Uuid) -> Self {
        Self { uuid }
    }

    pub fn from_url(url: &Url) -> Result<Self, RemoteAttachmentIdParseError> {
        if url.scheme() != "air" {
            return Err(RemoteAttachmentIdParseError::InvalidScheme);
        }
        let suffix = url
            .path()
            .strip_prefix("/attachment/")
            .ok_or(RemoteAttachmentIdParseError::InvalidPrefix)?;
        let uuid = suffix
            .parse()
            .map_err(|_| RemoteAttachmentIdParseError::InvalidUuid)?;
        Ok(Self { uuid })
    }

    /// Whether `url` is an attachment URL in canonical form.
    ///
    /// Accepts `air:///attachment/<hyphenated uuid>`, optionally followed by a
    /// query. Every URL accepted here is also accepted by [`Self::from_url`].
    pub fn is_canonical(url: &str) -> bool {
        let Some((scheme, rest)) = url.split_once(':') else {
            return false;
        };
        if !scheme.eq_ignore_ascii_case("air") {
            return false;
        }
        let Some(rest) = rest.strip_prefix("///attachment/") else {
            return false;
        };
        let uuid = match rest.split_once('?') {
            Some((uuid, _query)) => uuid,
            None => rest,
        };
        // Only the hyphenated form has 36 characters
        uuid.len() == 36 && Uuid::try_parse(uuid).is_ok()
    }

    pub fn uuid(&self) -> Uuid {
        self.uuid
    }
}

impl FromStr for RemoteAttachmentId {
    type Err = RemoteAttachmentIdParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::from_url(&s.parse()?)
    }
}

#[derive(Debug, Display, Error)]
pub enum RemoteAttachmentIdParseError {
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

    impl Type<Sqlite> for RemoteAttachmentId {
        fn type_info() -> <Sqlite as Database>::TypeInfo {
            <Uuid as Type<Sqlite>>::type_info()
        }
    }

    impl<'q> Encode<'q, Sqlite> for RemoteAttachmentId {
        fn encode_by_ref(
            &self,
            buf: &mut <Sqlite as Database>::ArgumentBuffer,
        ) -> Result<IsNull, BoxDynError> {
            Encode::<Sqlite>::encode_by_ref(&self.uuid, buf)
        }
    }

    impl<'r> Decode<'r, Sqlite> for RemoteAttachmentId {
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
        let remote_attachment_id = super::RemoteAttachmentId::from_url(&url).unwrap();
        assert_eq!(
            remote_attachment_id.uuid,
            Uuid::parse_str("b6a42a7a-62fa-4c10-acfb-6124d80aae09").unwrap()
        );
    }

    #[test]
    fn is_canonical() {
        let uuid = "b6a42a7a-62fa-4c10-acfb-6124d80aae09";
        let cases = [
            (format!("air:///attachment/{uuid}"), true),
            (format!("air:///attachment/{uuid}?width=1&height=2"), true),
            (format!("AIR:///attachment/{uuid}"), true),
            (format!("air:///attachment/{}", uuid.to_uppercase()), true),
            (format!("air:/attachment/{uuid}"), false),
            (format!("air://host/attachment/{uuid}"), false),
            (format!("air:///attachment/../attachment/{uuid}"), false),
            (format!("air:///attachment/{uuid}#fragment"), false),
            (format!("air:///attachment/{{{uuid}}}"), false),
            (
                format!("air:///attachment/{}", uuid.replace('-', "")),
                false,
            ),
            (format!("air:///attachment/urn:uuid:{uuid}"), false),
            (format!(" air:///attachment/{uuid}"), false),
            (format!("https:///attachment/{uuid}"), false),
            ("air:///attachment/not-a-uuid".to_owned(), false),
            ("air:///attachment/".to_owned(), false),
            (String::new(), false),
        ];
        for (url, expected) in cases {
            let canonical = super::RemoteAttachmentId::is_canonical(&url);
            assert_eq!(canonical, expected, "{url}");
            if canonical {
                let parsed = url.parse::<url::Url>().unwrap();
                assert!(
                    super::RemoteAttachmentId::from_url(&parsed).is_ok(),
                    "{url}"
                );
            }
        }
    }
}
