// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::time::Duration;

use aircommon::{
    crypto::aead::{AeadCiphertext, AeadDecryptable, keys::AttachmentEarKey},
    identifiers::AttachmentId,
};
use airprotos::delivery_service::v1::StorageObjectType;
use anyhow::{Context, anyhow, ensure};
use mimi_content::content_container::EncryptionAlgorithm;
use sha2::{Digest, Sha256};
use tokio_stream::StreamExt;
use tracing::{debug, error, info, warn};

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
    utils::connection_ext::StoreExt,
};

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
            .with_transaction(async |txn| -> anyhow::Result<_> {
                let Some(pending_record) =
                    PendingAttachmentRecord::load_pending(txn.as_mut(), attachment_id).await?
                else {
                    debug!(
                        ?attachment_id,
                        "Skipping downloading non-pending attachment"
                    );
                    return Ok(None);
                };
                let Some(record) = AttachmentRecord::load(txn.as_mut(), attachment_id).await?
                else {
                    error!(?attachment_id, "Attachment record not found");
                    return Ok(None);
                };
                let chat_id = record.chat_id;
                let Some(group) = Group::load_with_chat_id(txn, chat_id).await? else {
                    error!(?attachment_id, "Group not found");
                    return Ok(None);
                };

                AttachmentRecord::update_status(
                    txn.as_mut(),
                    attachment_id,
                    AttachmentStatus::Downloading,
                )
                .await?;

                Ok(Some((pending_record, group)))
            })
            .await?
        else {
            return Ok(());
        };

        // Check encryption parameters
        debug!(?attachment_id, "Checking encryption parameters");
        ensure!(
            pending_record.enc_alg == AIR_ATTACHMENT_ENCRYPTION_ALG
            // Older clients (<= v0.9.0) specified Aes256Gcm12 as encryption algorithm, however
            // they were actually using Aes256Gcm. To be forward compatible, we also accept the the
            // correctly specified algorithm.
                || pending_record.enc_alg == EncryptionAlgorithm::Aes256Gcm,
            "unsupported encryption algorithm: {:?}",
            pending_record.enc_alg
        );
        let nonce: [u8; 12] = pending_record
            .nonce
            .try_into()
            .map_err(|_| anyhow!("invalid nonce length"))?;
        let key = AttachmentEarKey::from_bytes(
            pending_record
                .enc_key
                .try_into()
                .map_err(|_| anyhow!("invalid key length"))?,
        );
        ensure!(
            pending_record.hash_alg == AIR_ATTACHMENT_HASH_ALG,
            "unsupported hash algorithm: {:?}",
            pending_record.hash_alg
        );

        // TODO: Retries and marking as failed

        // Get the download URL from DS
        let api_client = self.api_client()?;
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
        let response = self
            .http_client()
            .get(download_url)
            .send()
            .await?
            .error_for_status()?;
        let total_len = pending_record
            .size
            .try_into()
            .context("Attachment size overflow")?;
        let mut bytes = Vec::with_capacity(total_len);
        let mut bytes_stream = response.bytes_stream();
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
        let hash = Sha256::digest(&content.bytes);
        ensure!(hash.as_slice() == pending_record.hash, "hash mismatch");

        // Store the attachment and mark it as downloaded
        //
        // When catching up with many messages, it might happen that the database is locked for a
        // longer time. In this case, we retry the commit every second until it succeeds. Since this
        // operation is in memory, we can retry many times. It will be cleaned up in case the app
        // closed. We just need to make sure that we don't run too many downloads at the same time,
        // otherwise we might run out of memory.
        //
        // TODO: Refactor and use a crate or an abstraction for this.
        const ATTACHMENT_COMMIT_RETRY_DELAY: Duration = Duration::from_secs(1);
        const ATTACHMENT_COMMIT_MAX_RETRIES: u32 = 30;
        let bytes = content.bytes.as_slice();
        let mut retries = 0u32;
        loop {
            let res = self
                .with_transaction_and_notifier(async |txn, notifier| {
                    AttachmentRecord::set_content(txn.as_mut(), notifier, attachment_id, bytes)
                        .await?;
                    PendingAttachmentRecord::delete(txn.as_mut(), attachment_id).await?;
                    Ok(())
                })
                .await;

            match res {
                Ok(()) => {
                    progress_tx.finish();
                    break;
                }
                Err(error) => {
                    const DB_LOCKED_CODE: &str = "5"; // SQLITE_BUSY
                    let is_db_locked = error
                        .downcast_ref::<sqlx::Error>()
                        .and_then(|e| e.as_database_error())
                        .is_some_and(|e| e.code().as_deref() == Some(DB_LOCKED_CODE));
                    if is_db_locked {
                        retries += 1;
                        if retries >= ATTACHMENT_COMMIT_MAX_RETRIES {
                            return Err(error);
                        }
                        warn!(
                            ?attachment_id,
                            retries,
                            "Database is locked; retrying in {ATTACHMENT_COMMIT_RETRY_DELAY:?}"
                        );
                    } else {
                        return Err(error);
                    }
                }
            }

            tokio::time::sleep(ATTACHMENT_COMMIT_RETRY_DELAY).await;
        }

        Ok(())
    }
}
