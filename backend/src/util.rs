// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use tonic::{Code, Status};

/// Find the first error in the source chain that is of type `T`.
pub(crate) fn find_cause<T: std::error::Error + 'static>(
    error: &dyn std::error::Error,
) -> Option<&T> {
    let mut source = error.source();
    while let Some(error) = source {
        if let Some(typed) = error.downcast_ref() {
            return Some(typed);
        }
        source = error.source();
    }
    None
}

pub(crate) trait StatusExt {
    /// Returns true if the error is caused by the client closing the connection
    /// or resetting the stream.
    fn is_client_disconnect(&self) -> bool;
}

impl StatusExt for Status {
    fn is_client_disconnect(&self) -> bool {
        let Some(h2_error) = find_cause::<h2::Error>(self) else {
            return false;
        };
        let broken_pipe = matches!(self.code(), Code::Unknown)
            && h2_error
                .get_io()
                .is_some_and(|io_error| io_error.kind() == std::io::ErrorKind::BrokenPipe);
        let remote_reset = h2_error.is_reset() && h2_error.is_remote();
        broken_pipe || remote_reset
    }
}

/// Waits until a statement on the test database waits for a lock.
#[cfg(test)]
pub(crate) async fn wait_for_lock_waiter(pool: &sqlx::PgPool) -> anyhow::Result<()> {
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            let is_waiting: bool = sqlx::query_scalar(
                "SELECT EXISTS (
                    SELECT 1 FROM pg_stat_activity
                    WHERE datname = current_database() AND wait_event_type = 'Lock'
                )",
            )
            .fetch_one(pool)
            .await?;
            if is_waiting {
                return Ok(());
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await?
}
