// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use anyhow::{Context, Result, bail, ensure};
use semver::Version;
use xshell::{Shell, cmd};

use crate::{cut_release::version_at_commit, util::workspace_root};

#[derive(clap::Args)]
pub(crate) struct TagArgs {
    /// Released build, as <version>-<build number> (e.g. "0.23.0-1490").
    #[arg(long)]
    version: String,
    /// Only check that the build can be tagged, don't create the tag.
    #[arg(long)]
    check: bool,
}

/// Tags the commit of a released build as `v<version>`.
///
/// The commit is the one the `app-release.yml` run of the build number was
/// started for, and it must carry the version. An existing tag on the same
/// commit is fine, on another commit it is an error.
pub(crate) fn run(args: TagArgs) -> Result<()> {
    let shell = Shell::new()?;
    shell.change_dir(workspace_root().as_std_path());

    let (version, build) = args
        .version
        .rsplit_once('-')
        .with_context(|| format!("expected <version>-<build number>: {}", args.version))?;
    let version = Version::parse(version).with_context(|| format!("invalid version: {version}"))?;
    let build: u64 = build
        .parse()
        .with_context(|| format!("invalid build number: {build}"))?;

    let commit = release_commit(&shell, build)?;
    println!("Build {build} is commit {commit}");

    let at_commit = version_at_commit(&shell, &commit)?;
    ensure!(
        at_commit == version,
        "build {build} ({commit}) is version {at_commit}, not {version}"
    );

    let tag = format!("v{version}");
    let tag_spec = format!("refs/tags/{tag}^{{commit}}");
    let existing = cmd!(shell, "git rev-parse --verify --quiet {tag_spec}")
        .ignore_status()
        .read()?;
    match existing.trim() {
        "" => {}
        tagged if tagged == commit => {
            println!("{tag} already tags {commit}");
            return Ok(());
        }
        tagged => bail!("{tag} already tags {tagged}, bump the patch version to release {commit}"),
    }

    if args.check {
        println!("{commit} can be tagged {tag}");
        return Ok(());
    }

    let message = format!("{tag} (build {build})");
    cmd!(
        shell,
        "git tag --annotate {tag} {commit} --message {message}"
    )
    .run()?;
    let tag_ref = format!("refs/tags/{tag}");
    cmd!(shell, "git push origin {tag_ref}").run()?;
    println!("Tagged {commit} as {tag}");
    Ok(())
}

/// Finds the commit the `app-release.yml` run of the build number built.
fn release_commit(shell: &Shell, build: u64) -> Result<String> {
    // Inverse of scripts/build-number.sh.
    let offset: u64 = cmd!(shell, "bash scripts/build-number.sh")
        .env("GITHUB_RUN_NUMBER", "0")
        .read()?
        .trim()
        .parse()
        .context("invalid build number offset")?;
    let run_number = build
        .checked_sub(offset)
        .with_context(|| format!("build {build} is below the offset {offset}"))?;

    let endpoint = "repos/{owner}/{repo}/actions/workflows/app-release.yml/runs?per_page=100";
    let jq = format!(".workflow_runs[] | select(.run_number == {run_number}) | .head_sha");
    let commit = cmd!(shell, "gh api --paginate {endpoint} --jq {jq}").read()?;
    let commit = commit.trim();
    ensure!(
        !commit.is_empty(),
        "no app-release run {run_number} (build {build})"
    );
    Ok(commit.to_owned())
}
