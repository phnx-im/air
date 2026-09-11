// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Progress reporting for the client DB migrations.
//!
//! Migrations run before there is a [`CoreUser`] to hang a stream off, so the
//! observer is passed down from the caller that opens the DB.
//!
//! [`CoreUser`]: crate::clients::CoreUser

use std::{fmt, sync::Arc};

/// State of the client DB migrations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MigrationProgress {
    /// Nothing to migrate, or the pending migrations are done.
    #[default]
    Idle,
    /// Pending migrations are being applied.
    Running,
}

/// Receives [`MigrationProgress`] reports while a client DB is opened.
///
/// Reports are made synchronously from the task running the migrations, so the
/// callback must not block. Cloning is cheap. The default observer discards
/// every report, which is what every caller without a UI to update uses.
#[derive(Clone, Default)]
pub struct MigrationObserver(Option<Arc<dyn Fn(MigrationProgress) + Send + Sync>>);

impl MigrationObserver {
    pub fn new(report: impl Fn(MigrationProgress) + Send + Sync + 'static) -> Self {
        Self(Some(Arc::new(report)))
    }

    /// Reports [`MigrationProgress::Running`] and returns a guard that reports
    /// [`MigrationProgress::Idle`] when dropped.
    ///
    /// The guard covers the runner's many `?` exits: a migration that fails or
    /// is cancelled must not leave the UI stuck on a progress indicator.
    pub(crate) fn start(&self) -> MigrationRunGuard {
        self.report(MigrationProgress::Running);
        MigrationRunGuard(self.clone())
    }

    fn report(&self, progress: MigrationProgress) {
        if let Some(report) = &self.0 {
            report(progress);
        }
    }
}

impl fmt::Debug for MigrationObserver {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let state = if self.0.is_some() {
            "installed"
        } else {
            "noop"
        };
        write!(f, "MigrationObserver({state})")
    }
}

pub(crate) struct MigrationRunGuard(MigrationObserver);

impl Drop for MigrationRunGuard {
    fn drop(&mut self) {
        self.0.report(MigrationProgress::Idle);
    }
}
