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
    markdown::MessageContent,
    message_content::{UiAttachment, UiImageMetadata, UiMimiContent},
};

pub(crate) trait MimiContentExt {
    fn plain_body(&self) -> Option<&str>;
}

impl MimiContentExt for MimiContent {
    // Message editing relies on this function returning the original input again. When we add processing to the input or the plain_body function, we need to adjust message editing.
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
}

impl UiMimiContent {
    fn error_message(mut self, message: impl Into<String>) -> Self {
        let message = message.into();
        self.plain_body = Some(message.clone());
        self.content = Some(MessageContent::error(message));
        self
    }
}

impl From<MimiContent> for UiMimiContent {
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

fn convert_attachment(parts: Vec<NestedPart>) -> Option<UiAttachment> {
    let mut attachment: Option<UiAttachment> = None;
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

                attachment = Some(UiAttachment {
                    attachment_id: attachment_url.attachment_id(),
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
            (Some(blurhash), Some((width, height))) => {
                attachment.image_metadata = Some(UiImageMetadata {
                    blurhash,
                    width,
                    height,
                })
            }
            (None, Some(_)) => {
                warn!("Invalid image attachment: missing blurhash, but dimensions are present")
            }
            (Some(_), None) => {
                warn!("Invalid image attachment: missing dimensions, but blurhash is present")
            }
            (None, None) => (),
        }
    }

    attachment
}
