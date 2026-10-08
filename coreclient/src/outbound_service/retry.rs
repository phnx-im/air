// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Retry budgets for items that are kept after a recoverable error.

use chrono::TimeDelta;

use crate::outbound_service::error::OutboundServiceError;

/// How often an item is retried after recoverable errors, and how long to wait
/// in between.
#[derive(Debug, Clone, Copy)]
pub(crate) struct RetryPolicy {
    /// The attempt that reaches this number gives up.
    pub(crate) max_attempts: u32,
    /// The wait after the first attempt. It doubles with every further one.
    pub(crate) base: TimeDelta,
    pub(crate) max: TimeDelta,
}

/// What to do with an item after a recoverable error.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum RetryDecision {
    /// Spend an attempt and defer the next one.
    Backoff { attempts: u32, retry_in: TimeDelta },
    /// Spend the last attempt and give up.
    GiveUp,
}

impl RetryPolicy {
    pub(crate) const PROFILE_FETCHES: Self = Self {
        max_attempts: 7,
        base: TimeDelta::seconds(5),
        max: TimeDelta::hours(24),
    };

    pub(crate) const MESSAGES: Self = Self {
        max_attempts: 4,
        base: TimeDelta::seconds(5),
        max: TimeDelta::minutes(1),
    };

    pub(crate) const REACTIONS_AND_RECEIPTS: Self = Self {
        max_attempts: 5,
        base: TimeDelta::seconds(30),
        max: TimeDelta::minutes(10),
    };

    pub(crate) const RESYNC: Self = Self {
        max_attempts: 5,
        base: TimeDelta::minutes(1),
        max: TimeDelta::hours(1),
    };

    pub(crate) fn decide(&self, attempts: u32) -> RetryDecision {
        let attempts = attempts + 1;
        if attempts >= self.max_attempts {
            RetryDecision::GiveUp
        } else {
            RetryDecision::Backoff {
                attempts,
                retry_in: self.backoff(attempts),
            }
        }
    }

    /// Turns a recoverable error into a fatal one once it would spend the last
    /// attempt, so the item is dropped like after any other fatal error.
    pub(crate) fn fatal_when_exhausted<T>(
        &self,
        result: Result<T, OutboundServiceError>,
        attempts: u32,
    ) -> Result<T, OutboundServiceError> {
        match result {
            Err(OutboundServiceError::Recoverable(error))
                if self.decide(attempts) == RetryDecision::GiveUp =>
            {
                Err(OutboundServiceError::Fatal(
                    anyhow::Error::from(error)
                        .context(format!("giving up after {} attempts", self.max_attempts)),
                ))
            }
            result => result,
        }
    }

    fn backoff(&self, attempts: u32) -> TimeDelta {
        let factor = 1i32 << attempts.saturating_sub(1).min(16);
        (self.base * factor).min(self.max)
    }
}

#[cfg(test)]
mod tests {
    use std::assert_matches;

    use anyhow::anyhow;

    use crate::job::recoverable::Recoverable;

    use super::*;

    #[test]
    fn error_spends_an_attempt_with_backoff() {
        let policy = RetryPolicy::RESYNC;
        assert_eq!(
            policy.decide(0),
            RetryDecision::Backoff {
                attempts: 1,
                retry_in: TimeDelta::minutes(1),
            }
        );
        assert_eq!(
            policy.decide(3),
            RetryDecision::Backoff {
                attempts: 4,
                retry_in: TimeDelta::minutes(8),
            }
        );
    }

    #[test]
    fn backoff_is_capped() {
        let policy = RetryPolicy {
            max_attempts: 10,
            ..RetryPolicy::MESSAGES
        };
        assert_eq!(
            policy.decide(8),
            RetryDecision::Backoff {
                attempts: 9,
                retry_in: policy.max,
            }
        );
    }

    #[test]
    fn last_error_gives_up() {
        let policy = RetryPolicy::MESSAGES;
        assert_eq!(
            policy.decide(policy.max_attempts - 1),
            RetryDecision::GiveUp
        );
    }

    #[test]
    fn exhausted_budget_turns_recoverable_errors_fatal() {
        let policy = RetryPolicy::MESSAGES;
        let wrong_epoch = || {
            Err::<(), _>(OutboundServiceError::Recoverable(Recoverable::wrong_epoch(
                anyhow!("epoch"),
            )))
        };

        assert_matches!(
            policy.fatal_when_exhausted(wrong_epoch(), policy.max_attempts - 2),
            Err(OutboundServiceError::Recoverable(_))
        );
        assert_matches!(
            policy.fatal_when_exhausted(wrong_epoch(), policy.max_attempts - 1),
            Err(OutboundServiceError::Fatal(_))
        );
    }
}
