// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Database access and observability

pub mod access;
pub mod notification;
mod persistence;

/// Whether the database failed due to lock contention.
pub(crate) fn is_db_busy(error: &sqlx::Error) -> bool {
    const SQLITE_BUSY: i32 = 5;
    const SQLITE_LOCKED: i32 = 6;
    match error {
        sqlx::Error::PoolTimedOut => true,
        sqlx::Error::Database(error) => error
            .code()
            .and_then(|code| code.parse::<i32>().ok())
            // Extended result codes keep the primary code in the low byte
            .is_some_and(|code| matches!(code & 0xff, SQLITE_BUSY | SQLITE_LOCKED)),
        _ => false,
    }
}
