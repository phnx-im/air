// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Data migrations implemented in Rust that cannot be expressed in SQL.

use sqlx::{SqlitePool, migrate::Migrate};
use tracing::error;

use crate::outbound_service::timed_tasks_queue::TimedTaskQueue;

const TIMED_TASKS_QUEUE_MIGRATION_VERSION: i64 = 20251104145255;

/// Migrate data in the database that cannot be expressed in SQL.
pub(crate) async fn migrate(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    let Some(migrations) = pool.acquire().await?.list_applied_migrations().await.ok() else {
        // The migrations might not yet exist
        return Ok(());
    };

    // Check for specific migrations and do post-processing here.
    let has_timed_tasks_queue_migration = migrations
        .iter()
        .any(|m| m.version == TIMED_TASKS_QUEUE_MIGRATION_VERSION);
    let now = chrono::Utc::now();
    let due_at = now - chrono::Duration::minutes(5);
    let mut connection = pool.acquire().await?;
    if !has_timed_tasks_queue_migration
        && let Err(error) = TimedTaskQueue::new_key_package_upload_task(due_at)
            .enqueue(connection.as_mut())
            .await
    {
        error!(?error, "Failed to enqueue initial key package upload task");
    }

    Ok(())
}
