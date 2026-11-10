// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use airprotos::client::link_preview::{LinkPreviewCard, LinkPreviewImage};
use mimi_content::{Disposition, MimiContent, NestedPart};
use url::Url;

use crate::{MediaType, MimiContentExt as _, PartIndex, VisitedPart, link_preview::select_links};

#[derive(Debug)]
pub struct LinkPreview {
    card: LinkPreviewCard,
    host: String,
    image: Option<PartIndex>,
}

impl LinkPreview {
    /// Parses `card` and checks against the rendered `texts` of `content`.
    pub fn parse(card: &[u8], texts: &[&str], content: &MimiContent) -> Option<Self> {
        let card = LinkPreviewCard::decode(card).ok()?;
        if !texts
            .iter()
            .any(|text| select_links(text).any(|link| link == card.url()))
        {
            return None;
        }
        let host = Url::parse(card.url()).ok()?.host_str()?.to_owned();
        let image = card
            .image()
            .map(|image| PartIndex(image.part_index()))
            .filter(|&index| content.part_at(index).is_some_and(is_preview_image));
        let card = match image {
            Some(_) => card,
            None => card.without_image(),
        };
        Some(Self { card, host, image })
    }

    /// First link preview in `content`.
    pub(crate) fn from_content(content: &MimiContent) -> Option<Self> {
        let mut texts = Vec::new();
        let mut card = None;
        content
            .visit_parts(|visited_part| {
                match visited_part {
                    VisitedPart::Text { content, .. } => {
                        texts.push(content);
                    }
                    VisitedPart::LinkPreview { card: bytes, .. } => {
                        card.get_or_insert(bytes);
                    }
                    _ => {}
                }
                Ok(())
            })
            .ok()?;
        Self::parse(card?, texts.as_slice(), content)
    }

    pub(crate) fn into_card(self) -> LinkPreviewCard {
        self.card
    }

    pub fn card(&self) -> &LinkPreviewCard {
        &self.card
    }

    pub fn host(&self) -> &str {
        &self.host
    }

    pub fn image(&self) -> Option<(&LinkPreviewImage, PartIndex)> {
        self.card.image().zip(self.image)
    }
}

/// Checks if `part` is a preview with an image content type.
pub(super) fn is_preview_image(part: &NestedPart) -> bool {
    match part {
        NestedPart::ExternalPart {
            disposition: Disposition::Preview,
            content_type,
            ..
        } => MediaType::of(content_type).has_type("image"),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use aircommon::identifiers::RemoteAttachmentId;
    use airprotos::client::link_preview::LinkPreviewKind;
    use mimi_content::content_container::{EncryptionAlgorithm, HashAlgorithm, PartSemantics};
    use uuid::Uuid;

    use super::*;
    use crate::AttachmentUrl;

    fn single(disposition: Disposition, content_type: &str, content: &[u8]) -> NestedPart {
        NestedPart::SinglePart {
            disposition,
            language: String::new(),
            content_type: content_type.to_owned(),
            content: content.to_vec(),
        }
    }

    fn text(s: &str) -> NestedPart {
        single(Disposition::Render, "text/markdown", s.as_bytes())
    }

    fn card(url: &str, image: Option<u32>) -> NestedPart {
        let image = image.and_then(|index| LinkPreviewImage::new(index, 4, 3, None, None));
        let card =
            LinkPreviewCard::new(url.to_owned(), "title", "", LinkPreviewKind::Default, image)
                .unwrap();
        single(
            Disposition::Preview,
            LinkPreviewCard::CONTENT_TYPE,
            &card.encode().unwrap(),
        )
    }

    fn external(disposition: Disposition, content_type: &str) -> NestedPart {
        let url = AttachmentUrl::new(RemoteAttachmentId::new(Uuid::new_v4()), None);
        NestedPart::ExternalPart {
            disposition,
            language: String::new(),
            content_type: content_type.to_owned(),
            url: url.to_string(),
            expires: 0,
            size: 1,
            enc_alg: EncryptionAlgorithm::None,
            key: Vec::new(),
            nonce: Vec::new(),
            aad: Vec::new(),
            hash_alg: HashAlgorithm::Unspecified,
            content_hash: Vec::new(),
            description: String::new(),
            filename: String::new(),
        }
    }

    fn image() -> NestedPart {
        external(Disposition::Preview, "image/webp")
    }

    fn multi(part_semantics: PartSemantics, parts: Vec<NestedPart>) -> NestedPart {
        NestedPart::MultiPart {
            disposition: Disposition::Render,
            language: String::new(),
            part_semantics,
            parts,
        }
    }

    fn all(parts: Vec<NestedPart>) -> NestedPart {
        multi(PartSemantics::ProcessAll, parts)
    }

    fn preview(nested_part: NestedPart) -> Option<LinkPreview> {
        LinkPreview::from_content(&MimiContent {
            nested_part,
            ..Default::default()
        })
    }

    fn url_and_image(preview: Option<LinkPreview>) -> Option<(String, Option<u32>)> {
        let preview = preview?;
        let image = preview.image().map(|(_, PartIndex(index))| index);
        Some((preview.card().url().to_owned(), image))
    }

    #[test]
    fn root_layout() {
        let preview = preview(all(vec![
            text("see https://a.org"),
            card("https://a.org", Some(3)),
            image(),
        ]));
        assert_eq!(
            url_and_image(preview),
            Some(("https://a.org".to_owned(), Some(3)))
        );
    }

    #[test]
    fn host_of_url() {
        let preview = preview(all(vec![
            text("https://Air.ms/x"),
            card("https://Air.ms/x", None),
        ]));
        assert_eq!(preview.unwrap().host(), "air.ms");
    }

    #[test]
    fn first_card_wins() {
        // root 0, layout A 1, text 2, card 3, image 4, layout B 5, text 6, card 7
        let preview = preview(all(vec![
            all(vec![
                text("https://a.org"),
                card("https://a.org", Some(4)),
                image(),
            ]),
            all(vec![text("https://b.org"), card("https://b.org", None)]),
        ]));
        assert_eq!(
            url_and_image(preview),
            Some(("https://a.org".to_owned(), Some(4)))
        );
    }

    #[test]
    fn nested_card_after_attachment() {
        // root 0, attachment 1, layout 2, text 3, card 4, image 5
        let preview = preview(all(vec![
            external(Disposition::Attachment, "image/png"),
            all(vec![
                text("https://b.org"),
                card("https://b.org", Some(5)),
                image(),
            ]),
        ]));
        assert_eq!(
            url_and_image(preview),
            Some(("https://b.org".to_owned(), Some(5)))
        );
    }

    #[test]
    fn card_before_text() {
        let preview = preview(all(vec![
            card("https://a.org", None),
            text("https://a.org"),
        ]));
        assert!(preview.is_some());
    }

    #[test]
    fn link_in_any_rendered_text() {
        let preview = preview(all(vec![
            text("https://a.org"),
            all(vec![text("see above"), card("https://a.org", None)]),
        ]));
        assert!(preview.is_some());
    }

    #[test]
    fn url_must_be_a_selected_link() {
        for body in [
            "https://a.org/x",
            "no link",
            "http://a.org",
            "xhttps://a.org",
        ] {
            let preview = preview(all(vec![text(body), card("https://a.org", None)]));
            assert!(preview.is_none(), "{body}");
        }
    }

    #[test]
    fn no_card_without_text() {
        assert!(preview(all(vec![card("https://a.org", None)])).is_none());
    }

    #[test]
    fn link_in_unchosen_alternative() {
        // Tie, so the first alternative is chosen
        let preview = preview(all(vec![
            multi(
                PartSemantics::ChooseOne,
                vec![text("x"), text("https://a.org")],
            ),
            card("https://a.org", None),
        ]));
        assert!(preview.is_none());
    }

    #[test]
    fn only_first_card_counts() {
        // Only the first card counts, even if it does not decode
        let preview = preview(all(vec![
            text("https://a.org"),
            single(Disposition::Preview, LinkPreviewCard::CONTENT_TYPE, b"\x00"),
            card("https://a.org", None),
        ]));
        assert!(preview.is_none());
    }

    #[test]
    fn image_must_be_a_preview_image() {
        // text 1, card 2, attachment 3
        for index in [1, 2, 3, 9] {
            let preview = preview(all(vec![
                text("https://a.org"),
                card("https://a.org", Some(index)),
                external(Disposition::Attachment, "image/png"),
            ]));
            let preview = preview.unwrap();
            assert!(preview.image().is_none(), "{index}");
            assert!(preview.card().image().is_none(), "{index}");
        }
    }

    #[test]
    fn preview_image_must_be_an_image() {
        let preview = preview(all(vec![
            text("https://a.org"),
            card("https://a.org", Some(3)),
            external(Disposition::Preview, "application/pdf"),
        ]));
        assert!(preview.unwrap().image().is_none());
    }
}
