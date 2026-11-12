// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{iter, ops::RangeInclusive, sync::LazyLock};

use regex::Regex;
use unicode_security::{RestrictionLevel, RestrictionLevelDetection};
use url::{Host, Url};

/// Links in `text` that pass the link selection rules, in text order.
///
/// A link is selected if all of the following rules hold:
///
/// 1. Boundaries: it starts at `http://` or `https://` (case-insensitive). The character before it
///    is not alphanumeric, `_`, `@` or a format character, and the character after is its extent is
///    not a format character. A rejected candidate still takes up its extent, so a URL inside
///    another URL is not a link.
/// 2. Extent: it ends at whitespace, a format character, or one of `` < > " ` | \ ``, and before a
///    `,` or `;` directly followed by another candidate. Within the host it also ends at `'`.
///    Trailing `. , ! ? ; : ' "` are trimmed, and a closing bracket is trimmed unless it is
///    balances an opening one in the link. Balancing brackets is important for recognizing links
///    inside markdown links.
/// 3. Scheme: `https`. Not folded into the rule 1, because candidates are needed for their extent.
/// 4. Host: parses (WHATWG) as a domain, never an IP address, and has no userinfo before it.
/// 5. Not reserved: the host, without one trailing dot, has more than one label and is not one of
///    [`RESERVED`] or a subdomain of one.
/// 6. Not a homograph: each label of decoded host passes the UTS #39 highly restrictive check.
/// 7. Length: at most [`MAX_LINK_LEN`] bytes.
///
/// Format characters are  U+200B-U+200D, U+FEFF, U+202A-U+202E and U+2066-U+2069.
pub fn select_links(text: &str) -> impl Iterator<Item = &str> {
    candidates(text).filter(|link| is_selectable(link))
}

fn candidates(text: &str) -> impl Iterator<Item = &str> {
    let mut pos = 0;
    iter::from_fn(move || {
        loop {
            let found = link_regex().find_at(text, pos)?;
            let end = found.start() + split_before_next(found.as_str());
            // Move to end even if the candidate is rejected.
            pos = end;
            let link = trim_end(&text[found.start()..end]);
            if has_boundaries(text, found.start(), end) {
                return Some(link);
            }
        }
    })
}

/// Scheme, host up to `'`, then path, query or fragment, up to a link end.
fn link_regex() -> &'static Regex {
    static LINK: LazyLock<Regex> = LazyLock::new(|| {
        let format: String = FORMAT_CHARS
            .iter()
            .map(|range| {
                format!(
                    r"\x{{{:X}}}-\x{{{:X}}}",
                    *range.start() as u32,
                    *range.end() as u32
                )
            })
            .collect();
        let end = format!(r#"\s{format}<>"`|\\"#);
        Regex::new(&format!(r"(?i)https?://[^{end}'/?#]+(?:[/?#][^{end}]*)?"))
            .expect("link regex is valid")
    });
    &LINK
}

/// Bidi controls and zero-width characters, see the link selection rules.
const FORMAT_CHARS: [RangeInclusive<char>; 4] = [
    '\u{200B}'..='\u{200D}',
    '\u{FEFF}'..='\u{FEFF}',
    '\u{202A}'..='\u{202E}',
    '\u{2066}'..='\u{2069}',
];

fn is_format_char(c: char) -> bool {
    FORMAT_CHARS.iter().any(|range| range.contains(&c))
}

fn split_before_next(link: &str) -> usize {
    link.match_indices([',', ';'])
        .find(|&(idx, _)| starts_with_scheme(&link[idx + 1..]))
        .map(|(idx, _)| idx)
        .unwrap_or_else(|| link.len())
}

fn starts_with_scheme(s: &str) -> bool {
    ["http://", "https://"].iter().any(|scheme| {
        s.get(..scheme.len())
            .is_some_and(|head| head.eq_ignore_ascii_case(scheme))
    })
}

/// Trims trailing punctuation and closing brackets without an opening one.
///
/// Brackets must be trimmed to solve the parsing of the following examples:
///
/// 1. https://en.wikipedia.org/wiki/Rust_(programming_language) -> keep
/// 2. (see https://a.org/x) -> trim
/// 3. [text](https://a.org/x) -> trim
/// 4. (see https://en.wikipedia.org/wiki/Rust_(lang)) -> trim one, keep other
fn trim_end(link: &str) -> &str {
    // Opening minus closing brackets in link[..end], for (), [] and {}
    let mut balance = [0isize; 3];
    for c in link.chars() {
        match c {
            '(' => balance[0] += 1,
            ')' => balance[0] -= 1,
            '[' => balance[1] += 1,
            ']' => balance[1] -= 1,
            '{' => balance[2] += 1,
            '}' => balance[2] -= 1,
            _ => {}
        }
    }

    let mut end = link.len();
    for (i, c) in link.char_indices().rev() {
        let k = match c {
            '.' | ',' | '!' | '?' | ';' | ':' | '\'' | '"' => {
                end = i;
                continue;
            }
            ')' => 0,
            ']' => 1,
            '}' => 2,
            _ => break,
        };
        // A closing bracket that balances an opening bracket stays
        if balance[k] >= 0 {
            break;
        }
        balance[k] += 1;
        end = i;
    }
    &link[..end]
}

/// Whether `text[start..end]` is not glued to the text around it.
///
/// Rejects a word character, `_` or `@` before it and a format character on either side. `end` is
/// the extent end before trimming.
fn has_boundaries(text: &str, start: usize, end: usize) -> bool {
    let glued_before = text[..start]
        .chars()
        .next_back()
        .is_some_and(|c| c.is_alphanumeric() || c == '_' || c == '@' || is_format_char(c));
    !glued_before && !text[end..].chars().next().is_some_and(is_format_char)
}

const MAX_LINK_LEN: usize = 2048;

const RESERVED: [&str; 12] = [
    "localhost",
    "test",
    "invalid",
    "example",
    "example.com",
    "example.net",
    "example.org",
    "onion",
    "i2p",
    "local",
    "internal",
    "home.arpa",
];

pub(super) fn is_selectable(link: &str) -> bool {
    link.len() <= MAX_LINK_LEN
        && Url::parse(link).is_ok_and(|url| {
            url.scheme() == "https"
                && url.username().is_empty()
                && url.password().is_none()
                && match url.host() {
                    Some(Host::Domain(host)) => !is_reserved(host) && !is_homograph(host),
                    _ => false,
                }
        })
}

/// Whether `host` is single-label host, or a reserved name, or a subdomain of one.
fn is_reserved(host: &str) -> bool {
    let host = host.strip_suffix('.').unwrap_or(host);
    !host.contains('.')
        || RESERVED.iter().any(|name| {
            host.strip_suffix(name)
                .is_some_and(|rest| rest.is_empty() || rest.ends_with('.'))
        })
}

/// Whether a label of `host` fails the [UTS #39 highly restrictive] check.
///
/// [UTS #39 highly restrictive]: https://www.unicode.org/reports/tr39/#highly_restrictive
fn is_homograph(host: &str) -> bool {
    // The URL parser returns the host in punycode
    if !host.contains("xn--") {
        return false;
    }
    let (unicode, result) = idna::domain_to_unicode(host);
    result.is_err()
        || unicode
            .split('.')
            .any(|label| label.detect_restriction_level() > RestrictionLevel::HighlyRestrictive)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn links(text: &str) -> Vec<&str> {
        select_links(text).collect()
    }

    #[track_caller]
    fn assert_no_links(texts: &[&str]) {
        for text in texts {
            assert_eq!(links(text), [] as [&str; 0], "{text:?}");
        }
    }

    #[test]
    fn selects_links_in_text_order() {
        assert_eq!(links("see https://air.ms"), ["https://air.ms"]);
        assert_eq!(
            links("https://a.org and https://b.org/x?q=1#f"),
            ["https://a.org", "https://b.org/x?q=1#f"]
        );
        assert_no_links(&["", "no links here", "https://"]);
    }

    #[test]
    fn boundaries() {
        assert_eq!(links("(https://air.ms)"), ["https://air.ms"]);
        assert_eq!(links("x:https://air.ms"), ["https://air.ms"]);
        assert_no_links(&[
            "foohttps://air.ms",
            "x_https://air.ms",
            "me@https://air.ms",
            "\u{200B}https://air.ms",
            "\u{202E}https://air.ms",
            "https://air.ms\u{200B}",
            "https://air.ms/x\u{2066}y",
            "https://air.ms.\u{200B}",
        ]);
    }

    #[test]
    fn rejected_candidate_takes_up_its_extent() {
        assert_no_links(&[
            "http://a.org/https://air.ms",
            "https://127.0.0.1/https://air.ms",
        ]);
    }

    #[test]
    fn extent_ends() {
        for end in [
            " ", "\n", "\t", "\u{00A0}", "\u{3000}", "<", ">", "\"", "`", "|", "\\",
        ] {
            let text = format!("https://air.ms/x{end}y");
            assert_eq!(links(&text), ["https://air.ms/x"], "{text:?}");
        }
        assert_eq!(links("<https://air.ms>"), ["https://air.ms"]);
    }

    #[test]
    fn split_before_next_candidate() {
        assert_eq!(
            links("https://a.org/x,https://b.org"),
            ["https://a.org/x", "https://b.org"]
        );
        assert_eq!(
            links("https://a.org/x;HTTPS://b.org"),
            ["https://a.org/x", "HTTPS://b.org"]
        );
        assert_eq!(links("https://a.org/x,y"), ["https://a.org/x,y"]);
    }

    #[test]
    fn apostrophe_ends_host() {
        assert_eq!(links("https://air.ms's blog"), ["https://air.ms"]);
        assert_eq!(links("https://air.ms/it's"), ["https://air.ms/it's"]);
    }

    #[test]
    fn trims_trailing_punctuation() {
        for punct in [".", ",", "!", "?", ";", ":", "'", "\"", "...", "?!"] {
            let text = format!("see https://air.ms/x{punct} ok");
            assert_eq!(links(&text), ["https://air.ms/x"], "{text:?}");
        }
        assert_eq!(links("https://air.ms/a.b"), ["https://air.ms/a.b"]);
    }

    #[test]
    fn trims_unbalanced_closing_brackets() {
        assert_eq!(
            links("https://en.wikipedia.org/wiki/Rust_(programming_language)"),
            ["https://en.wikipedia.org/wiki/Rust_(programming_language)"]
        );
        assert_eq!(links("(see https://air.ms/x)"), ["https://air.ms/x"]);
        assert_eq!(links("(see https://air.ms/x)."), ["https://air.ms/x"]);
        assert_eq!(links("[text](https://air.ms/x)"), ["https://air.ms/x"]);
        assert_eq!(links("{https://air.ms/x}"), ["https://air.ms/x"]);
        assert_eq!(
            links("(see https://en.wikipedia.org/wiki/Rust_(lang))"),
            ["https://en.wikipedia.org/wiki/Rust_(lang)"]
        );
        assert_eq!(links("https://air.ms/[a](b)"), ["https://air.ms/[a](b)"]);
    }

    #[test]
    fn many_closing_brackets() {
        let text = format!("https://air.ms/{}", ")".repeat(100_000));
        assert_eq!(links(&text), ["https://air.ms/"]);
    }

    #[test]
    fn only_https() {
        assert_eq!(links("HTTPS://Air.ms/X"), ["HTTPS://Air.ms/X"]);
        assert_eq!(links("hTTpS://air.ms"), ["hTTpS://air.ms"]);
        assert_no_links(&["http://air.ms", "ftp://air.ms", "air.ms", "www.air.ms"]);
    }

    #[test]
    fn returns_link_as_written() {
        assert_eq!(links("https://air.ms:443"), ["https://air.ms:443"]);
        assert_eq!(links("https://air.ms/a/../b"), ["https://air.ms/a/../b"]);
        assert_eq!(links("https://приклад.укр"), ["https://приклад.укр"]);
    }

    #[test]
    fn host_is_a_domain() {
        assert_no_links(&[
            "https://127.0.0.1/",
            "https://2130706433/",
            "https://0x7f.1/",
            "https://[::1]/",
            "https://[::ffff:127.0.0.1]/",
        ]);
    }

    #[test]
    fn no_userinfo() {
        assert_no_links(&[
            "https://air.ms@evil.ms/",
            "https://user:pass@air.ms/",
            "https://:pass@air.ms/",
        ]);
        assert_eq!(links("https://air.ms/@user"), ["https://air.ms/@user"]);
    }

    #[test]
    fn reserved_hosts() {
        for host in [
            "localhost",
            "localhost.",
            "wiki",
            "wiki.",
            "test",
            "a.test",
            "a.invalid",
            "example",
            "example.com",
            "www.example.com",
            "Example.COM",
            "a.example.net",
            "a.example.org",
            "x.onion",
            "x.i2p",
            "printer.local",
            "corp.internal",
            "router.home.arpa",
        ] {
            let text = format!("https://{host}/");
            assert_eq!(links(&text), [] as [&str; 0], "{text:?}");
        }
        for host in ["notexample.com", "example.co", "local.air.ms", "air.ms."] {
            let text = format!("https://{host}/");
            assert_eq!(links(&text), [text.as_str()], "{text:?}");
        }
    }

    #[test]
    fn homographs() {
        // Latin with Cyrillic o
        assert_no_links(&["https://g\u{43E}\u{43E}gle.com", "https://\u{3B1}ir.ms"]);
        assert_eq!(links("https://東京tokyo.jp"), ["https://東京tokyo.jp"]);
        assert_eq!(links("https://їжак.укр"), ["https://їжак.укр"]);
        // Whole-script confusables are not detected, the card shows the host
        assert_eq!(
            links("https://\u{430}\u{441}\u{435}.com"),
            ["https://\u{430}\u{441}\u{435}.com"]
        );
    }

    #[test]
    fn max_length() {
        let prefix = "https://air.ms/";
        let link = format!("{prefix}{}", "a".repeat(MAX_LINK_LEN - prefix.len()));
        assert_eq!(links(&link), [link.as_str()]);
        assert_no_links(&[&format!("{link}a")]);
    }
}
