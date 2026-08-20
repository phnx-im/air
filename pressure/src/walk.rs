// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! The random walk over the group the hub owns, run as rounds of concurrent
//! steps.
//!
//! A step picks an originator, brings it up to date, and has it drive one
//! operation against N targets, which are then drained so both sides of the
//! operation end up converged.
//!
//! Only the operations that are commits in MLS produce commits here. Reading a
//! message or a reaction does not, and leaf rotation is its own `SelfUpdate`
//! step on the same footing as the rest, mirroring the real client's daily
//! `SELF_UPDATE_INTERVAL` rather than rekeying in response to traffic.
//!
//! # Why rounds, and why partitions
//!
//! Steps run concurrently, but two steps may never touch the same member: a
//! drain advances that member's QS queue ratchet, so overlapping drains on one
//! member would corrupt it. Each round therefore deals the current membership
//! into disjoint [`Partition`]s, one per concurrent step, and a step may only
//! reach the members it was dealt.
//!
//! Commits, by contrast, are deliberately left to race. Only one commit per
//! epoch wins; coreclient answers the DS's `WrongEpoch` by marking the commit
//! failed and parking the job for retry, so the losers are a first-class path
//! worth exercising rather than something to design around. They are counted
//! as `commit_races` instead of failures.
//!
//! The hub is the roster oracle the verification pass compares against. It is
//! drained once at the top of a round, before any step starts, and is only
//! ever driven by the single partition that holds it.

use std::{
    collections::{HashMap, HashSet},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

use aircommon::identifiers::UserId;
use aircoreclient::{ChatId, clients::CoreUser};
use rand::{
    Rng, RngExt, SeedableRng,
    seq::{IteratorRandom, SliceRandom},
};
use rand_chacha::ChaCha8Rng;

use crate::{ops, rejoin::RejoinTracker, report::Report};

pub struct WalkContext<'a> {
    pub hub: &'a CoreUser,
    pub chat_id: ChatId,
    /// All non-hub members, indexable by user id.
    pub clients_by_id: &'a HashMap<UserId, &'a CoreUser>,
    /// Upper bound on how many targets a single operation may take.
    pub max_targets: usize,
    /// Members to add per member removed, once the group is at its target.
    pub growth_ratio: f64,
    /// Group size the walk grows toward before it starts removing members.
    pub target_members: usize,
    /// Full fan-out width, for the sequential parts of a round.
    pub concurrency: usize,
    /// How many steps run at once.
    pub concurrent_steps: usize,
    /// Fan-out width available to one step for its own per-member work.
    pub step_concurrency: usize,
}

/// Mutable state threaded through a round: the tallies, plus the tracker that
/// follows evicted members until they rejoin and converge.
pub struct WalkState {
    pub report: Report,
    pub rejoins: RejoinTracker,
    /// Number of steps taken so far, used to timestamp rejoins.
    pub step: usize,
    /// Members added and removed so far, which together size each round's
    /// invite budget (see [`WalkState::round_invite_budget`]).
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

    /// How many members this round may add in total, across every concurrent
    /// step.
    ///
    /// Two terms, whichever is larger:
    ///
    /// * the gap to `target_members`, so a group below its target climbs at the
    ///   full per-round cap. Without this the budget would depend entirely on
    ///   removals, and a group too small to have anything worth removing could
    ///   never grow: no removals means no deficit means the floor of one.
    /// * `growth_ratio` times the removals so far, minus the adds so far, which
    ///   is what keeps the group drifting upward once it is at its target and
    ///   churn is the only thing moving it.
    ///
    /// This is a *per-round* budget rather than a per-step batch size. Giving
    /// each step its own copy would let a round with several invite steps add
    /// `concurrent_steps` times the intended amount.
    ///
    /// Both terms are cumulative or self-limiting, so overshooting a round just
    /// drops the next rounds back to the floor of one.
    fn round_invite_budget(
        &self,
        growth_ratio: f64,
        max_targets: usize,
        members: usize,
        target_members: usize,
    ) -> usize {
        let ratio_target = (self.members_removed as f64 * growth_ratio) as usize;
        let ratio_deficit = ratio_target.saturating_sub(self.members_added);
        let gap_to_target = target_members.saturating_sub(members);
        ratio_deficit
            .max(gap_to_target)
            .clamp(1, max_targets.max(1))
    }
}

/// Claims up to `max` of the round's remaining invite budget, returning how
/// many were actually available. Concurrent steps draw from the same counter,
/// so the budget bounds the round rather than each step.
fn claim_invite_budget(budget: &AtomicUsize, max: usize) -> usize {
    let mut remaining = budget.load(Ordering::Relaxed);
    loop {
        let take = remaining.min(max);
        if take == 0 {
            return 0;
        }
        match budget.compare_exchange_weak(
            remaining,
            remaining - take,
            Ordering::Relaxed,
            Ordering::Relaxed,
        ) {
            Ok(_) => return take,
            Err(actual) => remaining = actual,
        }
    }
}

/// Relative frequency of each step kind once the group has reached its target
/// size. How *many* members an invite adds is the round's budget, not this
/// weight; these only set how often each kind of pressure is applied.
const STEADY_WEIGHTS: &[(StepKind, u32)] = &[
    (StepKind::SendMessage, 4),
    (StepKind::Invite, 3),
    (StepKind::Remove, 2),
    (StepKind::SelfUpdate, 2),
    (StepKind::React, 2),
    (StepKind::Leave, 1),
];

/// Relative frequency while the group is still below its target size.
///
/// Removals are left out entirely and invites dominate. A small group has few
/// members to target, so it also gets few concurrent steps per round -- with the
/// steady weights, most of those rounds are spent on churn and the group creeps
/// up at a fraction of a member per round. Suppressing the shrink operations
/// until the group is at size is what makes the climb actually happen.
const GROWTH_WEIGHTS: &[(StepKind, u32)] = &[
    (StepKind::Invite, 8),
    (StepKind::SendMessage, 3),
    (StepKind::SelfUpdate, 2),
    (StepKind::React, 2),
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum StepKind {
    SendMessage,
    Invite,
    Remove,
    SelfUpdate,
    React,
    Leave,
}

fn pick_step(rng: &mut impl Rng, below_target: bool) -> StepKind {
    let weights = if below_target {
        GROWTH_WEIGHTS
    } else {
        STEADY_WEIGHTS
    };
    let total: u32 = weights.iter().map(|(_, weight)| weight).sum();
    let mut roll = rng.random_range(0..total);
    for (kind, weight) in weights {
        if roll < *weight {
            return *kind;
        }
        roll -= weight;
    }
    StepKind::SendMessage
}

/// One concurrent step's exclusive slice of the fleet.
///
/// Partitions never share a member. That is the invariant that makes the steps
/// safe to run at once, and everything a step needs is owned here so the task
/// borrows nothing.
struct Partition {
    chat_id: ChatId,
    originator: CoreUser,
    /// Whether this step drives the hub. Exactly one partition ever does, so
    /// the roster oracle is never touched by two steps at once.
    is_hub: bool,
    /// Members this step may target.
    members: Vec<UserId>,
    /// Clients for the members above and for the invite candidates below.
    clients: HashMap<UserId, CoreUser>,
    /// Inactive members this step may invite: already narrowed to the
    /// originator's contacts, and disjoint from every other partition's.
    invite_candidates: Vec<UserId>,
    /// The round's remaining invite allowance, shared with the other steps in
    /// the same round so their adds sum to the budget instead of each getting
    /// the full amount.
    invite_budget: Arc<AtomicUsize>,
    max_targets: usize,
    /// Whether the group is still below its target size, which selects the
    /// step weights this step draws from.
    below_target: bool,
    concurrency: usize,
    seed: u64,
}

/// What one step did, folded back into [`WalkState`] once the round joins.
/// Collected rather than applied in place, so the shared trackers are only
/// ever touched from the round's own task.
#[derive(Default)]
struct StepOutcome {
    report: Report,
    evicted: Vec<UserId>,
    rejoined: Vec<UserId>,
    rejoin_epoch: u64,
    added: usize,
    removed: usize,
}

/// What a round reports back to the caller for display and cadence.
pub struct RoundOutcome {
    /// Steps that actually ran, which may be fewer than `concurrent_steps`
    /// when the group is too small to partition that many ways.
    pub steps: usize,
    /// Group size as the hub saw it at the top of the round.
    pub members: usize,
    /// Who drove the steps, for the caller's originator-diversity check: a
    /// healthy walk is driven by ever-changing members, and a walk that
    /// keeps picking the same few can only exercise their corner of the
    /// group (which is exactly what the broken shuffle did).
    pub originators: Vec<UserId>,
}

/// Runs one round of concurrent steps.
pub async fn run_round(
    ctx: &WalkContext<'_>,
    rng: &mut impl Rng,
    state: &mut WalkState,
) -> RoundOutcome {
    // The hub owns the group and is what verification compares against, so it
    // is brought up to date before any step starts. Nothing else is in flight
    // at this point, so this is the one safe place to drain it.
    let drain_result = ops::drain(ctx.hub).await;
    record_drain(&mut state.report, "drain_hub", &drain_result);
    if drain_result.is_err() {
        return RoundOutcome {
            steps: 0,
            members: 0,
            originators: Vec::new(),
        };
    }

    let Some(participants) = ctx.hub.chat_participants(ctx.chat_id).await else {
        state
            .report
            .record_divergence("hub has no local view of its own chat".to_owned());
        return RoundOutcome {
            steps: 0,
            members: 0,
            originators: Vec::new(),
        };
    };
    let members = participants.len();

    let partitions = build_partitions(ctx, &participants, state, rng).await;
    if partitions.is_empty() {
        return RoundOutcome {
            steps: 0,
            members,
            originators: Vec::new(),
        };
    }
    let originators: Vec<UserId> = partitions
        .iter()
        .map(|partition| partition.originator.user_id().clone())
        .collect();

    let outcomes = crate::parallel::map(partitions, ctx.concurrent_steps, run_step).await;

    let ran = outcomes.len();
    // Advanced before the fold so rejoins are stamped with the round they
    // actually happened in. The caller keeps its own running total.
    state.step += ran;
    for outcome in outcomes {
        state.report.merge(outcome.report);
        state.members_added += outcome.added;
        state.members_removed += outcome.removed;
        state.rejoins.record_eviction(&outcome.evicted);
        if !outcome.rejoined.is_empty() {
            state
                .rejoins
                .record_rejoin(&outcome.rejoined, state.step, outcome.rejoin_epoch);
        }
    }
    RoundOutcome {
        steps: ran,
        members,
        originators,
    }
}

/// Deals the current membership into disjoint partitions, one per concurrent
/// step.
///
/// The hub, when it is a participant, always heads the first partition. It has
/// no more contacts than anyone else, but it is the roster oracle the
/// verification pass compares against, so confining it to one partition keeps
/// it from being driven by two steps at once.
async fn build_partitions(
    ctx: &WalkContext<'_>,
    participants: &HashSet<UserId>,
    state: &WalkState,
    rng: &mut impl Rng,
) -> Vec<Partition> {
    // Hysteresis, so the group does not flip out of growth mode the instant it
    // touches the target and then back in on the first removal: churn stays on
    // anywhere in the top tenth of the target.
    let growth_threshold = ctx.target_members - ctx.target_members / 10;
    let below_target = participants.len() < growth_threshold;
    let hub_id = ctx.hub.user_id();
    let chat_id = ctx.chat_id;

    // The hub's roster is not enough to decide who can be driven. A
    // self-remove is only a proposal, so a member that left stays listed until
    // some commit sweeps it in -- and that sweep can lose its epoch race,
    // leaving the member listed by the hub while its own chat has already gone
    // inactive. Driving one of those fails with "inactive chat", so each
    // candidate is asked about its own state instead.
    // Sorted before anything random touches it. `participants` is a HashSet,
    // whose iteration order varies between processes, so feeding it to the rng
    // unsorted would make a seed unrepeatable.
    let mut candidate_ids: Vec<&UserId> = participants.iter().filter(|id| *id != hub_id).collect();
    candidate_ids.sort();
    let candidates: Vec<(UserId, CoreUser)> = candidate_ids
        .into_iter()
        .filter_map(|id| {
            ctx.clients_by_id
                .get(id)
                .map(|user| (id.clone(), (*user).clone()))
        })
        .collect();
    let active = crate::parallel::map(candidates, ctx.concurrency, move |(id, user)| async move {
        ops::is_member(&user, chat_id).await.then_some(id)
    })
    .await;
    let mut pool: Vec<UserId> = active.into_iter().flatten().collect();
    // A real shuffle, so originators and partitions are uniformly random.
    // This must not be `sample(rng, len)`: a full-size sample fills its
    // reservoir sequentially and returns the elements in their original
    // order, so the pool was never shuffled at all -- the same few members
    // (the tail of the sorted order) drove every round, and the group only
    // ever grew around wherever they sat on the ring.
    pool.shuffle(rng);

    // A step needs an originator plus something to target, so the number of
    // steps is capped by how many members there are to go round.
    let step_count = ctx.concurrent_steps.min(1 + pool.len() / 2).max(1);

    let mut originators: Vec<(CoreUser, UserId)> = Vec::with_capacity(step_count);
    let mut buckets: Vec<Vec<UserId>> = vec![Vec::new(); step_count];

    if participants.contains(hub_id) {
        originators.push((ctx.hub.clone(), hub_id.clone()));
    }
    while originators.len() < step_count {
        let Some(id) = pool.pop() else { break };
        let Some(user) = ctx.clients_by_id.get(&id).copied() else {
            continue;
        };
        originators.push((user.clone(), id));
    }
    if originators.is_empty() {
        return Vec::new();
    }

    for (index, id) in pool.into_iter().enumerate() {
        buckets[index % originators.len()].push(id);
    }

    let mut invite_pools = deal_invite_candidates(ctx, participants, &originators).await;
    let invite_budget = Arc::new(AtomicUsize::new(state.round_invite_budget(
        ctx.growth_ratio,
        ctx.max_targets,
        participants.len(),
        ctx.target_members,
    )));

    originators
        .into_iter()
        .enumerate()
        .map(|(index, (originator, originator_id))| {
            let members = std::mem::take(&mut buckets[index]);
            let invite_candidates = std::mem::take(&mut invite_pools[index]);
            // Invitees need a client too: they are settled by the step that
            // adds them, and they are not participants yet so they are not in
            // `members`.
            let clients = members
                .iter()
                .chain(invite_candidates.iter())
                .filter_map(|id| {
                    ctx.clients_by_id
                        .get(id)
                        .map(|user| (id.clone(), (*user).clone()))
                })
                .collect();
            Partition {
                chat_id: ctx.chat_id,
                is_hub: &originator_id == hub_id,
                originator,
                members,
                clients,
                invite_candidates,
                invite_budget: invite_budget.clone(),
                max_targets: ctx.max_targets,
                below_target,
                concurrency: ctx.step_concurrency,
                seed: rng.random(),
            }
        })
        .collect()
}

/// Splits the invitable members between the partitions, giving each candidate
/// to at most one.
///
/// Adds are contact-gated, so a candidate can only go to a partition whose
/// originator already holds it as a contact. Claiming is first come, which
/// leaves whatever only the hub can reach to the hub.
async fn deal_invite_candidates(
    ctx: &WalkContext<'_>,
    participants: &HashSet<UserId>,
    originators: &[(CoreUser, UserId)],
) -> Vec<Vec<UserId>> {
    let mut claimed: HashSet<UserId> = HashSet::new();
    let mut pools = Vec::with_capacity(originators.len());

    for (originator, _) in originators {
        let mut pool = Vec::new();
        if let Ok(contacts) = originator.contacts().await {
            for contact in contacts {
                let id = contact.user_id;
                if participants.contains(&id)
                    || claimed.contains(&id)
                    || !ctx.clients_by_id.contains_key(&id)
                {
                    continue;
                }
                claimed.insert(id.clone());
                pool.push(id);
            }
        }
        // Sorted for the same reason the member pool is: the invite draw must
        // depend only on the seed, not on any container's iteration order.
        pool.sort();
        pools.push(pool);
    }
    pools
}

async fn run_step(partition: Partition) -> StepOutcome {
    let mut rng = ChaCha8Rng::seed_from_u64(partition.seed);
    let mut outcome = StepOutcome::default();

    // Bring the originator up to date before it decides anything: an operation
    // staged from a stale epoch is rejected server-side. The hub was already
    // drained at the top of the round, and draining it again here would race
    // with nothing but is redundant.
    if !partition.is_hub {
        let drain_result = ops::drain(&partition.originator).await;
        record_drain(&mut outcome.report, "drain_originator", &drain_result);
        if drain_result.is_err() {
            return outcome;
        }
    }

    match pick_step(&mut rng, partition.below_target) {
        StepKind::SendMessage => send_message(&partition, &mut rng, &mut outcome).await,
        StepKind::Invite => invite_members(&partition, &mut rng, &mut outcome).await,
        StepKind::Remove => remove_members(&partition, &mut rng, &mut outcome).await,
        StepKind::SelfUpdate => self_update(&partition, &mut outcome).await,
        StepKind::React => react(&partition, &mut rng, &mut outcome).await,
        StepKind::Leave => leave_group(&partition, &mut rng, &mut outcome).await,
    }
    outcome
}

impl Partition {
    fn client(&self, id: &UserId) -> Option<&CoreUser> {
        self.clients.get(id)
    }

    /// Picks up to `n` distinct targets from this partition's members.
    fn pick_targets(&self, n: usize, rng: &mut impl Rng) -> Vec<UserId> {
        if self.members.is_empty() || n == 0 {
            return Vec::new();
        }
        self.members
            .iter()
            .cloned()
            .sample(rng, n.min(self.members.len()))
    }
}

/// Records a drain's outcome: the op's ok/err tally, plus any per-message
/// processing errors as their own (non-failing) counter.
fn record_drain(report: &mut Report, op: &'static str, result: &anyhow::Result<ops::DrainOutcome>) {
    report.record(op, result);
    if let Ok(drained) = result {
        report.record_drain_outcome(drained.fetched, drained.message_errors);
    }
}

/// Brings every target of an operation up to date.
///
/// Draining only. Targets deliberately do *not* commit anything here: reading a
/// message or being added has no commit in MLS, and the real client rekeys a
/// group on a timer ([`SELF_UPDATE_INTERVAL`], one day) rather than in response
/// to traffic. Rekeying every target of every step made target commits ~85% of
/// the run's total, inflating epoch velocity far past anything production sees
/// and turning the `MAX_PAST_EPOCHS` window into a bottleneck that only the
/// harness could hit. Leaf rotation is the `SelfUpdate` step's job instead.
///
/// The targets are distinct members, so the drains run concurrently.
/// Returns the ids of the targets that drained successfully, so callers can
/// assert delivery against exactly the members that are up to date.
async fn settle_targets(
    partition: &Partition,
    targets: &[UserId],
    outcome: &mut StepOutcome,
) -> Vec<UserId> {
    let pairs: Vec<(UserId, CoreUser)> = targets
        .iter()
        .filter_map(|id| partition.client(id).cloned().map(|user| (id.clone(), user)))
        .collect();
    if pairs.is_empty() {
        return Vec::new();
    }
    let results = crate::parallel::map(pairs, partition.concurrency, |(id, user)| async move {
        (id, ops::drain(&user).await)
    })
    .await;
    let mut drained = Vec::new();
    for (id, result) in results {
        record_drain(&mut outcome.report, "drain_target", &result);
        if result.is_ok() {
            drained.push(id);
        }
    }
    drained
}

async fn send_message(partition: &Partition, rng: &mut impl Rng, outcome: &mut StepOutcome) {
    // The marker ties this step's message to what the targets must receive.
    // The partition seed is unique per step, so markers never collide.
    let marker = format!("walk-{:016x}", partition.seed);
    let result = ops::send_message(&partition.originator, partition.chat_id, &marker).await;
    outcome.report.record("send_message", &result);
    if result.is_err() {
        return;
    }
    // The readers are this operation's targets: they consume the message the
    // same as the targets of a structural change consume its commit.
    let n = rng.random_range(1..=partition.max_targets);
    let targets = partition.pick_targets(n, rng);
    let drained = settle_targets(partition, &targets, outcome).await;

    // Fan-out completes within the send request, so every drained target
    // that is still a member must have the message. A missing marker is
    // silent message loss, the one failure mode nothing else here detects.
    for target_id in drained {
        let Some(target) = partition.client(&target_id) else {
            continue;
        };
        if !ops::is_member(target, partition.chat_id).await {
            continue;
        }
        match ops::has_message_with_marker(target, partition.chat_id, &marker).await {
            Ok(true) => {}
            Ok(false) => outcome.report.record_divergence(format!(
                "{target_id:?} drained but never received the message {marker}"
            )),
            Err(error) => outcome.report.record_divergence(format!(
                "delivery check of {marker} on {target_id:?} failed: {error}"
            )),
        }
    }
}

async fn invite_members(partition: &Partition, rng: &mut impl Rng, outcome: &mut StepOutcome) {
    if partition.invite_candidates.is_empty() {
        return;
    }
    let allowance = claim_invite_budget(&partition.invite_budget, partition.max_targets);
    let n = allowance.min(partition.invite_candidates.len());
    if n == 0 {
        // Another step in this round already used the whole budget.
        return;
    }
    let targets: Vec<UserId> = partition.invite_candidates.iter().cloned().sample(rng, n);
    if targets.is_empty() {
        return;
    }

    let result = ops::invite(&partition.originator, partition.chat_id, &targets).await;
    outcome.report.record("invite_members", &result);
    if result.is_err() {
        return;
    }
    outcome.added += targets.len();

    // Anyone here that was evicted before is now a rejoin, and owes us a
    // convergence check against the epoch the invite created.
    outcome.rejoin_epoch = partition
        .originator
        .group_epoch_and_own_index(partition.chat_id)
        .await
        .ok()
        .flatten()
        .map(|(epoch, _)| epoch)
        .unwrap_or_default();
    outcome.rejoined = targets.clone();

    // The invitees are not partition members (they were not participants
    // when the round was built), but they are in the partition's client map,
    // and the candidate pools were dealt out so no two steps invite the same
    // member. Draining them is what processes their Welcome.
    let _ = settle_targets(partition, &targets, outcome).await;
}

async fn remove_members(partition: &Partition, rng: &mut impl Rng, outcome: &mut StepOutcome) {
    let n = rng.random_range(1..=partition.max_targets);
    let targets = partition.pick_targets(n, rng);
    if targets.is_empty() {
        return;
    }
    let result = ops::remove(&partition.originator, partition.chat_id, targets.clone()).await;
    outcome.report.record("remove_members", &result);
    if result.is_err() {
        return;
    }
    outcome.removed += targets.len();
    outcome.evicted = targets.clone();

    // Removed targets still drain, so they observe their own eviction.
    let _ = settle_targets(partition, &targets, outcome).await;
}

async fn self_update(partition: &Partition, outcome: &mut StepOutcome) {
    let result = ops::update_key(&partition.originator, partition.chat_id).await;
    outcome.report.record("self_update", &result);
}

async fn react(partition: &Partition, rng: &mut ChaCha8Rng, outcome: &mut StepOutcome) {
    let result = ops::react(&partition.originator, partition.chat_id, rng).await;
    if matches!(result, Ok(None)) {
        // Nothing to react to yet; not an attempt worth counting.
        return;
    }
    outcome.report.record("send_reaction", &result);
    let proof = match result {
        Ok(Some(proof)) => proof,
        Ok(None) => unreachable!("handled above"),
        Err(_) => return,
    };
    // The readers are this operation's targets, same as for a text message.
    let n = rng.random_range(1..=partition.max_targets);
    let targets = partition.pick_targets(n, rng);
    let drained = settle_targets(partition, &targets, outcome).await;

    let reactor = partition.originator.user_id();
    for target_id in drained {
        let Some(target) = partition.client(&target_id) else {
            continue;
        };
        if !ops::is_member(target, partition.chat_id).await {
            continue;
        }
        match ops::reaction_visible(target, partition.chat_id, &proof, reactor).await {
            // The reacted-to message predates the target's join; nothing to
            // assert.
            Ok(None) => {}
            Ok(Some(true)) => {}
            Ok(Some(false)) => outcome.report.record_divergence(format!(
                "{target_id:?} has the reacted-to message but not {reactor:?}'s                  {} on it",
                proof.emoji
            )),
            Err(error) => outcome.report.record_divergence(format!(
                "reaction check on {target_id:?} failed: {error}"
            )),
        }
    }
}

async fn leave_group(partition: &Partition, rng: &mut impl Rng, outcome: &mut StepOutcome) {
    // One of the partition's own members leaves, and the originator sweeps the
    // proposal in. Keeping both inside the partition is what lets this run
    // alongside other steps -- in particular it never drives the hub.
    let Some(leaver_id) = partition.members.iter().choose(rng).cloned() else {
        return;
    };
    let Some(leaver) = partition.client(&leaver_id) else {
        return;
    };

    let drain_result = ops::drain(leaver).await;
    record_drain(&mut outcome.report, "drain_before_leave", &drain_result);
    if drain_result.is_err() {
        return;
    }

    let result = ops::leave(leaver, partition.chat_id).await;
    outcome.report.record("leave_group", &result);
    if result.is_err() {
        return;
    }
    outcome.removed += 1;
    outcome.evicted = vec![leaver_id];

    // A self-remove is only a proposal: it takes effect once someone else's
    // next commit sweeps it in. The originator drains to observe the proposal,
    // then issues a plain update to commit it, so the leave doesn't linger
    // indefinitely if no other structural change happens to pick it up.
    let drain_result = ops::drain(&partition.originator).await;
    record_drain(&mut outcome.report, "drain_before_commit", &drain_result);
    if drain_result.is_err() {
        return;
    }
    let commit_result = ops::update_key(&partition.originator, partition.chat_id).await;
    outcome
        .report
        .record("commit_pending_leave", &commit_result);
}
