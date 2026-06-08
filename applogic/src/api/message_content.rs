// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::identifiers::MimiId;
pub(crate) use aircoreclient::LocalAttachmentId;
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

/// Mirror of the [`LocalAttachmentId`] type
#[doc(hidden)]
#[frb(mirror(LocalAttachmentId))]
#[frb(dart_code = "
    @override
    String toString() => 'LocalAttachmentId($uuid)';
")]
pub struct _LocalAttachmentId {
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
    pub filename: String,
    pub content_type: String,
    pub description: Option<String>,
    pub size: u64,
    pub image_metadata: Option<UiImageMetadata>,
}

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
#[frb(dart_metadata = ("freezed"), type_64bit_int)]
pub struct UiAttachment {
    pub local_attachment_id: LocalAttachmentId,
    pub filename: String,
    pub content_type: String,
    pub description: Option<String>,
    pub size: u64,
    pub image_metadata: Option<UiImageMetadata>,
}

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
#[frb(dart_metadata = ("freezed"))]
pub struct UiImageMetadata {
    pub blurhash: String,
    pub width: u32,
    pub height: u32,
}

impl UnresolvedMimiContent {
    pub(crate) fn resolve(self, local_attachment_ids: &[LocalAttachmentId]) -> UiMimiContent {
        let attachments: Vec<UiAttachment> = self
            .attachments
            .into_iter()
            .zip(local_attachment_ids.iter().copied())
            .map(|(attachment, local_attachment_id)| UiAttachment {
                local_attachment_id,
                filename: attachment.filename,
                content_type: attachment.content_type,
                description: attachment.description,
                size: attachment.size,
                image_metadata: attachment.image_metadata,
            })
            .collect();
        UiMimiContent {
            plain_body: self.plain_body,
            replaces: self.replaces,
            topic_id: self.topic_id,
            in_reply_to: self.in_reply_to,
            content: self.content,
            attachments,
        }
    }
}
