// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Contact links, `https://air.ms/c/#<username>`

use aircommon::identifiers::Username;
use flutter_rust_bridge::frb;
use url::Url;

use crate::api::types::UiUsername;

const CONTACT_LINK_HOST: &str = "air.ms";
const CONTACT_LINK_PATH: &str = "/c/";

/// The link that opens a contact request to `username`.
#[frb(sync)]
pub fn contact_link(username: UiUsername) -> String {
    format!(
        "https://{CONTACT_LINK_HOST}{CONTACT_LINK_PATH}#{}",
        username.plaintext
    )
}

/// The username in a link made by [`contact_link`], or `None` for any other
/// URL.
#[frb(sync)]
pub fn parse_contact_link(url: String) -> Option<UiUsername> {
    let url = Url::parse(&url).ok()?;
    if url.scheme() != "https"
        || url.host_str() != Some(CONTACT_LINK_HOST)
        || url.path() != CONTACT_LINK_PATH
    {
        return None;
    }
    let username = Username::new(url.fragment()?.to_owned()).ok()?;
    Some(username.into())
}

#[cfg(test)]
mod tests {
    use quickcheck::{Arbitrary, Gen};
    use quickcheck_macros::quickcheck;

    use super::*;

    fn username(plaintext: &str) -> UiUsername {
        UiUsername {
            plaintext: plaintext.to_owned(),
        }
    }

    fn parse(url: &str) -> Option<UiUsername> {
        parse_contact_link(url.to_owned())
    }

    #[test]
    fn builds_link() {
        assert_eq!(contact_link(username("alice")), "https://air.ms/c/#alice");
    }

    #[test]
    fn reads_link() {
        assert_eq!(parse("https://air.ms/c/#alice"), Some(username("alice")));
    }

    #[test]
    fn ignores_query() {
        assert_eq!(
            parse("https://air.ms/c/?utm_source=chat#alice"),
            Some(username("alice"))
        );
    }

    #[test]
    fn rejects_other_urls() {
        let cases = [
            ("not a URL", "alice"),
            ("http", "http://air.ms/c/#alice"),
            ("custom scheme", "air://air.ms/c/#alice"),
            ("another host", "https://example.com/c/#alice"),
            ("www subdomain", "https://www.air.ms/c/#alice"),
            ("root", "https://air.ms"),
            ("root with fragment", "https://air.ms/#alice"),
            ("mention path", "https://air.ms/u/#alice"),
            ("no trailing slash", "https://air.ms/c#alice"),
            ("longer path", "https://air.ms/c/alice#alice"),
            ("no fragment", "https://air.ms/c/"),
            ("empty fragment", "https://air.ms/c/#"),
            ("uppercase", "https://air.ms/c/#Alice"),
            ("too short", "https://air.ms/c/#alic"),
            ("consecutive dashes", "https://air.ms/c/#al--ice"),
            ("leading digit", "https://air.ms/c/#1alice"),
            ("percent-encoded", "https://air.ms/c/#%61lice"),
        ];
        for (name, url) in cases {
            assert_eq!(parse(url), None, "{name}");
        }
    }

    /// A username that passes `Username::new`.
    #[derive(Debug, Clone)]
    struct ValidUsername(UiUsername);

    impl Arbitrary for ValidUsername {
        fn arbitrary(g: &mut Gen) -> Self {
            const ANY: &[u8] = b"-0123456789abcdefghijklmnopqrstuvwxyz";
            const NO_DIGIT: &[u8] = b"-abcdefghijklmnopqrstuvwxyz";
            const NO_DASH: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";
            // Between the 5 and 63 bytes `Username::new` accepts
            let len = 5 + usize::arbitrary(g) % 59;
            let mut plaintext = String::with_capacity(len);
            while plaintext.len() < len {
                let charset = match plaintext.as_bytes().last() {
                    None => NO_DIGIT,
                    Some(b'-') => NO_DASH,
                    Some(_) => ANY,
                };
                plaintext.push(char::from(*g.choose(charset).unwrap()));
            }
            Self(UiUsername { plaintext })
        }
    }

    #[quickcheck]
    fn link_round_trips(ValidUsername(username): ValidUsername) -> bool {
        assert!(username.validation_error().is_none(), "{username:?}");
        parse_contact_link(contact_link(username.clone())) == Some(username)
    }
}
