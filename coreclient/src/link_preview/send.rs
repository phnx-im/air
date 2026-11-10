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
    if !select_links(text).any(|link| link == preview.card().url()) {
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
