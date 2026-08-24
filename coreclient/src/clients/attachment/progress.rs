// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use tokio::sync::watch;
use tokio_stream::{Stream, wrappers::WatchStream};

/// Attachment upload or download progress tracker
#[derive(Debug, Clone)]
pub struct AttachmentProgress {
    rx: watch::Receiver<AttachmentProgressEvent>,
    /// Total number of bytes to transfer, if known upfront (uploads).
    total_bytes: Option<u64>,
}

/// Attachment upload or download progress event
#[derive(Debug, Clone, Copy)]
pub enum AttachmentProgressEvent {
    Init,
    Progress { bytes_loaded: usize },
    Completed,
    Failed,
    NotFound,
}

impl AttachmentProgress {
    pub(crate) fn new() -> (AttachmentProgressSender, Self) {
        let (tx, rx) = watch::channel(AttachmentProgressEvent::Init);
        (
            AttachmentProgressSender { tx: Some(tx) },
            Self {
                rx,
                total_bytes: None,
            },
        )
    }

    pub(crate) fn with_total_bytes(mut self, total_bytes: u64) -> Self {
        self.total_bytes = Some(total_bytes);
        self
    }

    /// Total number of bytes to transfer, if known upfront (uploads).
    pub fn total_bytes(&self) -> Option<u64> {
        self.total_bytes
    }

    pub fn is_failed(&self) -> bool {
        matches!(*self.rx.borrow(), AttachmentProgressEvent::Failed)
    }

    pub fn stream(&self) -> impl Stream<Item = AttachmentProgressEvent> + Send + use<> {
        WatchStream::new(self.rx.clone())
    }
}

pub(crate) struct AttachmentProgressSender {
    tx: Option<watch::Sender<AttachmentProgressEvent>>,
}

impl AttachmentProgressSender {
    pub(super) fn report(&self, bytes_loaded: usize) {
        if let Some(tx) = &self.tx {
            let _ignore_closed = tx.send(AttachmentProgressEvent::Progress { bytes_loaded });
        }
    }

    pub(super) fn not_found(&mut self) {
        if let Some(tx) = self.tx.take() {
            let _ignore_closed = tx.send(AttachmentProgressEvent::NotFound);
        }
    }

    pub(super) fn completed(&mut self) {
        if let Some(tx) = self.tx.take() {
            let _ignore_closed = tx.send(AttachmentProgressEvent::Completed);
        }
    }

    pub(super) fn tx(&self) -> Option<watch::Sender<AttachmentProgressEvent>> {
        self.tx.clone()
    }
}

impl Drop for AttachmentProgressSender {
    fn drop(&mut self) {
        if let Some(tx) = self.tx.take() {
            let _ignore_closed = tx.send(AttachmentProgressEvent::Failed);
        }
    }
}
