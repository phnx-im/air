// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::io::IsTerminal;

use anyhow::{Result, bail, ensure};
use clap::ValueEnum;
use semver::Version;
use xshell::{Shell, cmd};

use crate::{
    candidate::{self, SUGGESTED_BUILDS, confirm},
    cut_release::version_at_commit,
    tag_release::release_commit,
    util::workspace_root,
};

/// Submit runs to read for the suggestions, one per store and build.
const SUBMIT_RUNS: usize = 30;

#[derive(clap::Args)]
pub(crate) struct PublishArgs {
    /// Release branch of the build (skips the prompt).
    #[arg(long, value_name = "release/X.Y")]
    branch: Option<String>,
    /// Build number of the approved release candidate (skips the prompt).
    #[arg(long, value_name = "number")]
    build: Option<u64>,
    /// Platforms to publish the build on.
    #[arg(long, value_enum, value_delimiter = ',', default_values_t = Platform::ALL)]
    platform: Vec<Platform>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
enum Platform {
    Ios,
    Macos,
    Linux,
    Android,
}

impl Platform {
    /// Dispatch order. Linux tags the release, and Android is done by hand.
    const ALL: [Self; 4] = [Self::Ios, Self::Macos, Self::Linux, Self::Android];

    /// Where the build goes live, as listed before dispatching.
    fn destination(self) -> &'static str {
        match self {
            Self::Ios => "the App Store (iOS)",
            Self::Macos => "the App Store (macOS)",
            Self::Linux => "the stable Linux repositories, with the release tag",
            Self::Android => "Google Play, by hand in the Play Console",
        }
    }

    /// The `platform` input of `app-store-publish.yml`, for the Apple
    /// platforms.
    fn app_store_input(self) -> Option<&'static str> {
        match self {
            Self::Ios => Some("platform=ios"),
            Self::Macos => Some("platform=macos"),
            Self::Linux | Self::Android => None,
        }
    }
}

/// Publishes an approved release candidate to all users.
///
/// Dispatches `app-store-publish.yml` once per Apple platform and
/// `linux-promote.yml` for Linux, all on the release branch of the build.
/// Google Play holds an approved release through managed publishing, which has
/// no API. Android gets a pointer to the Play Console instead.
///
/// Without `--branch` and `--build`, it suggests the newest release branches
/// on origin and the builds submitted from the chosen branch, and asks before
/// dispatching.
pub(crate) fn run(args: PublishArgs) -> Result<()> {
    let shell = Shell::new()?;
    shell.change_dir(workspace_root().as_std_path());
    let platforms = candidate::in_order(&Platform::ALL, &args.platform);

    let prompted = args.branch.is_none() || args.build.is_none();
    if prompted && !std::io::stdin().is_terminal() {
        bail!("stdin is not a terminal; pass --branch <release/X.Y> and --build <number>");
    }

    let branch = candidate::choose_branch(&shell, args.branch)?;
    let build = match args.build {
        Some(build) => build,
        None => prompt_build(&shell, &branch)?,
    };

    // The fetch makes the commit of the build available. Unlike `submit-release`, we
    // read nothing from the tip, which may be a newer hotfix version by now.
    candidate::fetch_branch(&shell, &branch)?;
    let commit = release_commit(&shell, build, Some(&branch))?;
    let version = version_at_commit(&shell, &commit)?;

    println!("Build {build} ({version}) of {branch} goes live on:");
    for platform in &platforms {
        println!("  {}", platform.destination());
    }
    if prompted && !confirm("Publish?")? {
        bail!("aborted");
    }

    for platform in &platforms {
        dispatch(&shell, *platform, &branch, build, &version)?;
    }
    print_follow_up(&platforms);
    Ok(())
}

fn dispatch(
    shell: &Shell,
    platform: Platform,
    branch: &str,
    build: u64,
    version: &Version,
) -> Result<()> {
    if let Some(platform_field) = platform.app_store_input() {
        let workflow = "app-store-publish.yml";
        let build_field = format!("build={build}");
        cmd!(
            shell,
            "gh workflow run {workflow} --ref {branch} -f {platform_field} -f {build_field}"
        )
        .run()?;
    } else if platform == Platform::Linux {
        let version_field = format!("version={version}-{build}");
        cmd!(
            shell,
            "gh workflow run linux-promote.yml --ref {branch} -f {version_field}"
        )
        .run()?;
    }
    Ok(())
}

fn print_follow_up(platforms: &[Platform]) {
    let mut app_store_runs = 0;
    for platform in platforms {
        if platform.app_store_input().is_some() {
            app_store_runs += 1;
        }
    }
    let linux = platforms.contains(&Platform::Linux);
    if app_store_runs > 0 || linux {
        println!("Dispatched. Follow the runs with:");
    }
    if app_store_runs > 0 {
        println!("  gh run list --workflow app-store-publish.yml --limit {app_store_runs}");
    }
    if linux {
        println!("  gh run list --workflow linux-promote.yml --limit 1");
    }
    if platforms.contains(&Platform::Android) {
        println!("Google Play: publish the approved release in the Play Console,");
        println!("under Publishing overview > Changes ready to publish.");
    }
}

/// Asks for the build number, suggesting the builds submitted from the branch.
fn prompt_build(shell: &Shell, branch: &str) -> Result<u64> {
    let endpoint = format!(
        "repos/{{owner}}/{{repo}}/actions/workflows/app-submit.yml/runs\
         ?branch={branch}&per_page={SUBMIT_RUNS}"
    );
    let jq = ".workflow_runs[] | [.display_title, .conclusion // .status, .created_at[0:10]] \
              | @tsv";
    let runs = cmd!(shell, "gh api {endpoint} --jq {jq}").read()?;
    let builds = submitted_builds(&runs);
    ensure!(
        !builds.is_empty(),
        "{branch} has no submitted builds; pass --build <number>"
    );

    println!("Builds submitted from {branch}:");
    let mut numbers = Vec::new();
    for (i, build) in builds.iter().enumerate() {
        let mut stores = Vec::new();
        for (store, outcome) in &build.stores {
            stores.push(format!("{store} {outcome}"));
        }
        println!(
            "  {}) {}  {}  {}",
            i + 1,
            build.build,
            build.date,
            stores.join(", ")
        );
        numbers.push(build.build);
    }

    candidate::read_build(&numbers, Some(builds[0].build))
}

/// A build sent for review, as one line of the suggestion list.
#[derive(Debug, PartialEq)]
struct SubmittedBuild {
    build: u64,
    /// Date of the newest submission.
    date: String,
    /// Each store and the outcome of its newest submission.
    stores: Vec<(String, String)>,
}

/// Groups tab-separated lines of run title, outcome and date by build.
///
/// The lines come newest first, as the API lists them, and the builds keep
/// that order.
fn submitted_builds(runs: &str) -> Vec<SubmittedBuild> {
    let mut builds: Vec<SubmittedBuild> = Vec::new();
    for line in runs.lines() {
        let mut fields = line.split('\t');
        let (Some(title), Some(outcome), Some(date)) =
            (fields.next(), fields.next(), fields.next())
        else {
            continue;
        };
        let Some((store, build)) = parse_submit_title(title) else {
            continue;
        };
        let entry = (store.to_owned(), outcome.to_owned());
        match builds.iter_mut().find(|known| known.build == build) {
            Some(known) => {
                let seen = known
                    .stores
                    .iter()
                    .any(|(known_store, _)| known_store == store);
                if !seen {
                    known.stores.push(entry);
                }
            }
            None => builds.push(SubmittedBuild {
                build,
                date: date.to_owned(),
                stores: vec![entry],
            }),
        }
    }
    builds.truncate(SUGGESTED_BUILDS);
    builds
}

/// Reads store and build from the run title of `app-submit.yml`, such as
/// "Submit ios build 1567 for review".
fn parse_submit_title(title: &str) -> Option<(&str, u64)> {
    let (store, rest) = title.strip_prefix("Submit ")?.split_once(" build ")?;
    let build = rest.strip_suffix(" for review")?.parse().ok()?;
    Some((store, build))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(clap::Parser)]
    struct Cli {
        #[command(flatten)]
        args: PublishArgs,
    }

    fn parse_platforms(args: &[&str]) -> Vec<Platform> {
        use clap::Parser;
        let cli =
            Cli::try_parse_from(std::iter::once("publish-release").chain(args.iter().copied()))
                .expect("valid arguments");
        candidate::in_order(&Platform::ALL, &cli.args.platform)
    }

    #[test]
    fn all_platforms_by_default() {
        assert_eq!(parse_platforms(&[]), Platform::ALL);
    }

    #[test]
    fn platforms_are_deduplicated_in_dispatch_order() {
        assert_eq!(
            parse_platforms(&["--platform", "android,linux", "--platform", "ios,linux"]),
            [Platform::Ios, Platform::Linux, Platform::Android]
        );
    }

    #[test]
    fn only_apple_platforms_dispatch_the_app_store_workflow() {
        assert_eq!(Platform::Ios.app_store_input(), Some("platform=ios"));
        assert_eq!(Platform::Macos.app_store_input(), Some("platform=macos"));
        assert_eq!(Platform::Linux.app_store_input(), None);
        assert_eq!(Platform::Android.app_store_input(), None);
    }

    #[test]
    fn submit_title_gives_store_and_build() {
        assert_eq!(
            parse_submit_title("Submit ios build 1567 for review"),
            Some(("ios", 1567))
        );
        assert_eq!(parse_submit_title("Submit ios build 1567"), None);
        assert_eq!(parse_submit_title("Submit ios build x for review"), None);
        assert_eq!(parse_submit_title("Release app"), None);
    }

    #[test]
    fn submit_title_matches_the_run_name_of_the_workflow() {
        let path = workspace_root().join(".github/workflows/app-submit.yml");
        let workflow = std::fs::read_to_string(path).expect("app-submit.yml is readable");
        let run_name = workflow
            .lines()
            .find_map(|line| line.strip_prefix("run-name: "))
            .expect("app-submit.yml has a run-name");
        let title = run_name
            .replace("${{ inputs.platform }}", "ios")
            .replace("${{ inputs.build }}", "1567");
        assert_eq!(parse_submit_title(&title), Some(("ios", 1567)));
    }

    #[test]
    fn submitted_builds_group_stores_newest_first() {
        let runs = "\
Submit macos build 1570 for review\tin_progress\t2026-11-26
Submit ios build 1567 for review\tsuccess\t2026-11-25
Submit android build 1567 for review\tsuccess\t2026-11-25
Submit ios build 1567 for review\tfailure\t2026-11-24
Submit ios build 1560 for review\tsuccess\t2026-11-20
Release app\tsuccess\t2026-11-19
Submit ios build 1559 for review
";
        let entry = |store: &str, outcome: &str| (store.to_owned(), outcome.to_owned());
        assert_eq!(
            submitted_builds(runs),
            [
                SubmittedBuild {
                    build: 1570,
                    date: "2026-11-26".to_owned(),
                    stores: vec![entry("macos", "in_progress")],
                },
                SubmittedBuild {
                    build: 1567,
                    date: "2026-11-25".to_owned(),
                    stores: vec![entry("ios", "success"), entry("android", "success")],
                },
                SubmittedBuild {
                    build: 1560,
                    date: "2026-11-20".to_owned(),
                    stores: vec![entry("ios", "success")],
                },
            ]
        );
    }

    #[test]
    fn submitted_builds_are_limited() {
        let mut runs = String::new();
        for build in 0..10 {
            runs.push_str(&format!(
                "Submit ios build {build} for review\tsuccess\t2026-11-25\n"
            ));
        }
        assert_eq!(submitted_builds(&runs).len(), SUGGESTED_BUILDS);
    }
}
