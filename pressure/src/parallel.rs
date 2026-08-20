// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Bounded, multi-threaded fan-out.
//!
//! Fleet members are independent of each other: separate databases, separate
//! file locks, separate queues. The phases that touch each member on its own
//! (creating them, registering usernames, draining queues) are bound by
//! crypto and round trips rather than by anything shared, so they are spawned
//! onto the runtime's worker threads to keep every core busy.
//!
//! Two rules constrain what may be fanned out:
//!
//! - Never two concurrent operations on the *same* member. A drain advances
//!   that member's QS queue ratchet, so overlapping drains would corrupt it.
//! - Never concurrent commits to the same group. Only one commit per epoch
//!   wins; the rest are rejected, which is noise rather than pressure.
//!
//! `concurrency` bounds how many run at once, because every member shares one
//! source IP and the server rate-limits per IP.

use std::{future::Future, sync::Arc};

use tokio::{sync::Semaphore, task::JoinSet};

/// The default fan-out width: one in flight per core, which keeps the
/// runtime's workers busy without burying the server in requests from a
/// single IP.
pub fn default_concurrency() -> usize {
    std::thread::available_parallelism()
        .map(|value| value.get())
        .unwrap_or(4)
}

/// Runs `f` over every item on the runtime's worker threads, at most
/// `concurrency` at a time, returning the results in input order.
///
/// A panicking task is resumed on this thread rather than swallowed, so a bug
/// in the harness surfaces as a panic instead of a missing result.
pub async fn map<T, R, F, Fut>(items: Vec<T>, concurrency: usize, f: F) -> Vec<R>
where
    T: Send + 'static,
    R: Send + 'static,
    F: Fn(T) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = R> + Send + 'static,
{
    let f = Arc::new(f);
    let semaphore = Arc::new(Semaphore::new(concurrency.max(1)));
    let mut set = JoinSet::new();

    for (index, item) in items.into_iter().enumerate() {
        let f = f.clone();
        let semaphore = semaphore.clone();
        set.spawn(async move {
            let _permit = semaphore
                .acquire_owned()
                .await
                .expect("semaphore is never closed");
            (index, f(item).await)
        });
    }

    let mut results = Vec::with_capacity(set.len());
    while let Some(joined) = set.join_next().await {
        match joined {
            Ok(entry) => results.push(entry),
            Err(error) => match error.try_into_panic() {
                Ok(panic) => std::panic::resume_unwind(panic),
                Err(error) => panic!("fan-out task was cancelled: {error}"),
            },
        }
    }

    results.sort_by_key(|(index, _)| *index);
    results.into_iter().map(|(_, result)| result).collect()
}
