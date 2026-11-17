// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use airprotos::client::link_preview::LinkPreviewCard;
use anyhow::ensure;
use mimi_content::{Disposition, MimiContent, NestedPart, content_container::PartSemantics};

use crate::{
    AttachmentId, ChatId, MarkChatAsRead, MediaType, MessageId, MimiContentExt, PartIndex,
    clients::{
        CoreUser,
        attachment::upload::{PendingUpload, provisional_image_part},
    },
    link_preview::card::is_preview_image,
    media_type::TEXT_MARKDOWN,
};

use super::{card::LinkPreview, fetch::FetchedLinkPreview, links::select_links};

pub(crate) const LINK_PREVIEW_IMAGE_PART_INDEX: PartIndex = PartIndex(3);

impl CoreUser {
    pub async fn send_link_preview_message(
        &self,
        chat_id: ChatId,
        mut content: MimiContent,
        preview: FetchedLinkPreview,
        mark_as_read: MarkChatAsRead,
    ) -> anyhow::Result<Option<PendingUpload>> {
        ensure!(
            markdown_text(&content.nested_part).is_some(),
            "link preview message must have a markdown text body"
        );

        let (card, image) = preview.into_parts();
        debug_assert_eq!(card.image().is_some(), image.is_some());

        let Some((image_meta, bytes)) = card.image().zip(image) else {
            // Card without an image
            content.nested_part = link_preview_body(content.nested_part, card_part(&card)?, None);
            debug_assert!(LinkPreview::from_content(&content).is_some());
            self.send_message(chat_id, content, None, mark_as_read)
                .await?;
            return Ok(None);
        };

        // Card with an image
        let message_id = MessageId::random();
        let attachment_id = AttachmentId::derive(message_id, LINK_PREVIEW_IMAGE_PART_INDEX);
        let image_part = provisional_image_part(
            attachment_id,
            Disposition::Preview,
            &bytes,
            (image_meta.width(), image_meta.height()),
        );
        content.nested_part =
            link_preview_body(content.nested_part, card_part(&card)?, Some(image_part));
        debug_assert!(LinkPreview::from_content(&content).is_some_and(|p| p.image().is_some()));
        let upload = self
            .upload_processed_attachment(
                chat_id,
                message_id,
                attachment_id,
                content,
                bytes,
                mark_as_read,
            )
            .await?;
        Ok(Some(upload))
    }
}

pub(crate) fn carry_over_link_preview(
    original: &MimiContent,
    mut content: MimiContent,
) -> MimiContent {
    let Some(text) = markdown_text(&content.nested_part) else {
        return content;
    };
    let Some(preview) = LinkPreview::from_content(original) else {
        return content;
    };
    if select_links(text).next() != Some(preview.card().url()) {
        return content;
    }
    let image = preview
        .image()
        .filter(|&(_, index)| index == LINK_PREVIEW_IMAGE_PART_INDEX)
        .and_then(|(_, index)| original.part_at(index))
        .cloned();
    let mut card = preview.into_card();
    if image.is_none() {
        card = card.without_image();
    }
    let Some(card) = card_part(&card).ok() else {
        return content;
    };
    content.nested_part = link_preview_body(content.nested_part, card, image);
    content
}

/// Builds link preview layout from its parts.
///
/// ```text
/// MultiPart  render, processAll        ; partIndex 0
/// ├─ text part                         ; 1
/// ├─ card part                         ; 2
/// └─ image part (optional)             ; 3 = LINK_PREVIEW_IMAGE_PART_INDEX
/// ```
fn link_preview_body(text: NestedPart, card: NestedPart, image: Option<NestedPart>) -> NestedPart {
    NestedPart::MultiPart {
        disposition: Disposition::Render,
        language: String::new(),
        part_semantics: PartSemantics::ProcessAll,
        parts: [text, card].into_iter().chain(image).collect(),
    }
}

fn card_part(card: &LinkPreviewCard) -> anyhow::Result<NestedPart> {
    Ok(NestedPart::SinglePart {
        disposition: Disposition::Preview,
        language: String::new(),
        content_type: LinkPreviewCard::CONTENT_TYPE.to_owned(),
        content: card.encode()?,
    })
}

pub(crate) fn markdown_text(part: &NestedPart) -> Option<&str> {
    match part {
        NestedPart::SinglePart {
            disposition: Disposition::Render,
            content_type,
            content,
            ..
        } if MediaType::of(content_type) == TEXT_MARKDOWN => str::from_utf8(content).ok(),
        _ => None,
    }
}

/// Text an edit of this message starts from.
pub(crate) fn editable_text(part: &NestedPart) -> Option<&str> {
    markdown_text(part).or_else(|| link_preview_body_text(part))
}

fn link_preview_body_text(part: &NestedPart) -> Option<&str> {
    let NestedPart::MultiPart {
        disposition: Disposition::Render,
        part_semantics: PartSemantics::ProcessAll,
        parts,
        ..
    } = part
    else {
        return None;
    };
    let (text, card, image) = match parts.as_slice() {
        [text, card] => (text, card, None),
        [text, card, image] => (text, card, Some(image)),
        _ => return None,
    };
    if !is_card(card) || image.is_some_and(|image| !is_preview_image(image)) {
        return None;
    }
    markdown_text(text)
}

/// Checks if `part` is preview link card.
fn is_card(part: &NestedPart) -> bool {
    match part {
        NestedPart::SinglePart {
            disposition: Disposition::Preview,
            content_type,
            ..
        } => MediaType::of(content_type) == LinkPreviewCard::CONTENT_TYPE,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use aircommon::identifiers::RemoteAttachmentId;
    use airprotos::client::link_preview::{LinkPreviewImage, LinkPreviewKind};
    use mimi_content::content_container::{EncryptionAlgorithm, HashAlgorithm};
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
        card_part(&card).unwrap()
    }

    fn external(disposition: Disposition) -> NestedPart {
        let url = AttachmentUrl::new(RemoteAttachmentId::new(Uuid::new_v4()), None);
        NestedPart::ExternalPart {
            disposition,
            language: String::new(),
            content_type: "image/webp".to_owned(),
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

    fn content(nested_part: NestedPart) -> MimiContent {
        MimiContent {
            nested_part,
            ..Default::default()
        }
    }

    fn markdown(s: &str) -> MimiContent {
        MimiContent::simple_markdown_message(s.to_owned(), [0; 16])
    }

    #[test]
    fn editable_text_of_our_bodies() {
        let link = "https://a.org";
        for part in [
            text(link),
            all(vec![text(link), card(link, None)]),
            all(vec![
                text(link),
                card(link, Some(3)),
                external(Disposition::Preview),
            ]),
        ] {
            assert_eq!(editable_text(&part), Some(link));
        }
    }

    #[test]
    fn editable_text_of_other_bodies() {
        let link = "https://a.org";
        for part in [
            single(Disposition::Attachment, "text/markdown", link.as_bytes()),
            single(Disposition::Render, "text/plain", link.as_bytes()),
            all(vec![card(link, None), text(link)]),
            all(vec![text(link), text(link)]),
            all(vec![
                text(link),
                card(link, Some(3)),
                external(Disposition::Attachment),
            ]),
            all(vec![all(vec![text(link), card(link, None)])]),
            multi(PartSemantics::ChooseOne, vec![text(link), card(link, None)]),
        ] {
            assert_eq!(editable_text(&part), None, "{part:?}");
        }
    }

    #[test]
    fn carry_over_card_and_image() {
        let link = "https://a.org";
        let image = external(Disposition::Preview);
        let original = content(all(vec![
            text(&format!("see {link}")),
            card(link, Some(3)),
            image.clone(),
        ]));
        let edited = carry_over_link_preview(&original, markdown(&format!("now {link}")));

        assert_eq!(
            editable_text(&edited.nested_part),
            Some("now https://a.org")
        );
        let preview = LinkPreview::from_content(&edited).unwrap();
        assert_eq!(preview.card().url(), link);
        let (_, index) = preview.image().unwrap();
        assert_eq!(index, LINK_PREVIEW_IMAGE_PART_INDEX);
        assert_eq!(edited.part_at(index), Some(&image));
    }

    #[test]
    fn carry_over_without_link_drops_card() {
        let link = "https://a.org";
        let original = content(all(vec![
            text(link),
            card(link, Some(3)),
            external(Disposition::Preview),
        ]));
        let edit = markdown("no link");
        let edited = carry_over_link_preview(&original, edit.clone());
        assert_eq!(edited, edit);
    }

    #[test]
    fn carry_over_drops_image_elsewhere() {
        // root 0, layout 1, text 2, card 3, image 4
        let link = "https://a.org";
        let original = content(all(vec![all(vec![
            text(link),
            card(link, Some(4)),
            external(Disposition::Preview),
        ])]));
        assert!(
            LinkPreview::from_content(&original)
                .unwrap()
                .image()
                .is_some()
        );

        let edited = carry_over_link_preview(&original, markdown(link));
        let preview = LinkPreview::from_content(&edited).unwrap();
        assert!(preview.image().is_none());
        assert!(preview.card().image().is_none());
        assert_eq!(edited.part_at(PartIndex(3)), None);
    }

    #[test]
    fn carry_over_drops_card_of_later_link() {
        let link = "https://a.org";
        let original = content(all(vec![text(link), card(link, None)]));
        let edit = markdown(&format!("https://b.org and {link}"));
        let edited = carry_over_link_preview(&original, edit.clone());
        assert_eq!(edited, edit);
    }

    #[test]
    fn carry_over_without_card() {
        let edit = markdown("https://a.org");
        let edited = carry_over_link_preview(&markdown("https://a.org"), edit.clone());
        assert_eq!(edited, edit);
    }
}
