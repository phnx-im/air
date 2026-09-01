// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::sync::Arc;

use aircoreclient::{AttachmentContent, AttachmentId, AttachmentThumbnail, clients::CoreUser};
use flutter_rust_bridge::frb;
use indexmap::IndexMap;
use parking_lot::Mutex;
use tokio::sync::{mpsc, oneshot};
use tracing::error;

use super::loaded;

type ThumbnailWaiters = Vec<oneshot::Sender<Vec<u8>>>;

/// Pending thumbnail generations, served newest first.
pub(super) struct ThumbnailQueue {
    pending: Arc<Mutex<IndexMap<AttachmentId, ThumbnailWaiters>>>,
    wake: mpsc::UnboundedSender<()>,
}

impl ThumbnailQueue {
    pub(super) fn new(core_user: CoreUser) -> (Self, impl Future<Output = ()>) {
        let (wake_tx, wake) = mpsc::unbounded_channel();
        let pending = Arc::new(Mutex::new(IndexMap::new()));
        let worker = ThumbnailWorker {
            core_user,
            pending: pending.clone(),
            wake,
        };
        let queue = Self {
            pending,
            wake: wake_tx,
        };
        (queue, worker.run())
    }

    pub(super) fn request(
        &self,
        attachment_id: AttachmentId,
    ) -> impl Future<Output = Option<Vec<u8>>> {
        let (tx, rx) = oneshot::channel();
        {
            let mut pending = self.pending.lock();
            let mut waiters = pending.shift_remove(&attachment_id).unwrap_or_default();
            waiters.push(tx);
            pending.insert(attachment_id, waiters);
        }
        let _ = self.wake.send(());
        async move { rx.await.ok() }
    }
}

#[frb(ignore)]
struct ThumbnailWorker {
    core_user: CoreUser,
    pending: Arc<Mutex<IndexMap<AttachmentId, ThumbnailWaiters>>>,
    wake: mpsc::UnboundedReceiver<()>,
}

impl ThumbnailWorker {
    async fn run(mut self) {
        while self.wake.recv().await.is_some() {
            loop {
                // Don't hold the lock across await points
                let next = { self.pending.lock().pop() }; // newest first
                let Some((attachment_id, mut waiters)) = next else {
                    break;
                };
                match self.generate_thumbnail(attachment_id).await {
                    Ok(Some(loaded)) => {
                        // Only additional waiters pay the cost of cloning.
                        let last = waiters.pop();
                        for tx in waiters {
                            let _ = tx.send(loaded.clone());
                        }
                        if let Some(tx) = last {
                            let _ = tx.send(loaded);
                        }
                    }
                    // Senders drop => the waiters keep their blurhash
                    Ok(None) => {}
                    Err(error) => {
                        error!(
                            %error,
                            ?attachment_id, "Failed to generate attachment thumbnail"
                        );
                    }
                }
            }
        }
    }

    async fn generate_thumbnail(
        &self,
        attachment_id: AttachmentId,
    ) -> anyhow::Result<Option<Vec<u8>>> {
        match self.core_user.attachment_thumbnail(attachment_id).await? {
            Some(AttachmentThumbnail::Ready { bytes }) => Ok(Some(bytes)),
            existing => {
                let original = match self.core_user.load_attachment(attachment_id).await? {
                    AttachmentContent::Ready(bytes)
                    | AttachmentContent::Uploading(bytes)
                    | AttachmentContent::UploadFailed(bytes) => Arc::new(bytes),
                    // Content went away
                    _ => return Ok(None),
                };
                let thumbnail = match existing {
                    Some(thumbnail) => thumbnail,
                    None => {
                        self.core_user
                            .generate_attachment_thumbnail(attachment_id, original.clone())
                            .await?
                    }
                };
                let original = Arc::unwrap_or_clone(original);
                Ok(Some(loaded(thumbnail, original)))
            }
        }
    }
}
