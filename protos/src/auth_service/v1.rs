// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later
#![expect(clippy::doc_lazy_continuation)]

use chrono::{DateTime, Datelike, Days, Months, Utc};
use strum::VariantArray;

tonic::include_proto!("auth_service.v1");

include!(concat!(env!("OUT_DIR"), "/server/auth_service.v1.rs"));

const SECONDS_PER_DAY: i64 = 24 * 60 * 60;

impl OperationType {
    pub fn max_tokens_allowance(&self) -> u16 {
        match self {
            OperationType::Unspecified => 0,
            OperationType::AddUsername => 10,
            OperationType::GetInviteCode => 1,
        }
    }

    pub fn low_tokens_threshold(&self) -> u16 {
        match self {
            OperationType::Unspecified => 0,
            OperationType::AddUsername => 5,
            OperationType::GetInviteCode => 1,
        }
    }

    pub fn valid_until_starting_at(&self, at: DateTime<Utc>) -> DateTime<Utc> {
        match self {
            OperationType::Unspecified => at,
            OperationType::AddUsername => at + Months::new(1),
            OperationType::GetInviteCode => at + Days::new(1),
        }
    }

    /// Calendar bucket a token batch requested at `at` belongs to.
    ///
    /// Months since 1970-01 for `AddUsername`, days since 1970-01-01 for
    /// `GetInviteCode`, both derived from UTC. Server and client compute this
    /// independently, which is why it lives here rather than in either of
    /// them.
    pub fn allowance_epoch_at(&self, at: DateTime<Utc>) -> u32 {
        let bucket = match self {
            OperationType::Unspecified => 0,
            OperationType::AddUsername => i64::from(at.year() - 1970) * 12 + i64::from(at.month0()),
            OperationType::GetInviteCode => at.timestamp().div_euclid(SECONDS_PER_DAY),
        };
        u32::try_from(bucket).unwrap_or(0)
    }

    pub fn all() -> impl Iterator<Item = OperationType> {
        Self::VARIANTS
            .iter()
            .filter_map(|v| (*v != Self::Unspecified).then_some(*v))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(rfc3339: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(rfc3339).unwrap().to_utc()
    }

    #[test]
    fn allowance_epoch_buckets() {
        let start = at("1970-01-15T23:59:59Z");
        assert_eq!(OperationType::AddUsername.allowance_epoch_at(start), 0);
        assert_eq!(OperationType::GetInviteCode.allowance_epoch_at(start), 14);

        let later = at("2026-08-04T12:00:00Z");
        assert_eq!(OperationType::AddUsername.allowance_epoch_at(later), 679);
        assert_eq!(
            OperationType::GetInviteCode.allowance_epoch_at(later),
            20_669
        );

        // Non-UTC input is bucketed by its UTC calendar day, not its local one.
        let just_before_utc_midnight = at("2026-08-04T23:30:00-01:00");
        assert_eq!(
            OperationType::GetInviteCode.allowance_epoch_at(just_before_utc_midnight),
            20_670
        );

        assert_eq!(OperationType::Unspecified.allowance_epoch_at(later), 0);
    }
}
