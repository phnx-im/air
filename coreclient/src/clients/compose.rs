// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::pin::Pin;

use aircommon::{crypto::secrets::Secret, identifiers::MimiId};
use anyhow::{Context, bail, ensure};
use chrono::Utc;
use mimi_content::{
    Disposition, MessageStatus, MimiContent, NestedPart, content_container::PartSemantics,
};
use thiserror::Error;
use tokio::task::JoinSet;
use tracing::error;

use crate::{
    AttachmentContent, AttachmentId, AttachmentProgress, AttachmentStatus, AttachmentThumbnail,
    BlockedContactError, Chat, ChatId, ChatMessage, MarkChatAsRead, MessageId, UploadTaskError,
    chats::messages::content::{MimiContentExt as _, is_caption_slot},
    clients::{
        CoreUser,
        attachment::{
            AttachmentRecord, AttachmentTransferRecord, MessageAttachmentRecord, NewAttachment,
            persistence::{AttachmentRole, TransferFailure},
            process::AttachmentPart,
            stage::StagedAttachment,
            thumbnail::store_thumbnail,
            upload::{
                AttachmentMetadata, AttachmentSource, AttachmentSpec,
                hold_message_with_failed_upload,
            },
        },
        message::UnsentContent,
    },
    link_preview::{self, CardImage, FetchedLinkPreview},
    media_type,
    utils::image::{SourceThumbnail, encode_source_thumbnail},
};

impl CoreUser {
    pub fn compose(&self, chat_id: ChatId) -> MessageComposer<'_> {
        MessageComposer {
            core_user: self,
            chat_id,
            text: None,
            in_reply_to: None,
            replaces: None,
            link_preview: None,
            attachments: Vec::new(),
            mark_as_read: MarkChatAsRead::Yes,
        }
    }

    /// Restarts the failed uploads of a held message and queues it again.
    ///
    /// Fails if an upload failed before its attachment was processed, because
    /// there is nothing to upload again.
    pub async fn retry_message(&self, message_id: MessageId) -> anyhow::Result<Vec<UploadHandle>> {
        let (chat_id, uploads) = self
            .db()
            .with_write_transaction(async |txn| -> anyhow::Result<_> {
                let mut message = ChatMessage::load(&mut *txn, message_id)
                    .await?
                    .context("Message not found")?;
                ensure!(!message.is_sent(), "Message is already sent");
                ensure!(
                    !AttachmentTransferRecord::has_active_upload(&mut *txn, message_id).await?,
                    "Message is still uploading"
                );
                let chat_id = message.chat_id();

                let failed =
                    AttachmentTransferRecord::load_failed_uploads(&mut *txn, message_id).await?;
                let mut uploads = Vec::with_capacity(failed.len());
                for attachment_id in failed {
                    let reference = MessageAttachmentRecord::load_first(&mut *txn, attachment_id)
                        .await?
                        .context("Attachment without a message")?;
                    let bytes =
                        match AttachmentRecord::load_content(&mut *txn, attachment_id).await? {
                            AttachmentContent::UploadFailed(bytes) => bytes,
                            _ => bail!("Attachment {attachment_id} has no content to upload"),
                        };
                    uploads.push((reference, bytes));
                }

                let now = Utc::now();
                for (reference, _bytes) in &uploads {
                    AttachmentTransferRecord::restart_upload(
                        &mut *txn,
                        reference.attachment_id,
                        now,
                    )
                    .await?;
                }
                // Holding the message marked it as failed
                message.set_status(MessageStatus::Unread);
                message.update(&mut *txn).await?;
                self.outbound_service()
                    .enqueue_chat_message_in_chat(txn, chat_id, message_id)
                    .await?;

                Ok((chat_id, uploads))
            })
            .await?;

        Ok(uploads
            .into_iter()
            .map(|(reference, bytes)| {
                self.upload_handle(chat_id, reference, AttachmentSource::Processed(bytes))
            })
            .collect())
    }

    /// Restarts the uploads of queued messages that a killed process left
    /// running.
    ///
    /// An upload without stored content cannot be restarted. It is failed,
    /// which holds its message.
    pub async fn resume_interrupted_uploads(&self) -> anyhow::Result<Vec<UploadHandle>> {
        let uploads = self
            .db()
            .with_write_transaction(async |txn| -> anyhow::Result<_> {
                let interrupted = AttachmentTransferRecord::load_active_uploads(&mut *txn).await?;
                let now = Utc::now();
                let mut uploads = Vec::with_capacity(interrupted.len());
                for (chat_id, attachment_id) in interrupted {
                    let Some(reference) =
                        MessageAttachmentRecord::load_first(&mut *txn, attachment_id).await?
                    else {
                        continue;
                    };
                    match AttachmentRecord::load_content(&mut *txn, attachment_id).await? {
                        AttachmentContent::Uploading(bytes) => {
                            AttachmentTransferRecord::restart_upload(&mut *txn, attachment_id, now)
                                .await?;
                            uploads.push((chat_id, reference, bytes));
                        }
                        _ => {
                            AttachmentTransferRecord::fail_upload(
                                &mut *txn,
                                attachment_id,
                                TransferFailure::Unknown,
                            )
                            .await?;
                            hold_message_with_failed_upload(txn, reference.message_id).await?;
                        }
                    }
                }
                Ok(uploads)
            })
            .await?;

        Ok(uploads
            .into_iter()
            .map(|(chat_id, reference, bytes)| {
                self.upload_handle(chat_id, reference, AttachmentSource::Processed(bytes))
            })
            .collect())
    }

    /// Handle of the upload of an attachment that is stored with its message.
    fn upload_handle(
        &self,
        chat_id: ChatId,
        reference: MessageAttachmentRecord,
        source: AttachmentSource,
    ) -> UploadHandle {
        let (progress_tx, progress) = AttachmentProgress::new();
        UploadHandle {
            attachment_id: reference.attachment_id,
            progress,
            task: Box::pin(self.upload_message_attachment(chat_id, reference, source, progress_tx)),
        }
    }
}

#[must_use]
pub struct MessageComposer<'a> {
    core_user: &'a CoreUser,
    chat_id: ChatId,
    text: Option<String>,
    in_reply_to: Option<MimiId>,
    replaces: Option<ChatMessage>,
    link_preview: Option<ComposedLinkPreview>,
    attachments: Vec<StagedAttachment>,
    mark_as_read: MarkChatAsRead,
}

#[derive(Debug)]
pub enum ComposedLinkPreview {
    /// A newly fetched card.
    ///
    /// Uploads its image, if it has one.
    Fetched(FetchedLinkPreview),
    /// The card of the replaced message.
    ///
    /// Only valid with `replaces`.
    Original,
}

impl MessageComposer<'_> {
    pub fn text(mut self, markdown: String) -> Self {
        self.text = Some(markdown);
        self
    }

    pub fn in_reply_to(mut self, mimi_id: impl Into<Option<MimiId>>) -> Self {
        self.in_reply_to = mimi_id.into();
        self
    }

    pub fn replaces(mut self, original: impl Into<Option<ChatMessage>>) -> Self {
        self.replaces = original.into();
        self
    }

    pub fn link_preview(mut self, preview: impl Into<Option<ComposedLinkPreview>>) -> Self {
        self.link_preview = preview.into();
        self
    }

    pub fn attachment(mut self, staged: StagedAttachment) -> Self {
        self.attachments.push(staged);
        self
    }

    pub fn mark_as_read(mut self, mark_as_read: MarkChatAsRead) -> Self {
        self.mark_as_read = mark_as_read;
        self
    }

    pub async fn send(mut self) -> anyhow::Result<ComposedMessage> {
        let core_user = self.core_user;
        let message_id = match &self.replaces {
            Some(original) => original.id(),
            None => MessageId::random(),
        };

        // checked before any image is decoded
        let shape = self.verify_shape()?;

        // one entry per staged attachment, None for a non-image
        let thumbnails = decode_thumbnails(shape.attachments()).await?;

        // pure, fails before anything is stored
        let Layout { content, uploads } = self.build_layout(shape, message_id, thumbnails)?;

        // size and hash of a file are unknown until it is processed
        let is_provisional = uploads
            .iter()
            .any(|upload| matches!(upload.source, AttachmentSource::File { .. }));

        let message = core_user
            .db()
            .with_write_transaction(async |txn| {
                let (group_id, is_blocked) =
                    Chat::load_group_id_and_is_blocked(&mut *txn, self.chat_id)
                        .await?
                        .with_context(|| format!("Can't find chat with id {}", self.chat_id))?;
                if is_blocked {
                    bail!(BlockedContactError);
                }

                let is_edit = self.replaces.is_some();
                let keeps_card = matches!(self.link_preview, Some(ComposedLinkPreview::Original));

                let message = if is_provisional {
                    // no Mimi ID yet
                    core_user
                        .store_provisional_message(
                            txn,
                            self.chat_id,
                            message_id,
                            content,
                            self.mark_as_read,
                        )
                        .await?
                } else {
                    let message = UnsentContent {
                        chat_id: self.chat_id,
                        message_id,
                        content,
                    }
                    .store_unsent_message(&mut *txn, core_user.user_id(), &group_id, self.replaces)
                    .await?;
                    core_user
                        .mark_as_read_until_message(&mut *txn, &message, self.mark_as_read)
                        .await?;
                    message
                };

                // old card goes unless the edit keeps the card
                if is_edit && !keeps_card {
                    MessageAttachmentRecord::delete_by_role(
                        &mut *txn,
                        message_id,
                        AttachmentRole::Preview,
                    )
                    .await?;
                }

                // Only a message with uploads needs its parts
                let parts = match (uploads.is_empty(), message.message().mimi_content()) {
                    (false, Some(content)) => AttachmentPart::collect(content)?,
                    _ => Vec::new(),
                };
                for upload in &uploads {
                    let attachment_id = upload.attachment.attachment_id;
                    match &upload.source {
                        AttachmentSource::File { .. } => {
                            upload.attachment.store(&mut *txn, None).await?;
                        }
                        AttachmentSource::Processed(bytes) => {
                            upload.attachment.store(&mut *txn, Some(bytes)).await?;
                            let remote = parts
                                .iter()
                                .find(|part| part.index == upload.attachment.part_index)
                                .and_then(AttachmentPart::remote)
                                .context("upload without its part")?;
                            remote.store(&mut *txn, attachment_id).await?;
                        }
                    }
                    if let Some(thumbnail) = &upload.thumbnail {
                        store_thumbnail(&mut *txn, attachment_id, thumbnail).await?;
                    }
                }

                // held back by a gate in `ChatMessageQueue::dequeue` while uploads run
                core_user
                    .outbound_service()
                    .enqueue_chat_message_in_chat(txn, self.chat_id, message_id)
                    .await?;

                Ok(message)
            })
            .await?;

        let uploads = uploads
            .into_iter()
            .map(|upload| {
                core_user.upload_handle(self.chat_id, upload.attachment.reference(), upload.source)
            })
            .collect();

        Ok(ComposedMessage { message, uploads })
    }

    /// Checks that the parts of the composer fit together.
    ///
    /// Moves the text and the attachments into the shape.
    fn verify_shape(&mut self) -> anyhow::Result<Shape> {
        ensure!(
            self.replaces.is_none() || self.attachments.is_empty(),
            "an edit cannot add attachments"
        );
        ensure!(
            self.link_preview.is_none() || self.attachments.is_empty(),
            "link preview and attachments are mutually exclusive"
        );

        let mut attachments = std::mem::take(&mut self.attachments).into_iter();
        match (self.text.take(), attachments.next(), attachments.next()) {
            // text message with an optional link preview
            (Some(text), None, _) => {
                let keeps_card = matches!(self.link_preview, Some(ComposedLinkPreview::Original));
                ensure!(
                    !keeps_card || self.replaces.is_some(),
                    "the original card needs a replaced message"
                );
                let original = self
                    .replaces
                    .as_ref()
                    .and_then(|message| message.message().mimi_content());
                if let Some(original) = original {
                    let has_attachment = AttachmentPart::collect(original)?
                        .iter()
                        .any(|part| part.role == AttachmentRole::Attachment);
                    if has_attachment {
                        ensure!(
                            self.link_preview.is_none(),
                            "link preview and attachments are mutually exclusive"
                        );
                        ensure!(
                            original.has_caption_slot(),
                            "the message has no caption slot"
                        );
                        return Ok(Shape::CaptionEdit { caption: text });
                    }
                }
                Ok(Shape::Text { text })
            }
            // attachments with a caption text
            (caption, Some(staged), None) => Ok(Shape::Attachments {
                staged: vec![staged],
                caption,
            }),
            (None, None, _) => bail!("nothing to send"),
            _ => bail!("several attachments are not supported yet"),
        }
    }

    fn build_layout(
        &self,
        shape: Shape,
        message_id: MessageId,
        thumbnails: Vec<Option<SourceThumbnail>>,
    ) -> anyhow::Result<Layout> {
        let (mut content, uploads) = match shape {
            Shape::Text { text } => build_text_layout(
                text,
                self.link_preview.as_ref(),
                self.replaces.as_ref(),
                message_id,
            )?,
            Shape::Attachments { staged, caption } => {
                build_attachments_layout(staged, thumbnails, caption, message_id)?
            }
            Shape::CaptionEdit { caption } => {
                let original = self
                    .replaces
                    .as_ref()
                    .and_then(|message| message.message().mimi_content())
                    .context("a caption edit needs a replaced message")?;
                build_caption_edit_layout(caption, original)?
            }
        };
        content.in_reply_to = self.in_reply_to.map(From::from);
        Ok(Layout { content, uploads })
    }
}

/// A text with an optional card.
///
/// ```text
/// text part                            ; partIndex 0
/// ```
///
/// With a card the text moves into a container.
///
/// ```text
/// MultiPart  render, processAll        ; partIndex 0
/// ├─ text part                         ; 1
/// ├─ card part                         ; 2
/// └─ card image part (optional)        ; 3
/// ```
///
/// The image of a fetched card is returned as an upload. The original card
/// keeps the image of `replaces`.
fn build_text_layout(
    text: String,
    link_preview: Option<&ComposedLinkPreview>,
    replaces: Option<&ChatMessage>,
    message_id: MessageId,
) -> anyhow::Result<(MimiContent, Vec<PlannedUpload>)> {
    let salt = *Secret::<16>::random()?.secret();
    let mut content = MimiContent::simple_markdown_message(text, salt);
    let mut uploads = Vec::new();

    match link_preview {
        None => (),
        Some(ComposedLinkPreview::Original) => {
            let original = replaces
                .and_then(|message| message.message().mimi_content())
                .context("the original card needs a replaced message")?;
            content = link_preview::with_original_card(content, original)?;
        }
        Some(ComposedLinkPreview::Fetched(fetched)) => {
            let (with_card, image) = link_preview::with_fetched_card(content, fetched, message_id)?;
            content = with_card;
            uploads.extend(image.map(|CardImage { attachment, bytes }| PlannedUpload {
                attachment,
                source: AttachmentSource::Processed(bytes),
                thumbnail: None,
            }));
        }
    }

    Ok((content, uploads))
}

/// Attachments with an optional caption.
///
/// ```text
/// MultiPart  render, processAll        ; partIndex 0
/// ├─ caption slot                      ; 1, a null part without a caption
/// ├─ item                              ; 2..
/// └─ ...
///
/// image item  MultiPart  inline, processAll
///             ├─ external part
///             └─ text/blurhash  preview
/// file item   external part
/// ```
fn build_attachments_layout(
    staged: Vec<StagedAttachment>,
    thumbnails: Vec<Option<SourceThumbnail>>,
    caption: Option<String>,
    message_id: MessageId,
) -> anyhow::Result<(MimiContent, Vec<PlannedUpload>)> {
    ensure!(
        staged.len() == thumbnails.len(),
        "every attachment needs a thumbnail entry"
    );

    let mut parts = Vec::with_capacity(staged.len() + 1);
    parts.push(caption_slot(caption.unwrap_or_default()));
    let mut attachments = Vec::with_capacity(staged.len());
    for (staged, thumbnail) in staged.into_iter().zip(thumbnails) {
        ensure!(
            staged.is_image() == thumbnail.is_some(),
            "an image needs its thumbnail"
        );
        let mut part = staged.provisional_part();
        AttachmentMetadata::random()?.write_to(&mut part)?;
        let item = match &thumbnail {
            Some(thumbnail) => NestedPart::MultiPart {
                disposition: Disposition::Inline,
                part_semantics: PartSemantics::ProcessAll,
                parts: vec![
                    part,
                    NestedPart::SinglePart {
                        disposition: Disposition::Preview,
                        language: String::new(),
                        content_type: media_type::TEXT_BLURHASH.to_owned(),
                        content: thumbnail.blurhash.clone().into_bytes(),
                    },
                ],
                language: String::new(),
            },
            None => part,
        };
        parts.push(item);
        attachments.push((staged, thumbnail));
    }
    let content = MimiContent {
        salt: Secret::<16>::random()?.secret().to_vec(),
        nested_part: NestedPart::MultiPart {
            disposition: Disposition::Render,
            part_semantics: PartSemantics::ProcessAll,
            parts,
            language: String::new(),
        },
        ..Default::default()
    };

    let part_indices: Vec<_> = AttachmentPart::collect(&content)?
        .into_iter()
        .filter(|part| part.role == AttachmentRole::Attachment)
        .map(|part| part.index)
        .collect();
    ensure!(
        part_indices.len() == attachments.len(),
        "the layout needs one part per attachment"
    );

    let uploads = attachments
        .into_iter()
        .zip(part_indices)
        .map(|((staged, thumbnail), part_index)| {
            let blurhash = thumbnail
                .as_ref()
                .map(|thumbnail| thumbnail.blurhash.clone());
            let is_animated = thumbnail.as_ref().map(|thumbnail| thumbnail.is_animated);
            PlannedUpload {
                attachment: NewAttachment {
                    attachment_id: staged.id(),
                    message_id,
                    part_index,
                    role: AttachmentRole::Attachment,
                    filename: Some(staged.filename().to_owned()),
                    content_type: staged.content_type().to_owned(),
                    size: Some(staged.size()),
                    content_hash: None,
                    dimensions: staged.image_dimensions(),
                    blurhash,
                    status: AttachmentStatus::Uploading,
                    is_animated,
                    created_at: Utc::now(),
                },
                source: AttachmentSource::File {
                    spec: AttachmentSpec {
                        filename: staged.filename().to_owned(),
                        content_type: staged.content_type().to_owned(),
                        is_image: staged.is_image(),
                    },
                    file: staged.into_file(),
                },
                thumbnail: thumbnail.map(|thumbnail| AttachmentThumbnail::Ready {
                    bytes: thumbnail.bytes,
                }),
            }
        })
        .collect();

    Ok((content, uploads))
}

/// The replaced message with another caption.
///
/// Only the caption slot is swapped, an empty caption becomes a null part.
fn build_caption_edit_layout(
    caption: String,
    original: &MimiContent,
) -> anyhow::Result<(MimiContent, Vec<PlannedUpload>)> {
    let mut nested_part = original.nested_part.clone();
    match &mut nested_part {
        NestedPart::MultiPart {
            disposition: Disposition::Render,
            part_semantics: PartSemantics::ProcessAll,
            parts,
            ..
        } => match parts.first_mut() {
            Some(slot) if is_caption_slot(slot) => *slot = caption_slot(caption),
            _ => bail!("the message has no caption slot"),
        },
        _ => bail!("the message has no caption slot"),
    }
    let content = MimiContent {
        salt: Secret::<16>::random()?.secret().to_vec(),
        nested_part,
        ..Default::default()
    };
    Ok((content, Vec::new()))
}

/// Caption slot part, a null part for an empty caption.
fn caption_slot(caption: String) -> NestedPart {
    if caption.is_empty() {
        NestedPart::NullPart {
            disposition: Disposition::Render,
            language: String::new(),
        }
    } else {
        NestedPart::SinglePart {
            disposition: Disposition::Render,
            language: String::new(),
            content_type: media_type::TEXT_MARKDOWN.to_owned(),
            content: caption.into_bytes(),
        }
    }
}

pub struct ComposedMessage {
    pub message: ChatMessage,
    pub uploads: Vec<UploadHandle>,
}

/// Upload of an attachment of a stored message.
///
/// Awaiting `task` uploads the attachment.
pub struct UploadHandle {
    pub attachment_id: AttachmentId,
    pub progress: AttachmentProgress,
    pub task: Pin<Box<dyn Future<Output = Result<(), UploadTaskError>> + Send>>,
}

/// A staged image whose body does not decode.
#[derive(Debug, Error)]
#[error("attachment {index} cannot be decoded")]
pub struct UndecodableAttachment {
    /// Position among the attachments of the message
    pub index: usize,
}

/// What a composer holds, once its parts are known to fit together.
enum Shape {
    Text {
        text: String,
    },
    Attachments {
        staged: Vec<StagedAttachment>,
        caption: Option<String>,
    },
    CaptionEdit {
        caption: String,
    },
}

impl Shape {
    fn attachments(&self) -> &[StagedAttachment] {
        match self {
            Self::Text { .. } | Self::CaptionEdit { .. } => &[],
            Self::Attachments { staged, .. } => staged,
        }
    }
}

/// Content of a message to store and the uploads it depends on.
struct Layout {
    content: MimiContent,
    uploads: Vec<PlannedUpload>,
}

/// An attachment to store with the message and upload afterwards.
struct PlannedUpload {
    attachment: NewAttachment,
    source: AttachmentSource,
    thumbnail: Option<AttachmentThumbnail>,
}

/// Decode attachments and produce thumbnails concurrently without blocking the runtime.
///
/// Fails with [`UndecodableAttachment`] for an image that does not decode.
///
/// Also see [`encode_source_thumbnail`].
async fn decode_thumbnails(
    attachments: &[StagedAttachment],
) -> anyhow::Result<Vec<Option<SourceThumbnail>>> {
    // Bounds the memory of decoded images held at once.
    const MAX_PARALLEL_THUMBNAIL_DECODES: usize = 2;

    let mut thumbnails = Vec::new();
    thumbnails.resize_with(attachments.len(), || None);

    let mut images = attachments
        .iter()
        .enumerate()
        .filter(|(_index, staged)| staged.is_image());
    let mut tasks = JoinSet::new();

    loop {
        while tasks.len() < MAX_PARALLEL_THUMBNAIL_DECODES
            && let Some((index, staged)) = images.next()
        {
            let path = staged.path().to_owned();
            tasks.spawn_blocking(move || (index, encode_source_thumbnail(path)));
        }
        let Some(joined) = tasks.join_next().await else {
            break;
        };
        let (index, thumbnail) = joined?;
        match thumbnail {
            Ok(thumbnail) => {
                thumbnails[index] = Some(thumbnail);
            }
            Err(error) => {
                error!(%error, index, "failed to decode attachment image");
                bail!(UndecodableAttachment { index });
            }
        }
    }
    Ok(thumbnails)
}

#[cfg(test)]
mod tests {
    use tempfile::{TempDir, tempdir};

    use super::*;
    use crate::{
        chats::messages::content::{PartIndex, VisitedPart},
        clients::attachment::stage::AttachmentFile,
    };

    fn staged(directory: &TempDir, name: &str, bytes: &[u8]) -> anyhow::Result<StagedAttachment> {
        let path = directory.path().join(name);
        std::fs::write(&path, bytes)?;
        match StagedAttachment::from_file(AttachmentFile::borrowed(path))? {
            Ok(staged) => Ok(staged),
            Err(_error) => bail!("attachment must be accepted"),
        }
    }

    fn staged_image(directory: &TempDir) -> anyhow::Result<StagedAttachment> {
        let rgba = vec![127u8; 16 * 16 * 4];
        let bytes = webpx::Encoder::new_rgba(&rgba, 16, 16)
            .quality(80.0)
            .encode(webpx::Unstoppable)?;
        staged(directory, "image.webp", &bytes)
    }

    fn staged_file(directory: &TempDir) -> anyhow::Result<StagedAttachment> {
        staged(directory, "note.txt", b"plain file content")
    }

    fn thumbnail_of(staged: &StagedAttachment) -> anyhow::Result<Option<SourceThumbnail>> {
        if staged.is_image() {
            Ok(Some(encode_source_thumbnail(staged.path())?))
        } else {
            Ok(None)
        }
    }

    fn attachments_layout(
        staged: StagedAttachment,
        caption: Option<&str>,
    ) -> anyhow::Result<(MimiContent, Vec<PlannedUpload>)> {
        let thumbnail = thumbnail_of(&staged)?;
        build_attachments_layout(
            vec![staged],
            vec![thumbnail],
            caption.map(str::to_owned),
            MessageId::random(),
        )
    }

    /// Texts and attachments (index, blurhash) of the content, in order.
    fn visit(content: &MimiContent) -> (Vec<String>, Vec<(PartIndex, Option<String>)>) {
        let mut texts = Vec::new();
        let mut attachments = Vec::new();
        content
            .visit_parts(|part| {
                match part {
                    VisitedPart::Text { content, .. } => texts.push(content.to_owned()),
                    VisitedPart::Attachment {
                        index, blurhash, ..
                    } => attachments.push((index, blurhash.map(str::to_owned))),
                    VisitedPart::LinkPreview { .. } | VisitedPart::Unknown(_) => {}
                }
                Ok(())
            })
            .unwrap();
        (texts, attachments)
    }

    #[test]
    fn captioned_image_is_laid_out_at_part_index_3() -> anyhow::Result<()> {
        let directory = tempdir()?;
        let (content, uploads) = attachments_layout(staged_image(&directory)?, Some("hello"))?;

        let (texts, attachments) = visit(&content);
        assert_eq!(texts, ["hello"]);
        let [(index, blurhash)] = attachments.as_slice() else {
            panic!("expected exactly one attachment");
        };
        assert_eq!(*index, PartIndex(3));
        assert!(blurhash.is_some());

        let [upload] = uploads.as_slice() else {
            panic!("expected exactly one upload");
        };
        assert_eq!(upload.attachment.part_index, PartIndex(3));
        assert!(upload.attachment.blurhash.is_some());
        assert!(upload.thumbnail.is_some());
        Ok(())
    }

    #[test]
    fn image_without_caption_keeps_part_index_3() -> anyhow::Result<()> {
        let directory = tempdir()?;
        let (content, uploads) = attachments_layout(staged_image(&directory)?, None)?;

        let (texts, attachments) = visit(&content);
        assert!(texts.is_empty());
        let [(index, blurhash)] = attachments.as_slice() else {
            panic!("expected exactly one attachment");
        };
        assert_eq!(*index, PartIndex(3));
        assert!(blurhash.is_some());
        assert!(content.has_caption_slot());

        let [upload] = uploads.as_slice() else {
            panic!("expected exactly one upload");
        };
        assert_eq!(upload.attachment.part_index, PartIndex(3));
        Ok(())
    }

    #[test]
    fn captioned_file_is_laid_out_at_part_index_2_without_blurhash() -> anyhow::Result<()> {
        let directory = tempdir()?;
        let (content, uploads) = attachments_layout(staged_file(&directory)?, Some("notes"))?;

        let (texts, attachments) = visit(&content);
        assert_eq!(texts, ["notes"]);
        assert_eq!(attachments, [(PartIndex(2), None)]);
        assert!(content.has_caption_slot());

        let [upload] = uploads.as_slice() else {
            panic!("expected exactly one upload");
        };
        assert_eq!(upload.attachment.part_index, PartIndex(2));
        assert_eq!(upload.attachment.blurhash, None);
        assert!(upload.thumbnail.is_none());
        Ok(())
    }

    #[test]
    fn caption_edit_keeps_the_attachment_at_part_index_3() -> anyhow::Result<()> {
        let directory = tempdir()?;
        let (original, _uploads) = attachments_layout(staged_image(&directory)?, Some("first"))?;

        let (changed, uploads) = build_caption_edit_layout("second".to_owned(), &original)?;
        assert!(uploads.is_empty());
        let (texts, attachments) = visit(&changed);
        assert_eq!(texts, ["second"]);
        let [(index, blurhash)] = attachments.as_slice() else {
            panic!("expected exactly one attachment");
        };
        assert_eq!(*index, PartIndex(3));
        assert!(blurhash.is_some());

        let (removed, uploads) = build_caption_edit_layout(String::new(), &changed)?;
        assert!(uploads.is_empty());
        let (texts, attachments) = visit(&removed);
        assert!(texts.is_empty());
        assert_eq!(attachments.len(), 1);
        assert_eq!(attachments[0].0, PartIndex(3));
        assert!(removed.has_caption_slot());

        let (added, uploads) = build_caption_edit_layout("third".to_owned(), &removed)?;
        assert!(uploads.is_empty());
        let (texts, attachments) = visit(&added);
        assert_eq!(texts, ["third"]);
        assert_eq!(attachments.len(), 1);
        assert_eq!(attachments[0].0, PartIndex(3));
        assert!(added.has_caption_slot());
        Ok(())
    }

    #[test]
    fn caption_edit_of_plain_markdown_fails() {
        let original = MimiContent::simple_markdown_message("hi".to_owned(), [0; 16]);
        assert!(build_caption_edit_layout("caption".to_owned(), &original).is_err());
    }

    #[test]
    fn caption_edit_of_flat_attachment_fails() -> anyhow::Result<()> {
        let directory = tempdir()?;
        let original = MimiContent {
            nested_part: staged_file(&directory)?.provisional_part(),
            ..Default::default()
        };
        assert!(build_caption_edit_layout("caption".to_owned(), &original).is_err());
        Ok(())
    }
}
