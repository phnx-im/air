// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use tokio::sync::watch;
use tokio_stream::{Stream, wrappers::WatchStream};

/// Attachment upload or download progress tracker
#[derive(Debug, Clone)]
pub struct AttachmentProgress {
    rx: watch::Receiver<AttachmentProgressEvent>,
}

/// Attachment upload or download progress event
#[derive(Debug, Clone, Copy)]
pub enum AttachmentProgressEvent {
    Init,
    Progress { bytes_loaded: usize },
    Completed,
    Failed,
}

impl AttachmentProgress {
    pub(super) fn new() -> (AttachmentProgressSender, Self) {
        let (tx, rx) = watch::channel(AttachmentProgressEvent::Init);
        (AttachmentProgressSender { tx: Some(tx) }, Self { rx })
    }

    pub fn stream(&self) -> impl Stream<Item = AttachmentProgressEvent> + Send + use<> {
        WatchStream::new(self.rx.clone())
    }
}

pub(super) struct AttachmentProgressSender {
    tx: Option<watch::Sender<AttachmentProgressEvent>>,
}

impl AttachmentProgressSender {
    pub(super) fn report(&self, bytes_loaded: usize) {
        if let Some(tx) = &self.tx {
            let _ignore_closed = tx.send(AttachmentProgressEvent::Progress { bytes_loaded });
        }
    }

    pub(super) fn finish(&mut self) {
        if let Some(tx) = self.tx.take() {
            let _ignore_closed = tx.send(AttachmentProgressEvent::Completed);
        }
    }
}

impl Drop for AttachmentProgressSender {
    fn drop(&mut self) {
        if let Some(tx) = self.tx.take() {
            let _ignore_closed = tx.send(AttachmentProgressEvent::Failed);
        }
    }
}
