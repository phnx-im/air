// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Stress-tests a staging (or local) server by building a fleet of
//! persisted clients around a single hub-owned group and driving it with a
//! random walk of adds, removes, leaves, and messages, while periodically
//! checking sampled members converge with the hub's view.

mod bootstrap;
mod fleet;
mod ops;
mod progress;
mod rejoin;
mod report;
mod verify;
mod walk;

use std::{collections::HashMap, path::PathBuf, time::Duration};

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

    /// Members invited per commit while bootstrapping the group.
    #[arg(long, default_value_t = 20)]
    invite_batch_size: usize,

    /// Peers each member connects to on top of the hub. Adds are
    /// contact-gated, so this is what lets members other than the hub drive
    /// invites. Zero leaves the fleet a pure star.
    #[arg(long, default_value_t = 5)]
    contact_mesh_degree: usize,

    /// Skip the bootstrap pass where every member drains and commits a key
    /// update. That pass costs one commit per member plus the fan-out to
    /// process them, but starts the walk from a fully converged group.
    #[arg(long)]
    no_bootstrap_full_update: bool,

    /// Upper bound on how many targets a single walk operation takes.
    #[arg(long, default_value_t = 5)]
    max_targets: usize,

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

    /// Seed for the walk's RNG. Random if unset; logged either way so a run
    /// can be replayed.
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

    let fleet = Fleet::load_or_create(
        &args.root,
        args.server_url.clone(),
        args.count,
        invitation_codes,
        &multi,
    )
    .await?;

    let mut report = Report::default();

    let bootstrap_params = bootstrap::BootstrapParams {
        chat_title: &args.chat_title,
        invite_batch_size: args.invite_batch_size,
        contact_mesh_degree: args.contact_mesh_degree,
        full_update: !args.no_bootstrap_full_update,
    };
    let chat_id = bootstrap::run(&fleet, &bootstrap_params, &mut report, &multi).await?;

    let hub = &fleet.members[0].user;
    let clients_by_id: HashMap<UserId, &aircoreclient::clients::CoreUser> = fleet.members[1..]
        .iter()
        .map(|member| (member.user.user_id().clone(), &member.user))
        .collect();
    let client_ids: Vec<UserId> = clients_by_id.keys().cloned().collect();

    let ctx = walk::WalkContext {
        hub,
        chat_id,
        clients_by_id: &clients_by_id,
        max_targets: args.max_targets.max(1),
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

    let mut step_count: usize = 0;
    loop {
        if let Some(deadline) = deadline
            && tokio::time::Instant::now() >= deadline
        {
            break;
        }

        tokio::select! {
            _ = walk::step(&ctx, &mut rng, &mut state) => {}
            _ = tokio::signal::ctrl_c() => {
                info!("received Ctrl-C, stopping");
                break;
            }
        }
        step_count += 1;
        state.step = step_count;

        if deadline.is_some() {
            let elapsed_secs = tokio::time::Instant::now()
                .saturating_duration_since(start)
                .as_secs();
            walk_bar.set_position(elapsed_secs.min(args.duration_secs));
        }
        walk_bar.set_message(format!("step {step_count} | {}", state.report.summary_line()));

        if step_count.is_multiple_of(args.verify_every) {
            run_verification(
                hub,
                chat_id,
                &client_ids,
                &clients_by_id,
                args.verify_sample,
                &mut rng,
                &mut state.report,
            )
            .await;
            rejoin::check_pending(
                &mut state.rejoins,
                hub,
                chat_id,
                &clients_by_id,
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
            hub,
            chat_id,
            &clients_by_id,
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

async fn run_verification(
    hub: &aircoreclient::clients::CoreUser,
    chat_id: aircoreclient::ChatId,
    client_ids: &[UserId],
    clients_by_id: &HashMap<UserId, &aircoreclient::clients::CoreUser>,
    sample_size: usize,
    rng: &mut ChaCha8Rng,
    report: &mut Report,
) {
    use rand::seq::IteratorRandom;

    let sample_size = sample_size.min(client_ids.len());
    for member_id in client_ids.iter().sample(rng, sample_size) {
        let Some(member) = clients_by_id.get(member_id) else {
            continue;
        };
        match verify::check_member(hub, chat_id, member, member_id).await {
            Ok(Some(divergence)) => report.record_divergence(divergence),
            Ok(None) => {}
            Err(error) => {
                report.record_divergence(format!("verification of {member_id:?} failed: {error}"))
            }
        }
    }
}
