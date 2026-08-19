// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! The random walk over the group the hub owns, run as rounds of concurrent
//! steps.
//!
//! A step picks an originator, brings it up to date, and has it drive one
//! operation against N targets. Every target is then drained and, if the
//! operation left it in the group, commits a key update of its own, so a step
//! leaves both sides of the operation converged and rekeyed.
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

use std::collections::{HashMap, HashSet};

use aircommon::identifiers::UserId;
use aircoreclient::{ChatId, clients::CoreUser};
use rand::{Rng, RngExt, SeedableRng, seq::IteratorRandom};
use rand_chacha::ChaCha8Rng;

use crate::{ops, rejoin::RejoinTracker, report::Report};

pub struct WalkContext<'a> {
    pub hub: &'a CoreUser,
    pub chat_id: ChatId,
    /// All non-hub members, indexable by user id.
    pub clients_by_id: &'a HashMap<UserId, &'a CoreUser>,
    /// Upper bound on how many targets a single operation may take.
    pub max_targets: usize,
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
    /// The batch size the invite should aim for, decided before the round so
    /// concurrent steps do not all read the same stale balance.
    invite_batch: usize,
    max_targets: usize,
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

/// Runs one round of concurrent steps, returning how many actually ran.
pub async fn run_round(ctx: &WalkContext<'_>, rng: &mut impl Rng, state: &mut WalkState) -> usize {
    // The hub owns the group and is what verification compares against, so it
    // is brought up to date before any step starts. Nothing else is in flight
    // at this point, so this is the one safe place to drain it.
    let drain_result = ops::drain(ctx.hub).await;
    state.report.record("drain_hub", &drain_result);
    if let Ok(outcome) = &drain_result {
        state
            .report
            .record_drain_outcome(outcome.fetched, outcome.message_errors);
    }
    if drain_result.is_err() {
        return 0;
    }

    let Some(participants) = ctx.hub.chat_participants(ctx.chat_id).await else {
        state
            .report
            .record_divergence("hub has no local view of its own chat".to_owned());
        return 0;
    };

    let partitions = build_partitions(ctx, &participants, state, rng).await;
    if partitions.is_empty() {
        return 0;
    }

    let outcomes = crate::parallel::map(partitions, ctx.concurrent_steps, run_step).await;

    let ran = outcomes.len();
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
    ran
}

/// Deals the current membership into disjoint partitions, one per concurrent
/// step.
///
/// The hub, when it is a participant, always heads the first partition: it is
/// the only member every other one is connected to, so it is the only reliable
/// inviter, and confining it to one partition keeps it from being driven by
/// two steps at once.
async fn build_partitions(
    ctx: &WalkContext<'_>,
    participants: &HashSet<UserId>,
    state: &WalkState,
    rng: &mut impl Rng,
) -> Vec<Partition> {
    let hub_id = ctx.hub.user_id();
    let chat_id = ctx.chat_id;

    // The hub's roster is not enough to decide who can be driven. A
    // self-remove is only a proposal, so a member that left stays listed until
    // some commit sweeps it in -- and that sweep can lose its epoch race,
    // leaving the member listed by the hub while its own chat has already gone
    // inactive. Driving one of those fails with "inactive chat", so each
    // candidate is asked about its own state instead.
    let candidates: Vec<(UserId, CoreUser)> = participants
        .iter()
        .filter(|id| *id != hub_id)
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
    let pool: Vec<UserId> = active.into_iter().flatten().collect();
    // Shuffled so partition membership is not tied to user id ordering, which
    // would have the same members share a step every round.
    let mut pool = pool.iter().cloned().sample(rng, pool.len());

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
    let invite_batch = state.invite_batch_size(ctx.max_targets);

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
                invite_batch,
                max_targets: ctx.max_targets,
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

    match pick_step(&mut rng) {
        StepKind::SendMessage => send_message(&partition, &mut rng, &mut outcome).await,
        StepKind::Invite => invite_members(&partition, &mut rng, &mut outcome).await,
        StepKind::Remove => remove_members(&partition, &mut rng, &mut outcome).await,
        StepKind::SelfUpdate => self_update(&partition, &mut outcome).await,
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

/// Drains every target and, when `still_members` says the operation left them
/// in the group, has each commit a key update.
///
/// The catch-up drains run concurrently, since the targets are distinct
/// members. The commits stay sequential: each target drains to the epoch its
/// predecessor just created, so a rejection there means it lost the race with
/// another *step*, not with its own partition-mate.
async fn settle_targets(
    partition: &Partition,
    targets: &[UserId],
    still_members: bool,
    outcome: &mut StepOutcome,
) {
    let users: Vec<CoreUser> = targets
        .iter()
        .filter_map(|id| partition.client(id).cloned())
        .collect();
    if users.len() > 1 {
        let results = crate::parallel::map(users, partition.concurrency, |user| async move {
            ops::drain(&user).await
        })
        .await;
        for result in &results {
            record_drain(&mut outcome.report, "drain_target", result);
        }
    }

    for target_id in targets {
        let Some(target) = partition.client(target_id) else {
            continue;
        };
        let drain_result = ops::drain(target).await;
        record_drain(&mut outcome.report, "drain_target", &drain_result);
        if drain_result.is_err() || !still_members {
            continue;
        }
        // The drain may itself have evicted the target (a removal by another
        // step that it had not yet seen), leaving no group to update.
        if !ops::is_member(target, partition.chat_id).await {
            continue;
        }
        let result = ops::update_key(target, partition.chat_id).await;
        outcome.report.record("target_self_update", &result);
    }
}

async fn send_message(partition: &Partition, rng: &mut impl Rng, outcome: &mut StepOutcome) {
    let result = ops::send_message(&partition.originator, partition.chat_id).await;
    outcome.report.record("send_message", &result);
    if result.is_err() {
        return;
    }
    // The readers are this operation's targets: they consume the message and
    // then rotate, the same as the targets of a structural change.
    let n = rng.random_range(1..=partition.max_targets);
    let targets = partition.pick_targets(n, rng);
    settle_targets(partition, &targets, true, outcome).await;
}

async fn invite_members(partition: &Partition, rng: &mut impl Rng, outcome: &mut StepOutcome) {
    if partition.invite_candidates.is_empty() {
        return;
    }
    let n = partition
        .invite_batch
        .min(partition.invite_candidates.len());
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

    // The invitees are not in this partition's client map (they were not
    // participants when it was built), so they are settled directly.
    settle_invitees(partition, &targets, outcome).await;
}

/// Settles freshly invited members, which are outside the partition's own
/// membership. Safe for the same reason the partitions are: the invite
/// candidates were dealt out so no two steps can invite the same member.
async fn settle_invitees(partition: &Partition, targets: &[UserId], outcome: &mut StepOutcome) {
    for target_id in targets {
        let Some(target) = partition.client(target_id) else {
            continue;
        };
        let drain_result = ops::drain(target).await;
        record_drain(&mut outcome.report, "drain_target", &drain_result);
        if drain_result.is_err() {
            continue;
        }
        if !ops::is_member(target, partition.chat_id).await {
            continue;
        }
        let result = ops::update_key(target, partition.chat_id).await;
        outcome.report.record("target_self_update", &result);
    }
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

    // Removed targets still drain, so they observe their own eviction, but
    // they have no group left to rotate in.
    settle_targets(partition, &targets, false, outcome).await;
}

async fn self_update(partition: &Partition, outcome: &mut StepOutcome) {
    let result = ops::update_key(&partition.originator, partition.chat_id).await;
    outcome.report.record("self_update", &result);
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
