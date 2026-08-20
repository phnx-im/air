// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Stress-tests a staging (or local) server by building a fleet of persisted,
//! mesh-connected clients around a hub-owned group, then driving it with a
//! random walk of adds, removes, leaves, messages and reactions, while
//! periodically checking sampled members converge with the hub's view.
//!
//! The group starts with the hub alone and the walk grows it, so joining is
//! exercised throughout rather than once during setup.

mod bootstrap;
mod fleet;
mod ops;
mod parallel;
mod progress;
mod rejoin;
mod report;
mod verify;
mod walk;

use std::{
    collections::{HashMap, HashSet, VecDeque},
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use aircommon::identifiers::UserId;
use clap::Parser;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use rand::{RngExt, SeedableRng};
use rand_chacha::ChaCha8Rng;
use report::Report;
use tracing::info;
use tracing_subscriber::EnvFilter;
use url::Url;

use crate::fleet::Fleet;

#[derive(Parser, Debug)]
#[command(about = "Random-walk stress test for the Air protocol")]
struct Args {
    /// URL of the server to stress test.
    #[arg(long)]
    server_url: Url,

    /// Directory to persist fleet member databases in. Reused across runs:
    /// members already present there are resumed instead of recreated.
    #[arg(long)]
    root: PathBuf,

    /// Number of fleet members, including the hub.
    #[arg(long, default_value_t = 100)]
    count: usize,

    /// File with one invitation code per line, consumed for freshly created
    /// members. Not needed if the server has invitations disabled or all
    /// members already exist under `root`.
    #[arg(long)]
    invitation_codes: Option<PathBuf>,

    /// Title of the group the walk operates on.
    #[arg(long, default_value = "stress-test")]
    chat_title: String,

    /// Peers each member connects to, as a ring over the whole fleet. Adds are
    /// contact-gated, so this decides who can invite whom: the group grows
    /// outward from the hub along these edges. Zero leaves the fleet
    /// unconnected, so the group stays at one member.
    #[arg(long, default_value_t = 5)]
    contact_mesh_degree: usize,

    /// Upper bound on how many targets a single walk operation takes. Also
    /// caps how many members one round may add in total.
    #[arg(long, default_value_t = 5)]
    max_targets: usize,

    /// Group size the walk grows toward. The walk suppresses removals until the
    /// group is within a tenth of this, so it climbs instead of churning in
    /// place. 0 means the whole fleet. Capped at the fleet size, which is a hard
    /// ceiling either way.
    #[arg(long, default_value_t = 0)]
    target_members: usize,

    /// Members to add per member removed, applied once the group has reached
    /// `--target-members`. Above 1.0 keeps it drifting up against churn; 1.0
    /// holds it roughly steady.
    #[arg(long, default_value_t = 2.0)]
    growth_ratio: f64,

    /// How many per-member operations to run at once. Defaults to the number
    /// of cores. Lower it if the server rate-limits, since the whole fleet
    /// shares one source IP.
    #[arg(long)]
    concurrency: Option<usize>,

    /// How many walk steps to run at once. Each step gets a disjoint slice of
    /// the membership, so no member is ever driven by two steps, but their
    /// commits do race for the epoch. Losing commits are counted as `races`
    /// rather than failures. 1 runs the walk strictly sequentially.
    #[arg(long, default_value_t = 4)]
    concurrent_steps: usize,

    /// How long to run the random walk, in seconds. 0 runs until Ctrl-C.
    #[arg(long, default_value_t = 0)]
    duration_secs: u64,

    /// Walk steps between sampled convergence checks.
    #[arg(long, default_value_t = 20)]
    verify_every: usize,

    /// Members sampled per convergence check.
    #[arg(long, default_value_t = 5)]
    verify_sample: usize,

    /// How many convergence checks a rejoined member may fail before the
    /// divergence is reported. Gives churn a chance to settle without
    /// hiding a member that never recovers.
    #[arg(long, default_value_t = 3)]
    rejoin_grace_checks: usize,

    /// Seed for the walk's RNG. Random if unset, and logged either way, so
    /// passing it back reproduces the same order of operations against the
    /// same `--root` and `--count`. Only the walk's choices are seeded: crypto
    /// and user ids stay random, and concurrent steps still interleave by
    /// wall-clock, so a replay is close but not bit-identical.
    #[arg(long)]
    seed: Option<u64>,

    /// Where to write logs. Defaults to `airstress.log` under `--root`, so
    /// the terminal is free for the progress bar.
    #[arg(long)]
    log_file: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    std::fs::create_dir_all(&args.root)?;
    let log_path = args
        .log_file
        .clone()
        .unwrap_or_else(|| args.root.join("airstress.log"));
    let log_file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)?;
    let (log_writer, _log_guard) = tracing_appender::non_blocking(log_file);
    tracing_subscriber::fmt()
        .with_ansi(false)
        .with_writer(log_writer)
        .with_env_filter(EnvFilter::from_default_env().add_directive(tracing::Level::INFO.into()))
        .init();

    let seed = args.seed.unwrap_or_else(|| rand::rng().random());
    info!(seed, "starting stress run");
    let mut rng = ChaCha8Rng::seed_from_u64(seed);

    let invitation_codes = args
        .invitation_codes
        .as_ref()
        .map(|path| fleet::read_invitation_codes(path))
        .transpose()?;

    let multi = MultiProgress::new();
    let concurrency = args
        .concurrency
        .unwrap_or_else(parallel::default_concurrency);
    info!(concurrency, "fan-out width for per-member work");

    let fleet = Fleet::load_or_create(
        &args.root,
        args.server_url.clone(),
        args.count,
        invitation_codes,
        concurrency,
        &multi,
    )
    .await?;

    let mut report = Report::default();

    let bootstrap_params = bootstrap::BootstrapParams {
        chat_title: &args.chat_title,
        contact_mesh_degree: args.contact_mesh_degree,
        concurrency,
    };
    let chat_id = bootstrap::run(&fleet, &bootstrap_params, &mut report, &multi).await?;

    let hub = &fleet.members[0].user;
    let clients_by_id: HashMap<UserId, &aircoreclient::clients::CoreUser> = fleet.members[1..]
        .iter()
        .map(|member| (member.user.user_id().clone(), &member.user))
        .collect();
    // Sorted: this feeds the verification sample, and HashMap iteration order
    // varies between processes, which would make a seeded run unrepeatable.
    let mut client_ids: Vec<UserId> = clients_by_id.keys().cloned().collect();
    client_ids.sort();

    let ctx = walk::WalkContext {
        hub,
        chat_id,
        clients_by_id: &clients_by_id,
        max_targets: args.max_targets.max(1),
        growth_ratio: args.growth_ratio.max(0.0),
        // 0 means the whole fleet, and the fleet is the ceiling regardless.
        target_members: match args.target_members {
            0 => fleet.members.len(),
            requested => requested.min(fleet.members.len()),
        },
        concurrency,
        concurrent_steps: args.concurrent_steps.max(1),
        // Split between the steps in flight, so the two levels of fan-out
        // together stay near the configured width rather than multiplying.
        step_concurrency: (concurrency / args.concurrent_steps.max(1)).max(1),
    };
    let check_ctx = verify::CheckContext {
        hub,
        chat_id,
        clients_by_id: &clients_by_id,
        concurrency,
    };

    let mut rejoins = rejoin::RejoinTracker::default();
    if let Some(hub_participants) = hub.chat_participants(chat_id).await {
        rejoins
            .seed_evicted(chat_id, &clients_by_id, &hub_participants)
            .await;
    }
    let mut state = walk::WalkState::new(report, rejoins);

    let start = tokio::time::Instant::now();
    let deadline =
        (args.duration_secs > 0).then(|| start + Duration::from_secs(args.duration_secs));

    let walk_bar = multi.add(walk_progress_bar(&args));
    let shutdown = watch_for_shutdown();

    let mut step_count: usize = 0;
    let mut next_verify = args.verify_every.max(1);
    let mut liveness = Liveness::new(ctx.target_members, ctx.concurrent_steps);
    loop {
        if let Some(deadline) = deadline
            && tokio::time::Instant::now() >= deadline
        {
            break;
        }

        if shutdown.load(Ordering::Relaxed) {
            info!("stopping after the current round");
            break;
        }

        // Deliberately not raced against Ctrl-C. A drain is not
        // cancellation-safe: dropping it partway leaves the group state
        // advanced while the queue position is uncommitted, so the same
        // messages are redelivered and replayed, and a replayed application
        // message decrypts against a ratchet generation already consumed
        // (`SecretTreeError(TooDistantInThePast)`). Letting the round finish
        // is what keeps a killed run from leaving the fleet unusable.
        let round = walk::run_round(&ctx, &mut rng, &mut state).await;
        if round.steps == 0 {
            // Nothing to partition: the group is too small to drive. Give the
            // hub a moment rather than spinning on it.
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
        // run_round advances state.step itself, so rejoins inside the round
        // are stamped correctly. This total only drives display and cadence.
        step_count += round.steps;

        if round.steps > 0 {
            liveness.observe(&round, &mut state.report);
        }

        if deadline.is_some() {
            let elapsed_secs = tokio::time::Instant::now()
                .saturating_duration_since(start)
                .as_secs();
            walk_bar.set_position(elapsed_secs.min(args.duration_secs));
        }
        walk_bar.set_message(format!(
            "step {step_count} | members {}/{} | {}",
            round.members,
            ctx.target_members,
            state.report.summary_line()
        ));

        // A threshold rather than an exact multiple: a round advances the
        // count by however many steps it managed to run, so it can step over
        // any given multiple. Verification runs between rounds, with nothing
        // else in flight, so it sees a settled group.
        if step_count >= next_verify {
            next_verify = step_count + args.verify_every.max(1);
            run_verification(
                &check_ctx,
                &client_ids,
                args.verify_sample,
                &mut rng,
                &mut state.report,
            )
            .await;
            rejoin::check_pending(
                &mut state.rejoins,
                &check_ctx,
                args.rejoin_grace_checks,
                false,
                &mut state.report,
            )
            .await;
            state.report.log_summary();
        }
    }

    // Anything still awaiting convergence has had all the churn it is going
    // to get, so this pass is the verdict.
    if state.rejoins.pending_count() > 0 {
        walk_bar.set_message(format!(
            "final rejoin check ({} pending)",
            state.rejoins.pending_count()
        ));
        rejoin::check_pending(
            &mut state.rejoins,
            &check_ctx,
            args.rejoin_grace_checks,
            true,
            &mut state.report,
        )
        .await;
    }

    let report = state.report;
    walk_bar.finish_with_message(format!("done | {}", report.summary_line()));
    report.log_summary();

    println!("{}", report.summary_line());
    println!("full logs: {}", log_path.display());

    if !report.divergences.is_empty() {
        anyhow::bail!(
            "{} divergence(s) detected during the run",
            report.divergences.len()
        );
    }
    Ok(())
}

/// Watches for Ctrl-C, returning the flag the walk loop checks between rounds.
///
/// The first press asks for a clean stop, which takes effect once the round in
/// flight has finished: aborting a round mid-drain is what leaves a member's
/// group state ahead of its committed queue position, and the fleet is
/// persisted, so the damage outlives the run. A second press gives up on that
/// and exits immediately.
fn watch_for_shutdown() -> Arc<AtomicBool> {
    let shutdown = Arc::new(AtomicBool::new(false));
    tokio::spawn({
        let shutdown = shutdown.clone();
        async move {
            loop {
                if tokio::signal::ctrl_c().await.is_err() {
                    return;
                }
                if shutdown.swap(true, Ordering::Relaxed) {
                    eprintln!("\nsecond Ctrl-C: exiting now, the fleet may be left mid-drain");
                    std::process::exit(130);
                }
                eprintln!("\nCtrl-C: finishing the current round, press again to force");
            }
        }
    });
    shutdown
}

fn walk_progress_bar(args: &Args) -> ProgressBar {
    if args.duration_secs > 0 {
        let bar = ProgressBar::new(args.duration_secs);
        bar.set_style(
            ProgressStyle::with_template(
                "{spinner} [{elapsed_precise}] [{bar:30.cyan/blue}] {pos}/{len}s | {msg}",
            )
            .expect("static progress template is valid")
            .progress_chars("=> "),
        );
        bar
    } else {
        let bar = ProgressBar::new_spinner();
        bar.set_style(
            ProgressStyle::with_template("{spinner} [{elapsed_precise}] {msg}")
                .expect("static progress template is valid"),
        );
        bar.enable_steady_tick(Duration::from_millis(120));
        bar
    }
}

/// Checks a sample of members against the hub. Each check drains its own
/// member and only reads the hub, so the sample is checked concurrently.
/// Self-checks on the walk itself, so a harness regression fails a run
/// instead of silently flattening it.
///
/// Watches a sliding window of recent rounds for two symptoms this harness
/// has actually had:
///
/// * no growth while below the target size (invite starvation: candidates
///   dealt to nobody, budget stuck, mesh broken), and
/// * the same few members driving every round (a degenerate originator
///   shuffle), which confines all activity to their corner of the group.
struct Liveness {
    target_members: usize,
    concurrent_steps: usize,
    /// Member count and originators of the last [`Self::WINDOW`] rounds that
    /// ran steps.
    window: VecDeque<(usize, Vec<UserId>)>,
}

impl Liveness {
    /// Rounds per observation window. Small enough to fire within a minute
    /// of a stall, large enough that a quiet stretch of the weighted step
    /// mix cannot fire it spuriously: at invite weight 8/15 across 4 steps a
    /// round, 30 rounds without a single invite is a chance of roughly
    /// 10^-30.
    const WINDOW: usize = 30;

    fn new(target_members: usize, concurrent_steps: usize) -> Self {
        Self {
            target_members,
            concurrent_steps,
            window: VecDeque::with_capacity(Self::WINDOW),
        }
    }

    fn observe(&mut self, round: &walk::RoundOutcome, report: &mut Report) {
        self.window
            .push_back((round.members, round.originators.clone()));
        if self.window.len() < Self::WINDOW {
            return;
        }

        let (oldest_members, _) = self.window.front().expect("window is full");
        let growth_threshold = self.target_members - self.target_members / 10;
        if round.members < growth_threshold && round.members <= *oldest_members {
            report.record_divergence(format!(
                "walk stalled: {} members after {} rounds below the growth                  threshold of {growth_threshold}, was {oldest_members}",
                round.members,
                Self::WINDOW,
            ));
            self.window.clear();
            return;
        }

        let distinct: HashSet<&UserId> = self
            .window
            .iter()
            .flat_map(|(_, originators)| originators)
            .collect();
        // With uniformly random originators the window draws far more
        // distinct members than steps per round; a degenerate selection
        // yields exactly the per-round step count.
        let expected_min = (self.concurrent_steps + 2).min(round.members);
        if round.members > 2 * self.concurrent_steps && distinct.len() < expected_min {
            report.record_divergence(format!(
                "originator selection degenerated: only {} distinct members                  drove the last {} rounds of a {}-member group",
                distinct.len(),
                Self::WINDOW,
                round.members,
            ));
            self.window.clear();
            return;
        }

        self.window.pop_front();
    }
}

async fn run_verification(
    ctx: &verify::CheckContext<'_>,
    client_ids: &[UserId],
    sample_size: usize,
    rng: &mut ChaCha8Rng,
    report: &mut Report,
) {
    use rand::seq::IteratorRandom;

    let sample_size = sample_size.min(client_ids.len());
    let sample: Vec<(UserId, aircoreclient::clients::CoreUser)> = client_ids
        .iter()
        .sample(rng, sample_size)
        .into_iter()
        .filter_map(|id| {
            ctx.clients_by_id
                .get(id)
                .map(|member| (id.clone(), (*member).clone()))
        })
        .collect();

    // Taken once, from here, so the concurrent checks below never drain the
    // hub themselves.
    let chat_id = ctx.chat_id;
    let view = match verify::hub_view(ctx.hub, chat_id).await {
        Ok(view) => view,
        Err(error) => {
            report.record_divergence(format!("could not read the hub's view: {error}"));
            return;
        }
    };

    let outcomes = parallel::map(sample, ctx.concurrency, move |(member_id, member)| {
        let view = view.clone();
        async move {
            let outcome = verify::check_member(&view, chat_id, &member, &member_id).await;
            (member_id, outcome)
        }
    })
    .await;

    for (member_id, outcome) in outcomes {
        match outcome {
            Ok(Some(divergence)) => report.record_divergence(divergence),
            Ok(None) => {}
            Err(error) => {
                report.record_divergence(format!("verification of {member_id:?} failed: {error}"))
            }
        }
    }
}
