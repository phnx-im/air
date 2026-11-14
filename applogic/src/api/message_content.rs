// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::identifiers::{MimiId, RemoteAttachmentId};
pub(crate) use aircoreclient::AttachmentId;
use aircoreclient::{
    MessageAttachments, PartIndex,
    link_preview::{FetchedLinkPreview, LinkPreview},
};
use airprotos::client::link_preview::{LinkPreviewCard, LinkPreviewKind};
use flutter_rust_bridge::frb;
use uuid::Uuid;

use crate::api::markdown::MessageContent;

/// Shown for a message whose attachments have no stored record.
const ATTACHMENT_UNAVAILABLE: &str = "Attachment unavailable";

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

impl From<UiMimiId> for mimi_content::MimiId {
    fn from(id: UiMimiId) -> Self {
        Self(id.0)
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
    /// Card with its image not yet paired with a record
    pub link_preview: Option<UnresolvedLinkPreview>,
}

#[derive(Debug)]
#[frb(ignore)]
pub(crate) struct UnresolvedLinkPreview {
    pub preview: LinkPreview,
    pub image: Option<UnresolvedAttachment>,
}

/// The actual content of a message
#[derive(Debug, Clone, Eq, PartialEq)]
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
    pub link_preview: Option<UiLinkPreview>,
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

#[derive(Debug, Clone, Eq, PartialEq)]
#[frb(dart_metadata = ("freezed"), type_64bit_int)]
pub struct UiAttachment {
    pub attachment_id: AttachmentId,
    pub filename: String,
    pub content_type: String,
    pub description: Option<String>,
    pub size: u64,
    pub image_metadata: Option<UiImageMetadata>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
#[frb(dart_metadata = ("freezed"))]
pub struct UiImageMetadata {
    pub blurhash: Option<String>,
    pub width: u32,
    pub height: u32,
    /// Whether the image is animated, if already classified locally
    pub is_animated: Option<bool>,
}

/// Text of a link preview card, shared by messages and the composer.
#[derive(Debug, Clone, Eq, PartialEq)]
#[frb(dart_metadata = ("freezed"))]
pub struct UiLinkPreviewCard {
    pub url: String,
    pub title: String,
    pub description: String,
    pub kind: UiLinkPreviewKind,
}

impl UiLinkPreviewCard {
    fn new(card: &LinkPreviewCard) -> Self {
        Self {
            url: card.url().to_owned(),
            title: card.title().to_owned(),
            description: card.description().to_owned(),
            kind: card.kind().into(),
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
#[frb(dart_metadata = ("freezed"))]
pub struct UiLinkPreviewImage {
    pub attachment: UiAttachment,
    pub alt: Option<String>,
}

/// Link preview of a received or sent message.
#[derive(Debug, Clone, Eq, PartialEq)]
#[frb(dart_metadata = ("freezed"))]
pub struct UiLinkPreview {
    pub card: UiLinkPreviewCard,
    pub image: Option<UiLinkPreviewImage>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
#[frb(dart_metadata = ("freezed"))]
pub enum UiComposerLinkPreview {
    Loading {
        url: String,
    },
    Ready {
        card: UiLinkPreviewCard,
        /// Dimensions and blurhash, no bytes to keep the state changes small
        image: Option<UiImageMetadata>,
    },
}

impl UiComposerLinkPreview {
    pub(crate) fn ready(fetched: &FetchedLinkPreview) -> Self {
        let image = fetched.card().image().map(|image| UiImageMetadata {
            blurhash: image.blur_hash().map(str::to_owned),
            width: image.width(),
            height: image.height(),
            is_animated: Some(false),
        });
        Self::Ready {
            card: UiLinkPreviewCard::new(fetched.card()),
            image,
        }
    }
}

/// How a link preview card is shown.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum UiLinkPreviewKind {
    Default,
    Video,
    Music,
    Profile,
}

impl From<LinkPreviewKind> for UiLinkPreviewKind {
    fn from(kind: LinkPreviewKind) -> Self {
        match kind {
            LinkPreviewKind::Video => Self::Video,
            LinkPreviewKind::Music => Self::Music,
            LinkPreviewKind::Profile => Self::Profile,
            LinkPreviewKind::Default | LinkPreviewKind::Other(_) => Self::Default,
        }
    }
}

impl UnresolvedMimiContent {
    /// Pairs the attachments with their stored records.
    ///
    /// `None` means the records were not loaded. Attachments are dropped then,
    /// but the content is otherwise unchanged. With records loaded, a message
    /// that has neither text nor a stored attachment shows an error instead
    /// of an empty bubble.
    pub(crate) fn resolve(self, records: Option<&MessageAttachments>) -> UiMimiContent {
        let preview_text = self.preview_text();
        let had_attachments = !self.attachments.is_empty();
        let attachments: Vec<UiAttachment> = self
            .attachments
            .into_iter()
            .filter_map(|a| a.resolve(records))
            .collect();

        let content = match self.content {
            None if records.is_some() && had_attachments && attachments.is_empty() => {
                Some(MessageContent::error(ATTACHMENT_UNAVAILABLE.to_owned()))
            }
            content => content,
        };

        let link_preview = self.link_preview.map(
            |UnresolvedLinkPreview {
                 preview,
                 image: attachment,
             }| UiLinkPreview {
                card: UiLinkPreviewCard::new(preview.card()),
                image: attachment
                    .zip(preview.image())
                    .and_then(|(attachment, (image, _))| {
                        let mut attachment = attachment.resolve(records)?;
                        attachment.image_metadata = Some(UiImageMetadata {
                            blurhash: image.blur_hash().map(str::to_owned),
                            width: image.width(),
                            height: image.height(),
                            is_animated: Some(false),
                        });
                        Some(UiLinkPreviewImage {
                            attachment,
                            alt: image.alt().map(str::to_owned),
                        })
                    }),
            },
        );

        UiMimiContent {
            plain_body: self.plain_body,
            preview_text,
            replaces: self.replaces,
            topic_id: self.topic_id,
            in_reply_to: self.in_reply_to,
            content,
            attachments,
            link_preview,
        }
    }
}

impl UnresolvedAttachment {
    fn resolve(self, records: Option<&MessageAttachments>) -> Option<UiAttachment> {
        let info = records?.find(self.index, self.remote_attachment_id)?;
        Some(UiAttachment {
            attachment_id: info.attachment_id,
            filename: self.filename,
            content_type: self.content_type,
            description: self.description,
            size: self.size,
            image_metadata: self.image_metadata.map(|metadata| UiImageMetadata {
                is_animated: info.is_animated,
                ..metadata
            }),
        })
    }
}
