// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Retry budgets for items that are kept after a recoverable error.

use chrono::TimeDelta;

use crate::{job::recoverable::RecoverableCause, outbound_service::error::OutboundServiceError};

/// How often an item is retried after server errors, and how long to wait in
/// between.
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
    /// Leave the item untouched. The next run picks it up again.
    Retry,
    /// Spend an attempt and defer the next one.
    Backoff { attempts: u32, retry_in: TimeDelta },
    /// Spend the last attempt and give up.
    GiveUp,
}

impl RetryPolicy {
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

    /// Only server errors spend an attempt. The other causes wait for
    /// something else to catch up, the item itself is fine.
    pub(crate) fn decide(&self, cause: RecoverableCause, attempts: u32) -> RetryDecision {
        match cause {
            RecoverableCause::Server => (),
            RecoverableCause::WrongEpoch | RecoverableCause::Busy | RecoverableCause::Deferred => {
                return RetryDecision::Retry;
            }
        }
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
                if self.decide(error.cause, attempts) == RetryDecision::GiveUp =>
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
    fn only_server_errors_spend_an_attempt() {
        for cause in [
            RecoverableCause::WrongEpoch,
            RecoverableCause::Busy,
            RecoverableCause::Deferred,
        ] {
            assert_eq!(RetryPolicy::RESYNC.decide(cause, 3), RetryDecision::Retry);
        }
    }

    #[test]
    fn server_error_spends_an_attempt_with_backoff() {
        let policy = RetryPolicy::RESYNC;
        assert_eq!(
            policy.decide(RecoverableCause::Server, 0),
            RetryDecision::Backoff {
                attempts: 1,
                retry_in: TimeDelta::minutes(1),
            }
        );
        assert_eq!(
            policy.decide(RecoverableCause::Server, 3),
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
            policy.decide(RecoverableCause::Server, 8),
            RetryDecision::Backoff {
                attempts: 9,
                retry_in: policy.max,
            }
        );
    }

    #[test]
    fn last_server_error_gives_up() {
        let policy = RetryPolicy::MESSAGES;
        assert_eq!(
            policy.decide(RecoverableCause::Server, policy.max_attempts - 1),
            RetryDecision::GiveUp
        );
    }

    #[test]
    fn exhausted_budget_turns_server_errors_fatal() {
        let policy = RetryPolicy::MESSAGES;
        let server_error =
            || Err::<(), _>(OutboundServiceError::Recoverable(Recoverable::server(anyhow!("boom"))));

        assert_matches!(
            policy.fatal_when_exhausted(server_error(), policy.max_attempts - 2),
            Err(OutboundServiceError::Recoverable(_))
        );
        assert_matches!(
            policy.fatal_when_exhausted(server_error(), policy.max_attempts - 1),
            Err(OutboundServiceError::Fatal(_))
        );

        let wrong_epoch = Err::<(), _>(OutboundServiceError::Recoverable(
            Recoverable::wrong_epoch(anyhow!("epoch")),
        ));
        assert_matches!(
            policy.fatal_when_exhausted(wrong_epoch, policy.max_attempts * 10),
            Err(OutboundServiceError::Recoverable(_))
        );
    }
}
