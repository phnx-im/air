// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::pin::Pin;

use aircommon::{crypto::secrets::Secret, identifiers::MimiId};
use anyhow::{Context, bail, ensure};
use chrono::Utc;
use mimi_content::{MessageStatus, MimiContent};
use thiserror::Error;
use tokio::task::JoinSet;
use tracing::error;

use crate::{
    AttachmentContent, AttachmentId, AttachmentProgress, AttachmentStatus, AttachmentThumbnail,
    BlockedContactError, Chat, ChatId, ChatMessage, MarkChatAsRead, MessageId, UploadTaskError,
    clients::{
        CoreUser,
        attachment::{
            AttachmentRecord, AttachmentTransferRecord, MessageAttachmentRecord, NewAttachment,
            persistence::{AttachmentRole, TransferFailure},
            process::AttachmentPart,
            stage::StagedAttachment,
            thumbnail::store_thumbnail,
            upload::{
                ATTACHMENT_PART_INDEX, AttachmentMetadata, AttachmentSource, AttachmentSpec,
                attachment_content, hold_message_with_failed_upload,
            },
        },
        message::UnsentContent,
    },
    link_preview::{self, CardImage, FetchedLinkPreview},
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

    pub async fn send(self) -> anyhow::Result<ComposedMessage> {
        let core_user = self.core_user;
        let message_id = match &self.replaces {
            Some(original) => original.id(),
            None => MessageId::random(),
        };

        // checked before any image is decoded
        let shape = self.verify_shape()?;

        // one entry per staged attachment, None for a non-image
        let thumbnails = decode_thumbnails(&self.attachments).await?;

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
    fn verify_shape(&self) -> anyhow::Result<Shape<'_>> {
        ensure!(
            self.replaces.is_none() || self.attachments.is_empty(),
            "an edit cannot add attachments"
        );
        match (&self.text, self.attachments.as_slice()) {
            (Some(text), []) => {
                let keeps_card = matches!(self.link_preview, Some(ComposedLinkPreview::Original));
                ensure!(
                    !keeps_card || self.replaces.is_some(),
                    "the original card needs a replaced message"
                );
                Ok(Shape::Text(text))
            }
            (None, [staged]) => {
                ensure!(self.link_preview.is_none(), "link preview without a text");
                Ok(Shape::SingleAttachment(staged))
            }
            (None, []) => bail!("nothing to send"),
            _ => bail!("text with attachments and several attachments are not supported yet"),
        }
    }

    fn build_layout(
        &self,
        shape: Shape<'_>,
        message_id: MessageId,
        thumbnails: Vec<Option<SourceThumbnail>>,
    ) -> anyhow::Result<Layout> {
        let (mut content, uploads) = match shape {
            Shape::Text(text) => self.build_text_layout(text, message_id)?,
            Shape::SingleAttachment(staged) => {
                let thumbnail = thumbnails.into_iter().next().flatten();
                build_single_attachment_layout(staged, thumbnail, message_id)?
            }
        };
        content.in_reply_to = self.in_reply_to.map(From::from);
        Ok(Layout { content, uploads })
    }

    fn build_text_layout(
        &self,
        text: &str,
        message_id: MessageId,
    ) -> anyhow::Result<(MimiContent, Vec<PlannedUpload>)> {
        let salt = *Secret::<16>::random()?.secret();
        let mut content = MimiContent::simple_markdown_message(text.to_owned(), salt);
        let mut uploads = Vec::new();

        match &self.link_preview {
            None => (),
            Some(ComposedLinkPreview::Original) => {
                let original = self
                    .replaces
                    .as_ref()
                    .and_then(|message| message.message().mimi_content())
                    .context("the original card needs a replaced message")?;
                content = link_preview::with_original_card(content, original)?;
            }
            Some(ComposedLinkPreview::Fetched(fetched)) => {
                let (with_card, image) =
                    link_preview::with_fetched_card(content, fetched, message_id)?;
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
}

/// A single attachment without text.
fn build_single_attachment_layout(
    staged: &StagedAttachment,
    thumbnail: Option<SourceThumbnail>,
    message_id: MessageId,
) -> anyhow::Result<(MimiContent, Vec<PlannedUpload>)> {
    let mut part = staged.provisional_part();
    AttachmentMetadata::random()?.write_to(&mut part)?;
    let content = attachment_content(vec![part])?;

    let is_animated = thumbnail.as_ref().map(|thumbnail| thumbnail.is_animated);
    let upload = PlannedUpload {
        attachment: NewAttachment {
            attachment_id: staged.id(),
            message_id,
            part_index: ATTACHMENT_PART_INDEX,
            role: AttachmentRole::Attachment,
            filename: Some(staged.filename().to_owned()),
            content_type: staged.content_type().to_owned(),
            size: Some(staged.size()),
            content_hash: None,
            dimensions: staged.image_dimensions(),
            blurhash: None,
            status: AttachmentStatus::Uploading,
            is_animated,
            created_at: Utc::now(),
        },
        source: AttachmentSource::File {
            path: staged.path().to_owned(),
            spec: AttachmentSpec {
                filename: staged.filename().to_owned(),
                content_type: staged.content_type().to_owned(),
                is_image: staged.is_image(),
            },
        },
        thumbnail: thumbnail.map(|thumbnail| AttachmentThumbnail::Ready {
            bytes: thumbnail.bytes,
        }),
    };

    Ok((content, vec![upload]))
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
enum Shape<'a> {
    Text(&'a str),
    SingleAttachment(&'a StagedAttachment),
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
