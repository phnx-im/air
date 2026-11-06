// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::convert::Infallible;

use airmacros::{DeserializeTaggedMap, SerializeTaggedMap};
use minicbor_serde::error::{DecodeError, EncodeError};
use serde::{Deserialize, Serialize};

/// Maximum size of an encoded card in bytes.
///
/// Checked before decoding, so no field is allocated beyond it. A card with
/// every field at its cap is about 4 KiB, the rest leaves room for new tags.
const MAX_CARD_LEN: usize = 8 * 1024;
/// Maximum size of the url in UTF-8 bytes.
const MAX_URL_LEN: usize = 2048;
/// Maximum size of the title in UTF-8 bytes.
const MAX_TITLE_LEN: usize = 256;
/// Maximum size of the description in UTF-8 bytes.
const MAX_DESCRIPTION_LEN: usize = 1024;
/// Maximum size of the image blur hash in UTF-8 bytes.
const MAX_BLUR_HASH_LEN: usize = 64;
/// Maximum size of the image alt text in UTF-8 bytes.
const MAX_ALT_LEN: usize = 512;

/// Link preview card stored in MIMI content as a `Preview` part.
///
/// The format is proprietary and has content type [`Self::CONTENT_TYPE`].
/// A value of this type always satisfies the card validity rules.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LinkPreviewCard {
    url: String,
    title: String,
    description: String,
    kind: LinkPreviewKind,
    image: Option<LinkPreviewImage>,
}

/// Image attached to a [`LinkPreviewCard`].
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LinkPreviewImage {
    part_index: u32,
    width: u32,
    height: u32,
    blur_hash: Option<String>,
    alt: Option<String>,
}

/// A kind of a [`LinkPreviewCard`] used to decide on how to show it.
///
/// ```cddl
/// Kind = &(
///   default: 0,
///   video: 1,
///   music: 2,
///   profile: 3,
/// ) / uint
/// ```
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(from = "u64", into = "u64")]
pub enum LinkPreviewKind {
    #[default]
    Default,
    /// play badge on the image
    Video,
    /// play badge on the image
    Music,
    /// round image
    Profile,
    /// A kind this version does not know, rendered like `Default`
    Other(u64),
}

impl From<u64> for LinkPreviewKind {
    fn from(value: u64) -> Self {
        match value {
            0 => Self::Default,
            1 => Self::Video,
            2 => Self::Music,
            3 => Self::Profile,
            other => Self::Other(other),
        }
    }
}

impl From<LinkPreviewKind> for u64 {
    fn from(kind: LinkPreviewKind) -> Self {
        match kind {
            LinkPreviewKind::Default => 0,
            LinkPreviewKind::Video => 1,
            LinkPreviewKind::Music => 2,
            LinkPreviewKind::Profile => 3,
            LinkPreviewKind::Other(value) => value,
        }
    }
}

/// Error when decoding or building a [`LinkPreviewCard`].
#[derive(Debug, thiserror::Error)]
pub enum LinkPreviewCardError {
    #[error("invalid CBOR: {0}")]
    Decode(#[from] DecodeError),
    #[error("trailing bytes after the card")]
    TrailingBytes,
    #[error("empty url")]
    EmptyUrl,
    #[error("empty title")]
    EmptyTitle,
    #[error("{field} is too long")]
    TooLong { field: &'static str },
}

impl LinkPreviewCard {
    pub const CONTENT_TYPE: &'static str = "application/vnd.air.link-preview+cbor";

    /// Builds a card on the sender side.
    ///
    /// `title` and `description` are sanitised and truncated to their caps.
    /// `url` is taken as is and never truncated.
    pub fn new(
        url: String,
        title: &str,
        description: &str,
        kind: LinkPreviewKind,
        image: Option<LinkPreviewImage>,
    ) -> Result<Self, LinkPreviewCardError> {
        if url.is_empty() {
            return Err(LinkPreviewCardError::EmptyUrl);
        }
        check_len("url", &url, MAX_URL_LEN)?;
        let title = truncate(sanitize(title), MAX_TITLE_LEN);
        if title.is_empty() {
            return Err(LinkPreviewCardError::EmptyTitle);
        }
        let description = truncate(sanitize(description), MAX_DESCRIPTION_LEN);
        Ok(Self {
            url,
            title,
            description,
            kind,
            image,
        })
    }

    /// Decodes and validates a received card.
    ///
    /// Text fields are sanitised. An invalid image is dropped instead of
    /// failing the whole card.
    pub fn decode(bytes: &[u8]) -> Result<Self, LinkPreviewCardError> {
        check_len("card", bytes, MAX_CARD_LEN)?;
        let mut deserializer = minicbor_serde::Deserializer::new(bytes);
        let raw = RawCard::deserialize(&mut deserializer)?;
        if deserializer.decoder().position() != bytes.len() {
            return Err(LinkPreviewCardError::TrailingBytes);
        }

        check_len("url", &raw.url, MAX_URL_LEN)?;
        check_len("title", &raw.title, MAX_TITLE_LEN)?;
        check_len("description", &raw.description, MAX_DESCRIPTION_LEN)?;
        if raw.url.is_empty() {
            return Err(LinkPreviewCardError::EmptyUrl);
        }
        let title = sanitize(&raw.title);
        if title.is_empty() {
            return Err(LinkPreviewCardError::EmptyTitle);
        }

        Ok(Self {
            url: raw.url,
            title,
            description: sanitize(&raw.description),
            kind: raw.kind,
            image: raw.image.and_then(LinkPreviewImage::from_raw),
        })
    }

    /// Encodes the card to CBOR.
    pub fn encode(&self) -> Result<Vec<u8>, EncodeError<Infallible>> {
        let raw = RawCard {
            url: self.url.clone(),
            title: self.title.clone(),
            description: self.description.clone(),
            kind: self.kind,
            image: self.image.as_ref().map(LinkPreviewImage::to_raw),
        };
        minicbor_serde::to_vec(&raw)
    }

    /// The link exactly as written in the message.
    pub fn url(&self) -> &str {
        &self.url
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    /// Empty if the card has no description.
    pub fn description(&self) -> &str {
        &self.description
    }

    pub fn kind(&self) -> LinkPreviewKind {
        self.kind
    }

    pub fn image(&self) -> Option<&LinkPreviewImage> {
        self.image.as_ref()
    }
}

impl LinkPreviewImage {
    /// Builds an image on the sender side.
    ///
    /// Returns `None` if `part_index`, `width` or `height` is 0. A blur hash
    /// over its cap is dropped. `alt` is sanitised and truncated to its cap.
    pub fn new(
        part_index: u32,
        width: u32,
        height: u32,
        blur_hash: Option<String>,
        alt: Option<&str>,
    ) -> Option<Self> {
        if part_index == 0 || width == 0 || height == 0 {
            return None;
        }
        let blur_hash = blur_hash.filter(|h| !h.is_empty() && h.len() <= MAX_BLUR_HASH_LEN);
        let alt = alt
            .map(|alt| truncate(sanitize(alt), MAX_ALT_LEN))
            .filter(|alt| !alt.is_empty());
        Some(Self {
            part_index,
            width,
            height,
            blur_hash,
            alt,
        })
    }

    fn from_raw(raw: RawImage) -> Option<Self> {
        if raw.part_index == 0 || raw.width == 0 || raw.height == 0 {
            return None;
        }
        if raw
            .blur_hash
            .as_ref()
            .is_some_and(|h| h.len() > MAX_BLUR_HASH_LEN)
            || raw.alt.as_ref().is_some_and(|a| a.len() > MAX_ALT_LEN)
        {
            return None;
        }
        Some(Self {
            part_index: raw.part_index,
            width: raw.width,
            height: raw.height,
            blur_hash: raw.blur_hash.filter(|h| !h.is_empty()),
            alt: raw.alt.map(|a| sanitize(&a)).filter(|a| !a.is_empty()),
        })
    }

    fn to_raw(&self) -> RawImage {
        RawImage {
            part_index: self.part_index,
            width: self.width,
            height: self.height,
            blur_hash: self.blur_hash.clone(),
            alt: self.alt.clone(),
        }
    }

    /// `partIndex` of the image `ExternalPart` as defined in MIMI.
    pub fn part_index(&self) -> u32 {
        self.part_index
    }

    /// Width of the encoded image in pixels.
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Height of the encoded image in pixels.
    pub fn height(&self) -> u32 {
        self.height
    }

    /// Placeholder while downloading.
    pub fn blur_hash(&self) -> Option<&str> {
        self.blur_hash.as_deref()
    }

    /// Accessibility text.
    pub fn alt(&self) -> Option<&str> {
        self.alt.as_deref()
    }
}

/// Wire form of a [`LinkPreviewCard`].
///
/// ```cddl
/// LinkPreviewCard = {
///     1 => tstr .size (1..2048),     ; url
///     2 => tstr .size (1..256),      ; title
///   ? 3 => tstr .size (0..1024),     ; description
///   ? 4 => Kind,                     ; absent = default
///   ? 5 => Image,                    ; absent = no image
///   * uint => any                    ; unknown tags, ignored
/// }
/// ```
#[derive(Debug, SerializeTaggedMap, DeserializeTaggedMap)]
struct RawCard {
    #[tag(1)]
    url: String,
    #[tag(2)]
    title: String,
    #[tag(3)]
    description: String,
    #[tag(4)]
    kind: LinkPreviewKind,
    #[tag(5)]
    image: Option<RawImage>,
}

/// Wire form of a [`LinkPreviewImage`].
///
/// ```cddl
/// Image = {
///     1 => uint,                     ; part_index: partIndex of the image ExternalPart
///     2 => uint,                     ; width: pixels of the encoded image
///     3 => uint,                     ; height: pixels of the encoded image
///   ? 4 => tstr .size (0..64),       ; blur_hash: placeholder while downloading
///   ? 5 => tstr .size (0..512),      ; alt: accessibility text
///   * uint => any                    ; unknown tags, ignored
/// }
/// ```
#[derive(Debug, PartialEq, SerializeTaggedMap, DeserializeTaggedMap)]
struct RawImage {
    #[tag(1)]
    part_index: u32,
    #[tag(2)]
    width: u32,
    #[tag(3)]
    height: u32,
    #[tag(4)]
    blur_hash: Option<String>,
    #[tag(5)]
    alt: Option<String>,
}

fn check_len(
    field: &'static str,
    value: impl AsRef<[u8]>,
    max: usize,
) -> Result<(), LinkPreviewCardError> {
    if value.as_ref().len() > max {
        return Err(LinkPreviewCardError::TooLong { field });
    }
    Ok(())
}

/// Removes controls, bidi controls, U+200B and U+FEFF and collapses
/// whitespace.
///
/// Whitespace controls like tab, newline and U+2028 count as whitespace.
/// ZWJ and ZWNJ are kept, emoji sequences and some scripts need them.
fn sanitize(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut pending_space = false;
    for c in text.chars() {
        if is_removed(c) {
            continue;
        }
        if c.is_whitespace() {
            pending_space = !out.is_empty();
            continue;
        }
        if pending_space {
            out.push(' ');
            pending_space = false;
        }
        out.push(c);
    }
    out
}

/// Whether `sanitize` drops `c`.
fn is_removed(c: char) -> bool {
    match c {
        _ if c.is_whitespace() => false, // tab, newline, NEL become a space
        '\u{0000}'..='\u{001F}'          // C0 controls
        | '\u{007F}'..='\u{009F}'        // DEL, C1 controls
        | '\u{061C}'                     // Arabic letter mark
        | '\u{200B}'                     // zero width space
        | '\u{200E}'                     // LRM
        | '\u{200F}'                     // RLM
        | '\u{202A}'..='\u{202E}'        // LRE, RLE, PDF, LRO, RLO
        | '\u{2066}'..='\u{2069}'        // LRI, RLI, FSI, PDI
        | '\u{FEFF}' => true,            // byte order mark
        _ => false,
    }
}

fn truncate(mut text: String, max: usize) -> String {
    let len = text.floor_char_boundary(max);
    text.truncate(len);
    // Truncating may leave a trailing space.
    let trimmed = text.trim_end().len();
    text.truncate(trimmed);
    text
}

#[cfg(test)]
mod test {
    use super::*;

    fn cbor(diag: &str) -> Vec<u8> {
        cbor_diag::parse_diag(diag).unwrap().to_bytes()
    }

    fn minimal_card() -> LinkPreviewCard {
        LinkPreviewCard::new(
            "https://air.ms".to_owned(),
            "Air",
            "",
            LinkPreviewKind::Default,
            None,
        )
        .unwrap()
    }

    fn full_card() -> LinkPreviewCard {
        LinkPreviewCard::new(
            "https://github.com/openmls/openmls/pull/2046".to_owned(),
            "feat: P-384, ML-DSA · Pull Request #2046 · openmls/openmls",
            "Add post-quantum and P-384 ciphersuite support",
            LinkPreviewKind::Video,
            LinkPreviewImage::new(
                3,
                1024,
                512,
                Some("LKO2?U%2Tw=w]~RBVZRi};RPxuwH".to_owned()),
                Some("openmls repository card"),
            ),
        )
        .unwrap()
    }

    fn card_with_title(title: &str) -> Vec<u8> {
        let raw = RawCard {
            url: "https://air.ms".to_owned(),
            title: title.to_owned(),
            description: String::new(),
            kind: LinkPreviewKind::Default,
            image: None,
        };
        minicbor_serde::to_vec(&raw).unwrap()
    }

    #[test]
    fn wire_format_stability() {
        let bytes = full_card().encode().unwrap();
        let diag = cbor_diag::parse_bytes(&bytes).unwrap().to_hex();
        insta::assert_snapshot!(diag);
    }

    #[test]
    fn roundtrip() {
        for card in [minimal_card(), full_card()] {
            let decoded = LinkPreviewCard::decode(&card.encode().unwrap()).unwrap();
            assert_eq!(decoded, card);
        }
    }

    #[test]
    fn defaults_are_omitted() {
        let bytes = minimal_card().encode().unwrap();
        assert_eq!(bytes, cbor(r#"{1: "https://air.ms", 2: "Air"}"#));

        let image = RawImage {
            part_index: 3,
            width: 1024,
            height: 512,
            blur_hash: None,
            alt: None,
        };
        let bytes = minicbor_serde::to_vec(&image).unwrap();
        assert_eq!(bytes, cbor("{1: 3, 2: 1024, 3: 512}"));
    }

    #[test]
    fn kind_values() {
        let kinds = [
            (0, LinkPreviewKind::Default),
            (1, LinkPreviewKind::Video),
            (2, LinkPreviewKind::Music),
            (3, LinkPreviewKind::Profile),
            (4, LinkPreviewKind::Other(4)),
            (u64::MAX, LinkPreviewKind::Other(u64::MAX)),
        ];
        for (value, kind) in kinds {
            assert_eq!(LinkPreviewKind::from(value), kind);
            assert_eq!(u64::from(kind), value);
        }
    }

    #[test]
    fn unknown_kind_survives_reencoding() {
        let bytes = cbor(r#"{1: "https://air.ms", 2: "Air", 4: 7}"#);
        let card = LinkPreviewCard::decode(&bytes).unwrap();
        assert_eq!(card.kind(), LinkPreviewKind::Other(7));
        assert_eq!(card.encode().unwrap(), bytes);
    }

    #[test]
    fn explicit_default_kind_is_omitted_on_reencoding() {
        let bytes = cbor(r#"{1: "https://air.ms", 2: "Air", 4: 0}"#);
        let card = LinkPreviewCard::decode(&bytes).unwrap();
        assert_eq!(card.kind(), LinkPreviewKind::Default);
        assert_eq!(
            card.encode().unwrap(),
            cbor(r#"{1: "https://air.ms", 2: "Air"}"#)
        );
    }

    #[test]
    fn unknown_tags_are_ignored() {
        let bytes = cbor(
            r#"{
                1: "https://air.ms",
                2: "Air",
                5: {1: 3, 2: 4, 3: 5, 9: [1, "x"]},
                9: {"future": h'00'}
            }"#,
        );
        let card = LinkPreviewCard::decode(&bytes).unwrap();
        let expected = LinkPreviewCard::new(
            "https://air.ms".to_owned(),
            "Air",
            "",
            LinkPreviewKind::Default,
            LinkPreviewImage::new(3, 4, 5, None, None),
        )
        .unwrap();
        assert_eq!(card, expected);
    }

    #[test]
    fn duplicate_keys_are_rejected() {
        let bytes = cbor(r#"{1: "https://air.ms", 2: "Air", 1: "https://evil.example"}"#);
        assert!(LinkPreviewCard::decode(&bytes).is_err());
        let bytes = cbor(r#"{1: "https://air.ms", 2: "Air", 5: {1: 3, 1: 4}}"#);
        assert!(LinkPreviewCard::decode(&bytes).is_err());
    }

    #[test]
    fn trailing_bytes_are_rejected() {
        let mut bytes = minimal_card().encode().unwrap();
        bytes.push(0x00);
        assert!(matches!(
            LinkPreviewCard::decode(&bytes),
            Err(LinkPreviewCardError::TrailingBytes)
        ));
    }

    #[test]
    fn missing_url_or_title_is_rejected() {
        assert!(matches!(
            LinkPreviewCard::decode(&cbor("{}")),
            Err(LinkPreviewCardError::EmptyUrl)
        ));
        assert!(matches!(
            LinkPreviewCard::decode(&cbor(r#"{1: ""}"#)),
            Err(LinkPreviewCardError::EmptyUrl)
        ));
        assert!(matches!(
            LinkPreviewCard::decode(&cbor(r#"{1: "https://air.ms"}"#)),
            Err(LinkPreviewCardError::EmptyTitle)
        ));
        assert!(matches!(
            LinkPreviewCard::new(String::new(), "Air", "", LinkPreviewKind::Default, None),
            Err(LinkPreviewCardError::EmptyUrl)
        ));
    }

    #[test]
    fn whitespace_only_title_is_rejected() {
        assert!(matches!(
            LinkPreviewCard::decode(&card_with_title(" \n\t\u{200B} ")),
            Err(LinkPreviewCardError::EmptyTitle)
        ));
        assert!(matches!(
            LinkPreviewCard::new(
                "https://air.ms".to_owned(),
                " \u{FEFF} ",
                "",
                LinkPreviewKind::Default,
                None,
            ),
            Err(LinkPreviewCardError::EmptyTitle)
        ));
    }

    #[test]
    fn field_over_cap_is_rejected_on_decode() {
        let bytes = card_with_title(&"a".repeat(MAX_TITLE_LEN + 1));
        assert!(matches!(
            LinkPreviewCard::decode(&bytes),
            Err(LinkPreviewCardError::TooLong { field: "title" })
        ));

        let url = format!("https://air.ms/{}", "a".repeat(MAX_URL_LEN));
        assert!(matches!(
            LinkPreviewCard::new(url, "Air", "", LinkPreviewKind::Default, None),
            Err(LinkPreviewCardError::TooLong { field: "url" })
        ));
    }

    #[test]
    fn field_over_cap_is_truncated_by_sender() {
        // "ä" is 2 bytes and straddles the cap.
        let title = format!("{}ä", "a".repeat(MAX_TITLE_LEN - 1));
        let description = "b".repeat(MAX_DESCRIPTION_LEN + 10);
        let card = LinkPreviewCard::new(
            "https://air.ms".to_owned(),
            &title,
            &description,
            LinkPreviewKind::Default,
            None,
        )
        .unwrap();
        assert_eq!(card.title(), "a".repeat(MAX_TITLE_LEN - 1));
        assert_eq!(card.description().len(), MAX_DESCRIPTION_LEN);
        assert_eq!(
            LinkPreviewCard::decode(&card.encode().unwrap()).unwrap(),
            card
        );

        let alt = "c".repeat(MAX_ALT_LEN + 1);
        let image = LinkPreviewImage::new(1, 1, 1, None, Some(&alt)).unwrap();
        assert_eq!(image.alt().unwrap().len(), MAX_ALT_LEN);
    }

    #[test]
    fn invalid_image_is_dropped() {
        let images = [
            "{1: 0, 2: 4, 3: 5}",
            "{1: 3, 2: 0, 3: 5}",
            "{1: 3, 2: 4, 3: 0}",
            "{1: 3, 3: 5}",
        ];
        for image in images {
            let bytes = cbor(&format!(r#"{{1: "https://air.ms", 2: "Air", 5: {image}}}"#));
            let card = LinkPreviewCard::decode(&bytes).unwrap();
            assert_eq!(card.image(), None, "{image}");
        }
        assert_eq!(LinkPreviewImage::new(0, 4, 5, None, None), None);
        assert_eq!(LinkPreviewImage::new(3, 0, 5, None, None), None);
        assert_eq!(LinkPreviewImage::new(3, 4, 0, None, None), None);
    }

    #[test]
    fn oversize_blur_hash_drops_image() {
        let blur_hash = "x".repeat(MAX_BLUR_HASH_LEN + 1);
        let bytes = cbor(&format!(
            r#"{{1: "https://air.ms", 2: "Air", 5: {{1: 3, 2: 4, 3: 5, 4: "{blur_hash}"}}}}"#
        ));
        let card = LinkPreviewCard::decode(&bytes).unwrap();
        assert_eq!(card.image(), None);

        let image = LinkPreviewImage::new(3, 4, 5, Some(blur_hash), None).unwrap();
        assert_eq!(image.blur_hash(), None);
    }

    #[test]
    fn text_is_sanitized() {
        assert_eq!(sanitize("a \n\t b"), "a b");
        assert_eq!(sanitize("\u{202E}ab\u{200B}c\u{FEFF}\u{2066}"), "abc");
        assert_eq!(sanitize("a\u{0007}b\u{0085}c\u{2028}d"), "ab c d");
        // ZWJ and ZWNJ are kept: emoji sequences and Persian spelling
        let family = "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}";
        assert_eq!(sanitize(family), family);
        let persian = "\u{0645}\u{06CC}\u{200C}\u{062E}\u{0648}\u{0627}\u{0647}\u{0645}";
        assert_eq!(sanitize(persian), persian);

        let url = "https://air.ms/\u{200B}x";
        let raw = RawCard {
            url: url.to_owned(),
            title: " Air\u{200E}\nnews ".to_owned(),
            description: "a  \u{202A}b".to_owned(),
            kind: LinkPreviewKind::Default,
            image: Some(RawImage {
                part_index: 1,
                width: 1,
                height: 1,
                blur_hash: None,
                alt: Some("\u{200B} ".to_owned()),
            }),
        };
        let card = LinkPreviewCard::decode(&minicbor_serde::to_vec(&raw).unwrap()).unwrap();
        assert_eq!(card.url(), url);
        assert_eq!(card.title(), "Air news");
        assert_eq!(card.description(), "a b");
        assert_eq!(card.image().unwrap().alt(), None);
    }

    #[test]
    fn oversize_card_is_rejected_before_decoding() {
        let bytes = card_with_title(&"a".repeat(MAX_CARD_LEN));
        assert!(matches!(
            LinkPreviewCard::decode(&bytes),
            Err(LinkPreviewCardError::TooLong { field: "card" })
        ));
    }

    #[test]
    fn deeply_nested_unknown_tag_is_skipped() {
        let depth = MAX_CARD_LEN - 64;
        let mut bytes = cbor(r#"{1: "https://air.ms", 2: "Air", 9: 0}"#);
        bytes[0] = 0xA3;
        bytes.pop();
        bytes.extend(std::iter::repeat_n(0x81, depth));
        bytes.push(0x00);
        assert!(bytes.len() <= MAX_CARD_LEN);
        assert!(LinkPreviewCard::decode(&bytes).is_ok());
    }
}
