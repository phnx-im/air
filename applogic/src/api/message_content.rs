// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::identifiers::{MimiId, RemoteAttachmentId};
pub(crate) use aircoreclient::AttachmentId;
use aircoreclient::{MessageAttachments, PartIndex};
use flutter_rust_bridge::frb;
use uuid::Uuid;

use crate::api::markdown::MessageContent;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
#[doc(hidden)]
#[frb(dart_metadata = ("freezed"))]
pub struct UiMimiId(pub(crate) [u8; 32]);

impl From<MimiId> for UiMimiId {
    fn from(id: MimiId) -> Self {
        UiMimiId(id.into_inner())
    }
}

impl From<UiMimiId> for MimiId {
    fn from(id: UiMimiId) -> Self {
        Self::from(id.0)
    }
}

impl From<UiMimiId> for Vec<u8> {
    fn from(id: UiMimiId) -> Self {
        Vec::from(id.0)
    }
}

/// Mirror of the [`AttachmentId`] type
#[doc(hidden)]
#[frb(mirror(AttachmentId))]
#[frb(dart_code = "
    @override
    String toString() => 'AttachmentId($uuid)';
")]
pub struct _AttachmentId {
    pub uuid: Uuid,
}

/// Not yet fully resolved [`UiMimiContent`]
#[derive(Debug)]
#[frb(ignore)]
pub(crate) struct UnresolvedMimiContent {
    pub plain_body: Option<String>,
    pub replaces: Option<Vec<u8>>,
    pub topic_id: Vec<u8>,
    pub in_reply_to: Option<Vec<u8>>,
    pub content: Option<MessageContent>,
    /// Atachmment without local attachment ID yet
    pub attachments: Vec<UnresolvedAttachment>,
}

/// The actual content of a message
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
#[frb(dart_metadata = ("freezed"))]
pub struct UiMimiContent {
    pub plain_body: Option<String>,
    /// Plain text that stands in for the message in the chat list and in
    /// quotes. `None` for a deleted message.
    pub preview_text: Option<String>,
    pub replaces: Option<Vec<u8>>,
    pub topic_id: Vec<u8>,
    pub in_reply_to: Option<Vec<u8>>,
    pub content: Option<MessageContent>,
    pub attachments: Vec<UiAttachment>,
}

/// [`UiAttachment`] without local attachment ID
#[derive(Debug)]
#[frb(ignore)]
pub(crate) struct UnresolvedAttachment {
    /// Depth-first index of the part among the message's non-container parts.
    pub index: PartIndex,
    /// Remote id from the url of an external part.
    pub remote_attachment_id: Option<RemoteAttachmentId>,
    pub filename: String,
    pub content_type: String,
    pub description: Option<String>,
    pub size: u64,
    pub image_metadata: Option<UiImageMetadata>,
}

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
#[frb(dart_metadata = ("freezed"), type_64bit_int)]
pub struct UiAttachment {
    pub attachment_id: AttachmentId,
    pub filename: String,
    pub content_type: String,
    pub description: Option<String>,
    pub size: u64,
    pub image_metadata: Option<UiImageMetadata>,
}

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
#[frb(dart_metadata = ("freezed"))]
pub struct UiImageMetadata {
    pub blurhash: Option<String>,
    pub width: u32,
    pub height: u32,
    /// Whether the image is animated, if already classified locally
    pub is_animated: Option<bool>,
}

impl UnresolvedMimiContent {
    pub(crate) fn resolve(self, attachments: Option<&MessageAttachments>) -> UiMimiContent {
        let preview_text = self.preview_text();
        let attachments: Vec<UiAttachment> = self
            .attachments
            .into_iter()
            .filter_map(|attachment| {
                let info = attachments.and_then(|attachments| {
                    attachments.find(attachment.index, attachment.remote_attachment_id)
                })?;
                Some((attachment, info))
            })
            .map(|(attachment, info)| UiAttachment {
                attachment_id: info.attachment_id,
                filename: attachment.filename,
                content_type: attachment.content_type,
                description: attachment.description,
                size: attachment.size,
                image_metadata: attachment.image_metadata.map(|metadata| UiImageMetadata {
                    is_animated: info.is_animated,
                    ..metadata
                }),
            })
            .collect();

        UiMimiContent {
            plain_body: self.plain_body,
            preview_text,
            replaces: self.replaces,
            topic_id: self.topic_id,
            in_reply_to: self.in_reply_to,
            content: self.content,
            attachments,
        }
    }
}
