// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{
    ffi::OsStr,
    io::Cursor,
    mem,
    path::{Path, PathBuf},
};

use airapiclient::{ApiClient, ds_api::ProvisionAttachmentResponse};
use aircommon::{
    credentials::keys::ClientSigningKey,
    crypto::ear::{AeadCiphertext, EarEncryptable, keys::AttachmentEarKey},
    identifiers::AttachmentId,
};
use airprotos::delivery_service::v1::SignedPostPolicy;
use anyhow::{Context, bail, ensure};
use base64::{Engine, prelude::BASE64_STANDARD};
use chrono::{DateTime, Utc};
use infer::MatcherType;
use mimi_content::{
    MimiContent,
    content_container::{Disposition, NestedPart, NestedPartContent, PartSemantics},
};
use reqwest::{Body, multipart};
use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use tokio_stream::StreamExt;
use tokio_util::io::ReaderStream;

use crate::{
    AttachmentContent, AttachmentStatus, AttachmentUrl, Chat, ChatId, ChatMessage, MessageId,
    clients::{
        CoreUser,
        attachment::{
            AttachmentBytes, AttachmentRecord,
            ear::{AIR_ATTACHMENT_ENCRYPTION_ALG, AIR_ATTACHMENT_HASH_ALG},
            progress::{AttachmentProgress, AttachmentProgressSender},
        },
    },
    groups::Group,
    store::{Store, StoreNotifier},
    utils::{
        connection_ext::StoreExt,
        image::{ReencodedAttachmentImage, reencode_attachment_image},
    },
};

impl CoreUser {
    /// Uploads an attachment and sends a message containing it.
    pub(crate) async fn upload_attachment(
        &self,
        chat_id: ChatId,
        path: &Path,
    ) -> anyhow::Result<(
        AttachmentId,
        AttachmentProgress,
        impl Future<Output = anyhow::Result<ChatMessage>> + use<>,
    )> {
        let (chat, group) = self
            .with_transaction(async |txn| {
                let chat = Chat::load(txn, &chat_id)
                    .await?
                    .with_context(|| format!("Can't find chat with id {chat_id}"))?;

                let group_id = chat.group_id();
                let group = Group::load_clean(txn, group_id)
                    .await?
                    .with_context(|| format!("Can't find group with id {group_id:?}"))?;
                Ok((chat, group))
            })
            .await?;

        // load the attachment data
        let mut attachment = ProcessedAttachment::from_file(path)?;

        // encrypt the content and provision the attachment, but don't upload it yet
        let (attachment_metadata, ciphertext, provision_response) = encrypt_and_provision(
            &self.api_client()?,
            self.signing_key(),
            &group,
            &attachment.content,
        )
        .await?;

        // store local attachment message
        let attachment_id = attachment_metadata.attachment_id;
        let content_bytes = mem::replace(&mut attachment.content.bytes, Vec::new().into());
        let content_type = attachment.content_type;

        let content = MimiContent {
            nested_part: NestedPart {
                disposition: Disposition::Attachment,
                part: NestedPartContent::MultiPart {
                    part_semantics: PartSemantics::ProcessAll,
                    parts: attachment.into_nested_parts(attachment_metadata)?,
                },
                ..Default::default()
            },
            ..Default::default()
        };

        // Note: Acquire a transaction here to ensure that the attachment will be deleted from the
        // local database in case of an error.
        let message = self
            .with_transaction_and_notifier(async |txn, notifier| {
                let message_id = MessageId::random();
                let message = self
                    .send_message_transactional(txn, notifier, chat_id, message_id, content)
                    .await?;

                // store attachment locally
                // (must be done after the message is stored locally due to foreign key constraints)
                let record = AttachmentRecord {
                    attachment_id,
                    chat_id: chat.id(),
                    message_id,
                    content_type: content_type.to_owned(),
                    status: AttachmentStatus::Uploading,
                    created_at: Utc::now(),
                };
                record
                    .store(txn.as_mut(), notifier, Some(content_bytes.as_slice()))
                    .await?;

                Ok(message)
            })
            .await?;

        // upload the encrypted attachment
        let (progress, task) =
            self.upload_attachment_task(attachment_id, message, ciphertext, provision_response);
        Ok((attachment_id, progress, task))
    }

    pub async fn retry_upload_attachment(
        &self,
        attachment_id: AttachmentId,
    ) -> anyhow::Result<(
        AttachmentId,
        AttachmentProgress,
        impl Future<Output = anyhow::Result<ChatMessage>> + use<>,
    )> {
        // load locally stored data
        let (group, mut message, content) = self
            .with_transaction(async |txn| {
                let AttachmentContent::Uploading(bytes) =
                    self.load_attachment(attachment_id).await?
                else {
                    bail!("Attachment {attachment_id:?} is not uploading");
                };
                let content = AttachmentBytes::from(bytes);

                let attachment_record = AttachmentRecord::load(self.pool(), attachment_id)
                    .await?
                    .context("Attachment not found")?;
                ensure!(
                    matches!(attachment_record.status, AttachmentStatus::Uploading),
                    "Attachment is not uploading"
                );

                let message = self
                    .message(attachment_record.message_id)
                    .await?
                    .context("Message not found")?;
                ensure!(!message.is_sent(), "Message is already sent");

                let chat_id = message.chat_id();
                let chat = Chat::load(txn, &chat_id)
                    .await?
                    .with_context(|| format!("Can't find chat with id {chat_id}"))?;

                let group_id = chat.group_id();
                let group = Group::load_clean(txn, group_id)
                    .await?
                    .with_context(|| format!("Can't find group with id {group_id:?}"))?;
                Ok((group, message, content))
            })
            .await?;

        // encrypt the content and provision the attachment, but don't upload it yet
        let (attachment_metadata, ciphertext, provision_response) =
            encrypt_and_provision(&self.api_client()?, self.signing_key(), &group, &content)
                .await?;

        // update local attachment message

        // Note: The url of the attachment also changes here, so the relationship between the old
        // attachment record and this message is broken. We must copy the attachment record with
        // the new attachment id.
        if let Some(mimi_content) = message.message_mut().mimi_content_mut()
            && let NestedPartContent::MultiPart { parts, .. } = &mut mimi_content.nested_part.part
            && let Some(attachment_part) = parts
                .iter_mut()
                .find(|part| part.disposition == Disposition::Attachment)
            && let NestedPartContent::ExternalPart {
                url, key, nonce, ..
            } = &mut attachment_part.part
            && let Ok(attachment_url) = AttachmentUrl::from_url(&url.parse()?)
        {
            *url = AttachmentUrl::new(attachment_metadata.attachment_id, attachment_url.dimensions)
                .to_string();
            *key = attachment_metadata.key.into_bytes().to_vec().into();
            *nonce = attachment_metadata.nonce.to_vec().into();

            self.with_transaction_and_notifier(async |txn, notifier| {
                message.update(txn.as_mut(), notifier).await?;
                // Since we just move the attachment record, we don't need to notify the store.
                let mut noop_notifier = StoreNotifier::noop();
                AttachmentRecord::copy(
                    txn.as_mut(),
                    &mut noop_notifier,
                    attachment_id,
                    attachment_metadata.attachment_id,
                )
                .await?;
                AttachmentRecord::delete(txn.as_mut(), &mut noop_notifier, attachment_id).await?;
                Ok(())
            })
            .await?;
        } else {
            bail!("Invalid attachment mimi content");
        }

        // upload task
        let (progress, upload_task) = self.upload_attachment_task(
            attachment_metadata.attachment_id,
            message,
            ciphertext,
            provision_response,
        );
        Ok((attachment_metadata.attachment_id, progress, upload_task))
    }

    fn upload_attachment_task(
        &self,
        attachment_id: AttachmentId,
        message: ChatMessage,
        ciphertext: Vec<u8>,
        provision_response: ProvisionAttachmentResponse,
    ) -> (
        AttachmentProgress,
        impl Future<Output = anyhow::Result<ChatMessage>> + use<>,
    ) {
        let (progress_tx, progress) = AttachmentProgress::new();
        let http_client = self.http_client();
        let pool = self.pool().clone();
        let task = async move {
            let res = upload_encrypted_attachment(
                &http_client,
                provision_response,
                progress_tx,
                ciphertext,
            )
            .await;
            let status = if res.is_ok() {
                AttachmentStatus::Ready
            } else {
                AttachmentStatus::Failed
            };
            AttachmentRecord::update_status(&pool, attachment_id, status).await?;
            Ok(message)
        };
        (progress, task)
    }
}

/// In-memory loaded and processed attachment
///
/// If it is an image, it will contain additional image data, like a blurhash.
struct ProcessedAttachment {
    filename: String,
    content: AttachmentBytes,
    content_hash: Vec<u8>,
    content_type: &'static str,
    image_data: Option<ProcessedAttachmentImageData>,
    size: u64,
}

struct ProcessedAttachmentImageData {
    blurhash: String,
    width: u32,
    height: u32,
}

impl ProcessedAttachment {
    fn from_file(path: &Path) -> anyhow::Result<Self> {
        // TODO(#589): Avoid reading the whole file into memory when it is an image.
        // Instead, it should be re-encoded directly from the file.
        let content = std::fs::read(path)
            .with_context(|| format!("Failed to read file at {}", path.display()))?;
        let mime = infer::get(&content);

        let (content, content_type, image_data): (AttachmentBytes, _, _) = if mime
            .map(|mime| mime.matcher_type() == MatcherType::Image)
            .unwrap_or(false)
        {
            let ReencodedAttachmentImage {
                webp_image,
                image_dimensions: (width, height),
                blurhash,
            } = reencode_attachment_image(content)?;
            let image_data = ProcessedAttachmentImageData {
                blurhash,
                width,
                height,
            };
            (webp_image.into(), "image/webp", Some(image_data))
        } else {
            let content_type = mime
                .as_ref()
                .map(|mime| mime.mime_type())
                .unwrap_or("application/octet-stream");
            (content.into(), content_type, None)
        };

        let content_hash = Sha256::digest(&content).to_vec();

        let mut filename = PathBuf::from(
            path.file_name()
                .unwrap_or_else(|| OsStr::new("attachment.bin")),
        );
        if image_data.is_some() {
            filename.set_extension("webp");
        }

        let size = content
            .as_ref()
            .len()
            .try_into()
            .context("attachment size overflow")?;

        Ok(Self {
            filename: filename.to_string_lossy().to_string(),
            content,
            content_type,
            content_hash,
            image_data,
            size,
        })
    }

    fn into_nested_parts(self, metadata: AttachmentMetadata) -> anyhow::Result<Vec<NestedPart>> {
        let url = AttachmentUrl::new(
            metadata.attachment_id,
            self.image_data
                .as_ref()
                .map(|data| (data.width, data.height)),
        );

        let attachment = NestedPart {
            disposition: Disposition::Attachment,
            language: String::new(),
            part: NestedPartContent::ExternalPart {
                content_type: self.content_type.to_owned(),
                url: url.to_string(),
                expires: 0,
                size: self.size,
                enc_alg: AIR_ATTACHMENT_ENCRYPTION_ALG,
                key: metadata.key.into_bytes().to_vec().into(),
                nonce: metadata.nonce.to_vec().into(),
                aad: Default::default(),
                hash_alg: AIR_ATTACHMENT_HASH_ALG,
                content_hash: self.content_hash.into(),
                description: Default::default(),
                filename: self.filename,
            },
        };

        let blurhash = self.image_data.map(|data| NestedPart {
            disposition: Disposition::Preview,
            language: String::new(),
            part: NestedPartContent::SinglePart {
                content_type: "text/blurhash".to_owned(),
                content: data.blurhash.into_bytes().into(),
            },
        });

        Ok([Some(attachment), blurhash].into_iter().flatten().collect())
    }
}

/// Metadata of an encrypted and uploaded attachment
struct AttachmentMetadata {
    attachment_id: AttachmentId,
    key: AttachmentEarKey,
    nonce: [u8; 12],
}

async fn encrypt_and_provision(
    api_client: &ApiClient,
    signing_key: &ClientSigningKey,
    group: &Group,
    content: &AttachmentBytes,
) -> anyhow::Result<(AttachmentMetadata, Vec<u8>, ProvisionAttachmentResponse)> {
    // encrypt the content
    let key = AttachmentEarKey::random()?;
    let ciphertext: AeadCiphertext = content.encrypt(&key)?.into();
    let (ciphertext, nonce) = ciphertext.into_parts();

    // provision attachment
    let content_length = ciphertext.len().try_into().context("usize overflow")?;
    let response = api_client
        .ds_provision_attachment(
            signing_key,
            group.group_state_ear_key(),
            group.group_id(),
            group.own_index(),
            content_length,
        )
        .await?;

    let attachment_id =
        AttachmentId::new(response.attachment_id.context("no attachment id")?.into());

    let metadata = AttachmentMetadata {
        attachment_id,
        key,
        nonce,
    };
    Ok((metadata, ciphertext, response))
}

async fn upload_encrypted_attachment(
    http_client: &reqwest::Client,
    provision_response: ProvisionAttachmentResponse,
    mut progress_tx: AttachmentProgressSender,
    ciphertext: Vec<u8>,
) -> anyhow::Result<()> {
    if let Some(signed_post_policy) = provision_response.post_policy {
        // upload encrypted content via multipart upload
        progress_tx.report(0);
        let total_len = ciphertext.len();
        multipart_upload(
            http_client,
            &provision_response.upload_url,
            signed_post_policy,
            ciphertext,
        )
        .await?;
        // Note: multipart does not support reporting progress for now
        progress_tx.report(total_len);
        progress_tx.finish();
    } else {
        // upload encrypted content via signed PUT url
        let mut request = http_client.put(provision_response.upload_url);
        for header in provision_response.upload_headers {
            request = request.header(header.key, header.value);
        }

        let mut uploaded = 0;
        let total_len = ciphertext.len();

        let stream = ReaderStream::new(Cursor::new(ciphertext)).map(move |chunk| {
            if let Ok(chunk) = &chunk {
                uploaded += chunk.len();
                if uploaded == total_len {
                    progress_tx.finish();
                } else {
                    progress_tx.report(uploaded);
                }
            }
            chunk
        });

        request
            .body(Body::wrap_stream(stream))
            .send()
            .await?
            .error_for_status()?;
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
) -> anyhow::Result<()> {
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

    let form = form.part("file", multipart::Part::bytes(ciphertext));

    http_client
        .post(upload_url)
        .multipart(form)
        .send()
        .await?
        .error_for_status()?;

    Ok(())
}
