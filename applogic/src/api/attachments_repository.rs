// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{fs, io::Write, sync::Arc};

use aircoreclient::{
    AttachmentContent, AttachmentId, AttachmentProgress, AttachmentProgressEvent, AttachmentStatus,
    AttachmentThumbnail,
    clients::CoreUser,
    db::notification::{DbEntityId, DbOperation},
    image_is_animated,
};
use anyhow::{Context, bail};
use dashmap::{DashMap, Entry};
use flutter_rust_bridge::frb;
use futures_util::StreamExt;
use indexmap::IndexMap;
use parking_lot::Mutex;
use tokio::sync::{OwnedSemaphorePermit, Semaphore, mpsc, oneshot};
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
    core_user: CoreUser,
    cancel: CancellationToken,
    /// Upload or download tasks that are currently in progress
    in_progress: InProgressMap,
    thumbnail_queue: ThumbnailQueue,
    _cancel: DropGuard,
}

impl AttachmentsRepository {
    #[frb(sync)]
    pub fn new(user_cubit: &UserCubitBase) -> Self {
        let core_user = user_cubit.core_user().clone();

        let cancel = CancellationToken::new();
        let in_progress = InProgressMap::default();

        let (thumbnail_queue, thumbnail_worker) = ThumbnailQueue::new(core_user.clone());
        spawn_from_sync(
            cancel
                .clone()
                .run_until_cancelled_owned(thumbnail_worker.run()),
        );

        spawn_attachment_downloads(core_user.clone(), cancel.clone(), in_progress.clone());

        Self {
            core_user,
            in_progress,
            thumbnail_queue,
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
                    AttachmentProgressEvent::NotFound => {
                        sink.add(UiAttachmentStatus::NotFound).ok();
                        break;
                    }
                }
            }
        } else {
            // No task in progress, so report the persisted status directly.
            let ui_status = match self.core_user.attachment_status(attachment_id).await {
                Ok(Some(AttachmentStatus::Ready)) => UiAttachmentStatus::Completed,
                Ok(Some(AttachmentStatus::NotFound)) => UiAttachmentStatus::NotFound,
                // Still in flight, a task will pick it up. No failure is
                // reported while it is uploading or downloading.
                Ok(Some(
                    AttachmentStatus::Uploading
                    | AttachmentStatus::Downloading
                    | AttachmentStatus::Pending,
                )) => UiAttachmentStatus::Progress(0),
                // UploadFailed / DownloadFailed / Unknown / missing row.
                _ => UiAttachmentStatus::Failed,
            };
            sink.add(ui_status).ok();
        }
    }

    /// Load attachment's data from database
    pub async fn load_attachment(
        &self,
        attachment_id: AttachmentId,
    ) -> anyhow::Result<Option<Vec<u8>>> {
        match self.core_user.load_attachment(attachment_id).await? {
            AttachmentContent::Ready(data)
            | AttachmentContent::Uploading(data)
            | AttachmentContent::UploadFailed(data) => Ok(Some(data)),
            _ => Ok(None),
        }
    }

    pub async fn load_image_attachment(
        &self,
        attachment_id: AttachmentId,
        retry_download_if_failed: bool,
    ) -> anyhow::Result<LoadedImageAttachment> {
        let bytes = self
            .attachment_bytes(attachment_id, retry_download_if_failed)
            .await?
            .context("Attachment not found")?;
        let is_animated = image_is_animated(&bytes);
        Ok(LoadedImageAttachment { bytes, is_animated })
    }

    pub async fn load_thumbnail(
        &self,
        attachment_id: AttachmentId,
        retry_download_if_failed: bool,
    ) -> anyhow::Result<Option<LoadedImageAttachment>> {
        match self.core_user.attachment_thumbnail(attachment_id).await? {
            // Hot path: no original bytes are touched
            Some(AttachmentThumbnail::Ready { bytes, is_animated }) => {
                Ok(Some(LoadedImageAttachment { bytes, is_animated }))
            }

            // Terminal outcomes: serve the original, never regenerate
            Some(thumbnail) => {
                let Some(original) = self.load_attachment(attachment_id).await? else {
                    return Ok(None);
                };
                Ok(Some(loaded(thumbnail, original)))
            }

            None => {
                // Trigger download if needed
                if !self
                    .ensure_attachment_content(attachment_id, retry_download_if_failed)
                    .await?
                {
                    return Ok(None);
                }
                Ok(self.thumbnail_queue.request(attachment_id).await)
            }
        }
    }

    async fn attachment_bytes(
        &self,
        attachment_id: AttachmentId,
        retry_download_if_failed: bool,
    ) -> anyhow::Result<Option<Vec<u8>>> {
        if !self
            .ensure_attachment_content(attachment_id, retry_download_if_failed)
            .await?
        {
            return Ok(None);
        }
        self.load_attachment(attachment_id).await
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
    ) -> anyhow::Result<bool> {
        debug!(?attachment_id, "Tracking attachment download");
        let mut events_stream = handle.progress.stream();
        while let Some(event) = events_stream.next().await {
            match event {
                AttachmentProgressEvent::Init | AttachmentProgressEvent::Progress { .. } => {}
                AttachmentProgressEvent::Completed => return Ok(true),
                AttachmentProgressEvent::Failed => bail!("Attachment download failed"),
                AttachmentProgressEvent::NotFound => return Ok(false),
            }
        }
        bail!("Attachment download aborted")
    }

    /// Ensures the content is on device, downloading it if needed.
    ///
    /// `false` means there is nothing to fetch: expired, or no such attachment.
    async fn ensure_attachment_content(
        &self,
        attachment_id: AttachmentId,
        retry_download_if_failed: bool,
    ) -> anyhow::Result<bool> {
        // Remove cancelled handles
        self.in_progress.retain(|_, handle| !handle.is_cancelled());

        let spawn_download = async move || {
            let handle = spawn_download_task(
                &self.core_user,
                &self.cancel,
                None,
                &self.in_progress,
                attachment_id,
            );
            self.track_attachment_download(attachment_id, handle).await
        };

        match self.core_user.attachment_status(attachment_id).await? {
            Some(
                AttachmentStatus::Ready
                | AttachmentStatus::Uploading
                | AttachmentStatus::UploadFailed,
            ) => Ok(true),
            Some(AttachmentStatus::Pending) => {
                debug!(?attachment_id, "Attachment is pending; spawn download task");
                spawn_download().await
            }
            Some(AttachmentStatus::DownloadFailed) if retry_download_if_failed => {
                debug!(
                    ?attachment_id,
                    "Attachment failed to download but a retry was requested; spawn download task"
                );
                spawn_download().await
            }
            // A download can be stuck in this state after a crash
            Some(AttachmentStatus::Downloading) if retry_download_if_failed => {
                debug!(
                    ?attachment_id,
                    "Retry requested while downloading; spawn or attach to a download task"
                );
                spawn_download().await
            }
            Some(AttachmentStatus::Downloading) => {
                match self.in_progress.get(&attachment_id).as_deref().cloned() {
                    Some(handle) => self.track_attachment_download(attachment_id, handle).await,
                    // No task tracking it => it may have landed meanwhile
                    None => match self.core_user.attachment_status(attachment_id).await? {
                        Some(AttachmentStatus::Ready) => Ok(true),
                        _ => bail!("Attachment download failed"),
                    },
                }
            }
            Some(AttachmentStatus::NotFound) | None => Ok(false),
            Some(AttachmentStatus::DownloadFailed | AttachmentStatus::Unknown) => {
                bail!("Attachment download failed")
            }
        }
    }

    pub(crate) fn in_progress(&self) -> &InProgressMap {
        &self.in_progress
    }
}

/// Maps an outcome to what the UI paints.
///
/// `original` is the fallback for the outcomes that are not a stored blob, and is unused for
/// `Ready`.
fn loaded(thumbnail: AttachmentThumbnail, original: Vec<u8>) -> LoadedImageAttachment {
    match thumbnail {
        AttachmentThumbnail::Ready { bytes, is_animated } => {
            LoadedImageAttachment { bytes, is_animated }
        }
        // Never animated: an animated source always stores a first frame.
        AttachmentThumbnail::OriginalFits => LoadedImageAttachment {
            bytes: original,
            is_animated: false,
        },
        // The `image` crate could not decode it, so let Flutter try.
        AttachmentThumbnail::Failed => LoadedImageAttachment {
            is_animated: image_is_animated(&original),
            bytes: original,
        },
    }
}

fn spawn_attachment_downloads(
    store: CoreUser,
    cancel: CancellationToken,
    in_progress: InProgressMap,
) {
    spawn_from_sync(
        cancel
            .clone()
            .run_until_cancelled_owned(attachment_downloads_loop(store, cancel, in_progress)),
    );
}

async fn attachment_downloads_loop(
    store: CoreUser,
    cancel: CancellationToken,
    in_progress: InProgressMap,
) {
    const NUM_CONCURRENT_DOWNLOADS: usize = 5;
    let download_tasks_semaphore = Arc::new(Semaphore::new(NUM_CONCURRENT_DOWNLOADS));

    // filter the store notifications stream to only care about attachments
    let store_notifications = store.db_notifications().flat_map(|notification| {
        let attachment_ids =
            notification
                .ops
                .clone()
                .into_iter()
                .filter_map(|(id, ops)| match id {
                    DbEntityId::Attachment(remote_attachment_id)
                        if ops.contains(DbOperation::Add) =>
                    {
                        Some(remote_attachment_id)
                    }
                    _ => None,
                });
        futures_util::stream::iter(attachment_ids)
    });

    // download pending attachments once
    let pending_attachment_ids = store
        .pending_attachments()
        .await
        .inspect_err(|error| error!(%error, "failed to load pending attachments"))
        .unwrap_or_default();

    let mut local_attachment_ids =
        tokio_stream::iter(pending_attachment_ids).chain(store_notifications);

    info!("starting attachments download task");
    while let Some(attachment_id) = local_attachment_ids.next().await {
        // Attachment `Add` notifications also fire for our own outgoing
        // attachments, stored with an `Uploading` or `Ready` status. A
        // download task for those finds no pending record and drops the
        // progress sender, which reports a spurious `Failed` to the UI.
        // Only pending (incoming) attachments are downloaded here.
        match store.attachment_status(attachment_id).await {
            Ok(Some(AttachmentStatus::Pending)) => {}
            Ok(_) => continue,
            Err(error) => {
                error!(%error, ?attachment_id, "failed to read attachment status; skipping");
                continue;
            }
        }
        let Ok(permit) = download_tasks_semaphore.clone().acquire_owned().await else {
            return;
        };
        spawn_download_task(&store, &cancel, Some(permit), &in_progress, attachment_id);
    }
}

fn spawn_download_task(
    store: &CoreUser,
    cancel: &CancellationToken,
    permit: Option<OwnedSemaphorePermit>,
    in_progress: &InProgressMap,
    attachment_id: AttachmentId,
) -> AttachmentTaskHandle {
    let (task, cancel, handle) = match in_progress.entry(attachment_id) {
        Entry::Occupied(mut entry) if entry.get().is_cancelled() || entry.get().is_failed() => {
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
        drop(permit);
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

    fn is_failed(&self) -> bool {
        self.progress.is_failed()
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
    /// Not found on the server
    NotFound,
}

/// Bytes of an image attachment (thumbnail or original) and an animation classification
#[derive(Clone)]
pub struct LoadedImageAttachment {
    pub bytes: Vec<u8>,
    pub is_animated: bool,
}

type ThumbnailWaiters = Vec<oneshot::Sender<LoadedImageAttachment>>;

/// Pending thumbnail generations, served newest first.
#[frb(ignore)]
struct ThumbnailQueue {
    pending: Arc<Mutex<IndexMap<AttachmentId, ThumbnailWaiters>>>,
    wake: mpsc::UnboundedSender<()>,
}

impl ThumbnailQueue {
    fn new(core_user: CoreUser) -> (Self, ThumbnailWorker) {
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
        (queue, worker)
    }

    fn request(
        &self,
        attachment_id: AttachmentId,
    ) -> impl Future<Output = Option<LoadedImageAttachment>> {
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
                        error!(%error, ?attachment_id, "Failed to generate attachment thumbnail");
                    }
                }
            }
        }
    }

    async fn generate_thumbnail(
        &self,
        attachment_id: AttachmentId,
    ) -> anyhow::Result<Option<LoadedImageAttachment>> {
        match self.core_user.attachment_thumbnail(attachment_id).await? {
            Some(AttachmentThumbnail::Ready { bytes, is_animated }) => {
                Ok(Some(LoadedImageAttachment { bytes, is_animated }))
            }
            existing => {
                let original = match self.core_user.load_attachment(attachment_id).await? {
                    AttachmentContent::Ready(bytes)
                    | AttachmentContent::Uploading(bytes)
                    | AttachmentContent::UploadFailed(bytes) => bytes,
                    // Content went away
                    _ => return Ok(None),
                };
                let thumbnail = match existing {
                    Some(thumbnail) => thumbnail,
                    None => {
                        self.core_user
                            .generate_attachment_thumbnail(attachment_id, &original)
                            .await?
                    }
                };
                Ok(Some(loaded(thumbnail, original)))
            }
        }
    }
}
