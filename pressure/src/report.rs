// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Tallies operation outcomes and divergences found during a run. The walk
//! loop is a single sequential task, so this is a plain struct rather than
//! something behind a mutex.

use std::collections::BTreeMap;

use tracing::{debug, info, warn};

#[derive(Default)]
pub struct Report {
    outcomes: BTreeMap<&'static str, Outcome>,
    pub divergences: Vec<String>,
    /// Individual QS messages that a drain couldn't apply (see
    /// [`crate::ops::DrainOutcome::message_errors`]). Tracked separately from
    /// op failures since coreclient already handles these safely; a nonzero
    /// count is a sign of churn, not necessarily a bug.
    pub qs_message_errors: u64,
    pub qs_messages_fetched: u64,
    /// Members that were evicted, invited back, and then did (or did not)
    /// recover a local group state matching the hub's.
    pub rejoins_converged: u64,
    pub rejoins_diverged: u64,
    /// Commits the DS rejected because another commit reached the epoch
    /// first. Expected whenever steps run concurrently: only one commit per
    /// epoch can win, and coreclient parks the loser as a retryable job
    /// rather than failing it. Counted apart from real op failures.
    pub commit_races: u64,
}

/// Whether an operation failed only because it lost the race for an epoch.
///
/// Coreclient answers a `WrongEpoch` from the DS by marking the commit failed
/// and returning `JobError::Blocked`, which reaches us as an opaque
/// `anyhow::Error`. `JobError` is crate-private, so its message is the only
/// thing left to match on.
fn is_commit_race(error: &anyhow::Error) -> bool {
    error.chain().any(|cause| cause.to_string() == "Blocked")
}

#[derive(Default, Clone, Copy)]
struct Outcome {
    ok: u64,
    err: u64,
}

/// The walk operations broken out in [`Report::summary_line`], with the short
/// labels the progress bar uses, in display order. Anything recorded under a
/// name not listed here still shows up in [`Report::log_summary`].
const WALK_OPS: &[(&str, &str)] = &[
    ("send_message", "send"),
    ("invite_members", "inv"),
    ("remove_members", "rem"),
    ("leave_group", "leave"),
    ("self_update", "upd"),
    ("target_self_update", "tupd"),
    ("commit_pending_leave", "sweep"),
];

impl Report {
    pub fn record<T>(&mut self, op: &'static str, result: &anyhow::Result<T>) {
        match result {
            Ok(_) => self.outcomes.entry(op).or_default().ok += 1,
            Err(error) if is_commit_race(error) => {
                self.commit_races += 1;
                debug!(op, "commit lost the race for its epoch, will be retried");
            }
            Err(error) => {
                self.outcomes.entry(op).or_default().err += 1;
                warn!(op, %error, "operation failed");
            }
        }
    }

    /// Folds a concurrent step's tallies into this one.
    pub fn merge(&mut self, other: Report) {
        for (op, outcome) in other.outcomes {
            let entry = self.outcomes.entry(op).or_default();
            entry.ok += outcome.ok;
            entry.err += outcome.err;
        }
        self.divergences.extend(other.divergences);
        self.qs_message_errors += other.qs_message_errors;
        self.qs_messages_fetched += other.qs_messages_fetched;
        self.rejoins_converged += other.rejoins_converged;
        self.rejoins_diverged += other.rejoins_diverged;
        self.commit_races += other.commit_races;
    }

    pub fn record_divergence(&mut self, message: String) {
        warn!(%message, "divergence detected");
        self.divergences.push(message);
    }

    pub fn record_drain_outcome(&mut self, fetched: usize, message_errors: usize) {
        self.qs_messages_fetched += fetched as u64;
        self.qs_message_errors += message_errors as u64;
    }

    pub fn record_rejoin_outcome(&mut self, converged: bool) {
        if converged {
            self.rejoins_converged += 1;
        } else {
            self.rejoins_diverged += 1;
        }
    }

    /// Total ok/err counts across every op kind.
    pub fn totals(&self) -> (u64, u64) {
        self.outcomes.values().fold((0, 0), |(ok, err), outcome| {
            (ok + outcome.ok, err + outcome.err)
        })
    }

    /// One-line tally, meant for a progress bar's message rather than the log.
    ///
    /// Only the walk's own operations are broken out, in [`WALK_OPS`] order.
    /// Drains run several times per step and their volume already shows up in
    /// the QS counters, and the bootstrap ops are finished by the time this is
    /// displayed; both are left to [`Report::log_summary`].
    pub fn summary_line(&self) -> String {
        let mut parts = Vec::with_capacity(WALK_OPS.len());
        for (op, label) in WALK_OPS {
            let Some(outcome) = self.outcomes.get(op) else {
                continue;
            };
            if outcome.err > 0 {
                parts.push(format!("{label}={}!{}", outcome.ok, outcome.err));
            } else {
                parts.push(format!("{label}={}", outcome.ok));
            }
        }
        let (_, err) = self.totals();
        format!(
            "{} | err={err} races={} qs_errs={} rejoin={}/{} div={}",
            parts.join(" "),
            self.commit_races,
            self.qs_message_errors,
            self.rejoins_converged,
            self.rejoins_converged + self.rejoins_diverged,
            self.divergences.len()
        )
    }

    pub fn log_summary(&self) {
        for (op, outcome) in &self.outcomes {
            info!(op, ok = outcome.ok, err = outcome.err, "op tally");
        }
        info!(
            fetched = self.qs_messages_fetched,
            errors = self.qs_message_errors,
            "QS message tally"
        );
        info!(
            converged = self.rejoins_converged,
            diverged = self.rejoins_diverged,
            "rejoin tally"
        );
        info!(
            commit_races = self.commit_races,
            "commits that lost their epoch race"
        );
        info!(divergences = self.divergences.len(), "divergence tally");
    }
}
