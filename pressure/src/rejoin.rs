// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Follows members through eviction and back, checking that a member invited
//! again after being removed (or after leaving) recovers a working local
//! group state.
//!
//! This is the gap the sampled check in [`crate::verify`] leaves open: it
//! skips anyone the hub no longer lists, so a member that is invited back but
//! never recovers would go unreported. OpenMLS marks an evicted group as
//! unusable (`MlsGroupStateError::UseAfterEviction`) and refuses to process
//! further messages against it, so whether a fresh Welcome for the same group
//! id can displace that state is exactly what this verifies.

use std::collections::{HashMap, HashSet};

use aircommon::identifiers::UserId;
use aircoreclient::{ChatId, clients::CoreUser};
use tracing::{debug, info};

use crate::{report::Report, verify};

#[derive(Default)]
pub struct RejoinTracker {
    /// Members evicted at some point, so their next invite is a rejoin
    /// rather than a first join.
    evicted: HashSet<UserId>,
    /// Rejoined members awaiting a convergence check.
    pending: HashMap<UserId, Pending>,
}

struct Pending {
    invited_at_step: usize,
    hub_epoch_at_invite: u64,
    checks: usize,
}

impl RejoinTracker {
    /// Marks members that hold local group state for `chat_id` but are not
    /// current hub participants as previously evicted. Without this a resumed
    /// run would treat a member evicted by an earlier run as a first-time
    /// joiner and skip the check that matters most for it -- it is the one
    /// carrying stale local state.
    pub async fn seed_evicted(
        &mut self,
        chat_id: ChatId,
        clients_by_id: &HashMap<UserId, &CoreUser>,
        hub_participants: &HashSet<UserId>,
    ) {
        for (user_id, user) in clients_by_id {
            if hub_participants.contains(user_id) {
                continue;
            }
            if matches!(user.group_epoch_and_own_index(chat_id).await, Ok(Some(_))) {
                self.evicted.insert(user_id.clone());
            }
        }
        if !self.evicted.is_empty() {
            info!(
                count = self.evicted.len(),
                "seeded members evicted by an earlier run"
            );
        }
    }

    pub fn record_eviction<'a>(&mut self, members: impl IntoIterator<Item = &'a UserId>) {
        for member in members {
            self.evicted.insert(member.clone());
            // Evicted again, so it is no longer expected to converge from the
            // earlier rejoin.
            self.pending.remove(member);
        }
    }

    /// Registers members as pending a convergence check, ignoring any that
    /// were never evicted (those are first joins, already covered by the
    /// sampled check).
    pub fn record_rejoin<'a>(
        &mut self,
        members: impl IntoIterator<Item = &'a UserId>,
        step: usize,
        hub_epoch: u64,
    ) {
        for member in members {
            if !self.evicted.contains(member) {
                continue;
            }
            info!(?member, step, hub_epoch, "member rejoined, awaiting convergence");
            self.pending.insert(
                member.clone(),
                Pending {
                    invited_at_step: step,
                    hub_epoch_at_invite: hub_epoch,
                    checks: 0,
                },
            );
        }
    }

    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }
}

/// Checks every pending rejoin, resolving each one that has converged. A
/// member that still diverges is kept for a later pass until `grace_checks`
/// is used up, at which point the divergence is reported. `last_pass` forces
/// that verdict immediately, for the final check at the end of a run.
pub async fn check_pending(
    tracker: &mut RejoinTracker,
    hub: &CoreUser,
    chat_id: ChatId,
    clients_by_id: &HashMap<UserId, &CoreUser>,
    grace_checks: usize,
    last_pass: bool,
    report: &mut Report,
) {
    if tracker.pending.is_empty() {
        return;
    }

    let Some(hub_participants) = hub.chat_participants(chat_id).await else {
        report.record_divergence("hub has no view of its own chat during rejoin check".to_owned());
        return;
    };

    for member_id in tracker.pending.keys().cloned().collect::<Vec<_>>() {
        let Some(member) = clients_by_id.get(&member_id).copied() else {
            tracker.pending.remove(&member_id);
            continue;
        };

        // A later eviction cancels the pending check, so a member still
        // pending that the hub does not list means the rejoin commit itself
        // did not take effect.
        if !hub_participants.contains(&member_id) {
            let pending = tracker.pending.remove(&member_id).expect("key came from pending");
            report.record_divergence(format!(
                "{member_id:?} rejoined at step {} (hub epoch {}) but the hub does not list it \
                 as a participant",
                pending.invited_at_step, pending.hub_epoch_at_invite
            ));
            report.record_rejoin_outcome(false);
            continue;
        }

        let detail = match verify::check_member(hub, chat_id, member, &member_id).await {
            Ok(None) => {
                let pending = tracker.pending.remove(&member_id).expect("key came from pending");
                info!(
                    ?member_id,
                    invited_at_step = pending.invited_at_step,
                    checks = pending.checks + 1,
                    "rejoined member converged"
                );
                report.record_rejoin_outcome(true);
                continue;
            }
            Ok(Some(divergence)) => divergence,
            Err(error) => format!("convergence check failed: {error}"),
        };

        let pending = tracker.pending.get_mut(&member_id).expect("key came from pending");
        pending.checks += 1;
        if !last_pass && pending.checks <= grace_checks {
            debug!(?member_id, checks = pending.checks, %detail, "rejoined member not converged yet");
            continue;
        }

        let pending = tracker.pending.remove(&member_id).expect("key came from pending");
        report.record_divergence(format!(
            "{member_id:?} rejoined at step {} (hub epoch {}) never converged after {} check(s): \
             {detail}",
            pending.invited_at_step, pending.hub_epoch_at_invite, pending.checks
        ));
        report.record_rejoin_outcome(false);
    }
}
