// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{
    ffi::OsStr,
    io::Cursor,
    path::{Path, PathBuf},
};

use airapiclient::{
    ApiClient,
    ds_api::{DsAttachmentTarget, ProvisionAttachmentResponse},
};
use aircommon::{
    DEFAULT_MAX_ATTACHMENT_SIZE,
    credentials::keys::UserSigningKey,
    crypto::aead::{AeadCiphertext, AeadEncryptable, keys::AttachmentEarKey},
    identifiers::{RemoteAttachmentId, UserId},
};
use airprotos::{
    common::v1::AttachmentTooLargeDetail,
    delivery_service::v1::{SignedPostPolicy, StorageObjectType},
    validation::MissingFieldExt,
};
use anyhow::{Context, bail, ensure};
use base64::{Engine, prelude::BASE64_STANDARD};
use chrono::{DateTime, Local, Utc};
use mimi_content::{
    MimiContent,
    content_container::{Disposition, NestedPart, PartSemantics},
};
use reqwest::{Body, multipart};
use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use tokio::task::spawn_blocking;
use tokio_stream::StreamExt;
use tokio_util::io::ReaderStream;
use tracing::error;
use url::Url;

use crate::{
    AttachmentContent, AttachmentId, AttachmentProgressEvent, AttachmentStatus,
    AttachmentThumbnail, AttachmentUrl, Chat, ChatId, ChatMessage, ContentMessage, MessageId,
    clients::{
        CoreUser, MarkChatAsRead,
        attachment::{
            AttachmentBytes, AttachmentRecord,
            aead::{AIR_ATTACHMENT_ENCRYPTION_ALG, AIR_ATTACHMENT_HASH_ALG},
            content::MimiContentExt,
            progress::{AttachmentProgress, AttachmentProgressSender},
            thumbnail::store_thumbnail,
        },
    },
    groups::Group,
    utils::image::{
        ReencodedAttachmentImage, placeholder_blurhash, probe_attachment_image,
        reencode_attachment_image,
    },
};

/// Sanity ceiling for an image source file, guarding against reading something
/// absurd into memory.
///
/// Images are not held to [`DEFAULT_MAX_ATTACHMENT_SIZE`] up front, because the
/// WebP re-encode decides the size the server actually judges. Their real limit
/// is enforced at provisioning.
const MAX_IMAGE_SOURCE_SIZE: u64 = 100 * 1024 * 1024;

impl CoreUser {
    /// Uploads an attachment tied to the user (signed with their signing key)
    pub async fn upload_user_attachment(
        &self,
        object_type: StorageObjectType,
        bytes: Vec<u8>,
    ) -> anyhow::Result<Result<(AttachmentMetadata, Url), ProvisionAttachmentError>> {
        let api_client = self.api_client()?;
        let http_client = self.http_client();
        let data = AttachmentBytes::from(bytes);
        let user_id = self.user_id().clone();

        match encrypt_and_provision(
            &api_client,
            self.signing_key(),
            AttachmentTarget::User(&user_id),
            object_type,
            &data,
        )
        .await?
        {
            Ok(ProvisionedAttachment {
                metadata,
                ciphertext,
                response,
            }) => {
                let remote_attachment_id = RemoteAttachmentId::new(
                    response.object_id.ok_or_missing_field("object_id")?.into(),
                );
                let (progress_tx, _progress) = AttachmentProgress::new();
                upload_encrypted_attachment(&http_client, response, progress_tx, ciphertext)
                    .await?;

                let download_url = self
                    .get_attachment_url(
                        object_type,
                        DsAttachmentTarget::User { user_id: &user_id },
                        remote_attachment_id,
                    )
                    .await?;

                Ok(Ok((metadata, download_url)))
            }
            Err(error) => Ok(Err(error)),
        }
    }

    /// Stores a message for an attachment and returns a task that sends it.
    ///
    /// The message and the attachment record are stored from a cheap header
    /// sniff before the file is read, so the message shows up in the chat
    /// right away. The returned task reads and processes the file, persists
    /// the processed content and only then talks to the server.
    pub async fn upload_chat_attachment(
        &self,
        chat_id: ChatId,
        path: &Path,
        mark_as_read: MarkChatAsRead,
    ) -> anyhow::Result<
        Result<
            (
                AttachmentId,
                AttachmentProgress,
                impl Future<Output = Result<ChatMessage, UploadTaskError>> + use<>,
            ),
            ProvisionAttachmentError,
        >,
    > {
        if !Chat::exists(self.db().read().await?, &chat_id).await? {
            anyhow::bail!("group does not exist");
        }

        let probe_path = path.to_owned();

        let probed = match spawn_blocking(move || ProbedAttachment::from_path(probe_path)).await?? {
            Ok(probed) => probed,
            Err(error) => return Ok(Err(error)),
        };

        let attachment_id = AttachmentId::random();
        let message_id = MessageId::random();
        let content = attachment_content(probed.provisional_nested_parts(attachment_id));

        // Note: Acquire a transaction here to ensure that the attachment will be deleted from the
        // local database in case of an error.
        let message = Box::pin(self.db().with_write_transaction(
            async |txn| -> anyhow::Result<ChatMessage> {
                let message = self
                    .store_provisional_message(
                        &mut *txn,
                        chat_id,
                        message_id,
                        content,
                        mark_as_read,
                    )
                    .await?;

                // store attachment locally
                // (must be done after the message is stored locally due to foreign key constraints)
                AttachmentRecord {
                    attachment_id,
                    remote_attachment_id: None,
                    chat_id,
                    message_id,
                    content_type: probed.content_type.clone(),
                    status: AttachmentStatus::Uploading,
                    is_animated: None,
                    created_at: Utc::now(),
                }
                .store(txn, None)
                .await?;

                Ok(message)
            },
        ))
        .await?;

        // The message is visible from here on.
        let (progress_tx, progress) = AttachmentProgress::new();
        let task = self.send_attachment_task(
            attachment_id,
            message,
            AttachmentSource::File {
                path: path.to_owned(),
                spec: probed.into_spec(),
            },
            progress_tx,
        );

        Ok(Ok((attachment_id, progress, task)))
    }

    /// Retries a failed attachment send.
    pub async fn retry_upload_chat_attachment(
        &self,
        attachment_id: AttachmentId,
    ) -> anyhow::Result<(
        AttachmentProgress,
        impl Future<Output = Result<ChatMessage, UploadTaskError>> + use<>,
    )> {
        // load locally stored data
        let (message, content) = self
            .db()
            .with_read_transaction(async |txn| {
                let attachment_record = AttachmentRecord::load(&mut *txn, attachment_id)
                    .await?
                    .context("Attachment not found")?;
                ensure!(
                    matches!(attachment_record.status, AttachmentStatus::UploadFailed),
                    "For retrying, the attachment must be in UploadFailed status"
                );

                let message = self
                    .message(attachment_record.message_id)
                    .await?
                    .context("Message not found")?;
                ensure!(!message.is_sent(), "Message is already sent");

                let content = match self.load_attachment(attachment_id).await? {
                    AttachmentContent::UploadFailed(bytes) => Some(bytes),
                    _ => None,
                };

                Ok((message, content))
            })
            .await?;

        // An upload interrupted before processing finished left no content
        // behind, so the send can never be completed.
        let Some(content) = content else {
            self.delete_attachment_message(message.id()).await?;
            bail!("Attachment content is gone, removed the message");
        };

        self.db()
            .with_write_transaction(async |txn| -> anyhow::Result<()> {
                AttachmentRecord::restart_upload(&mut *txn, attachment_id, Utc::now()).await?;
                Ok(())
            })
            .await?;

        let (progress_tx, progress) = AttachmentProgress::new();
        let task = self.send_attachment_task(
            attachment_id,
            message,
            AttachmentSource::Processed(content),
            progress_tx,
        );
        Ok((progress, task))
    }

    /// Processes, provisions and uploads an attachment whose message is already
    /// stored.
    fn send_attachment_task(
        &self,
        attachment_id: AttachmentId,
        message: ChatMessage,
        source: AttachmentSource,
        progress_tx: AttachmentProgressSender,
    ) -> impl Future<Output = Result<ChatMessage, UploadTaskError>> + use<> {
        let core_user = self.clone();
        let message_id = message.id();
        async move {
            match Box::pin(core_user.send_attachment(attachment_id, message, source, progress_tx))
                .await
            {
                Ok(Ok(message)) => Ok(message),
                Ok(Err(error)) => {
                    // The server refused the attachment, so no retry can ever
                    // succeed.
                    if let Err(error) = core_user.delete_attachment_message(message_id).await {
                        error!(%error, ?attachment_id, "Failed to remove rejected message");
                    }
                    Err(UploadTaskError::Provision(error))
                }
                Err(error) => {
                    if let Err(error) = core_user
                        .db()
                        .with_write_transaction(async |txn| -> anyhow::Result<()> {
                            AttachmentRecord::update_status(
                                &mut *txn,
                                attachment_id,
                                AttachmentStatus::UploadFailed,
                            )
                            .await?;
                            Ok(())
                        })
                        .await
                    {
                        error!(%error, ?attachment_id, "Failed to mark attachment as failed");
                    }
                    Err(UploadTaskError::Failed { message_id, error })
                }
            }
        }
    }

    async fn send_attachment(
        &self,
        attachment_id: AttachmentId,
        mut message: ChatMessage,
        source: AttachmentSource,
        progress_tx: AttachmentProgressSender,
    ) -> anyhow::Result<Result<ChatMessage, ProvisionAttachmentError>> {
        let content = match source {
            AttachmentSource::File { path, spec } => {
                let processed = spawn_blocking(move || {
                    let bytes = std::fs::read(&path)
                        .with_context(|| format!("Failed to read file at {}", path.display()))?;
                    ProcessedAttachment::from_bytes(bytes, spec)
                })
                .await?;
                let mut processed = match processed {
                    Ok(processed) => processed,
                    Err(error) => {
                        error!(%error, "failed to process attachment to send");
                        return Ok(Err(ProvisionAttachmentError::DecodingError));
                    }
                };
                if !self
                    .persist_processed(attachment_id, &mut message, &mut processed)
                    .await?
                {
                    bail!("Attachment {attachment_id:?} was deleted while processing");
                }
                processed.content
            }
            AttachmentSource::Processed(bytes) => bytes.into(),
        };

        let chat_id = message.chat_id();
        let group = Group::load_with_chat_id_clean(self.db().read().await?, chat_id)
            .await?
            .with_context(|| format!("Can't find group with id {chat_id:?}"))?;

        let provisioned = encrypt_and_provision(
            &self.api_client()?,
            self.signing_key(),
            AttachmentTarget::Group(&group),
            StorageObjectType::Attachment,
            &content,
        )
        .await?;

        let ProvisionedAttachment {
            metadata,
            ciphertext,
            response,
        } = match provisioned {
            Ok(provisioned) => provisioned,
            Err(error) => return Ok(Err(error)),
        };
        drop(content);

        let remote_attachment_id = metadata.remote_attachment_id;

        let mut content = message
            .message()
            .mimi_content()
            .context("Attachment message without content")?
            .clone();
        patch_provisioned_parts(&mut content, &metadata)?;

        // The content is final now, so this is where the message gets its Mimi
        // ID. It was stored without one, which is why nothing can already
        // reference the ID it is about to get.
        message.set_content_message(ContentMessage::new(
            self.user_id().clone(),
            false,
            content,
            group.group_id(),
        ));

        self.upload_and_finalize(
            attachment_id,
            &mut message,
            remote_attachment_id,
            ciphertext,
            response,
            progress_tx,
        )
        .await?;

        Ok(Ok(message))
    }

    /// Persists the processed attachment before any network work.
    ///
    /// The processed bytes replace the empty attachment content, the thumbnail
    /// and animation flag are stored, and the stored message parts get the
    /// processed values. A retry resumes from here and never needs the
    /// original file again. The thumbnail also makes the echo bubble render
    /// while the upload is still running.
    ///
    /// Returns `false` if the attachment row is gone (message deleted).
    async fn persist_processed(
        &self,
        attachment_id: AttachmentId,
        message: &mut ChatMessage,
        processed: &mut ProcessedAttachment,
    ) -> anyhow::Result<bool> {
        let content = message
            .message_mut()
            .mimi_content_mut()
            .context("Attachment message without content")?;
        processed.patch_parts(content)?;

        let is_animated = processed
            .image_data
            .as_ref()
            .is_some_and(|data| data.is_animated);
        let thumbnail = processed
            .image_data
            .as_mut()
            .map(|data| match data.thumbnail.take() {
                Some(bytes) => AttachmentThumbnail::Ready { bytes },
                None => AttachmentThumbnail::OriginalFits,
            });

        self.db()
            .with_write_transaction(async |txn| -> anyhow::Result<bool> {
                let stored = AttachmentRecord::store_processed_content(
                    &mut *txn,
                    attachment_id,
                    processed.content.as_ref(),
                    &processed.content_type,
                    is_animated,
                )
                .await?;
                if !stored {
                    return Ok(false);
                }
                message.update(&mut *txn).await?;
                if let Some(thumbnail) = &thumbnail {
                    store_thumbnail(&mut *txn, attachment_id, thumbnail).await?;
                }
                Ok(true)
            })
            .await
    }

    /// Removes a message whose attachment can never be sent.
    async fn delete_attachment_message(&self, message_id: MessageId) -> anyhow::Result<()> {
        self.db()
            .with_write_transaction(async |txn| -> anyhow::Result<()> {
                // The attachment record is removed by the foreign key cascade.
                ChatMessage::delete(&mut *txn, message_id).await?;
                Ok(())
            })
            .await
    }

    /// Marks an interrupted upload as failed, so that it can be retried.
    ///
    /// The task that would have recorded the outcome was dropped mid-flight
    /// (the user cancelled), which is why this is done from the outside. An
    /// upload that finished in the meantime keeps its status.
    pub async fn fail_interrupted_attachment_upload(
        &self,
        attachment_id: AttachmentId,
    ) -> anyhow::Result<()> {
        self.db()
            .with_write_transaction(async |txn| -> anyhow::Result<()> {
                AttachmentRecord::update_status_from(
                    &mut *txn,
                    attachment_id,
                    AttachmentStatus::Uploading,
                    AttachmentStatus::UploadFailed,
                )
                .await?;
                Ok(())
            })
            .await
    }

    /// Uploads the ciphertext, then persists the finalized content and marks
    /// the attachment as ready.
    ///
    /// The processed content was already persisted before provisioning, so
    /// only the message (now carrying the remote id, key and Mimi ID) and the
    /// record status change here.
    async fn upload_and_finalize(
        &self,
        attachment_id: AttachmentId,
        message: &mut ChatMessage,
        remote_attachment_id: RemoteAttachmentId,
        ciphertext: Vec<u8>,
        provision_response: ProvisionAttachmentResponse,
        progress_tx: AttachmentProgressSender,
    ) -> anyhow::Result<()> {
        let http_client = self.http_client();
        upload_encrypted_attachment(&http_client, provision_response, progress_tx, ciphertext)
            .await?;
        self.db()
            .with_write_transaction(async |txn| -> anyhow::Result<()> {
                message.update(&mut *txn).await?;
                AttachmentRecord::update_remote_attachment_id(
                    &mut *txn,
                    attachment_id,
                    remote_attachment_id,
                )
                .await?;
                AttachmentRecord::update_status(&mut *txn, attachment_id, AttachmentStatus::Ready)
                    .await?;
                Ok(())
            })
            .await
    }
}

/// The message content an attachment is stored with.
///
/// The final message is derived by patching field values into these parts, so
/// what the UI shows first and what goes on the wire have the same shape.
fn attachment_content(parts: Vec<NestedPart>) -> MimiContent {
    MimiContent {
        nested_part: NestedPart::MultiPart {
            disposition: Disposition::Attachment,
            part_semantics: PartSemantics::ProcessAll,
            parts,
            language: Default::default(),
        },
        ..Default::default()
    }
}

/// What a send task starts from.
enum AttachmentSource {
    /// First send: the file has not been read or processed yet.
    File { path: PathBuf, spec: AttachmentSpec },
    /// Retry: the processed bytes persisted by the first send.
    Processed(Vec<u8>),
}

/// What the send needs to know about an attachment besides its bytes.
///
/// The stored part tree is built from this before the attachment is processed,
/// so the send has to stay consistent with it rather than decide any of it
/// again.
struct AttachmentSpec {
    filename: String,
    content_type: String,
    is_image: bool,
}

#[derive(Debug)]
pub enum UploadTaskError {
    /// The attachment failed to be decoded or provisioned by the server.
    Provision(ProvisionAttachmentError),
    /// The send failed. The message is still stored, marked as failed.
    Failed {
        message_id: MessageId,
        error: anyhow::Error,
    },
}

/// What the header sniff learned about an attachment, before its bytes have
/// been read or decoded.
struct ProbedAttachment {
    filename: String,
    content_type: String,
    /// Size of the source file.
    ///
    /// The size the message ends up with, once the attachment has been
    /// processed: unchanged for a file, smaller for a re-encoded image.
    size: u64,
    /// Dimensions from the header, `None` if this is not an image we re-encode.
    ///
    /// These are the source dimensions. The re-encode caps them at 4096 while
    /// preserving the aspect ratio, which is all the UI takes from them, so the
    /// message does not reflow once the real values arrive.
    image_dimensions: Option<(u32, u32)>,
}

impl ProbedAttachment {
    fn from_path(path: PathBuf) -> anyhow::Result<Result<Self, ProvisionAttachmentError>> {
        let size = std::fs::metadata(&path)?.len();
        let image_dimensions = probe_attachment_image(&path)?;

        let max_size = if image_dimensions.is_some() {
            MAX_IMAGE_SOURCE_SIZE
        } else {
            DEFAULT_MAX_ATTACHMENT_SIZE
        };
        if size > max_size {
            return Ok(Err(ProvisionAttachmentError::TooLarge(
                AttachmentTooLargeDetail {
                    max_size_bytes: max_size,
                    actual_size_bytes: size,
                },
            )));
        }

        let filename = path
            .file_name()
            .unwrap_or_else(|| OsStr::new("attachment.bin"))
            .to_string_lossy()
            .to_string();
        // Reads at most the first 8 KiB of the file.
        let content_type = infer::get_from_path(path)?
            .map(|mime| mime.mime_type())
            .unwrap_or("application/octet-stream")
            .to_owned();

        Ok(Ok(Self {
            filename,
            content_type,
            size,
            image_dimensions,
        }))
    }

    fn into_spec(self) -> AttachmentSpec {
        AttachmentSpec {
            is_image: self.is_image(),
            filename: self.filename,
            content_type: self.content_type,
        }
    }

    fn is_image(&self) -> bool {
        self.image_dimensions.is_some()
    }

    /// The part tree a message is stored with before its attachment has been
    /// processed and provisioned.
    ///
    /// The shape is final -- whether the blurhash sibling exists is decided
    /// here, so part indices never move. Only field values are replaced later.
    /// The placeholder URL is built from the local attachment id so that it
    /// parses like any other, and the blurhash is a valid neutral hash because
    /// the UI paints it behind the picture from the first frame.
    fn provisional_nested_parts(&self, attachment_id: AttachmentId) -> Vec<NestedPart> {
        let url = AttachmentUrl::new(
            RemoteAttachmentId::new(attachment_id.uuid),
            self.image_dimensions,
        );

        let attachment = NestedPart::ExternalPart {
            disposition: Disposition::Attachment,
            language: String::new(),
            content_type: self.content_type.clone(),
            url: url.to_string(),
            expires: 0,
            size: self.size,
            enc_alg: AIR_ATTACHMENT_ENCRYPTION_ALG,
            key: Vec::new(),
            nonce: Vec::new(),
            aad: Default::default(),
            hash_alg: AIR_ATTACHMENT_HASH_ALG,
            content_hash: Vec::new(),
            description: Default::default(),
            filename: self.filename.clone(),
        };

        let blurhash = self.is_image().then(|| NestedPart::SinglePart {
            disposition: Disposition::Preview,
            language: String::new(),
            content_type: "text/blurhash".to_owned(),
            content: placeholder_blurhash().as_bytes().to_vec(),
        });

        [Some(attachment), blurhash].into_iter().flatten().collect()
    }
}

/// In-memory loaded and processed attachment
///
/// If it is an image, it will contain additional image data, like a blurhash.
struct ProcessedAttachment {
    filename: String,
    content: AttachmentBytes,
    content_hash: Vec<u8>,
    content_type: String,
    image_data: Option<ProcessedAttachmentImageData>,
    size: u64,
}

struct ProcessedAttachmentImageData {
    blurhash: String,
    width: u32,
    height: u32,
    is_animated: bool,
    /// WebP encoded thumbnail, or `None` if the original fits as thumbnail
    thumbnail: Option<Vec<u8>>,
}

impl ProcessedAttachment {
    /// Processes bytes that have already been read from disk.
    ///
    /// The spec decided the shape of the stored part tree, so the re-encode
    /// honours it instead of deciding again: a picture that turns out not to
    /// decode fails the send rather than silently becoming a file.
    fn from_bytes(bytes: Vec<u8>, spec: AttachmentSpec) -> anyhow::Result<Self> {
        let AttachmentSpec {
            filename,
            content_type,
            is_image,
        } = spec;

        let (content, content_type, filename, image_data): (AttachmentBytes, _, _, _) = if is_image
        {
            let ReencodedAttachmentImage {
                webp_image,
                image_dimensions: (width, height),
                blurhash,
                is_animated,
                thumbnail,
            } = reencode_attachment_image(bytes)?;
            let image_data = ProcessedAttachmentImageData {
                blurhash,
                width,
                height,
                is_animated,
                thumbnail,
            };
            (
                webp_image.into(),
                "image/webp".to_owned(),
                Self::image_filename(),
                Some(image_data),
            )
        } else {
            (bytes.into(), content_type, filename, None)
        };

        let content_hash = Sha256::digest(&content).to_vec();

        let size = content
            .as_ref()
            .len()
            .try_into()
            .context("attachment size overflow")?;

        Ok(Self {
            filename,
            content,
            content_type,
            content_hash,
            image_data,
            size,
        })
    }

    fn image_filename() -> String {
        let timestamp = Local::now().format("%Y-%m-%d--%H-%M-%S");
        format!("Air--{timestamp}.webp")
    }

    /// Writes the processed values into the provisional part tree.
    ///
    /// The URL keeps its local attachment id, only its dimensions change to
    /// the re-encoded ones. Provisioning later fills in the remote id, key and
    /// nonce.
    fn patch_parts(&self, content: &mut MimiContent) -> anyhow::Result<()> {
        let dimensions = self
            .image_data
            .as_ref()
            .map(|data| (data.width, data.height));
        content.visit_attachments_mut(|part| {
            if let NestedPart::ExternalPart {
                content_type,
                url,
                size,
                content_hash,
                filename,
                ..
            } = part
            {
                let local_id = url.parse::<AttachmentUrl>()?.remote_attachment_id();
                *url = AttachmentUrl::new(local_id, dimensions).to_string();
                *content_type = self.content_type.clone();
                *size = self.size;
                *content_hash = self.content_hash.clone();
                *filename = self.filename.clone();
            }
            Ok(())
        })?;
        if let Some(image_data) = &self.image_data {
            patch_blurhash(content, &image_data.blurhash);
        }
        Ok(())
    }
}

/// Replaces the placeholder value of the blurhash preview part.
fn patch_blurhash(content: &mut MimiContent, blurhash: &str) {
    let NestedPart::MultiPart { parts, .. } = &mut content.nested_part else {
        return;
    };
    for part in parts {
        if let NestedPart::SinglePart {
            content_type,
            content,
            ..
        } = part
            && content_type == "text/blurhash"
        {
            *content = blurhash.as_bytes().to_vec();
        }
    }
}

/// Writes the provisioned remote id, key and nonce into the part tree.
///
/// Everything else was already final when the processed content was persisted,
/// so this is the only difference between what is stored and what goes on the
/// wire.
fn patch_provisioned_parts(
    content: &mut MimiContent,
    metadata: &AttachmentMetadata,
) -> anyhow::Result<()> {
    content.visit_attachments_mut(|part| {
        if let NestedPart::ExternalPart {
            url, key, nonce, ..
        } = part
        {
            let dimensions = url.parse::<AttachmentUrl>()?.dimensions();
            *url = AttachmentUrl::new(metadata.remote_attachment_id, dimensions).to_string();
            *key = metadata.key.as_bytes().to_vec();
            *nonce = metadata.nonce.to_vec();
        }
        Ok(())
    })
}

/// Metadata of an encrypted and uploaded attachment
pub struct AttachmentMetadata {
    remote_attachment_id: RemoteAttachmentId,
    key: AttachmentEarKey,
    nonce: [u8; 12],
}

impl AttachmentMetadata {
    pub fn encryption_key(&self) -> &[u8] {
        self.key.as_bytes()
    }

    pub fn nonce(&self) -> &[u8; 12] {
        &self.nonce
    }
}

#[derive(Debug)]
pub enum ProvisionAttachmentError {
    DecodingError,
    TooLarge(AttachmentTooLargeDetail),
}

enum AttachmentTarget<'a> {
    Group(&'a Group),
    User(&'a UserId),
}

impl<'a> From<AttachmentTarget<'a>> for DsAttachmentTarget<'a> {
    fn from(target: AttachmentTarget<'a>) -> Self {
        match target {
            AttachmentTarget::Group(group) => DsAttachmentTarget::Group {
                group_state_ear_key: group.group_state_ear_key(),
                group_id: group.group_id(),
                sender_index: group.own_index(),
            },
            AttachmentTarget::User(user_id) => DsAttachmentTarget::User { user_id },
        }
    }
}

struct ProvisionedAttachment {
    metadata: AttachmentMetadata,
    ciphertext: Vec<u8>,
    response: ProvisionAttachmentResponse,
}

async fn encrypt_and_provision(
    api_client: &ApiClient,
    signing_key: &UserSigningKey,
    target: AttachmentTarget<'_>,
    object_type: StorageObjectType,
    content: &AttachmentBytes,
) -> anyhow::Result<Result<ProvisionedAttachment, ProvisionAttachmentError>> {
    // encrypt the content
    let key = AttachmentEarKey::random()?;
    let ciphertext: AeadCiphertext = content.encrypt(&key)?.into();
    let (ciphertext, nonce) = ciphertext.into_parts();

    // provision attachment
    let content_length = ciphertext.len().try_into().context("usize overflow")?;
    let response = match api_client
        .ds_provision_attachment(signing_key, target.into(), content_length, object_type)
        .await
    {
        Ok(response) => response,
        Err(error) => {
            return match error.get_attachment_too_large() {
                Some(attachment_too_large) => Ok(Err(ProvisionAttachmentError::TooLarge(
                    attachment_too_large,
                ))),
                None => Err(error.into()),
            };
        }
    };

    let remote_attachment_id =
        RemoteAttachmentId::new(response.object_id.context("no object id")?.into());
    let attachment = ProvisionedAttachment {
        metadata: AttachmentMetadata {
            remote_attachment_id,
            key,
            nonce,
        },
        ciphertext,
        response,
    };
    Ok(Ok(attachment))
}

async fn upload_encrypted_attachment(
    http_client: &reqwest::Client,
    provision_response: ProvisionAttachmentResponse,
    mut progress_tx: AttachmentProgressSender,
    ciphertext: Vec<u8>,
) -> anyhow::Result<()> {
    if let Some(signed_post_policy) = provision_response.post_policy {
        multipart_upload(
            http_client,
            &provision_response.upload_url,
            signed_post_policy,
            ciphertext,
            &progress_tx,
        )
        .await?;
        progress_tx.completed();
    } else {
        // upload encrypted content via signed PUT url
        let mut request = http_client.put(provision_response.upload_url);
        for header in provision_response.upload_headers {
            request = request.header(header.key, header.value);
        }

        let bytes_total = ciphertext.len();
        let mut uploaded = 0;
        let tx = progress_tx.tx();
        let stream = ReaderStream::new(Cursor::new(ciphertext)).map(move |chunk| {
            if let Ok(chunk) = &chunk {
                uploaded += chunk.len();
                if let Some(tx) = tx.as_ref() {
                    let _ignore_closed = tx.send(AttachmentProgressEvent::Progress {
                        bytes_total,
                        bytes_loaded: uploaded,
                    });
                }
            }
            chunk
        });

        request
            .body(Body::wrap_stream(stream))
            .send()
            .await?
            .error_for_status()?;

        progress_tx.completed();
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
struct PostPolicy {
    expiration: DateTime<Utc>,
    conditions: Vec<Value>,
}

async fn multipart_upload(
    http_client: &reqwest::Client,
    upload_url: &str,
    signed_post_policy: SignedPostPolicy,
    ciphertext: Vec<u8>,
    progress_tx: &AttachmentProgressSender,
) -> anyhow::Result<()> {
    let bytes_total = ciphertext.len();
    progress_tx.report(ciphertext.len(), 0);

    let post_policy = BASE64_STANDARD.decode(&signed_post_policy.base64)?;
    let post_policy: PostPolicy = serde_json::from_slice(&post_policy)?;

    ensure!(Utc::now() < post_policy.expiration, "post policy expired");

    let mut form = multipart::Form::new()
        .text("policy", signed_post_policy.base64)
        .text("x-amz-signature", signed_post_policy.signature);

    const KEYS: &[&str] = &["key", "x-amz-credential", "x-amz-algorithm", "x-amz-date"];
    for condition in post_policy.conditions {
        if let Value::Object(object) = condition
            && object.len() == 1
            && let Some((key, Value::String(value))) = object.into_iter().next()
            && KEYS.contains(&key.as_str())
        {
            form = form.text(key, value);
        }
    }

    // Stream the ciphertext so upload progress is reported incrementally
    // instead of jumping straight to completion. The content length is set
    // explicitly because S3 POST uploads require it and it avoids chunked
    // transfer encoding.
    let total_len = ciphertext.len() as u64;
    let mut uploaded = 0;
    let tx = progress_tx.tx();
    let stream = ReaderStream::new(Cursor::new(ciphertext)).map(move |chunk| {
        if let Ok(chunk) = &chunk {
            uploaded += chunk.len();
            if let Some(tx) = tx.as_ref() {
                let _ignore_closed = tx.send(AttachmentProgressEvent::Progress {
                    bytes_total,
                    bytes_loaded: uploaded,
                });
            }
        }
        chunk
    });
    let file_part = multipart::Part::stream_with_length(Body::wrap_stream(stream), total_len);
    let form = form.part("file", file_part);

    http_client
        .post(upload_url)
        .multipart(form)
        .send()
        .await?
        .error_for_status()?;

    Ok(())
}
