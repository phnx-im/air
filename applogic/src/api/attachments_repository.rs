// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{fs, io::Write, sync::Arc};

use aircommon::identifiers::AttachmentId;
use aircoreclient::{
    AttachmentContent, AttachmentProgress, AttachmentProgressEvent, AttachmentStatus,
    clients::CoreUser,
    store::{Store, StoreEntityId, StoreOperation},
};
use anyhow::{Context, bail};
use dashmap::{DashMap, Entry};
use flutter_rust_bridge::{DartFnFuture, frb};
use futures_util::StreamExt;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tokio_util::sync::{CancellationToken, DropGuard};
use tracing::{debug, error, info};

use crate::{StreamSink, api::user_cubit::UserCubitBase, util::spawn_from_sync};

pub(crate) type InProgressMap = Arc<DashMap<AttachmentId, AttachmentTaskHandle>>;

/// Repository managing attachments
///
/// * Listens to store notifications and spawns download tasks for attachments that are added or
/// pending.
/// * Provides access for loading attachments.
#[frb(opaque)]
pub struct AttachmentsRepository {
    store: CoreUser,
    cancel: CancellationToken,
    /// Limit the amount of concurrent tasks
    download_tasks_semaphore: Arc<Semaphore>,
    /// Upload or download tasks that are currently in progress
    in_progress: InProgressMap,
    _cancel: DropGuard,
}

impl AttachmentsRepository {
    #[frb(sync)]
    pub fn new(user_cubit: &UserCubitBase) -> Self {
        let store = user_cubit.core_user().clone();

        let cancel = CancellationToken::new();
        let download_tasks_semaphore = Arc::new(Semaphore::new(5));
        let in_progress = InProgressMap::default();

        spawn_attachment_downloads(
            store.clone(),
            cancel.clone(),
            download_tasks_semaphore.clone(),
            in_progress.clone(),
        );

        Self {
            store,
            download_tasks_semaphore,
            in_progress,
            cancel: cancel.clone(),
            _cancel: cancel.drop_guard(),
        }
    }

    pub async fn status_stream(
        &self,
        attachment_id: AttachmentId,
        sink: StreamSink<UiAttachmentStatus>,
    ) {
        let handle = self
            .in_progress
            .get(&attachment_id)
            .as_deref()
            .cloned()
            .filter(|handle| !handle.is_cancelled());
        if let Some(handle) = handle {
            let mut stream = handle.progress.stream();
            // Note: this stream will always emit at least one event.
            while let Some(event) = stream.next().await {
                match event {
                    AttachmentProgressEvent::Init => {
                        if sink.add(UiAttachmentStatus::Progress(0)).is_err() {
                            break; // sink is closed
                        }
                    }
                    AttachmentProgressEvent::Progress { bytes_loaded } => {
                        if sink
                            .add(UiAttachmentStatus::Progress(bytes_loaded))
                            .is_err()
                        {
                            break; // sink is closed
                        }
                    }
                    AttachmentProgressEvent::Completed => {
                        sink.add(UiAttachmentStatus::Completed).ok();
                        break;
                    }
                    AttachmentProgressEvent::Failed => {
                        sink.add(UiAttachmentStatus::Failed).ok();
                        break;
                    }
                }
            }
        } else if let Ok(Some(AttachmentStatus::Ready)) =
            self.store.attachment_status(attachment_id).await
        {
            sink.add(UiAttachmentStatus::Completed).ok();
        } else {
            sink.add(UiAttachmentStatus::Failed).ok();
        }
    }

    /// Load attachment's data from database
    pub async fn load_attachment(
        &self,
        attachment_id: AttachmentId,
    ) -> anyhow::Result<Option<Vec<u8>>> {
        match self.store.load_attachment(attachment_id).await? {
            AttachmentContent::Ready(data)
            | AttachmentContent::Uploading(data)
            | AttachmentContent::UploadFailed(data) => Ok(Some(data)),
            _ => Ok(None),
        }
    }

    pub async fn load_image_attachment(
        &self,
        attachment_id: AttachmentId,
        chunk_event_callback: impl Fn(u64) -> DartFnFuture<()> + Send + 'static,
    ) -> anyhow::Result<Vec<u8>> {
        // Remove cancelled handles
        self.in_progress.retain(|_, handle| !handle.is_cancelled());

        match self.store.load_attachment(attachment_id).await? {
            AttachmentContent::Ready(bytes) => Ok(bytes),
            AttachmentContent::Uploading(bytes) => Ok(bytes),
            AttachmentContent::UploadFailed(bytes) => Ok(bytes),
            AttachmentContent::Pending => {
                debug!(?attachment_id, "Attachment is pending; spawn download task");
                let semaphore_permit = self
                    .download_tasks_semaphore
                    .clone()
                    .acquire_owned()
                    .await?;
                let handle = spawn_download_task(
                    &self.store,
                    &self.cancel,
                    semaphore_permit,
                    &self.in_progress,
                    attachment_id,
                );
                self.track_attachment_download(attachment_id, handle, chunk_event_callback)
                    .await
            }
            AttachmentContent::Downloading => {
                let handle = self.in_progress.get(&attachment_id).as_deref().cloned();
                if let Some(handle) = handle {
                    self.track_attachment_download(attachment_id, handle, chunk_event_callback)
                        .await
                } else {
                    match self.store.load_attachment(attachment_id).await? {
                        AttachmentContent::Ready(bytes) => Ok(bytes),
                        _ => bail!("Attachment download failed"),
                    }
                }
            }
            AttachmentContent::None => bail!("Attachment not found"),
            AttachmentContent::DownloadFailed | AttachmentContent::Unknown => {
                bail!("Attachment download failed")
            }
        }
    }

    pub fn cancel(&self, attachment_id: AttachmentId) {
        if let Some((_, handle)) = self.in_progress.remove(&attachment_id) {
            handle.cancel.cancel();
        }
    }

    pub async fn save_attachment(
        &self,
        attachment_id: AttachmentId,
        path: String,
    ) -> anyhow::Result<()> {
        let data = self
            .load_attachment(attachment_id)
            .await?
            .context("Attachment is not present on the device")?;
        let mut file = fs::File::create(&path)
            .with_context(|| format!("Failed to create file at path: {path}"))?;
        file.write_all(&data)?;
        Ok(())
    }

    async fn track_attachment_download(
        &self,
        attachment_id: AttachmentId,
        handle: AttachmentTaskHandle,
        chunk_event_callback: impl Fn(u64) -> DartFnFuture<()> + Send + 'static,
    ) -> anyhow::Result<Vec<u8>> {
        debug!(?attachment_id, "Tracking attachment download");
        let mut events_stream = handle.progress.stream();
        while let Some(event) = events_stream.next().await {
            match event {
                AttachmentProgressEvent::Init => {
                    chunk_event_callback(0).await;
                }
                AttachmentProgressEvent::Progress { bytes_loaded } => {
                    chunk_event_callback(bytes_loaded.try_into()?).await;
                }
                AttachmentProgressEvent::Completed => {
                    return self
                        .store
                        .load_attachment(attachment_id)
                        .await?
                        .into_bytes()
                        .context("Attachment download failed");
                }
                AttachmentProgressEvent::Failed => bail!("Attachment download failed"),
            }
        }
        bail!("Attachment download aborted")
    }

    pub(crate) fn in_progress(&self) -> &InProgressMap {
        &self.in_progress
    }
}

fn spawn_attachment_downloads(
    store: CoreUser,
    cancel: CancellationToken,
    download_tasks_semaphore: Arc<Semaphore>,
    in_progress: InProgressMap,
) {
    spawn_from_sync(attachment_downloads_loop(
        store,
        cancel,
        download_tasks_semaphore,
        in_progress,
    ));
}

async fn attachment_downloads_loop(
    store: CoreUser,
    cancel: CancellationToken,
    download_tasks_semaphore: Arc<Semaphore>,
    in_progress: InProgressMap,
) {
    // download pending attachments once
    match store.pending_attachments().await {
        Ok(pending_attachments) => {
            debug!(
                ?pending_attachments,
                "Spawn download for pending attachments"
            );
            for attachment_id in pending_attachments {
                let Ok(semaphore_permit) = download_tasks_semaphore.clone().acquire_owned().await
                else {
                    error!("failed to acquire attachment download task semaphore permit");
                    return;
                };
                spawn_download_task(
                    &store,
                    &cancel,
                    semaphore_permit,
                    &in_progress,
                    attachment_id,
                );
            }
        }
        Err(error) => {
            error!(%error, "Failed to load pending attachments");
        }
    }

    // filter the store notifications stream to only care about attachments
    let mut store_notifications = store.subscribe().flat_map(|notification| {
        let attachment_ids =
            notification
                .ops
                .clone()
                .into_iter()
                .filter_map(|(id, ops)| match id {
                    StoreEntityId::Attachment(attachment_id)
                        if ops.contains(StoreOperation::Add) =>
                    {
                        Some(attachment_id)
                    }
                    _ => None,
                });
        futures_util::stream::iter(attachment_ids)
    });

    let attachments_download_task = async {
        info!("starting attachments download task");
        loop {
            // wait for the next relevant notification
            let Some(attachment_id) = store_notifications.next().await else {
                return;
            };
            let Ok(semaphore_permit) = download_tasks_semaphore.clone().acquire_owned().await
            else {
                return;
            };
            spawn_download_task(
                &store,
                &cancel,
                semaphore_permit,
                &in_progress,
                attachment_id,
            );
        }
    };

    tokio::select! {
        _ = cancel.cancelled() => return,
        _ = attachments_download_task => return,
    };
}

fn spawn_download_task(
    store: &CoreUser,
    cancel: &CancellationToken,
    semaphore_permit: OwnedSemaphorePermit,
    in_progress: &InProgressMap,
    attachment_id: AttachmentId,
) -> AttachmentTaskHandle {
    let (task, cancel, handle) = match in_progress.entry(attachment_id) {
        Entry::Occupied(mut entry) if entry.get().is_cancelled() => {
            let (progress, task) = store.download_attachment(attachment_id);
            let cancel = cancel.child_token();
            let handle = AttachmentTaskHandle::with_cancellation(progress, cancel.clone());
            entry.insert(handle.clone());
            (task, cancel, handle)
        }
        Entry::Occupied(entry) => {
            return entry.get().clone();
        }
        Entry::Vacant(entry) => {
            let (progress, task) = store.download_attachment(attachment_id);
            let cancel = cancel.child_token();
            let handle = AttachmentTaskHandle::with_cancellation(progress, cancel.clone());
            entry.insert(handle.clone());
            (task, cancel, handle)
        }
    };

    tokio::spawn(cancel.run_until_cancelled_owned(async move {
        if let Err(error) = task.await {
            error!(%error, "Failed to download attachment");
        }
        drop(semaphore_permit);
    }));

    handle
}

/// A handle to a download or upload attachment task
#[derive(Debug, Clone)]
pub(crate) struct AttachmentTaskHandle {
    progress: AttachmentProgress,
    cancel: CancellationToken,
    _drop_guard: Arc<DropGuard>,
}

impl AttachmentTaskHandle {
    pub(crate) fn new(progress: AttachmentProgress) -> Self {
        Self::with_cancellation(progress, CancellationToken::new())
    }

    pub(crate) fn with_cancellation(
        progress: AttachmentProgress,
        cancel: CancellationToken,
    ) -> Self {
        let drop_guard = Arc::new(cancel.clone().drop_guard());
        Self {
            progress,
            cancel,
            _drop_guard: drop_guard,
        }
    }

    pub(crate) fn cancellation_token(&self) -> &CancellationToken {
        &self.cancel
    }

    fn is_cancelled(&self) -> bool {
        self.cancel.is_cancelled()
    }
}

pub enum UiAttachmentStatus {
    /// Not in progress
    Pending,
    /// Uploading or downloading
    Progress(usize),
    /// Done uploading or downloading
    Completed,
    /// Failed to upload or download
    Failed,
}
