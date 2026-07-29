// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::io::{BufRead, IsTerminal, Write};

use anyhow::{Context, Result, bail, ensure};
use semver::Version;
use xshell::{Shell, cmd};

use crate::{
    bump_version::{self, Bump},
    util::workspace_root,
};

#[derive(clap::Args)]
pub(crate) struct CutArgs {
    /// Commit to cut the release branch at (defaults to main's HEAD).
    commit: Option<String>,
    /// Next version on main after the cut (skips the prompt).
    #[arg(long, value_name = "major|minor|patch|X.Y.Z")]
    next: Option<NextVersion>,
}

/// A bump kind or an explicit version.
#[derive(Clone)]
enum NextVersion {
    Bump(Bump),
    Version(Version),
}

impl std::str::FromStr for NextVersion {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        match s {
            "major" => Ok(Self::Bump(Bump::Major)),
            "minor" => Ok(Self::Bump(Bump::Minor)),
            "patch" => Ok(Self::Bump(Bump::Patch)),
            _ => Version::parse(s)
                .map(Self::Version)
                .with_context(|| format!("expected major, minor, patch or a version: {s}")),
        }
    }
}

/// Cuts a `release/0.X` branch at the given commit (defaults to the current
/// `main` HEAD) and commits the version bump on a `merge-release/0.X`
/// branch that merges the release branch back into main via a PR, asking
/// whether the next version is a major, minor or patch increment or a
/// custom version. Store builds are then published from the release branch,
/// while `main` continues to produce internal builds under the next
/// version.
pub(crate) fn run(args: CutArgs) -> Result<()> {
    let repo_root = workspace_root();
    let shell = Shell::new()?;
    shell.change_dir(repo_root.as_std_path());

    let branch = cmd!(shell, "git rev-parse --abbrev-ref HEAD").read()?;
    ensure!(
        branch.trim() == "main",
        "cut-release must be run on main (currently on: {})",
        branch.trim()
    );
    let status = cmd!(shell, "git status --porcelain").read()?;
    ensure!(
        status.trim().is_empty(),
        "cut-release requires a clean working tree"
    );

    let commit = args.commit.as_deref().unwrap_or("HEAD");
    let spec = format!("{commit}^{{commit}}");
    let commit = cmd!(shell, "git rev-parse --verify --quiet {spec}")
        .read()
        .with_context(|| format!("{commit} is not a commit"))?;
    let commit = commit.trim().to_owned();
    cmd!(shell, "git merge-base --is-ancestor {commit} HEAD")
        .quiet()
        .run()
        .with_context(|| format!("{commit} is not on main"))?;

    let current = bump_version::determine_current_version(repo_root.as_ref())?;
    let at_commit = version_at_commit(&shell, &commit)?;
    ensure!(
        at_commit == current,
        "version at {commit} is {at_commit}, but main is at {current}; \
         the commit must be from the current release cycle"
    );

    let release_branch = format!("release/{}.{}", current.major, current.minor);
    let next = match args.next {
        Some(NextVersion::Bump(bump)) => bump_version::increment(&current, bump),
        Some(NextVersion::Version(version)) => version,
        None => prompt_next(&release_branch, &current)?,
    };
    ensure!(
        next > current,
        "next version {next} must be greater than {current}"
    );

    cmd!(shell, "git branch {release_branch} {commit}").run()?;
    println!("Created branch {release_branch} at {commit}");

    // The version bump lands via a PR that merges the release branch back into
    // main. The bump commit sits on top of the release branch tip; merging the
    // PR makes the release branch an ancestor of main.
    let merge_branch = format!("merge-{release_branch}");
    cmd!(shell, "git switch --create {merge_branch} {release_branch}").run()?;
    println!("Bumping version {current} -> {next}");
    bump_version::set_version(&next)?;
    // Sync the workspace members' entries in Cargo.lock
    cmd!(shell, "cargo update --workspace --offline").run()?;
    let message = format!("chore: bump version to {next}");
    cmd!(shell, "git commit --all --message {message}").run()?;

    let title = format!("chore: merge {release_branch} back into main, bump to {next}");
    println!();
    println!("Cut {release_branch} ({current}); {merge_branch} merges it back into");
    println!("main and bumps main to {next}.");
    println!("Next steps:");
    println!("  git push origin {release_branch}");
    println!("  git push -u origin {merge_branch}");
    println!("  gh pr create --title {title:?}");
    println!("Merge the PR with a merge commit (not squash) so {release_branch}");
    println!("becomes an ancestor of main. Merge hotfixes on {release_branch} back");
    println!("the same way (-X ours keeps main's own version numbers on conflict).");
    Ok(())
}

/// Asks which version main should carry after the cut, suggesting the next
/// major, minor and patch versions and accepting a custom version. Defaults
/// to minor.
fn prompt_next(release_branch: &str, current: &Version) -> Result<Version> {
    let stdin = std::io::stdin();
    if !stdin.is_terminal() {
        bail!("stdin is not a terminal; pass --next <major|minor|patch|X.Y.Z>");
    }

    let choices = [
        ("major", Bump::Major),
        ("minor", Bump::Minor),
        ("patch", Bump::Patch),
    ];

    println!("Cutting {release_branch} ({current}). Next version on main:");
    for (i, (name, bump)) in choices.iter().enumerate() {
        let next = bump_version::increment(current, *bump);
        println!("  {}) {next} ({name})", i + 1);
    }
    println!("or enter a custom version.");

    let mut lines = stdin.lock().lines();
    loop {
        print!("Choice [minor]: ");
        std::io::stdout().flush()?;
        let line = lines.next().context("stdin closed")??;
        let input = line.trim();
        if input.is_empty() {
            return Ok(bump_version::increment(current, Bump::Minor));
        }
        let choice = choices
            .iter()
            .enumerate()
            .find(|(i, (name, _))| input == (i + 1).to_string() || input == *name);
        if let Some((_, (_, bump))) = choice {
            return Ok(bump_version::increment(current, *bump));
        }
        match Version::parse(input) {
            Ok(version) if version > *current => return Ok(version),
            Ok(version) => println!("{version} is not greater than {current}"),
            Err(_) => println!("Invalid choice: {input}"),
        }
    }
}

/// Reads the workspace version at the given commit. All workspace members
/// share the same version, so any member's manifest works.
fn version_at_commit(shell: &Shell, commit: &str) -> Result<Version> {
    let spec = format!("{commit}:common/Cargo.toml");
    let manifest = cmd!(shell, "git show {spec}")
        .read()
        .with_context(|| format!("failed to read common/Cargo.toml at {commit}"))?;
    let version = manifest
        .lines()
        .find_map(|line| line.strip_prefix("version = \""))
        .and_then(|rest| rest.strip_suffix('"'))
        .with_context(|| format!("no version found in common/Cargo.toml at {commit}"))?;
    Version::parse(version).with_context(|| format!("invalid version at {commit}: {version}"))
}
