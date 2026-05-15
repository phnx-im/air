// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::num::TryFromIntError;

use airapiclient::{ApiClientInitError, ds_api::DsRequestError};
use aircommon::{
    crypto::{
        aead::{AeadCiphertext, AeadDecryptable, keys::AttachmentEarKey},
        errors::DecryptionError,
    },
    identifiers::AttachmentId,
};
use airprotos::delivery_service::v1::StorageObjectType;
use mimi_content::content_container::{EncryptionAlgorithm, HashAlgorithm};
use reqwest::StatusCode;
use sha2::{Digest, Sha256};
use tokio_stream::StreamExt;
use tracing::{debug, error, info};

use crate::{
    AttachmentProgress,
    clients::{
        CoreUser,
        attachment::{
            AttachmentBytes, AttachmentRecord,
            aead::{AIR_ATTACHMENT_ENCRYPTION_ALG, AIR_ATTACHMENT_HASH_ALG, EncryptedAttachment},
            persistence::{AttachmentStatus, PendingAttachmentRecord},
            progress::AttachmentProgressSender,
        },
    },
    groups::Group,
};

#[derive(Debug, thiserror::Error)]
enum AttachmentDownloadError {
    #[error("failed to initialize API client: {0}")]
    ApiClientInit(#[from] ApiClientInitError),
    #[error("failed to get attachment download URL: {0}")]
    DsRequest(#[from] DsRequestError),
    #[error("failed to download attachment: {0}")]
    Http(#[from] reqwest::Error),
    #[error("attachment size overflow: {0}")]
    SizeOverflow(#[from] TryFromIntError),
    #[error("unsupported encryption algorithm: {0:?}")]
    UnsupportedEncryptionAlgorithm(EncryptionAlgorithm),
    #[error("invalid nonce length")]
    InvalidNonceLength,
    #[error("invalid key length")]
    InvalidKeyLength,
    #[error("unsupported hash algorithm: {0:?}")]
    UnsupportedHashAlgorithm(HashAlgorithm),
    #[error("failed to decrypt attachment: {0}")]
    Decryption(#[from] DecryptionError),
    #[error("hash mismatch")]
    HashMismatch,
    #[error("attachment not found")]
    NotFound,
}

impl CoreUser {
    pub(crate) fn download_attachment(
        &self,
        attachment_id: AttachmentId,
    ) -> (
        AttachmentProgress,
        impl Future<Output = anyhow::Result<()>> + use<>,
    ) {
        let (progress_tx, progress) = AttachmentProgress::new();
        let fut = self
            .clone()
            .download_attachment_impl(attachment_id, progress_tx);
        (progress, fut)
    }

    async fn download_attachment_impl(
        self,
        attachment_id: AttachmentId,
        mut progress_tx: AttachmentProgressSender,
    ) -> anyhow::Result<()> {
        info!(?attachment_id, "downloading attachment");
        progress_tx.report(0);

        // Load the pending attachment record and update the status to `Downloading`.
        let Some((pending_record, group)) = self
            .db()
            .with_write_transaction(async |txn| -> anyhow::Result<_> {
                let Some(pending_record) =
                    PendingAttachmentRecord::load_pending(&mut *txn, attachment_id).await?
                else {
                    debug!(
                        ?attachment_id,
                        "Skipping downloading non-pending attachment"
                    );
                    return Ok(None);
                };
                let Some(record) = AttachmentRecord::load(&mut *txn, attachment_id).await? else {
                    error!(?attachment_id, "Attachment record not found");
                    return Ok(None);
                };
                let chat_id = record.chat_id;
                let Some(group) = Group::load_with_chat_id(&mut *txn, chat_id).await? else {
                    error!(?attachment_id, "Group not found");
                    return Ok(None);
                };

                AttachmentRecord::update_status(txn, attachment_id, AttachmentStatus::Downloading)
                    .await?;

                Ok(Some((pending_record, group)))
            })
            .await?
        else {
            return Ok(());
        };

        match self
            .download_and_decrypt_attachment(pending_record, &group, &progress_tx)
            .await
        {
            Ok(content) => {
                // Store the attachment and mark it as downloaded
                let bytes = content.bytes.as_slice();
                self.db()
                    .with_write_transaction(async |txn| -> anyhow::Result<()> {
                        AttachmentRecord::set_content(&mut *txn, attachment_id, bytes).await?;
                        PendingAttachmentRecord::delete(txn, attachment_id).await?;
                        Ok(())
                    })
                    .await?;

                progress_tx.completed();

                Ok(())
            }
            Err(error @ AttachmentDownloadError::NotFound) => {
                error!(?attachment_id, "attachment not found (expired?)");

                // if the attachment wasn't found, we mark it as NotFound but also destroy
                // the pending attachment, which cannot be recovered from anyways.
                self.db()
                    .with_write_transaction(async |txn| -> anyhow::Result<()> {
                        AttachmentRecord::update_status(
                            &mut *txn,
                            attachment_id,
                            AttachmentStatus::NotFound,
                        )
                        .await?;
                        PendingAttachmentRecord::delete(txn, attachment_id).await?;
                        Ok(())
                    })
                    .await
                    .inspect_err(
                        |e| error!(?attachment_id, %e, "failed to mark download as failed"),
                    )
                    .ok();

                progress_tx.not_found();

                Err(error.into())
            }
            Err(error) => {
                error!(
                    ?attachment_id,
                    %error,
                    "attachment download error, marking as failed"
                );

                // if this fails, the AttachmentRecord status stays in Downloading
                // and we still have the PendingAttachmentRecord, so we can retry
                AttachmentRecord::update_status(
                    self.db().write().await?,
                    attachment_id,
                    AttachmentStatus::DownloadFailed,
                )
                .await
                .inspect_err(|e| error!(?attachment_id, %e, "failed to mark download as failed"))
                .ok();

                Err(error.into())
            }
        }
    }

    async fn download_and_decrypt_attachment(
        &self,
        PendingAttachmentRecord {
            aad: _,
            attachment_id,
            enc_alg,
            enc_key,
            hash,
            hash_alg,
            nonce,
            size,
        }: PendingAttachmentRecord,
        group: &Group,
        progress_tx: &AttachmentProgressSender,
    ) -> Result<AttachmentBytes, AttachmentDownloadError> {
        // Check encryption parameters
        debug!(?attachment_id, "Checking encryption parameters");
        if enc_alg != AIR_ATTACHMENT_ENCRYPTION_ALG
            // Older clients (<= v0.9.0) specified Aes256Gcm12 as encryption algorithm, however
            // they were actually using Aes256Gcm. To be forward compatible, we also accept the the
            // correctly specified algorithm.
            && enc_alg != EncryptionAlgorithm::Aes256Gcm
        {
            return Err(AttachmentDownloadError::UnsupportedEncryptionAlgorithm(
                enc_alg,
            ));
        }

        let nonce: [u8; 12] = nonce
            .try_into()
            .map_err(|_| AttachmentDownloadError::InvalidNonceLength)?;

        let key = AttachmentEarKey::from_bytes(
            enc_key
                .try_into()
                .map_err(|_| AttachmentDownloadError::InvalidKeyLength)?,
        );
        if hash_alg != AIR_ATTACHMENT_HASH_ALG {
            return Err(AttachmentDownloadError::UnsupportedHashAlgorithm(hash_alg));
        }

        // Get the download URL from DS
        let api_client = self.api_clients().default_client()?;
        let download_url = api_client
            .ds_get_attachment_url(
                self.signing_key(),
                group.group_state_ear_key(),
                group.group_id(),
                group.own_index(),
                attachment_id,
                StorageObjectType::Attachment,
            )
            .await?;
        debug!(?attachment_id, %download_url, "Got download URL from DS");

        // Download the attachment
        debug!(?attachment_id, "Downloading attachment");
        let http_response = self
            .http_client()
            .get(download_url)
            .send()
            .await?
            .error_for_status();

        let mut bytes_stream = match http_response {
            Ok(response) => response.bytes_stream(),
            Err(error) => match error.status() {
                Some(status) if status == StatusCode::NOT_FOUND => {
                    return Err(AttachmentDownloadError::NotFound);
                }
                _ => return Err(AttachmentDownloadError::Http(error)),
            },
        };

        let total_len = size.try_into()?;
        let mut bytes = Vec::with_capacity(total_len);
        while let Some(chunk) = bytes_stream.next().await.transpose()? {
            bytes.extend_from_slice(&chunk);
            progress_tx.report(bytes.len());
        }

        // Decrypt the attachment
        debug!(?attachment_id, "Decrypting attachment");

        let ciphertext = EncryptedAttachment::from(AeadCiphertext::new(bytes, nonce));
        let content: AttachmentBytes = AttachmentBytes::decrypt(&key, &ciphertext)?;

        // Verify hash
        debug!(?attachment_id, "Verifying hash");
        let recalculated_content_hash = Sha256::digest(&content.bytes);
        if recalculated_content_hash.as_slice() != hash {
            return Err(AttachmentDownloadError::HashMismatch);
        }

        Ok(content)
    }
}
