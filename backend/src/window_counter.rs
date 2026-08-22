// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Rolling-window counters over tables of timestamped rows.
//!
//! A counter carries a list of independent windows and reaching any one of them
//! is what the caller acts on. Rows are counted while they are younger than a
//! window and pruned once they age out of the longest one.

use chrono::Duration;

use crate::settings::RegistrationThreshold;

/// Whether any window of a counter reached its limit. The counts arrive in the
/// order the thresholds are configured.
pub(crate) fn reached(thresholds: &[RegistrationThreshold], counts: &[u64]) -> bool {
    thresholds
        .iter()
        .zip(counts)
        .any(|(threshold, count)| *count >= threshold.limit)
}

/// How long a counter's rows stay relevant to it. A counter without windows
/// keeps nothing.
pub(crate) fn longest_window(thresholds: &[RegistrationThreshold]) -> Duration {
    thresholds
        .iter()
        .map(|threshold| threshold.window)
        .max()
        .unwrap_or_else(Duration::zero)
}

pub(crate) fn window_seconds(thresholds: &[RegistrationThreshold]) -> Vec<i64> {
    thresholds
        .iter()
        .map(|threshold| threshold.window.num_seconds())
        .collect()
}

pub(crate) fn saturating_count(count: i64) -> u64 {
    count.try_into().unwrap_or(u64::MAX)
}

/// The lock object id a bucket serializes on, taken from its hash. Callers pair
/// this with a class of their own, since the two-int form of
/// `pg_advisory_xact_lock` shares one namespace across the database.
pub(crate) fn bucket_lock_key(bucket: &[u8; 32]) -> i32 {
    i32::from_be_bytes([bucket[0], bucket[1], bucket[2], bucket[3]])
}

#[cfg(test)]
mod test {
    use super::*;

    fn threshold(limit: u64, window: Duration) -> RegistrationThreshold {
        RegistrationThreshold { limit, window }
    }

    #[test]
    fn any_window_reaching_its_limit_is_enough() {
        let thresholds = [
            threshold(10, Duration::hours(1)),
            threshold(100, Duration::days(1)),
        ];

        assert!(!reached(&thresholds, &[9, 99]));
        assert!(reached(&thresholds, &[10, 0]));
        assert!(reached(&thresholds, &[0, 100]));
    }

    #[test]
    fn a_counter_without_windows_is_never_reached() {
        assert!(!reached(&[], &[]));
        assert_eq!(longest_window(&[]), Duration::zero());
    }

    /// The counts come back positionally.
    #[test]
    fn windows_keep_their_order() {
        let thresholds = [
            threshold(1, Duration::days(30)),
            threshold(2, Duration::hours(1)),
        ];

        assert_eq!(window_seconds(&thresholds), [2_592_000, 3_600]);
        assert_eq!(longest_window(&thresholds), Duration::days(30));
    }
}
