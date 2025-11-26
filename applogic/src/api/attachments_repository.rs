// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::Arc,
};

use aircommon::identifiers::AttachmentId;
use aircoreclient::{
    AttachmentContent, AttachmentProgress, AttachmentProgressEvent, AttachmentStatus,
    clients::CoreUser,
    store::{Store, StoreEntityId, StoreOperation},
};
use anyhow::{Context, bail};
use dashmap::{DashMap, Entry};
use flutter_rust_bridge::{DartFnFuture, frb};
use tokio_stream::StreamExt;
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
    /// Upload or download tasks that are currently in progress
    in_progress: InProgressMap,
    _cancel: DropGuard,
}

impl AttachmentsRepository {
    #[frb(sync)]
    pub fn new(user_cubit: &UserCubitBase) -> Self {
        let store = user_cubit.core_user().clone();

        let cancel = CancellationToken::new();
        let in_progress = InProgressMap::default();
        spawn_attachment_downloads(store.clone(), in_progress.clone(), cancel.clone());

        Self {
            store,
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
            AttachmentContent::Pending => {
                debug!(?attachment_id, "Attachment is pending; spawn download task");
                let handle = spawn_download_task(
                    &self.store,
                    &self.in_progress,
                    &self.cancel,
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
            AttachmentContent::Failed | AttachmentContent::Unknown => {
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
        destination_dir: String,
        filename: String,
        attachment_id: AttachmentId,
        overwrite: bool,
    ) -> anyhow::Result<()> {
        let (AttachmentContent::Ready(data) | AttachmentContent::Uploading(data)) =
            self.store.load_attachment(attachment_id).await?
        else {
            bail!("Attachment is not present on the device")
        };

        let dir = Path::new(&destination_dir);
        if !dir.exists() {
            fs::create_dir_all(dir)?;
        } else if !dir.is_dir() {
            bail!("Destination is not a directory");
        }

        let filename = PathBuf::from(filename);
        let path = if overwrite {
            dir.join(filename)
        } else {
            unique_path(dir, &filename)
        };

        let mut file = fs::File::create(&path)
            .with_context(|| format!("Failed to create file at path: {}", path.display()))?;
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
    in_progress: InProgressMap,

    cancel: CancellationToken,
) {
    spawn_from_sync(attachment_downloads_loop(store, in_progress, cancel));
}

async fn attachment_downloads_loop(
    store: CoreUser,
    in_progress: InProgressMap,
    cancel: CancellationToken,
) {
    info!("Starting attachments download loop");

    let mut store_notifications = store.subscribe();
    loop {
        if cancel.is_cancelled() {
            return;
        }

        // download pending attachments
        match store.pending_attachments().await {
            Ok(pending_attachments) => {
                debug!(
                    ?pending_attachments,
                    "Spawn download for pending attachments"
                );
                for attachment_id in pending_attachments {
                    spawn_download_task(&store, &in_progress, &cancel, attachment_id);
                }
            }
            Err(error) => {
                error!(%error, "Failed to load pending attachments");
            }
        }

        // wait for the next store notification
        let notification = tokio::select! {
            _ = cancel.cancelled() => return,
            notification = store_notifications.next() => notification,
        };
        let Some(notification) = notification else {
            return;
        };

        debug!(?notification, "Received store notification");

        // download newly added attachments
        for (id, ops) in &notification.ops {
            match id {
                StoreEntityId::Attachment(attachment_id) if ops.contains(StoreOperation::Add) => {
                    debug!(?attachment_id, "Spawn download for added attachment");
                    spawn_download_task(&store, &in_progress, &cancel, *attachment_id);
                }
                _ => (),
            }
        }
    }
}

fn spawn_download_task(
    store: &CoreUser,
    in_progress: &InProgressMap,
    cancel: &CancellationToken,
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

fn unique_path(base_path: &Path, filename: &Path) -> PathBuf {
    let mut path = base_path.join(filename);

    if !path.exists() {
        return path;
    }

    let stem = filename.file_stem().unwrap_or_default();
    let ext = filename.extension();

    for counter in 1.. {
        let mut filename = stem.to_os_string();
        filename.push("-");
        filename.push(counter.to_string());
        if let Some(ext) = ext {
            filename.push(".");
            filename.push(ext);
        }

        path = base_path.join(filename);
        if !path.exists() {
            break;
        }
    }

    path
}
