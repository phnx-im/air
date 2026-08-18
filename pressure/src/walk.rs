// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! One step of the random walk over the group the hub owns.
//!
//! A step picks an originator among the current members, brings it up to date,
//! and has it drive one operation against N targets. Every target is then
//! drained and, if the operation left it in the group, commits a key update of
//! its own. So a step leaves both sides of the operation converged and
//! rekeyed, rather than only the committer.
//!
//! The hub still owns the group and remains the roster oracle the verification
//! pass compares against, so it is drained at the top of every step and is
//! never a removal target.

use std::collections::{HashMap, HashSet};

use aircommon::identifiers::UserId;
use aircoreclient::{ChatId, clients::CoreUser};
use rand::{Rng, RngExt, seq::IteratorRandom};

use crate::{ops, rejoin::RejoinTracker, report::Report};

pub struct WalkContext<'a> {
    pub hub: &'a CoreUser,
    pub chat_id: ChatId,
    /// All non-hub members, indexable by user id.
    pub clients_by_id: &'a HashMap<UserId, &'a CoreUser>,
    /// Upper bound on how many targets a single operation may take.
    pub max_targets: usize,
}

/// Mutable state threaded through a step: the tallies, plus the tracker that
/// follows evicted members until they rejoin and converge.
pub struct WalkState {
    pub report: Report,
    pub rejoins: RejoinTracker,
    /// Number of steps taken so far, used to timestamp rejoins.
    pub step: usize,
    /// Members added and removed so far, which together size the next invite
    /// (see [`WalkState::invite_batch_size`]).
    members_added: usize,
    members_removed: usize,
}

impl WalkState {
    pub fn new(report: Report, rejoins: RejoinTracker) -> Self {
        Self {
            report,
            rejoins,
            step: 0,
            members_added: 0,
            members_removed: 0,
        }
    }

    /// How many members the next invite should add for the group to keep
    /// growing: enough to bring the running total of adds up to twice the
    /// removals, and never fewer than one so it still grows once already
    /// ahead. Removals keep regenerating the deficit, so over a run the adds
    /// stay at or above the 2:1 target.
    fn invite_batch_size(&self, max_targets: usize) -> usize {
        let deficit = (2 * self.members_removed).saturating_sub(self.members_added);
        deficit.clamp(1, max_targets.max(1))
    }
}

/// Relative frequency of each step kind. Invite batches are sized from the
/// running add/remove balance rather than by this weight, so these only set
/// how often each *kind* of pressure is applied.
const STEP_WEIGHTS: &[(StepKind, u32)] = &[
    (StepKind::SendMessage, 4),
    (StepKind::Invite, 3),
    (StepKind::Remove, 2),
    (StepKind::SelfUpdate, 2),
    (StepKind::Leave, 1),
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum StepKind {
    SendMessage,
    Invite,
    Remove,
    SelfUpdate,
    Leave,
}

fn pick_step(rng: &mut impl Rng) -> StepKind {
    let total: u32 = STEP_WEIGHTS.iter().map(|(_, weight)| weight).sum();
    let mut roll = rng.random_range(0..total);
    for (kind, weight) in STEP_WEIGHTS {
        if roll < *weight {
            return *kind;
        }
        roll -= weight;
    }
    StepKind::SendMessage
}

/// Records a drain's outcome: the op's ok/err tally, plus any per-message
/// processing errors as their own (non-failing) counter.
fn record_drain(report: &mut Report, op: &'static str, result: &anyhow::Result<ops::DrainOutcome>) {
    report.record(op, result);
    if let Ok(outcome) = result {
        report.record_drain_outcome(outcome.fetched, outcome.message_errors);
    }
}

pub async fn step(ctx: &WalkContext<'_>, rng: &mut impl Rng, state: &mut WalkState) {
    // The hub owns the group and is what verification compares against, so it
    // is brought up to date first whether or not it drives this step.
    let drain_result = ops::drain(ctx.hub).await;
    record_drain(&mut state.report, "drain_hub", &drain_result);
    if drain_result.is_err() {
        return;
    }

    let Some(participants) = ctx.hub.chat_participants(ctx.chat_id).await else {
        state
            .report
            .record_divergence("hub has no local view of its own chat".to_owned());
        return;
    };
    let hub_id = ctx.hub.user_id();

    // Pick who drives this step, then bring it up to date before it decides
    // anything: an operation staged from a stale epoch is rejected server-side.
    let (originator, originator_id) = pick_originator(ctx, &participants, rng);
    if &originator_id != hub_id {
        let drain_result = ops::drain(originator).await;
        record_drain(&mut state.report, "drain_originator", &drain_result);
        if drain_result.is_err() {
            return;
        }
    }

    // Candidate targets: everyone in the group but the hub (removing it would
    // destroy the roster oracle) and the originator (which drives instead).
    let others: Vec<UserId> = participants
        .iter()
        .filter(|id| *id != hub_id && **id != originator_id)
        .cloned()
        .collect();

    match pick_step(rng) {
        StepKind::SendMessage => send_message(ctx, originator, &others, rng, state).await,
        StepKind::Invite => {
            invite_members(ctx, originator, &originator_id, &participants, rng, state).await
        }
        StepKind::Remove => remove_members(ctx, originator, &others, rng, state).await,
        StepKind::SelfUpdate => self_update(ctx, originator, state).await,
        StepKind::Leave => leave_group(ctx, originator, &originator_id, &others, rng, state).await,
    }
}

fn pick_originator<'a>(
    ctx: &WalkContext<'a>,
    participants: &HashSet<UserId>,
    rng: &mut impl Rng,
) -> (&'a CoreUser, UserId) {
    let hub_id = ctx.hub.user_id();
    match participants.iter().choose(rng) {
        Some(id) if id != hub_id => match ctx.clients_by_id.get(id) {
            Some(user) => (*user, id.clone()),
            // A participant the fleet does not hold a client for (left over
            // from an earlier, larger run) cannot drive anything.
            None => (ctx.hub, hub_id.clone()),
        },
        _ => (ctx.hub, hub_id.clone()),
    }
}

/// Picks up to `n` distinct targets from `pool`.
fn pick_targets(pool: &[UserId], n: usize, rng: &mut impl Rng) -> Vec<UserId> {
    if pool.is_empty() || n == 0 {
        return Vec::new();
    }
    pool.iter().cloned().sample(rng, n.min(pool.len()))
}

/// Drains every target and, when `still_members` says the operation left them
/// in the group, has each commit a key update.
///
/// Sequential on purpose: every target drains to the epoch its predecessor
/// just created, so each commit is staged from the current epoch and a
/// rejection here is a real failure rather than an expected race.
async fn settle_targets(
    ctx: &WalkContext<'_>,
    targets: &[UserId],
    still_members: bool,
    state: &mut WalkState,
) {
    for target_id in targets {
        let Some(target) = ctx.clients_by_id.get(target_id).copied() else {
            continue;
        };
        let drain_result = ops::drain(target).await;
        record_drain(&mut state.report, "drain_target", &drain_result);
        if drain_result.is_err() || !still_members {
            continue;
        }
        // The drain may itself have evicted the target (a concurrent removal
        // it had not yet seen), in which case it has no group left to update.
        if !ops::is_member(target, ctx.chat_id).await {
            continue;
        }
        let result = ops::update_key(target, ctx.chat_id).await;
        state.report.record("target_self_update", &result);
    }
}

async fn send_message(
    ctx: &WalkContext<'_>,
    originator: &CoreUser,
    others: &[UserId],
    rng: &mut impl Rng,
    state: &mut WalkState,
) {
    let result = ops::send_message(originator, ctx.chat_id).await;
    state.report.record("send_message", &result);
    if result.is_err() {
        return;
    }
    // The readers are this operation's targets: they consume the message and
    // then rotate, the same as the targets of a structural change.
    let targets = pick_targets(others, rng.random_range(1..=ctx.max_targets), rng);
    settle_targets(ctx, &targets, true, state).await;
}

/// The invitees an inviter is actually able to add: adds are contact-gated
/// (`Contact::load` in coreclient's add path), so the pool is the inviter's
/// own contacts minus whoever is already in the group.
async fn invite_candidates(inviter: &CoreUser, participants: &HashSet<UserId>) -> Vec<UserId> {
    let Ok(contacts) = inviter.contacts().await else {
        return Vec::new();
    };
    contacts
        .into_iter()
        .map(|contact| contact.user_id)
        .filter(|id| !participants.contains(id))
        .collect()
}

async fn invite_members(
    ctx: &WalkContext<'_>,
    originator: &CoreUser,
    originator_id: &UserId,
    participants: &HashSet<UserId>,
    rng: &mut impl Rng,
    state: &mut WalkState,
) {
    // A mesh member holds only a handful of contacts, so it runs out of people
    // to invite as the group grows. The hub holds every contact, so it takes
    // over rather than letting the step go to waste.
    let mut inviter = originator;
    let mut candidates = invite_candidates(originator, participants).await;
    if candidates.is_empty() && originator_id != ctx.hub.user_id() {
        inviter = ctx.hub;
        candidates = invite_candidates(ctx.hub, participants).await;
    }
    if candidates.is_empty() {
        return;
    }

    let n = state.invite_batch_size(ctx.max_targets);
    let targets = pick_targets(&candidates, n, rng);
    if targets.is_empty() {
        return;
    }

    let result = ops::invite(inviter, ctx.chat_id, &targets).await;
    state.report.record("invite_members", &result);
    if result.is_err() {
        return;
    }
    state.members_added += targets.len();

    // Anyone here that was evicted before is now a rejoin, and owes us a
    // convergence check against the epoch the invite created.
    let inviter_epoch = inviter
        .group_epoch_and_own_index(ctx.chat_id)
        .await
        .ok()
        .flatten()
        .map(|(epoch, _)| epoch)
        .unwrap_or_default();
    state
        .rejoins
        .record_rejoin(&targets, state.step, inviter_epoch);

    settle_targets(ctx, &targets, true, state).await;
}

async fn remove_members(
    ctx: &WalkContext<'_>,
    originator: &CoreUser,
    others: &[UserId],
    rng: &mut impl Rng,
    state: &mut WalkState,
) {
    let targets = pick_targets(others, rng.random_range(1..=ctx.max_targets), rng);
    if targets.is_empty() {
        return;
    }
    let result = ops::remove(originator, ctx.chat_id, targets.clone()).await;
    state.report.record("remove_members", &result);
    if result.is_err() {
        return;
    }
    state.members_removed += targets.len();
    state.rejoins.record_eviction(&targets);

    // Removed targets still drain, so they observe their own eviction, but
    // they have no group left to rotate in.
    settle_targets(ctx, &targets, false, state).await;
}

async fn self_update(ctx: &WalkContext<'_>, originator: &CoreUser, state: &mut WalkState) {
    let result = ops::update_key(originator, ctx.chat_id).await;
    state.report.record("self_update", &result);
}

async fn leave_group(
    ctx: &WalkContext<'_>,
    originator: &CoreUser,
    originator_id: &UserId,
    others: &[UserId],
    rng: &mut impl Rng,
    state: &mut WalkState,
) {
    // A member leaves itself, so the originator is the leaver -- unless that
    // is the hub, whose departure would end the run.
    let (leaver, leaver_id) = if originator_id != ctx.hub.user_id() {
        (originator, originator_id.clone())
    } else {
        let Some(id) = others.iter().choose(rng).cloned() else {
            return;
        };
        let Some(user) = ctx.clients_by_id.get(&id).copied() else {
            return;
        };
        let drain_result = ops::drain(user).await;
        record_drain(&mut state.report, "drain_before_leave", &drain_result);
        if drain_result.is_err() {
            return;
        }
        (user, id)
    };

    let result = ops::leave(leaver, ctx.chat_id).await;
    state.report.record("leave_group", &result);
    if result.is_err() {
        return;
    }
    state.members_removed += 1;
    state.rejoins.record_eviction([&leaver_id]);

    // A self-remove is only a proposal: it takes effect once someone else's
    // next commit sweeps it in. The hub drains to observe the proposal, then
    // issues a plain update to commit it, so the leave doesn't linger
    // indefinitely if no other structural change happens to pick it up.
    let drain_result = ops::drain(ctx.hub).await;
    record_drain(&mut state.report, "drain_hub_before_commit", &drain_result);
    if drain_result.is_err() {
        return;
    }
    let commit_result = ops::update_key(ctx.hub, ctx.chat_id).await;
    state.report.record("commit_pending_leave", &commit_result);
}
