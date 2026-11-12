// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{io::Cursor, mem};

use aircoreclient::{
    AttachmentUrl, MediaType, MimiContentExt as _, PartIndex, TextFormat, VisitedPart,
    link_preview::LinkPreview,
};
use mimi_content::{MimiContent, content_container::NestedPart};
use tracing::warn;

use crate::api::{
    markdown::{FILE_PREVIEW, IMAGE_PREVIEW, MessageContent},
    message_content::{
        UiImageMetadata, UnresolvedAttachment, UnresolvedLinkPreview, UnresolvedMimiContent,
    },
};

pub(crate) trait MimiContentExt {
    fn plain_body(&self) -> Option<&str>;

    /// Plain text preview of the message.
    fn preview_text(&self) -> Option<String>;
}

impl MimiContentExt for MimiContent {
    fn plain_body(&self) -> Option<&str> {
        aircoreclient::MimiContentExt::markdown_text(self)
    }

    fn preview_text(&self) -> Option<String> {
        UnresolvedMimiContent::from(self.clone()).preview_text()
    }
}

impl UnresolvedMimiContent {
    fn error_message(mut self, message: impl Into<String>) -> Self {
        let message = message.into();
        self.plain_body = Some(message.clone());
        self.content = Some(MessageContent::error(message));
        self
    }

    /// Plain text preview of the message. Returns `None` when the message is
    /// deleted.
    pub(crate) fn preview_text(&self) -> Option<String> {
        if let Some(attachment) = self.attachments.first() {
            let preview = if attachment.image_metadata.is_some() {
                IMAGE_PREVIEW
            } else {
                FILE_PREVIEW
            };
            return Some(preview.to_owned());
        }
        let markdown = self.plain_body.as_deref().unwrap_or_default();
        self.content.as_ref()?.preview_text(markdown)
    }
}

impl From<MimiContent> for UnresolvedMimiContent {
    fn from(mut mimi_content: MimiContent) -> Self {
        let mut res = Self {
            plain_body: None,
            replaces: mimi_content
                .replaces
                .take()
                .map(|id| id.as_slice().to_vec()),
            topic_id: mem::take(&mut mimi_content.topic_id),
            in_reply_to: mimi_content
                .in_reply_to
                .take()
                .map(|id| id.as_slice().to_vec()),
            content: None,
            attachments: Vec::new(),
            link_preview: None,
        };

        // A deleted message stays without content
        if let NestedPart::NullPart { .. } = mimi_content.nested_part {
            return res;
        }

        // Parsed per part, so that syntax does not cross part boundaries
        let mut elements = Vec::new();
        let mut raw = Vec::new();
        let mut card = None;
        let _infallible = mimi_content.visit_parts(|part| {
            match part {
                VisitedPart::Text { content, format } => {
                    let parsed = match format {
                        TextFormat::Markdown => MessageContent::parse_markdown(content),
                        TextFormat::Plain => {
                            MessageContent::parse_markdown(&escape_markdown(content))
                        }
                    };
                    elements.extend(parsed.elements);
                    raw.push(content);
                }
                VisitedPart::Attachment { preview: true, .. } => {}
                VisitedPart::Attachment {
                    part,
                    index,
                    blurhash,
                    ..
                } => {
                    res.attachments
                        .extend(convert_attachment(part, index, blurhash));
                }
                VisitedPart::LinkPreview { card: bytes, .. } => {
                    card.get_or_insert(bytes);
                }
                VisitedPart::Unknown(_) => {}
            }
            Ok(())
        });
        res.link_preview = card
            .and_then(|card| LinkPreview::parse(card, &raw, &mimi_content))
            .map(|preview| {
                let image = preview.image().and_then(|(_, index)| {
                    convert_attachment(mimi_content.part_at(index)?, index, None)
                });
                UnresolvedLinkPreview { preview, image }
            });

        if raw.is_empty() && res.attachments.is_empty() {
            return res.error_message("Unsupported message");
        }
        if !raw.is_empty() {
            res.content = Some(MessageContent { elements });
            res.plain_body = Some(raw.join("\n\n"));
        }
        res
    }
}

/// Markdown source that renders `text` literally.
fn escape_markdown(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    let mut line_start = true;
    for c in text.chars() {
        // Indentation would start a code block
        if line_start && c == ' ' {
            escaped.push_str("&#32;");
            continue;
        }
        if line_start && c == '\t' {
            escaped.push_str("&#9;");
            continue;
        }
        line_start = c == '\n';
        if c.is_ascii_punctuation() {
            escaped.push('\\');
        }
        escaped.push(c);
    }
    escaped
}

fn convert_attachment(
    part: &NestedPart,
    index: PartIndex,
    blurhash: Option<&str>,
) -> Option<UnresolvedAttachment> {
    let image_metadata = |dimensions: Option<(u32, u32)>| {
        dimensions.map(|(width, height)| UiImageMetadata {
            blurhash: blurhash.map(str::to_owned),
            width,
            height,
            // Classified locally, filled in at resolve time
            is_animated: None,
        })
    };

    match part {
        NestedPart::ExternalPart {
            content_type,
            url,
            description,
            filename,
            size,
            ..
        } => {
            let (remote_attachment_id, dimensions) = match url.parse::<AttachmentUrl>() {
                Ok(url) => (Some(url.remote_attachment_id()), url.dimensions()),
                Err(error) => {
                    warn!(%error, url, "Visited attachment with invalid url");
                    (None, None)
                }
            };
            Some(UnresolvedAttachment {
                index,
                remote_attachment_id,
                filename: filename.clone(),
                content_type: content_type.clone(),
                description: Some(description.clone()).filter(|d| !d.is_empty()),
                size: *size,
                image_metadata: image_metadata(dimensions),
            })
        }
        NestedPart::SinglePart {
            content_type,
            content,
            ..
        } => {
            let media_type = MediaType::of(content_type);
            let dimensions = if media_type.has_type("image") {
                image::ImageReader::new(Cursor::new(content))
                    .with_guessed_format()
                    .ok()
                    .and_then(|reader| reader.into_dimensions().ok())
            } else {
                None
            };
            Some(UnresolvedAttachment {
                index,
                remote_attachment_id: None,
                filename: inline_filename(media_type),
                content_type: content_type.clone(),
                description: None,
                size: content.len() as u64,
                image_metadata: image_metadata(dimensions),
            })
        }
        NestedPart::NullPart { .. } | NestedPart::MultiPart { .. } => {
            warn!("Visited attachment is neither an external nor a single part");
            None
        }
    }
}

/// File name for an inline attachment, which carries none.
fn inline_filename(media_type: MediaType) -> String {
    const EXTENSIONS: &[(&str, &str)] = &[
        ("text/vcard", "vcf"),
        ("text/calendar", "ics"),
        ("text/html", "html"),
        ("text/csv", "csv"),
        ("text/plain", "txt"),
        ("text/markdown", "md"),
        ("application/pdf", "pdf"),
        ("application/json", "json"),
        ("image/png", "png"),
        ("image/jpeg", "jpg"),
        ("image/gif", "gif"),
        ("image/webp", "webp"),
    ];
    match EXTENSIONS.iter().find(|(known, _)| media_type == *known) {
        Some((_, extension)) => format!("attachment.{extension}"),
        None => "attachment".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use aircommon::identifiers::RemoteAttachmentId;
    use aircoreclient::{AttachmentId, AttachmentInfo, MessageAttachments, MessageId};
    use airprotos::client::link_preview::{LinkPreviewCard, LinkPreviewImage, LinkPreviewKind};
    use mimi_content::{
        Disposition,
        content_container::{EncryptionAlgorithm, HashAlgorithm, PartSemantics},
    };
    use quickcheck_macros::quickcheck;
    use uuid::Uuid;

    use super::*;
    use crate::api::{
        markdown::{BlockElement, RangedBlockElement},
        message_content::UiLinkPreviewKind,
    };

    fn markdown_message(markdown: &str) -> MimiContent {
        MimiContent::simple_markdown_message(markdown.to_owned(), [0; 16])
    }

    fn attachment_message(dimensions: Option<(u32, u32)>) -> MimiContent {
        let url = AttachmentUrl::new(RemoteAttachmentId::new(Uuid::nil()), dimensions);
        MimiContent {
            nested_part: NestedPart::MultiPart {
                disposition: Disposition::Attachment,
                language: String::new(),
                part_semantics: PartSemantics::ProcessAll,
                parts: vec![NestedPart::ExternalPart {
                    disposition: Disposition::Attachment,
                    language: String::new(),
                    content_type: "application/octet-stream".to_owned(),
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
                    filename: "file".to_owned(),
                }],
            },
            ..Default::default()
        }
    }

    #[test]
    fn markdown_preview_is_plain_text() {
        let content = markdown_message("**Hi**, see [the messenger](https://air.ms)\n\n- one");
        assert_eq!(
            content.preview_text().as_deref(),
            Some("Hi, see the messenger\none")
        );
    }

    #[test]
    fn markdown_preview_falls_back_to_source() {
        assert_eq!(
            markdown_message(" --- ").preview_text().as_deref(),
            Some("---")
        );
        assert_eq!(markdown_message("  ").preview_text(), None);
    }

    #[test]
    fn image_attachment_preview() {
        assert_eq!(
            attachment_message(Some((10, 20))).preview_text().as_deref(),
            Some(IMAGE_PREVIEW)
        );
    }

    #[test]
    fn file_attachment_preview() {
        assert_eq!(
            attachment_message(None).preview_text().as_deref(),
            Some(FILE_PREVIEW)
        );
    }

    #[test]
    fn empty_attachment_container_preview_is_the_error() {
        let content = MimiContent {
            nested_part: NestedPart::MultiPart {
                disposition: Disposition::Attachment,
                language: String::new(),
                part_semantics: PartSemantics::ProcessAll,
                parts: Vec::new(),
            },
            ..Default::default()
        };
        assert_eq!(
            content.preview_text().as_deref(),
            Some("Unsupported message")
        );
    }

    #[test]
    fn deleted_message_has_no_preview() {
        let content = MimiContent {
            nested_part: NestedPart::NullPart {
                disposition: Disposition::Render,
                language: String::new(),
            },
            ..Default::default()
        };
        assert_eq!(content.preview_text(), None);
    }

    #[test]
    fn plain_text_preview_is_the_literal_text() {
        let content = MimiContent {
            nested_part: NestedPart::SinglePart {
                disposition: Disposition::Render,
                language: String::new(),
                content_type: "text/plain".to_owned(),
                content: b"*hello* [x](y)".to_vec(),
            },
            ..Default::default()
        };
        assert_eq!(content.preview_text().as_deref(), Some("*hello* [x](y)"));
    }

    fn single_part(disposition: Disposition, content_type: &str, content: &[u8]) -> NestedPart {
        NestedPart::SinglePart {
            disposition,
            language: String::new(),
            content_type: content_type.to_owned(),
            content: content.to_vec(),
        }
    }

    fn external_part(
        disposition: Disposition,
        content_type: &str,
        id: u128,
        filename: &str,
    ) -> NestedPart {
        let url = AttachmentUrl::new(RemoteAttachmentId::new(Uuid::from_u128(id)), None);
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
            filename: filename.to_owned(),
        }
    }

    fn render_container(parts: Vec<NestedPart>) -> MimiContent {
        MimiContent {
            nested_part: NestedPart::MultiPart {
                disposition: Disposition::Render,
                language: String::new(),
                part_semantics: PartSemantics::ProcessAll,
                parts,
            },
            ..Default::default()
        }
    }

    fn attachment_info(attachment_id: AttachmentId) -> AttachmentInfo {
        AttachmentInfo {
            attachment_id,
            remote_attachment_id: None,
            is_animated: None,
        }
    }

    #[test]
    fn link_preview_body_hides_preview_parts() {
        let content = render_container(vec![
            single_part(
                Disposition::Render,
                "text/markdown",
                b"see https://example.com",
            ),
            single_part(
                Disposition::Preview,
                "application/vnd.air.link-preview+cbor",
                b"\x01\x02\x03",
            ),
            external_part(Disposition::Preview, "image/webp", 1, "preview.webp"),
        ]);
        assert_eq!(
            content.preview_text().as_deref(),
            Some("see https://example.com")
        );
        let unresolved = UnresolvedMimiContent::from(content);
        assert!(unresolved.attachments.is_empty());
        assert_eq!(
            unresolved.plain_body.as_deref(),
            Some("see https://example.com")
        );
    }

    fn link_preview_message() -> MimiContent {
        let image = LinkPreviewImage::new(3, 4, 3, Some("LEHV6nWB2yk8".to_owned()), Some("alt"));
        let card = LinkPreviewCard::new(
            "https://air.ms".to_owned(),
            "Air",
            "Secure messaging for everyone",
            LinkPreviewKind::Video,
            image,
        )
        .unwrap();
        render_container(vec![
            single_part(Disposition::Render, "text/markdown", b"see https://air.ms"),
            single_part(
                Disposition::Preview,
                LinkPreviewCard::CONTENT_TYPE,
                &card.encode().unwrap(),
            ),
            external_part(Disposition::Preview, "image/webp", 1, "preview.webp"),
        ])
    }

    #[test]
    fn link_preview_image_is_paired_with_its_record() {
        let content = link_preview_message();
        let message_id = MessageId::new(Uuid::from_u128(0x1234));
        let image_id = AttachmentId::derive(message_id, attachment_indices(&content)[0]);
        let infos = MessageAttachments::new(message_id, vec![attachment_info(image_id)]);

        let resolved = UnresolvedMimiContent::from(content).resolve(Some(&infos));
        assert!(resolved.attachments.is_empty());
        let preview = resolved.link_preview.unwrap();
        assert_eq!(preview.card.url, "https://air.ms");
        assert_eq!(preview.card.host, "air.ms");
        assert_eq!(preview.card.title, "Air");
        assert_eq!(preview.card.kind, UiLinkPreviewKind::Video);
        let image = preview.image.unwrap();
        assert_eq!(image.attachment.attachment_id, image_id);
        assert_eq!(image.attachment.filename, "preview.webp");
        assert_eq!(
            image.attachment.image_metadata,
            Some(UiImageMetadata {
                blurhash: Some("LEHV6nWB2yk8".to_owned()),
                width: 4,
                height: 3,
                is_animated: Some(false),
            })
        );
        assert_eq!(image.alt.as_deref(), Some("alt"));
    }

    #[test]
    fn link_preview_without_records_has_no_image() {
        let resolved = UnresolvedMimiContent::from(link_preview_message()).resolve(None);
        let preview = resolved.link_preview.unwrap();
        assert!(preview.image.is_none());
    }

    #[test]
    fn link_preview_needs_its_link_in_the_text() {
        let mut content = link_preview_message();
        if let NestedPart::MultiPart { parts, .. } = &mut content.nested_part {
            parts[0] = single_part(Disposition::Render, "text/markdown", b"no link");
        }
        let resolved = UnresolvedMimiContent::from(content).resolve(None);
        assert!(resolved.link_preview.is_none());
        assert!(resolved.attachments.is_empty());
    }

    fn attachment_indices(content: &MimiContent) -> Vec<PartIndex> {
        let mut indices = Vec::new();
        content
            .visit_parts(|part| {
                if let VisitedPart::Attachment { index, .. } = part {
                    indices.push(index);
                }
                Ok(())
            })
            .unwrap();
        indices
    }

    #[test]
    fn attachments_are_paired_with_infos_by_derived_id() {
        let content = render_container(vec![
            external_part(Disposition::Preview, "image/webp", 1, "preview.webp"),
            external_part(Disposition::Attachment, "application/pdf", 2, "doc.pdf"),
        ]);
        let message_id = MessageId::new(Uuid::from_u128(0x1234));
        let indices = attachment_indices(&content);
        let expected = AttachmentId::derive(message_id, indices[1]);

        let infos = MessageAttachments::new(
            message_id,
            vec![
                attachment_info(AttachmentId::derive(message_id, indices[0])),
                attachment_info(expected),
            ],
        );
        let resolved = UnresolvedMimiContent::from(content.clone()).resolve(Some(&infos));
        assert_eq!(resolved.attachments.len(), 1);
        assert_eq!(resolved.attachments[0].filename, "doc.pdf");
        assert_eq!(resolved.attachments[0].attachment_id, expected);

        let infos = MessageAttachments::new(
            message_id,
            vec![
                attachment_info(expected),
                attachment_info(AttachmentId::derive(message_id, indices[0])),
            ],
        );
        let resolved = UnresolvedMimiContent::from(content).resolve(Some(&infos));
        assert_eq!(resolved.attachments.len(), 1);
        assert_eq!(resolved.attachments[0].filename, "doc.pdf");
        assert_eq!(resolved.attachments[0].attachment_id, expected);
    }

    #[test]
    fn attachments_fall_back_to_the_remote_id() {
        let content = render_container(vec![
            external_part(Disposition::Preview, "image/webp", 1, "preview.webp"),
            external_part(Disposition::Attachment, "application/pdf", 2, "doc.pdf"),
        ]);
        let message_id = MessageId::new(Uuid::from_u128(0x1234));
        // Not derived, like rows stored before ids were derived
        let stored = AttachmentId {
            uuid: Uuid::from_u128(0xb),
        };
        let infos = MessageAttachments::new(
            message_id,
            vec![AttachmentInfo {
                remote_attachment_id: Some(RemoteAttachmentId::new(Uuid::from_u128(2))),
                ..attachment_info(stored)
            }],
        );
        let resolved = UnresolvedMimiContent::from(content).resolve(Some(&infos));
        assert_eq!(resolved.attachments.len(), 1);
        assert_eq!(resolved.attachments[0].attachment_id, stored);
    }

    #[test]
    fn attachment_without_record_shows_an_error() {
        let content = render_container(vec![external_part(
            Disposition::Attachment,
            "application/pdf",
            2,
            "doc.pdf",
        )]);
        let message_id = MessageId::new(Uuid::from_u128(0x1234));
        let infos = MessageAttachments::new(message_id, Vec::new());

        let resolved = UnresolvedMimiContent::from(content.clone()).resolve(Some(&infos));
        assert!(resolved.attachments.is_empty());
        let elements = resolved.content.unwrap().elements;
        let [
            RangedBlockElement {
                element: BlockElement::Error(_),
                ..
            },
        ] = &elements[..]
        else {
            panic!("expected an error, got {elements:?}");
        };
        assert_eq!(resolved.preview_text.as_deref(), Some(FILE_PREVIEW));

        // Records not loaded
        let resolved = UnresolvedMimiContent::from(content).resolve(None);
        assert!(resolved.content.is_none());
    }

    #[test]
    fn inline_attachment_gets_a_generated_filename() {
        let content = render_container(vec![single_part(
            Disposition::Render,
            "text/vcard",
            b"BEGIN:VCARD",
        )]);
        let indices = attachment_indices(&content);
        let unresolved = UnresolvedMimiContent::from(content);
        assert_eq!(unresolved.attachments.len(), 1);
        let attachment = &unresolved.attachments[0];
        assert_eq!(attachment.index, indices[0]);
        assert_eq!(attachment.filename, "attachment.vcf");
        assert_eq!(attachment.size, 11);
        assert_eq!(attachment.content_type, "text/vcard");
        assert!(attachment.image_metadata.is_none());
    }

    #[test]
    fn inline_image_has_dimensions_and_blurhash() {
        let mut bytes = Vec::new();
        image::RgbImage::new(3, 2)
            .write_to(&mut Cursor::new(&mut bytes), image::ImageFormat::Png)
            .unwrap();
        let content = render_container(vec![
            single_part(Disposition::Render, "image/png", &bytes),
            single_part(Disposition::Preview, "text/blurhash", b"LEHV6nWB2yk8"),
        ]);
        let unresolved = UnresolvedMimiContent::from(content);
        assert_eq!(unresolved.attachments.len(), 1);
        let attachment = &unresolved.attachments[0];
        assert_eq!(attachment.filename, "attachment.png");
        let metadata = attachment.image_metadata.as_ref().unwrap();
        assert_eq!((metadata.width, metadata.height), (3, 2));
        assert_eq!(metadata.blurhash.as_deref(), Some("LEHV6nWB2yk8"));
    }

    #[test]
    fn plain_text_indentation_is_not_a_code_block() {
        let content = MimiContent {
            nested_part: single_part(Disposition::Render, "text/plain", b"    indented\n*star*"),
            ..Default::default()
        };
        let unresolved = UnresolvedMimiContent::from(content);
        let elements = &unresolved.content.as_ref().unwrap().elements;
        assert!(
            !elements
                .iter()
                .any(|e| matches!(e.element, BlockElement::CodeBlock(_))),
            "{elements:?}"
        );
        let preview = unresolved.preview_text().unwrap();
        assert!(preview.contains("*star*"), "{preview:?}");
        assert!(preview.contains("indented"), "{preview:?}");
    }

    #[test]
    fn markdown_does_not_cross_part_boundaries() {
        let content = render_container(vec![
            single_part(Disposition::Render, "text/markdown", b"```\ncode"),
            single_part(Disposition::Render, "text/plain", b"*literal*"),
        ]);
        let unresolved = UnresolvedMimiContent::from(content);
        let elements = &unresolved.content.as_ref().unwrap().elements;
        assert_eq!(elements.len(), 2, "{elements:?}");
        assert!(
            matches!(elements[0].element, BlockElement::CodeBlock(_)),
            "{elements:?}"
        );
        assert!(
            matches!(elements[1].element, BlockElement::Paragraph(_)),
            "{elements:?}"
        );
        let paragraph = MessageContent {
            elements: vec![elements[1].clone()],
        };
        assert_eq!(paragraph.plain_text(), "*literal*");
    }

    #[quickcheck]
    fn non_blank_message_has_non_blank_preview(markdown: String) -> bool {
        let preview = markdown_message(&markdown).preview_text();
        match preview {
            Some(preview) => !preview.trim().is_empty(),
            None => markdown.trim().is_empty(),
        }
    }

    #[quickcheck]
    fn text_without_syntax_is_kept(text: String) -> bool {
        let text: String = text
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == ' ')
            .collect();
        let expected = Some(text.trim()).filter(|text| !text.is_empty());
        markdown_message(&text).preview_text().as_deref() == expected
    }
}
