// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Latches client DB migration progress for the Dart side.
//!
//! Migrations run while opening the client DB, that is, before there is a
//! `User` to hang a cubit stream off. So the state lives in a global `watch`
//! channel. Being a latch rather than a stream of events, it also frees Dart
//! from having to subscribe before the load starts: a subscriber that arrives
//! mid-run still learns that migrations are running.
//!
//! Processes without a Dart side (the notification service extension, the
//! share extension) report into a channel with no receivers, which is free.

use std::sync::LazyLock;

use aircoreclient::{MigrationObserver, MigrationProgress};
use tokio::sync::watch;

static PROGRESS: LazyLock<watch::Sender<MigrationProgress>> =
    LazyLock::new(|| watch::channel(MigrationProgress::Idle).0);

/// The observer to hand to `CoreUser::load_with_progress`.
pub(crate) fn observer() -> MigrationObserver {
    MigrationObserver::new(|progress| {
        PROGRESS.send_replace(progress);
    })
}

pub(crate) fn subscribe() -> watch::Receiver<MigrationProgress> {
    PROGRESS.subscribe()
}
