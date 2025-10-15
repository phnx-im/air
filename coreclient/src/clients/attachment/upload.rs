// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{
    ffi::OsStr,
    mem,
    path::{Path, PathBuf},
};

use airapiclient::ApiClient;
use aircommon::{
    credentials::keys::ClientSigningKey,
    crypto::ear::{AeadCiphertext, EarEncryptable, keys::AttachmentEarKey},
    identifiers::AttachmentId,
};
use anyhow::Context;
use chrono::Utc;
use infer::MatcherType;
use mimi_content::{
    MimiContent,
    content_container::{Disposition, NestedPart, NestedPartContent, PartSemantics},
};
use sha2::{Digest, Sha256};

use crate::{
    AttachmentStatus, AttachmentUrl, Chat, ChatId, ChatMessage, MessageId,
    clients::{
        CoreUser,
        attachment::{
            AttachmentBytes, AttachmentRecord,
            ear::{AIR_ATTACHMENT_ENCRYPTION_ALG, AIR_ATTACHMENT_HASH_ALG},
        },
    },
    groups::Group,
    utils::image::{ReencodedAttachmentImage, reencode_attachment_image},
};

impl CoreUser {
    /// Uploads an attachment and sends a message containing it.
    pub(crate) async fn upload_attachment(
        &self,
        chat_id: ChatId,
        path: &Path,
    ) -> anyhow::Result<ChatMessage> {
        let (chat, group) = self
            .with_transaction(async |txn| -> anyhow::Result<_> {
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

        // encrypt the content and upload the content
        let api_client = self.api_client()?;
        let http_client = self.http_client();

        let attachment_metadata = encrypt_and_upload(
            &api_client,
            &http_client,
            self.signing_key(),
            &attachment.content,
            &group,
        )
        .await?;

        // send attachment message
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
        self.with_transaction_and_notifier(async |txn, notifier| {
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
                status: AttachmentStatus::Ready,
                created_at: Utc::now(),
            };
            record
                .store(txn.as_mut(), notifier, Some(content_bytes.as_slice()))
                .await?;

            Ok(message)
        })
        .await
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

async fn encrypt_and_upload(
    api_client: &ApiClient,
    http_client: &reqwest::Client,
    signing_key: &ClientSigningKey,
    content: &AttachmentBytes,
    group: &Group,
) -> anyhow::Result<AttachmentMetadata> {
    // encrypt the content
    let key = AttachmentEarKey::random()?;
    let ciphertext: AeadCiphertext = content.encrypt(&key)?.into();
    let (ciphertext, nonce) = ciphertext.into_parts();

    // provision attachment
    let response = api_client
        .ds_provision_attachment(
            signing_key,
            group.group_state_ear_key(),
            group.group_id(),
            group.own_index(),
        )
        .await?;
    let attachment_id =
        AttachmentId::new(response.attachment_id.context("no attachment id")?.into());

    // upload encrypted content
    let mut request = http_client.put(response.upload_url);
    for header in response.upload_headers {
        request = request.header(header.key, header.value);
    }
    request.body(ciphertext).send().await?.error_for_status()?;

    Ok(AttachmentMetadata {
        attachment_id,
        key,
        nonce,
    })
}
