// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Client database migrations

use aircoreclient::MigrationProgress;

use crate::{StreamSink, migration_progress};

/// State of the client database migrations that run while a user's database is
/// opened.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DbMigrationState {
    /// Nothing to migrate, or the pending migrations are done.
    Idle,
    /// Pending migrations are being applied.
    Running,
}

impl From<MigrationProgress> for DbMigrationState {
    fn from(progress: MigrationProgress) -> Self {
        match progress {
            MigrationProgress::Idle => Self::Idle,
            MigrationProgress::Running => Self::Running,
        }
    }
}

/// Streams the state of the client database migrations.
///
/// The current state is emitted immediately, so a subscriber that arrives after
/// the migrations started still learns that they are running. The stream never
/// completes on its own; drop the Dart subscription to end it.
pub async fn create_db_migration_stream(sink: StreamSink<DbMigrationState>) {
    let mut rx = migration_progress::subscribe();
    if sink.add((*rx.borrow_and_update()).into()).is_err() {
        return;
    }
    while rx.changed().await.is_ok() {
        let state: DbMigrationState = (*rx.borrow_and_update()).into();
        if sink.add(state).is_err() {
            break; // sink is closed
        }
    }
}
