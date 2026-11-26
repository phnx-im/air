// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{ffi::OsStr, io::Cursor, path::PathBuf};

use airapiclient::{
    ApiClient,
    ds_api::{DsAttachmentTarget, ProvisionAttachmentResponse},
};
use aircommon::{
    credentials::keys::ClientKeyType,
    crypto::{
        aead::{AeadKey, keys::AttachmentEarKey},
        secrets::Secret,
        signatures::private_keys::SigningKey,
    },
    identifiers::{RemoteAttachmentId, UserId},
};
use airprotos::{
    common::v1::AttachmentTooLargeDetail, delivery_service::v1::StorageObjectType,
    validation::MissingFieldExt,
};
use anyhow::{Context, bail, ensure};
use chrono::Local;
use mimi_content::{
    MimiContent,
    content_container::{Disposition, NestedPart, PartSemantics},
};
use openmls::group::GroupId;
use reqwest::Body;
use sha2::{Digest, Sha256};
use tls_codec::Serialize;
use tokio::task::spawn_blocking;
use tokio_stream::StreamExt;
use tokio_util::io::ReaderStream;
use tracing::{debug, error};
use url::Url;
use uuid::Uuid;

use crate::{
    AttachmentId, AttachmentProgressEvent, AttachmentStatus, AttachmentUrl, ChatId, ChatMessage,
    ContentMessage, MessageId, MimiContentExt, PartIndex,
    clients::{
        CoreUser,
        attachment::{
            AttachmentBytes, AttachmentRecord, AttachmentTransferRecord, MessageAttachmentRecord,
            aead::{AIR_ATTACHMENT_ENCRYPTION_ALG, AIR_ATTACHMENT_HASH_ALG},
            persistence::{ProcessedMetadata, TransferFailure},
            process::AttachmentPart,
            progress::{AttachmentProgress, AttachmentProgressSender},
            stage::AttachmentFile,
        },
    },
    db::access::WriteDbTransaction,
    groups::{Group, persistence::GroupRef},
    media_type,
    outbound_service::chat_message_queue::ChatMessageQueue,
    utils::image::{
        ImageProcessingCancellation, ReencodedAttachmentImage, probe_attachment_image,
        reencode_attachment_image,
    },
};

/// We don't know how big the image will be after encoding, but still want to prevent
/// a local denial of service: the real limit is enforced at provisioning time
/// by the server.
const MAX_IMAGE_SOURCE_SIZE: u64 = 100 * 1024 * 1024;

/// Part index of the external part in [`attachment_content`], after the root.
pub(crate) const ATTACHMENT_PART_INDEX: PartIndex = PartIndex(1);

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

    /// Uploads the attachment of a stored message and clears its transfer.
    ///
    /// The message is already queued, so the caller does not enqueue it. A
    /// failure is recorded on the transfer and holds the message as failed
    /// once none of its uploads is in flight. The message is never deleted.
    pub(crate) fn upload_message_attachment(
        &self,
        chat_id: ChatId,
        reference: MessageAttachmentRecord,
        source: AttachmentSource,
        progress_tx: AttachmentProgressSender,
    ) -> impl Future<Output = Result<(), UploadTaskError>> + use<> {
        let core_user = self.clone();
        async move {
            let message_id = reference.message_id;
            let attachment_id = reference.attachment_id;
            let (failure, task_error) = match Box::pin(core_user.upload_attachment(
                chat_id,
                &reference,
                source,
                progress_tx,
            ))
            .await
            {
                Ok(Ok(())) => return Ok(()),
                Ok(Err(error)) => (TransferFailure::Rejected, UploadTaskError::Provision(error)),
                Err(error) => (
                    TransferFailure::Unknown,
                    UploadTaskError::Failed { message_id, error },
                ),
            };
            if let Err(error) = core_user.fail_upload(&reference, failure).await {
                error!(%error, ?attachment_id, "Failed to record upload failure");
            };
            Err(task_error)
        }
    }

    async fn upload_attachment(
        &self,
        chat_id: ChatId,
        reference: &MessageAttachmentRecord,
        source: AttachmentSource,
        progress_tx: AttachmentProgressSender,
    ) -> anyhow::Result<Result<(), ProvisionAttachmentError>> {
        let attachment_id = reference.attachment_id;

        let mut read = self.db().read().await?;
        let group = Group::load_ref_with_chat_id(&mut read, chat_id)
            .await?
            .context("Group of the attachment is gone")?;
        group.ensure_clean(read.as_mut())?;
        drop(read);

        let (content, metadata): (AttachmentBytes, _) = match source {
            AttachmentSource::File { file, spec } => {
                let path = file.path();
                let bytes = tokio::fs::read(path)
                    .await
                    .with_context(|| format!("Failed to read file at: {}", path.display()))?;
                let processing_cancel = ImageProcessingCancellation::new();
                let _cancel = processing_cancel.cancel_on_drop();
                let processed = match spawn_blocking(move || {
                    ProcessedAttachment::from_bytes(bytes, spec, processing_cancel)
                })
                .await?
                {
                    Ok(processed) => processed,
                    Err(error) => {
                        error!(%error, "failed to process attachment to send");
                        return Ok(Err(ProvisionAttachmentError::DecodingError));
                    }
                };
                let Some(metadata) = self
                    .persist_processed_part(reference, &processed, &group.group_id)
                    .await?
                else {
                    bail!("Attachment {attachment_id} was deleted while processing");
                };
                (processed.content, metadata)
            }
            AttachmentSource::Processed(bytes) => {
                let metadata = self
                    .processed_part_metadata(reference, &group.group_id)
                    .await?;
                (bytes.into(), metadata)
            }
        };

        // The part is final from here on
        let signer = self.attachment_signer(&group.attachment_target()).await?;
        let provisioned = encrypt_and_provision_with(
            &self.api_client()?,
            &signer,
            AttachmentTarget::Group(&group),
            StorageObjectType::Attachment,
            &content,
            metadata,
        )
        .await?;
        let ProvisionedAttachment {
            ciphertext,
            response,
            ..
        } = match provisioned {
            Ok(provisioned) => provisioned,
            Err(error) => return Ok(Err(error)),
        };
        drop(content); // release the buffer early

        upload_encrypted_attachment(&self.http_client(), response, progress_tx, ciphertext).await?;
        debug!(%attachment_id, "uploaded attachment ciphertext");

        self.db()
            .with_write_transaction(async |txn| -> anyhow::Result<()> {
                AttachmentRecord::update_status(&mut *txn, attachment_id, AttachmentStatus::Ready)
                    .await?;
                hold_message_with_failed_upload(txn, reference.message_id).await
            })
            .await?;
        self.outbound_service().notify_upload_finished();
        Ok(Ok(()))
    }

    async fn persist_processed_part(
        &self,
        reference: &MessageAttachmentRecord,
        processed: &ProcessedAttachment,
        group_id: &GroupId,
    ) -> anyhow::Result<Option<AttachmentMetadata>> {
        let attachment_id = reference.attachment_id;
        let image_data = processed.image_data.as_ref();
        let dimensions = image_data.map(|data| (data.width, data.height));

        self.db()
            .with_write_transaction(async |txn| {
                let Some(mut message) = ChatMessage::load(&mut *txn, reference.message_id).await?
                else {
                    return Ok(None);
                };
                let content = message
                    .message_mut()
                    .mimi_content_mut()
                    .context("Attachment message without content")?;

                let part = content
                    .part_at_mut(reference.part_index)
                    .context("Attachment part is missing in the message")?;
                let metadata = AttachmentMetadata::from_part(part)?;
                let NestedPart::ExternalPart {
                    content_type,
                    url,
                    size,
                    content_hash,
                    filename,
                    ..
                } = part
                else {
                    bail!("Attachment part is not an external part");
                };
                *url = AttachmentUrl::new(metadata.remote_attachment_id, dimensions).to_string();
                *content_type = processed.content_type.clone();
                *size = processed.size;
                *content_hash = processed.content_hash.clone();
                *filename = processed.filename.clone();

                let processed_metadata = ProcessedMetadata {
                    content_type: &processed.content_type,
                    filename: &processed.filename,
                    content_hash: &processed.content_hash,
                    dimensions,
                    blurhash: image_data.map(|data| data.blurhash.as_str()),
                    is_animated: image_data.map(|data| data.is_animated),
                };
                if !AttachmentRecord::store_processed_content(
                    &mut *txn,
                    attachment_id,
                    processed.content.as_ref(),
                    processed_metadata,
                )
                .await?
                {
                    return Ok(None);
                }

                // Appended, so that the index of the attachment part stays
                if let Some(image_data) = image_data
                    && let NestedPart::MultiPart { parts, .. } = &mut content.nested_part
                {
                    parts.push(NestedPart::SinglePart {
                        disposition: Disposition::Preview,
                        language: String::new(),
                        content_type: media_type::TEXT_BLURHASH.to_owned(),
                        content: image_data.blurhash.clone().into_bytes(),
                    });
                }

                self.finalize_processed_part(txn, &mut message, reference, group_id)
                    .await?;
                Ok(Some(metadata))
            })
            .await
    }

    /// Stores the remote record of a processed part and the message.
    ///
    /// The message gets its Mimi ID once none of its parts is left to process.
    async fn finalize_processed_part(
        &self,
        txn: &mut WriteDbTransaction<'_>,
        message: &mut ChatMessage,
        reference: &MessageAttachmentRecord,
        group_id: &GroupId,
    ) -> anyhow::Result<()> {
        let content = message
            .message_mut()
            .mimi_content_mut()
            .context("Attachment message without content")?;
        let parts = AttachmentPart::collect(&*content)?;
        let remote = parts
            .iter()
            .find(|part| part.index == reference.part_index)
            .and_then(AttachmentPart::remote)
            .context("Processed part has no remote record")?;
        remote.store(&mut *txn, reference.attachment_id).await?;

        // A part without a hash is not processed yet
        let is_final = parts.iter().all(|part| match part.part {
            NestedPart::ExternalPart { content_hash, .. } => !content_hash.is_empty(),
            _ => true,
        });
        drop(parts);

        if is_final {
            // The content is final, so the message gets its Mimi ID
            let content = std::mem::take(content);
            message.set_content_message(ContentMessage::new(
                self.user_id().clone(),
                false,
                content,
                group_id,
            ));
        }
        message.update(&mut *txn).await?;
        Ok(())
    }

    /// Remote id, key and nonce of a processed part.
    ///
    /// A part stored without them, as older versions did, gets them here, and
    /// the message is finalized.
    async fn processed_part_metadata(
        &self,
        reference: &MessageAttachmentRecord,
        group_id: &GroupId,
    ) -> anyhow::Result<AttachmentMetadata> {
        let message = ChatMessage::load(self.db().read().await?, reference.message_id)
            .await?
            .context("Message of the attachment is gone")?;
        let part = message
            .message()
            .mimi_content()
            .and_then(|content| content.part_at(reference.part_index))
            .context("Part of the attachment is missing")?;
        let has_metadata = match part {
            NestedPart::ExternalPart { key, .. } => !key.is_empty(),
            _ => bail!("Attachment part is not an external part"),
        };
        if has_metadata {
            return AttachmentMetadata::from_part(part);
        }

        self.db()
            .with_write_transaction(async |txn| -> anyhow::Result<_> {
                let mut message = ChatMessage::load(&mut *txn, reference.message_id)
                    .await?
                    .context("Message of the attachment is gone")?;
                let part = message
                    .message_mut()
                    .mimi_content_mut()
                    .and_then(|content| content.part_at_mut(reference.part_index))
                    .context("Part of the attachment is missing")?;
                let metadata = AttachmentMetadata::random()?;
                metadata.write_to(part)?;
                self.finalize_processed_part(txn, &mut message, reference, group_id)
                    .await?;
                Ok(metadata)
            })
            .await
    }

    /// Marks the upload as failed.
    ///
    /// The message is marked as failed if there is no active upload anymore.
    async fn fail_upload(
        &self,
        reference: &MessageAttachmentRecord,
        failure: TransferFailure,
    ) -> anyhow::Result<()> {
        self.db()
            .with_write_transaction(async |txn| -> anyhow::Result<()> {
                AttachmentTransferRecord::fail_upload(&mut *txn, reference.attachment_id, failure)
                    .await?;
                hold_message_with_failed_upload(txn, reference.message_id).await
            })
            .await?;
        // A held message no longer holds back its chat
        self.outbound_service().notify_upload_finished();
        Ok(())
    }

    /// Records an upload whose task was dropped as failed.
    ///
    /// Does nothing if the upload finished in the meantime.
    pub async fn fail_cancelled_upload(&self, attachment_id: AttachmentId) -> anyhow::Result<()> {
        self.db()
            .with_write_transaction(async |txn| -> anyhow::Result<()> {
                let Some(reference) =
                    MessageAttachmentRecord::load_first(&mut *txn, attachment_id).await?
                else {
                    return Ok(());
                };
                AttachmentTransferRecord::fail_upload(
                    &mut *txn,
                    attachment_id,
                    TransferFailure::Unknown,
                )
                .await?;
                hold_message_with_failed_upload(txn, reference.message_id).await
            })
            .await?;
        // A held message no longer holds back its chat
        self.outbound_service().notify_upload_finished();
        Ok(())
    }
}

#[cfg(feature = "test_utils")]
impl CoreUser {
    /// Provisions an attachment upload of `declared_size` bytes with the id
    /// `object_id` in the chat's group and uploads `content` to it as is, with
    /// a `content-length` of `content`'s actual size.
    ///
    /// # Errors
    ///
    /// Fails with a [`airapiclient::ds_api::DsRequestError`] when the DS
    /// rejects the provisioning, and with a [`reqwest::Error`] when the
    /// storage rejects the upload.
    pub async fn upload_with_declared_size(
        &self,
        chat_id: ChatId,
        object_id: Uuid,
        declared_size: i64,
        content: Vec<u8>,
    ) -> anyhow::Result<()> {
        let mut connection = self.db().read().await?;
        let group = Group::load_ref_with_chat_id(&mut connection, chat_id)
            .await?
            .with_context(|| format!("Can't find group with id {chat_id:?}"))?;
        drop(connection);

        let target = group.attachment_target();
        let signer = self.attachment_signer(&target).await?;
        let response = self
            .api_client()?
            .ds_provision_attachment(
                &signer,
                target,
                declared_size,
                object_id,
                StorageObjectType::Attachment,
            )
            .await?;
        let mut request = self.http_client().put(response.upload_url);
        for header in response.upload_headers {
            if !header
                .key
                .eq_ignore_ascii_case(reqwest::header::CONTENT_LENGTH.as_str())
            {
                request = request.header(header.key, header.value);
            }
        }
        request.body(content).send().await?.error_for_status()?;
        Ok(())
    }
}

/// The message content an attachment is stored with.
///
/// The final message is derived by patching field values into these parts, so
/// what the UI shows first and what goes on the wire have the same shape.
pub(crate) fn attachment_content(parts: Vec<NestedPart>) -> anyhow::Result<MimiContent> {
    Ok(MimiContent {
        salt: Secret::<16>::random()?.secret().to_vec(),
        nested_part: NestedPart::MultiPart {
            disposition: Disposition::Attachment,
            part_semantics: PartSemantics::ProcessAll,
            parts,
            language: Default::default(),
        },
        ..Default::default()
    })
}

/// Holds the message as failed once none of its uploads is in flight and at
/// least one failed.
pub(crate) async fn hold_message_with_failed_upload(
    txn: &mut WriteDbTransaction<'_>,
    message_id: MessageId,
) -> anyhow::Result<()> {
    if AttachmentTransferRecord::has_active_upload(&mut *txn, message_id).await?
        || !AttachmentTransferRecord::has_failed_upload(&mut *txn, message_id).await?
    {
        return Ok(());
    }
    let Some(message) = ChatMessage::load(&mut *txn, message_id).await? else {
        return Ok(());
    };
    ChatMessageQueue::new(message.chat_id(), message_id)
        .remove_and_mark_as_failed(txn)
        .await?;
    Ok(())
}

/// What a send task starts from.
pub(crate) enum AttachmentSource {
    /// First send: the file has not been read or processed yet.
    File {
        file: AttachmentFile,
        spec: AttachmentSpec,
    },
    /// Retry: the processed bytes persisted by the first send.
    Processed(Vec<u8>),
}

/// What the send needs to know about an attachment besides its bytes.
pub(crate) struct AttachmentSpec {
    pub(crate) filename: String,
    pub(crate) content_type: String,
    pub(crate) is_image: bool,
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

/// What the header sniff learned about an attachment, *before* its bytes have
/// been read or decoded.
#[derive(Debug, Clone)]
pub(crate) struct ProbedAttachment {
    pub(crate) filename: String,
    pub(crate) content_type: String,
    pub(crate) size: u64,
    pub(crate) image_dimensions: Option<(u32, u32)>,
}

impl ProbedAttachment {
    pub(crate) fn from_path(
        path: PathBuf,
    ) -> anyhow::Result<Result<Self, ProvisionAttachmentError>> {
        let size = std::fs::metadata(&path)?.len();
        let image_dimensions = match probe_attachment_image(&path)? {
            Ok(dimensions) => dimensions,
            Err(error) => {
                // The file looks like an image but its header does not decode.
                error!(%error, "failed to probe attachment image");
                return Ok(Err(ProvisionAttachmentError::DecodingError));
            }
        };

        if image_dimensions.is_some() && size > MAX_IMAGE_SOURCE_SIZE {
            return Ok(Err(ProvisionAttachmentError::TooLarge(
                AttachmentTooLargeDetail {
                    max_size_bytes: MAX_IMAGE_SOURCE_SIZE,
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

    pub(crate) fn is_image(&self) -> bool {
        self.image_dimensions.is_some()
    }
}

/// Provisional external part for already processed WebP image bytes.
///
/// Complete except for remote id, key and nonce, which provisioning patches in.
pub(crate) fn provisional_image_part(
    attachment_id: AttachmentId,
    disposition: Disposition,
    bytes: &[u8],
    dimensions: (u32, u32),
) -> NestedPart {
    let url = AttachmentUrl::new(
        RemoteAttachmentId::new(attachment_id.uuid),
        Some(dimensions),
    );
    NestedPart::ExternalPart {
        disposition,
        language: String::new(),
        content_type: WEBP_CONTENT_TYPE.to_owned(),
        url: url.to_string(),
        expires: 0,
        size: bytes.len() as u64,
        enc_alg: AIR_ATTACHMENT_ENCRYPTION_ALG,
        key: Vec::new(),
        nonce: Vec::new(),
        aad: Default::default(),
        hash_alg: AIR_ATTACHMENT_HASH_ALG,
        content_hash: Sha256::digest(bytes).to_vec(),
        description: Default::default(),
        filename: String::new(),
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
}

pub(crate) const WEBP_CONTENT_TYPE: &str = "image/webp";

impl ProcessedAttachment {
    fn from_bytes(
        bytes: Vec<u8>,
        spec: AttachmentSpec,
        processing_cancel: ImageProcessingCancellation,
    ) -> anyhow::Result<Self> {
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
            } = reencode_attachment_image(bytes, processing_cancel.clone())?;
            let image_data = ProcessedAttachmentImageData {
                blurhash,
                width,
                height,
                is_animated,
            };
            (
                webp_image.into(),
                WEBP_CONTENT_TYPE.to_owned(),
                Self::image_filename(),
                Some(image_data),
            )
        } else {
            (bytes.into(), content_type, filename, None)
        };

        let mut hasher = Sha256::new();
        for chunk in content.as_ref().chunks(1024 * 1024) {
            processing_cancel.ensure_running()?;
            hasher.update(chunk);
        }
        let content_hash = hasher.finalize().to_vec();

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
}

/// Metadata of an encrypted and uploaded attachment
pub struct AttachmentMetadata {
    remote_attachment_id: RemoteAttachmentId,
    key: AttachmentEarKey,
    nonce: [u8; 12],
}

impl AttachmentMetadata {
    pub(crate) fn random() -> anyhow::Result<Self> {
        let remote_attachment_id = RemoteAttachmentId::random();
        let key = AttachmentEarKey::random()?;
        let nonce = *Secret::<12>::random()?.secret();
        Ok(Self {
            remote_attachment_id,
            key,
            nonce,
        })
    }

    fn from_part(part: &NestedPart) -> anyhow::Result<Self> {
        let NestedPart::ExternalPart {
            url, key, nonce, ..
        } = part
        else {
            bail!("not an external part");
        };
        let remote_attachment_id = url.parse::<AttachmentUrl>()?.remote_attachment_id();
        let key = key
            .as_slice()
            .try_into()
            .map(AttachmentEarKey::from_bytes)
            .ok()
            .context("invalid attachment key length")?;
        let nonce = nonce
            .as_slice()
            .try_into()
            .ok()
            .context("invalid attachment nonce length")?;
        Ok(Self {
            remote_attachment_id,
            key,
            nonce,
        })
    }

    pub(crate) fn write_to(&self, part: &mut NestedPart) -> anyhow::Result<()> {
        let NestedPart::ExternalPart {
            url, key, nonce, ..
        } = part
        else {
            bail!("not an external part");
        };
        let dimensions = url.parse::<AttachmentUrl>()?.dimensions();
        *url = AttachmentUrl::new(self.remote_attachment_id, dimensions).to_string();
        *key = self.key.as_bytes().to_vec();
        *nonce = self.nonce.to_vec();
        Ok(())
    }

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
    Group(&'a GroupRef),
    User(&'a UserId),
}

impl<'a> From<AttachmentTarget<'a>> for DsAttachmentTarget<'a> {
    fn from(target: AttachmentTarget<'a>) -> Self {
        match target {
            AttachmentTarget::Group(group) => group.attachment_target(),
            AttachmentTarget::User(user_id) => DsAttachmentTarget::User { user_id },
        }
    }
}

struct ProvisionedAttachment {
    metadata: AttachmentMetadata,
    ciphertext: Vec<u8>,
    response: ProvisionAttachmentResponse,
}

async fn encrypt_and_provision_with(
    api_client: &ApiClient,
    signing_key: &SigningKey<ClientKeyType>,
    target: AttachmentTarget<'_>,
    object_type: StorageObjectType,
    content: &AttachmentBytes,
    metadata: AttachmentMetadata,
) -> anyhow::Result<Result<ProvisionedAttachment, ProvisionAttachmentError>> {
    // Same plaintext encoding as `AeadEncryptable::encrypt`
    let plaintext = content.tls_serialize_detached()?;
    let (ciphertext, _nonce) = metadata
        .key
        .encrypt_with_nonce(plaintext.as_slice(), metadata.nonce)?
        .into_parts();
    drop(plaintext);

    let content_length = ciphertext.len().try_into().context("usize overflow")?;
    let object_id = metadata.remote_attachment_id.uuid();
    let response = match api_client
        .ds_provision_attachment(
            signing_key,
            target.into(),
            content_length,
            object_id,
            object_type,
        )
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

    let response_object_id: Uuid = response.object_id.context("no object id")?.into();
    ensure!(response_object_id == object_id, "unexpected object id");
    Ok(Ok(ProvisionedAttachment {
        metadata,
        ciphertext,
        response,
    }))
}

async fn encrypt_and_provision(
    api_client: &ApiClient,
    signing_key: &SigningKey<ClientKeyType>,
    target: AttachmentTarget<'_>,
    object_type: StorageObjectType,
    content: &AttachmentBytes,
) -> anyhow::Result<Result<ProvisionedAttachment, ProvisionAttachmentError>> {
    let metadata = AttachmentMetadata::random()?;
    encrypt_and_provision_with(
        api_client,
        signing_key,
        target,
        object_type,
        content,
        metadata,
    )
    .await
}
async fn upload_encrypted_attachment(
    http_client: &reqwest::Client,
    provision_response: ProvisionAttachmentResponse,
    mut progress_tx: AttachmentProgressSender,
    ciphertext: Vec<u8>,
) -> anyhow::Result<()> {
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
                let _ = tx.send(AttachmentProgressEvent::Progress {
                    bytes_total,
                    bytes_loaded: uploaded,
                });
            }
        }
        chunk
    });

    let response = request.body(Body::wrap_stream(stream)).send().await?;
    // The status 412 is interpreted as object exists and it is this one (so
    // storage is assumed to refuse to override).
    //
    // FIXME: In this case, we could check the last 16 bytes of the AES-GCM
    // ciphertext of the already existing object.
    if response.status() != reqwest::StatusCode::PRECONDITION_FAILED {
        response.error_for_status()?;
    }

    progress_tx.completed();
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs::File;

    use aircommon::DEFAULT_MAX_ATTACHMENT_SIZE;
    use tempfile::tempdir;

    use super::ProbedAttachment;

    #[test]
    fn ordinary_file_can_exceed_the_default_server_limit() -> anyhow::Result<()> {
        let directory = tempdir()?;
        let path = directory.path().join("large.bin");
        let file = File::create(&path)?;
        file.set_len(DEFAULT_MAX_ATTACHMENT_SIZE + 1)?;

        assert!(ProbedAttachment::from_path(path)?.is_ok());
        Ok(())
    }
}
