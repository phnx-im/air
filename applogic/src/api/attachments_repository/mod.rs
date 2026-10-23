// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

mod thumbnail_queue;

use std::{fs, io::Write, sync::Arc};

use aircoreclient::{
    AttachmentContent, AttachmentId, AttachmentProgress, AttachmentProgressEvent, AttachmentStatus,
    AttachmentThumbnail,
    clients::CoreUser,
    db::notification::{DbEntityId, DbNotification, DbOperation},
};
use anyhow::{Context, bail};
use dashmap::{DashMap, Entry};
use flutter_rust_bridge::frb;
use futures_util::{Stream, StreamExt};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tokio_util::sync::{CancellationToken, DropGuard};
use tracing::{debug, error, info, instrument};

use crate::{StreamSink, api::user_cubit::UserCubitBase, util::spawn_from_sync};

use thumbnail_queue::ThumbnailQueue;

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
        spawn_from_sync(cancel.clone().run_until_cancelled_owned(thumbnail_worker));

        spawn_attachment_downloads(core_user.clone(), cancel.clone(), in_progress.clone());

        Self {
            core_user,
            in_progress,
            thumbnail_queue,
            cancel: cancel.clone(),
            _cancel: cancel.drop_guard(),
        }
    }

    /// Reports the attachment's status and follows its upload or download.
    pub async fn status_stream(
        &self,
        attachment_id: AttachmentId,
        sink: StreamSink<UiAttachmentStatus>,
    ) {
        let follow = follow_status(
            self.core_user.clone(),
            self.in_progress.clone(),
            attachment_id,
            move |status| sink.add(status).is_ok(),
        );
        tokio::spawn(self.cancel.clone().run_until_cancelled_owned(follow));
    }

    /// Returns whether the attachment is animated.
    ///
    /// The data is loaded from the attachment metadata. If attachment is not yet available, it is
    /// downloaded.
    #[instrument(level = "debug", skip(self))]
    pub async fn is_attachment_animated(
        &self,
        attachment_id: AttachmentId,
        retry_download_if_failed: bool,
    ) -> anyhow::Result<Option<bool>> {
        if let Some(value) = self.core_user.attachment_is_animated(attachment_id).await? {
            return Ok(Some(value));
        }

        // Fallback triggers download if needed
        if !self
            .ensure_attachment_content(attachment_id, retry_download_if_failed)
            .await?
        {
            return Ok(None);
        }

        // Download just completed => read the value again
        self.core_user.attachment_is_animated(attachment_id).await
    }

    /// Load attachment's data from database
    #[instrument(level = "debug", skip(self))]
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

    #[instrument(level = "debug", skip(self))]
    pub async fn load_image_attachment(
        &self,
        attachment_id: AttachmentId,
        retry_download_if_failed: bool,
    ) -> anyhow::Result<Vec<u8>> {
        self.attachment_bytes(attachment_id, retry_download_if_failed)
            .await?
            .context("Attachment not found")
    }

    /// Load thumbnail from the database.
    ///
    /// If the thumbnail is not yet generated, it is regenerated, and then the data is returned.
    #[instrument(level = "debug", skip(self))]
    pub async fn load_thumbnail(
        &self,
        attachment_id: AttachmentId,
        retry_download_if_failed: bool,
    ) -> anyhow::Result<Option<Vec<u8>>> {
        match self.core_user.attachment_thumbnail(attachment_id).await? {
            // Hot path: no original bytes are touched
            Some(AttachmentThumbnail::Ready { bytes }) => Ok(Some(bytes)),
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

    /// Starts downloading the attachment unless it is on the device or already
    /// downloading. Retries failed or interrupted downloads. Returns once the
    /// download is registered, without waiting for it to finish. A status
    /// stream subscribed after that follows the download.
    #[instrument(level = "debug", skip(self))]
    pub async fn download_attachment(&self, attachment_id: AttachmentId) -> anyhow::Result<()> {
        match self.core_user.attachment_status(attachment_id).await? {
            Some(
                AttachmentStatus::Pending
                | AttachmentStatus::Downloading
                | AttachmentStatus::DownloadFailed,
            ) => {
                spawn_download_task(
                    &self.core_user,
                    &self.cancel,
                    None,
                    &self.in_progress,
                    attachment_id,
                );
                Ok(())
            }
            Some(
                AttachmentStatus::Ready
                | AttachmentStatus::Uploading
                | AttachmentStatus::UploadFailed
                | AttachmentStatus::NotFound,
            )
            | None => Ok(()),
            Some(AttachmentStatus::Unknown) => bail!("Attachment status is unknown"),
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
fn loaded(thumbnail: AttachmentThumbnail, original: Vec<u8>) -> Vec<u8> {
    match thumbnail {
        AttachmentThumbnail::Ready { bytes } => bytes,
        // Never animated: an animated source always stores a first frame.
        AttachmentThumbnail::OriginalFits => original,
        // The `image` crate could not decode it, so let Flutter try.
        AttachmentThumbnail::Failed => original,
    }
}

/// Reports the attachment's status to `emit` until it can no longer change.
///
/// `emit` returns `false` once nobody listens.
async fn follow_status(
    core_user: CoreUser,
    in_progress: InProgressMap,
    attachment_id: AttachmentId,
    mut emit: impl FnMut(UiAttachmentStatus) -> bool,
) {
    let mut notifications = core_user.db_notifications();
    loop {
        let status = core_user
            .attachment_status(attachment_id)
            .await
            .inspect_err(
                |error| error!(%error, ?attachment_id, "failed to load attachment status"),
            );
        let handle = in_progress
            .get(&attachment_id)
            .as_deref()
            .cloned()
            .filter(|handle| !handle.is_cancelled());
        if let Some(handle) = handle {
            follow_progress(&handle, &mut emit).await;
            return;
        }

        // When there is no task in progress, we report the persisted status
        // directly.
        let ui_status = match &status {
            Ok(Some(AttachmentStatus::Ready)) => UiAttachmentStatus::Completed,
            Ok(Some(AttachmentStatus::NotFound)) => UiAttachmentStatus::NotFound,
            Ok(Some(AttachmentStatus::Pending)) => UiAttachmentStatus::Pending,
            // The upload may run in the share extension, which has no task
            // here.
            Ok(Some(AttachmentStatus::Uploading)) => UiAttachmentStatus::Progress(0),
            // A download without a task was interrupted or cancelled.
            Ok(Some(AttachmentStatus::Downloading)) => UiAttachmentStatus::Failed,
            // UploadFailed / DownloadFailed / Unknown / missing row.
            _ => UiAttachmentStatus::Failed,
        };
        if !emit(ui_status) {
            return;
        }

        let Ok(Some(AttachmentStatus::Pending | AttachmentStatus::Downloading)) = status else {
            return;
        };
        if !attachment_changed(&mut notifications, attachment_id).await {
            return;
        }
    }
}

async fn follow_progress(
    handle: &AttachmentTaskHandle,
    emit: &mut impl FnMut(UiAttachmentStatus) -> bool,
) {
    let mut stream = handle.progress.stream();
    // Note: this stream will always emit at least one event.
    while let Some(event) = stream.next().await {
        match event {
            AttachmentProgressEvent::Init => {
                if !emit(UiAttachmentStatus::Progress(0)) {
                    break; // sink is closed
                }
            }
            AttachmentProgressEvent::Progress {
                bytes_total: _,
                bytes_loaded,
            } => {
                if !emit(UiAttachmentStatus::Progress(bytes_loaded)) {
                    break; // sink is closed
                }
            }
            AttachmentProgressEvent::Completed => {
                emit(UiAttachmentStatus::Completed);
                break;
            }
            AttachmentProgressEvent::Failed => {
                emit(UiAttachmentStatus::Failed);
                break;
            }
            AttachmentProgressEvent::NotFound => {
                emit(UiAttachmentStatus::NotFound);
                break;
            }
        }
    }
}

/// Waits for a DB notification about the attachment. Returns `false` once the
/// notifications end.
async fn attachment_changed(
    notifications: &mut (impl Stream<Item = Arc<DbNotification>> + Unpin),
    attachment_id: AttachmentId,
) -> bool {
    let entity_id = DbEntityId::Attachment(attachment_id);
    while let Some(notification) = notifications.next().await {
        if notification.ops.contains_key(&entity_id) {
            return true;
        }
    }
    false
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

    if let Err(error) = store.reset_interrupted_attachment_downloads().await {
        error!(%error, "failed to reset interrupted attachment downloads");
    }

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

#[derive(Debug, PartialEq)]
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

#[cfg(test)]
mod tests {
    use std::{assert_matches, time::Duration};

    use airserver_test_harness::utils::setup::TestBackend;
    use tokio::{
        sync::mpsc::{UnboundedReceiver, unbounded_channel},
        task::JoinHandle,
        time::timeout,
    };

    use super::*;

    /// Bob with one pending attachment from Alice.
    async fn pending_attachment() -> (TestBackend, CoreUser, AttachmentId) {
        let mut setup = TestBackend::single().await;
        let [alice, bob] = Box::pin(setup.add_users()).await;
        let chat_id = setup.connect_users(&alice, &bob).await;
        setup
            .send_attachment(chat_id, &alice, &[&bob], &[0x00, 0x01, 0x02], "test.bin")
            .await
            .unwrap();
        let [attachment_id] = bob.pending_attachments().await.unwrap()[..] else {
            panic!("expected exactly one pending attachment");
        };
        (setup, bob, attachment_id)
    }

    fn subscribe(
        core_user: &CoreUser,
        in_progress: &InProgressMap,
        attachment_id: AttachmentId,
    ) -> (UnboundedReceiver<UiAttachmentStatus>, JoinHandle<()>) {
        let (tx, rx) = unbounded_channel();
        let follow = follow_status(
            core_user.clone(),
            in_progress.clone(),
            attachment_id,
            move |status| tx.send(status).is_ok(),
        );
        (rx, tokio::spawn(follow))
    }

    async fn next_status(
        statuses: &mut UnboundedReceiver<UiAttachmentStatus>,
    ) -> Option<UiAttachmentStatus> {
        timeout(Duration::from_secs(10), statuses.recv())
            .await
            .expect("status stream stalled")
    }

    /// Collects the statuses until the stream ends, which it has to do on its
    /// own.
    async fn remaining_statuses(
        statuses: &mut UnboundedReceiver<UiAttachmentStatus>,
    ) -> Vec<UiAttachmentStatus> {
        let mut remaining = Vec::new();
        while let Some(status) = next_status(statuses).await {
            remaining.push(status);
        }
        remaining
    }

    fn assert_download_completed(statuses: &[UiAttachmentStatus]) {
        let [progress @ .., last] = statuses else {
            panic!("no status after the download started");
        };
        assert_eq!(*last, UiAttachmentStatus::Completed);
        for status in progress {
            assert_matches!(status, UiAttachmentStatus::Progress(_));
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn status_follows_download_started_after_subscribing() {
        let (_setup, bob, attachment_id) = pending_attachment().await;
        let in_progress = InProgressMap::default();

        let (mut statuses, _follow) = subscribe(&bob, &in_progress, attachment_id);
        assert_eq!(
            next_status(&mut statuses).await,
            Some(UiAttachmentStatus::Pending)
        );

        // The download loop starts it once a permit frees up
        let cancel = CancellationToken::new();
        spawn_download_task(&bob, &cancel, None, &in_progress, attachment_id);
        assert_download_completed(&remaining_statuses(&mut statuses).await);

        let (mut statuses, _follow) = subscribe(&bob, &InProgressMap::default(), attachment_id);
        assert_eq!(
            remaining_statuses(&mut statuses).await,
            [UiAttachmentStatus::Completed]
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn status_follows_recovered_download() {
        let (_setup, bob, attachment_id) = pending_attachment().await;
        // A process killed during the download leaves the attachment like this
        bob.set_attachment_status(attachment_id, AttachmentStatus::Downloading)
            .await
            .unwrap();
        let in_progress = InProgressMap::default();

        let (mut statuses, _follow) = subscribe(&bob, &in_progress, attachment_id);
        assert_eq!(
            next_status(&mut statuses).await,
            Some(UiAttachmentStatus::Failed)
        );

        bob.reset_interrupted_attachment_downloads().await.unwrap();
        assert_eq!(
            next_status(&mut statuses).await,
            Some(UiAttachmentStatus::Pending)
        );

        let cancel = CancellationToken::new();
        spawn_download_task(&bob, &cancel, None, &in_progress, attachment_id);
        assert_download_completed(&remaining_statuses(&mut statuses).await);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn waiting_status_ends_without_listener() {
        let (_setup, bob, attachment_id) = pending_attachment().await;

        let (mut statuses, follow) = subscribe(&bob, &InProgressMap::default(), attachment_id);
        assert_eq!(
            next_status(&mut statuses).await,
            Some(UiAttachmentStatus::Pending)
        );
        drop(statuses);

        bob.set_attachment_status(attachment_id, AttachmentStatus::Pending)
            .await
            .unwrap();
        timeout(Duration::from_secs(10), follow)
            .await
            .expect("status stream kept waiting")
            .unwrap();
    }
}
