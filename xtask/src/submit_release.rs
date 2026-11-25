// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::io::IsTerminal;

use anyhow::{Context, Result, bail, ensure};
use clap::ValueEnum;
use semver::Version;
use xshell::{Shell, cmd};

use crate::{
    candidate::{self, SUGGESTED_BUILDS, confirm},
    cut_release::version_at_commit,
    release_notes,
    tag_release::{build_number_offset, release_commit},
    util::workspace_root,
};

#[derive(clap::Args)]
pub(crate) struct SubmitArgs {
    /// Release branch of the build (skips the prompt).
    #[arg(long, value_name = "release/X.Y")]
    branch: Option<String>,
    /// Build number of the release candidate (skips the prompt).
    #[arg(long, value_name = "number")]
    build: Option<u64>,
    /// Stores to submit the build to.
    #[arg(long, value_enum, value_delimiter = ',', default_values_t = Store::ALL)]
    platform: Vec<Store>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
enum Store {
    Android,
    Ios,
    Macos,
}

impl Store {
    const ALL: [Self; 3] = [Self::Android, Self::Ios, Self::Macos];

    /// Where the build goes, as listed before dispatching.
    fn destination(self) -> &'static str {
        match self {
            Self::Android => "Google Play review",
            Self::Ios => "App Store review (iOS)",
            Self::Macos => "App Store review (macOS)",
        }
    }

    /// The `platform` input of `app-submit.yml`.
    fn workflow_input(self) -> &'static str {
        match self {
            Self::Android => "platform=android",
            Self::Ios => "platform=ios",
            Self::Macos => "platform=macos",
        }
    }
}

/// Submits a release candidate to the app stores for review.
///
/// Dispatches the `app-submit.yml` workflow once per store, on the release
/// branch of the build. Without `--branch` and `--build`, it suggests the
/// newest release branches on origin and the latest builds of the chosen
/// branch, and asks before dispatching. An approved build goes live with
/// `cargo xtask publish-release`.
///
/// The stores get the release notes on the tip of the branch. This command
/// shows them first and stops if the branch has none.
pub(crate) fn run(args: SubmitArgs) -> Result<()> {
    let shell = Shell::new()?;
    shell.change_dir(workspace_root().as_std_path());
    let stores = candidate::in_order(&Store::ALL, &args.platform);

    let prompted = args.branch.is_none() || args.build.is_none();
    if prompted && !std::io::stdin().is_terminal() {
        bail!("stdin is not a terminal; pass --branch <release/X.Y> and --build <number>");
    }

    let branch = candidate::choose_branch(&shell, args.branch)?;
    let build = match args.build {
        Some(build) => build,
        None => prompt_build(&shell, &branch)?,
    };

    let tip = candidate::fetch_branch(&shell, &branch)?;
    let version = build_version(&shell, &branch, &tip, build)?;
    let notes = release_notes::at_revision(&shell, &tip, &version)?.with_context(|| {
        let path = release_notes::path(&version);
        format!("{branch} has no {path}; commit the release notes of {version} there first")
    })?;
    println!("Release notes of {version}:\n{notes}\n");

    println!("Build {build} ({version}) of {branch} goes to:");
    for store in &stores {
        println!("  {}", store.destination());
    }
    println!("Once approved, it waits for `just publish-release`.");
    if prompted && !confirm("Submit?")? {
        bail!("aborted");
    }

    let build_field = format!("build={build}");
    for store in &stores {
        let platform_field = store.workflow_input();
        cmd!(
            shell,
            "gh workflow run app-submit.yml --ref {branch} -f {platform_field} -f {build_field}"
        )
        .run()?;
    }
    println!("Dispatched. Follow the runs with:");
    println!(
        "  gh run list --workflow app-submit.yml --limit {}",
        stores.len()
    );
    Ok(())
}

/// Asks for the build number, suggesting the latest release runs of the branch.
fn prompt_build(shell: &Shell, branch: &str) -> Result<u64> {
    let offset = build_number_offset(shell)?;
    let endpoint = format!(
        "repos/{{owner}}/{{repo}}/actions/workflows/app-release.yml/runs\
         ?branch={branch}&per_page={SUGGESTED_BUILDS}"
    );
    let jq = ".workflow_runs[] | [.run_number, .head_sha[0:9], .conclusion // .status, \
              .created_at[0:10], (.head_commit.message | split(\"\\n\")[0])] | @tsv";
    let runs = cmd!(shell, "gh api {endpoint} --jq {jq}").read()?;
    let runs: Vec<ReleaseRun> = runs
        .lines()
        .filter_map(|line| ReleaseRun::parse(line, offset))
        .collect();
    ensure!(!runs.is_empty(), "{branch} has no app-release runs");

    println!("Builds of {branch}:");
    for (i, run) in runs.iter().enumerate() {
        println!(
            "  {}) {}  {}  {}  {}  {}",
            i + 1,
            run.build,
            run.date,
            run.commit,
            run.outcome,
            run.title
        );
    }

    // Only a run that succeeded has uploaded its draft.
    let default = runs.iter().find(|run| run.outcome == "success");
    let builds: Vec<u64> = runs.iter().map(|run| run.build).collect();
    candidate::read_build(&builds, default.map(|run| run.build))
}

/// Reads the version of the build, which the tip of its branch must still have.
///
/// The workflows read the listing and release notes from the tip, and the App
/// Store only takes a build for the version it was built as.
fn build_version(shell: &Shell, branch: &str, tip: &str, build: u64) -> Result<Version> {
    let commit = release_commit(shell, build, Some(branch))?;
    let version = version_at_commit(shell, &commit)?;
    let tip_version = version_at_commit(shell, tip)?;
    ensure!(
        version == tip_version,
        "build {build} is version {version}, but {branch} is at {tip_version} now; \
         submit a build of {tip_version}"
    );
    Ok(version)
}

/// A run of the release workflow, as one line of the suggestion list.
#[derive(Debug, PartialEq)]
struct ReleaseRun {
    build: u64,
    commit: String,
    /// The conclusion of a finished run, or the status of a running one.
    outcome: String,
    date: String,
    title: String,
}

impl ReleaseRun {
    /// Parses a tab-separated line of run number, commit, outcome, date, title.
    fn parse(line: &str, build_number_offset: u64) -> Option<Self> {
        let mut fields = line.split('\t');
        let run_number: u64 = fields.next()?.parse().ok()?;
        Some(Self {
            build: run_number + build_number_offset,
            commit: fields.next()?.to_owned(),
            outcome: fields.next()?.to_owned(),
            date: fields.next()?.to_owned(),
            title: fields.next()?.to_owned(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(clap::Parser)]
    struct Cli {
        #[command(flatten)]
        args: SubmitArgs,
    }

    fn parse_stores(args: &[&str]) -> Vec<Store> {
        use clap::Parser;
        let cli =
            Cli::try_parse_from(std::iter::once("submit-release").chain(args.iter().copied()))
                .expect("valid arguments");
        candidate::in_order(&Store::ALL, &cli.args.platform)
    }

    #[test]
    fn all_stores_by_default() {
        assert_eq!(parse_stores(&[]), Store::ALL);
    }

    #[test]
    fn stores_are_deduplicated_in_dispatch_order() {
        assert_eq!(
            parse_stores(&["--platform", "macos,ios", "--platform", "ios"]),
            [Store::Ios, Store::Macos]
        );
    }

    #[test]
    fn platforms_without_review_are_rejected() {
        use clap::Parser;
        assert!(Cli::try_parse_from(["submit-release", "--platform", "linux"]).is_err());
        assert!(Cli::try_parse_from(["submit-release", "--platform", "windows"]).is_err());
    }

    #[test]
    fn release_run_adds_the_offset_to_the_run_number() {
        let line = "267\tbf93a933b\tsuccess\t2026-11-13\tchore(app): self-group hardening";
        assert_eq!(
            ReleaseRun::parse(line, 1300),
            Some(ReleaseRun {
                build: 1567,
                commit: "bf93a933b".to_owned(),
                outcome: "success".to_owned(),
                date: "2026-11-13".to_owned(),
                title: "chore(app): self-group hardening".to_owned(),
            })
        );
    }

    #[test]
    fn release_run_rejects_malformed_lines() {
        assert_eq!(ReleaseRun::parse("", 1300), None);
        assert_eq!(
            ReleaseRun::parse("abc\tbf93\tsuccess\tdate\ttitle", 1300),
            None
        );
        assert_eq!(ReleaseRun::parse("267\tbf93a933b", 1300), None);
    }
}
