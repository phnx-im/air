// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Data migrations implemented in Rust that cannot be expressed in SQL.

use sqlx::{SqlitePool, migrate::Migrate};

/// Migrate data in the database that cannot be expressed in SQL.
pub(crate) async fn migrate(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    let Some(_migrations) = pool.acquire().await?.list_applied_migrations().await.ok() else {
        // The migrations might not yet exist
        return Ok(());
    };

    // Check for specific migrations and do post-processing here.

    Ok(())
}
