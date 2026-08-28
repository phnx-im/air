// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

use tokio::sync::watch;
use tokio_stream::{Stream, wrappers::WatchStream};

/// Attachment upload or download progress tracker
#[derive(Debug, Clone)]
pub struct AttachmentProgress {
    rx: watch::Receiver<AttachmentProgressEvent>,
    total_bytes: Arc<AtomicU64>,
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
        let total_bytes = Arc::new(AtomicU64::new(0));
        (
            AttachmentProgressSender {
                tx: Some(tx),
                total_bytes: total_bytes.clone(),
            },
            Self { rx, total_bytes },
        )
    }

    /// Total number of bytes to transfer, if known yet (uploads).
    pub fn total_bytes(&self) -> Option<u64> {
        match self.total_bytes.load(Ordering::Relaxed) {
            0 => None,
            total => Some(total),
        }
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
    total_bytes: Arc<AtomicU64>,
}

impl AttachmentProgressSender {
    pub(super) fn set_total_bytes(&self, total_bytes: u64) {
        self.total_bytes.store(total_bytes, Ordering::Relaxed);
    }

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
