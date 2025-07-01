// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use anyhow::{Context, anyhow, ensure};
use phnxcommon::{
    crypto::ear::{AeadCiphertext, EarDecryptable, keys::AttachmentEarKey},
    identifiers::AttachmentId,
};
use sha2::{Digest, Sha256};
use tokio::sync::watch;
use tokio_stream::{Stream, StreamExt, wrappers::WatchStream};
use tracing::{debug, info};

use crate::{
    clients::{
        CoreUser,
        attachment::{
            AttachmentBytes, AttachmentRecord,
            ear::{EncryptedAttachment, PHNX_ATTACHMENT_ENCRYPTION_ALG, PHNX_ATTACHMENT_HASH_ALG},
            persistence::{AttachmentStatus, PendingAttachmentRecord},
        },
    },
    groups::Group,
};

impl CoreUser {
    pub(crate) fn download_attachment(
        &self,
        attachment_id: AttachmentId,
    ) -> (
        DownloadProgress,
        impl Future<Output = anyhow::Result<()>> + use<>,
    ) {
        let (progress_tx, progress) = DownloadProgress::new();
        let fut = self
            .clone()
            .download_attachment_impl(attachment_id, progress_tx);
        (progress, fut)
    }

    async fn download_attachment_impl(
        self,
        attachment_id: AttachmentId,
        mut progress_tx: DownloadProgressSender,
    ) -> anyhow::Result<()> {
        info!(?attachment_id, "downloading attachment");
        progress_tx.report(0);

        // Load the pending attachment record and update the status to `Downloading`.
        let Some((pending_record, group)) = self
            .with_transaction(async |txn| {
                let Some(pending_record) =
                    PendingAttachmentRecord::load_pending(txn.as_mut(), attachment_id).await?
                else {
                    debug!(
                        ?attachment_id,
                        "Skipping downloading non-pending attachment"
                    );
                    return Ok(None);
                };
                let record = AttachmentRecord::load(txn.as_mut(), attachment_id)
                    .await?
                    .context("attachment record not found")?;
                let conversation_id = record.conversation_id;
                let group = Group::load_with_conversation_id_clean(txn, conversation_id)
                    .await?
                    .context("group not found")?;

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
            pending_record.enc_alg == PHNX_ATTACHMENT_ENCRYPTION_ALG,
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
            pending_record.hash_alg == PHNX_ATTACHMENT_HASH_ALG,
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
            let percent = (total_len * 100 / bytes.len()) as u8;
            progress_tx.report(percent);
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
        self.with_transaction_and_notifier(async move |txn, notifier| {
            AttachmentRecord::set_content(txn.as_mut(), notifier, attachment_id, &content.bytes)
                .await?;
            PendingAttachmentRecord::delete(txn.as_mut(), attachment_id).await?;
            Ok(())
        })
        .await?;

        progress_tx.finish();

        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct DownloadProgress {
    rx: watch::Receiver<DownloadProgressEvent>,
}

impl DownloadProgress {
    fn new() -> (DownloadProgressSender, Self) {
        let (tx, rx) = watch::channel(DownloadProgressEvent::Init);
        (DownloadProgressSender { tx: Some(tx) }, Self { rx })
    }

    pub async fn wait_for_completion(&mut self) -> DownloadProgressEvent {
        let _ = self
            .rx
            .wait_for(|value| {
                matches!(
                    value,
                    DownloadProgressEvent::Completed | DownloadProgressEvent::Failed
                )
            })
            .await;
        self.value()
    }

    pub fn stream(&self) -> impl Stream<Item = DownloadProgressEvent> + Send + use<> {
        WatchStream::new(self.rx.clone())
    }

    pub fn value(&mut self) -> DownloadProgressEvent {
        self.rx.borrow_and_update().clone()
    }
}

#[derive(Debug, Clone)]
pub enum DownloadProgressEvent {
    Init,
    Progress { percent: u8 },
    Completed,
    Failed,
}

struct DownloadProgressSender {
    tx: Option<watch::Sender<DownloadProgressEvent>>,
}

impl DownloadProgressSender {
    fn report(&mut self, percent: u8) {
        if let Some(tx) = &mut self.tx {
            let _ignore_closed = tx.send(DownloadProgressEvent::Progress { percent });
        }
    }

    fn finish(&mut self) {
        if let Some(tx) = self.tx.take() {
            let _ignore_closed = tx.send(DownloadProgressEvent::Completed);
        }
    }
}

impl Drop for DownloadProgressSender {
    fn drop(&mut self) {
        if let Some(tx) = self.tx.take() {
            let _ignore_closed = tx.send(DownloadProgressEvent::Failed);
        }
    }
}
