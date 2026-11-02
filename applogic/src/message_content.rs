// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircoreclient::AttachmentUrl;
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
            replaces: mimi_content.replaces,
            topic_id: mimi_content.topic_id,
            in_reply_to: mimi_content.in_reply_to,
            content: None,
            attachments: Default::default(),
        };

        match std::mem::take(&mut mimi_content.nested_part) {
            // multipart attachment message with ProcessAll semantics
            NestedPart::MultiPart {
                disposition: Disposition::Attachment,
                part_semantics: PartSemantics::ProcessAll,
                parts,
                ..
            } => {
                let Some(attachment) = convert_attachment(parts) else {
                    return res.error_message("Unsupported attachment message");
                };
                res.attachments = vec![attachment];
            }

            // single part message
            NestedPart::SinglePart {
                content,
                content_type,
                ..
            } if content_type == "text/markdown" => {
                let plain_body = String::from_utf8(content)
                    .unwrap_or_else(|_| "Invalid non-UTF8 message".to_owned());
                res.content = Some(MessageContent::parse_markdown(&plain_body));
                res.plain_body = Some(plain_body);
            }
            NestedPart::NullPart { .. } => {
                res.content = None;
            }

            // any other message
            part => {
                return res.error_message(format!("Unsupported message: {:?}", part.disposition()));
            }
        }

        res
    }
}

fn convert_attachment(parts: Vec<NestedPart>) -> Option<UnresolvedAttachment> {
    let mut attachment: Option<UnresolvedAttachment> = None;
    let mut blurhash: Option<String> = None;
    let mut dimensions: Option<(u32, u32)> = None;

    for part in parts {
        match part {
            // actual attachment
            NestedPart::ExternalPart {
                disposition: Disposition::Attachment,
                content_type,
                url,
                description,
                filename,
                size,
                ..
            } => {
                if attachment.is_some() {
                    warn!("Skipping duplicate attachment part");
                    continue;
                }

                let attachment_url: AttachmentUrl = match url.parse() {
                    Ok(url) => url,
                    Err(error) => {
                        warn!(%error, url, "Skipping attachment part with invalid url");
                        continue;
                    }
                };

                dimensions = attachment_url.dimensions();

                attachment = Some(UnresolvedAttachment {
                    filename,
                    content_type,
                    description: Some(description).filter(|d| !d.is_empty()),
                    size,
                    image_metadata: None,
                });
            }

            // blurhash preview
            NestedPart::SinglePart {
                disposition: Disposition::Preview,
                content,
                content_type,
                ..
            } if content_type == "text/blurhash" => {
                if blurhash.is_some() {
                    warn!("Skipping duplicate blurhash preview part");
                    continue;
                }
                let Ok(content) = String::from_utf8(content.to_vec()) else {
                    warn!("Skipping blurhash preview with non-UTF8 content");
                    continue;
                };
                blurhash = Some(content);
            }

            // other parts
            part => {
                warn!(
                    "Skipping unsupported attachment part: {:?}",
                    part.disposition()
                );
            }
        }
    }

    if let Some(attachment) = &mut attachment {
        match (blurhash, dimensions) {
            (blurhash, Some((width, height))) => {
                attachment.image_metadata = Some(UiImageMetadata {
                    blurhash,
                    width,
                    height,
                    // Classified locally, filled in at resolve time
                    is_animated: None,
                })
            }
            (Some(_), None) => {
                warn!("Invalid image attachment: missing dimensions, but blurhash is present")
            }
            (None, None) => (),
        }
    }

    attachment
}

#[cfg(test)]
mod tests {
    use aircommon::identifiers::RemoteAttachmentId;
    use mimi_content::content_container::{EncryptionAlgorithm, HashAlgorithm};
    use quickcheck_macros::quickcheck;
    use uuid::Uuid;

    use super::*;

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
    fn unsupported_attachment_preview_is_the_error() {
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
            Some("Unsupported attachment message")
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
    fn unsupported_message_preview_is_the_error() {
        let content = MimiContent {
            nested_part: NestedPart::SinglePart {
                disposition: Disposition::Render,
                language: String::new(),
                content_type: "text/plain".to_owned(),
                content: b"hello".to_vec(),
            },
            ..Default::default()
        };
        assert_eq!(
            content.preview_text().as_deref(),
            Some("Unsupported message: Render")
        );
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
