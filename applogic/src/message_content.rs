// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{io::Cursor, mem};

use aircoreclient::{AttachmentUrl, MimiContentExt as _, TextFormat, VisitedPart};
use mimi_content::{
    MimiContent,
    content_container::{Disposition, NestedPart, PartSemantics},
};
use tracing::warn;

use crate::api::{
    markdown::{FILE_PREVIEW, IMAGE_PREVIEW, MessageContent},
    message_content::{UiImageMetadata, UnresolvedAttachment, UnresolvedMimiContent},
};

pub(crate) trait MimiContentExt {
    fn plain_body(&self) -> Option<&str>;

    /// Plain text preview of the message.
    fn preview_text(&self) -> Option<String>;
}

impl MimiContentExt for MimiContent {
    // Message editing relies on this function returning the original input
    // again. When we add processing to the input or the plain_body function, we
    // need to adjust message editing.
    fn plain_body(&self) -> Option<&str> {
        match &self.nested_part {
            // single part message
            NestedPart::SinglePart {
                content,
                content_type,
                ..
            } if content_type == "text/markdown" => str::from_utf8(content).ok(),
            _ => None,
        }
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
            replaces: mem::take(&mut mimi_content.replaces),
            topic_id: mem::take(&mut mimi_content.topic_id),
            in_reply_to: mem::take(&mut mimi_content.in_reply_to),
            content: None,
            attachments: Vec::new(),
        };

        // A deleted message stays without content
        if let NestedPart::NullPart { .. } = mimi_content.nested_part {
            return res;
        }

        let mut markdown = Vec::new();
        let mut raw = Vec::new();
        let _infallible = mimi_content.visit_parts(|part| {
            match part {
                VisitedPart::Text { content, format } => {
                    markdown.push(match format {
                        TextFormat::Markdown => content.to_owned(),
                        TextFormat::Plain => escape_markdown(content),
                    });
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
                VisitedPart::Unknown(_) => {}
            }
            Ok(())
        });

        if markdown.is_empty() && res.attachments.is_empty() {
            return res.error_message("Unsupported message");
        }
        if !markdown.is_empty() {
            let source = markdown.join("\n\n");
            res.content = Some(MessageContent::parse_markdown(&source));
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
    index: usize,
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
            let dimensions = match url.parse::<AttachmentUrl>() {
                Ok(url) => url.dimensions(),
                Err(error) => {
                    warn!(%error, url, "Visited attachment with invalid url");
                    None
                }
            };
            Some(UnresolvedAttachment {
                index,
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
            let media_type = media_type(content_type);
            let dimensions = if media_type.starts_with("image/") {
                image::ImageReader::new(Cursor::new(content))
                    .with_guessed_format()
                    .ok()
                    .and_then(|reader| reader.into_dimensions().ok())
            } else {
                None
            };
            Some(UnresolvedAttachment {
                index,
                filename: inline_filename(&media_type),
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

/// Lowercased type and subtype, parameters dropped.
fn media_type(content_type: &str) -> String {
    let essence = match content_type.split_once(';') {
        Some((essence, _)) => essence,
        None => content_type,
    };
    essence.trim().to_ascii_lowercase()
}

/// File name for an inline attachment, which carries none.
fn inline_filename(media_type: &str) -> String {
    let extension = match media_type {
        "text/vcard" => "vcf",
        "text/calendar" => "ics",
        "text/html" => "html",
        "text/csv" => "csv",
        "text/plain" => "txt",
        "text/markdown" => "md",
        "application/pdf" => "pdf",
        "application/json" => "json",
        "image/png" => "png",
        "image/jpeg" => "jpg",
        "image/gif" => "gif",
        "image/webp" => "webp",
        _ => return "attachment".to_owned(),
    };
    format!("attachment.{extension}")
}

#[cfg(test)]
mod tests {
    use aircommon::identifiers::RemoteAttachmentId;
    use aircoreclient::{AttachmentId, AttachmentInfo};
    use mimi_content::content_container::{EncryptionAlgorithm, HashAlgorithm};
    use quickcheck_macros::quickcheck;
    use uuid::Uuid;

    use super::*;
    use crate::api::markdown::BlockElement;

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
        let content = markdown_message("**Hi**, see [the docs](https://example.com)\n\n- one");
        assert_eq!(
            content.preview_text().as_deref(),
            Some("Hi, see the docs\none")
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

    fn attachment_info(id: u128, part_index: u32) -> AttachmentInfo {
        AttachmentInfo {
            attachment_id: AttachmentId {
                uuid: Uuid::from_u128(id),
            },
            part_index,
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

    #[test]
    fn attachments_are_paired_with_infos_by_part_index() {
        let content = render_container(vec![
            external_part(Disposition::Preview, "image/webp", 1, "preview.webp"),
            external_part(Disposition::Attachment, "application/pdf", 2, "doc.pdf"),
        ]);
        let expected = attachment_info(0xb, 1).attachment_id;

        let infos = [attachment_info(0xa, 0), attachment_info(0xb, 1)];
        let resolved = UnresolvedMimiContent::from(content.clone()).resolve(&infos);
        assert_eq!(resolved.attachments.len(), 1);
        assert_eq!(resolved.attachments[0].filename, "doc.pdf");
        assert_eq!(resolved.attachments[0].attachment_id, expected);

        let infos = [attachment_info(0xb, 1), attachment_info(0xa, 0)];
        let resolved = UnresolvedMimiContent::from(content).resolve(&infos);
        assert_eq!(resolved.attachments.len(), 1);
        assert_eq!(resolved.attachments[0].filename, "doc.pdf");
        assert_eq!(resolved.attachments[0].attachment_id, expected);
    }

    #[test]
    fn inline_attachment_gets_a_generated_filename() {
        let content = render_container(vec![single_part(
            Disposition::Render,
            "text/vcard",
            b"BEGIN:VCARD",
        )]);
        let unresolved = UnresolvedMimiContent::from(content);
        assert_eq!(unresolved.attachments.len(), 1);
        let attachment = &unresolved.attachments[0];
        assert_eq!(attachment.index, 0);
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
