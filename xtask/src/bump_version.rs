// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::fs;

use anyhow::{Context, Result, anyhow, ensure};
use camino::Utf8Path;
use cargo_metadata::MetadataCommand;
use clap::{Args, ValueEnum};
use regex::Regex;
use semver::Version;
use xshell::{Cmd, Shell, cmd};

use crate::util::workspace_root;

#[derive(Args)]
pub(crate) struct BumpArgs {
    /// Which version component to increment.
    #[arg(value_enum, default_value_t = BumpKind::Minor)]
    kind: BumpKind,
    /// Print actions without executing them.
    #[arg(long)]
    dry_run: bool,
    /// Switch to main automatically instead of erroring when on another branch.
    #[arg(long)]
    force: bool,
}

#[derive(ValueEnum, Clone, Copy)]
enum BumpKind {
    Minor,
    Patch,
}

pub(crate) fn run(args: BumpArgs) -> Result<()> {
    let repo_root = workspace_root();
    let shell = Shell::new()?;
    shell.change_dir(repo_root.as_std_path());

    if args.dry_run {
        println!("[dry-run] no commands or file writes will be executed");
    }

    ensure_fresh_main(&shell, args.dry_run, args.force)?;

    let current = determine_current_version(repo_root.as_ref())?;
    let next = match args.kind {
        BumpKind::Minor => increment_minor(&current),
        BumpKind::Patch => increment_patch(&current),
    };

    println!("Bumping version {} -> {}", current, next);
    let next_string = next.to_string();
    run_or_print(cmd!(shell, "cargo set-version {next_string}"), args.dry_run)?;

    update_flutter_version(repo_root.as_ref(), &next, args.dry_run)?;
    println!("Updated Flutter version to {}+1", next);

    update_nfpm_version(repo_root.as_ref(), &next, args.dry_run)?;
    println!("Updated nFPM version to {}", next);

    open_bump_pr(&shell, &next, args.dry_run)?;

    cut_release_branch(&shell, &current, &next, args.dry_run)?;

    Ok(())
}

fn open_bump_pr(shell: &Shell, next: &Version, dry_run: bool) -> Result<()> {
    let branch_name = format!("bump-version/v{next}");
    let commit_message = format!("chore: v{next}");
    println!("Opening pull request {branch_name}");
    run_or_print(cmd!(shell, "git checkout -b {branch_name}"), dry_run)?;
    run_or_print(cmd!(shell, "git commit -am {commit_message}"), dry_run)?;
    run_or_print(cmd!(shell, "git push -u origin {branch_name}"), dry_run)?;
    run_or_print(
        cmd!(
            shell,
            "gh pr create --title {commit_message} --body {commit_message}"
        ),
        dry_run,
    )?;
    Ok(())
}

fn run_or_print(cmd: Cmd<'_>, dry_run: bool) -> Result<()> {
    if dry_run {
        println!("[dry-run] would run: {cmd}");
        Ok(())
    } else {
        cmd.run()?;
        Ok(())
    }
}

fn ensure_fresh_main(shell: &Shell, dry_run: bool, force: bool) -> Result<()> {
    let status = cmd!(shell, "git status --porcelain").read()?;
    ensure!(
        status.is_empty(),
        "Working tree is not clean, commit or stash changes first"
    );

    let current_branch = cmd!(shell, "git rev-parse --abbrev-ref HEAD").read()?;
    if current_branch != "main" {
        ensure!(
            force,
            "Must be on the main branch, currently on {current_branch} (pass --force to switch automatically)"
        );
        println!("Currently on {current_branch}, switching to main");
        run_or_print(cmd!(shell, "git checkout main"), dry_run)?;
    }

    run_or_print(cmd!(shell, "git pull --ff-only origin main"), dry_run)?;

    Ok(())
}

fn cut_release_branch(
    shell: &Shell,
    current: &Version,
    next: &Version,
    dry_run: bool,
) -> Result<()> {
    let release_branch = format!("release/{current}");
    let bump_branch = format!("bump-version/v{next}");
    let title = format!("chore: cut release v{current}");
    println!("Creating release branch {release_branch}");
    run_or_print(cmd!(shell, "git branch {release_branch} main"), dry_run)?;
    run_or_print(cmd!(shell, "git push -u origin {release_branch}"), dry_run)?;
    run_or_print(
        cmd!(
            shell,
            "gh pr create --base {release_branch} --head {bump_branch} --title {title} --body {title}"
        ),
        dry_run,
    )?;
    Ok(())
}

fn determine_current_version(repo_root: &Utf8Path) -> Result<Version> {
    let metadata = MetadataCommand::new()
        .current_dir(repo_root)
        .no_deps()
        .exec()
        .context("Failed to read cargo metadata")?;
    let first_id = metadata
        .workspace_members
        .first()
        .cloned()
        .ok_or_else(|| anyhow!("No workspace members found in cargo metadata output"))?;
    let package = metadata
        .packages
        .iter()
        .find(|pkg| pkg.id == first_id)
        .ok_or_else(|| anyhow!("Could not find metadata for {first_id}"))?;
    Ok(package.version.clone())
}

fn increment_minor(current: &Version) -> Version {
    Version {
        major: current.major,
        minor: current.minor + 1,
        patch: 0,
        pre: current.pre.clone(),
        build: current.build.clone(),
    }
}

fn increment_patch(current: &Version) -> Version {
    Version {
        major: current.major,
        minor: current.minor,
        patch: current.patch + 1,
        pre: current.pre.clone(),
        build: current.build.clone(),
    }
}

fn update_flutter_version(
    repo_root: &Utf8Path,
    new_version: &Version,
    dry_run: bool,
) -> Result<()> {
    let pubspec_path = repo_root.join("app/pubspec.yaml");
    ensure!(
        pubspec_path.exists(),
        "pubspec.yaml not found at {}",
        pubspec_path
    );

    let content = fs::read_to_string(&pubspec_path)
        .with_context(|| format!("Failed to read {}", pubspec_path))?;
    let regex = Regex::new(r"(?m)^version:\s*.+$").expect("valid regex");
    ensure!(
        regex.is_match(&content),
        "Could not locate version line in pubspec.yaml"
    );

    let replacement = format!("version: {}+1", new_version);
    if dry_run {
        println!("[dry-run] would write {pubspec_path} with `{replacement}`");
        return Ok(());
    }
    let updated = regex.replace(&content, replacement).to_string();
    fs::write(&pubspec_path, updated)
        .with_context(|| format!("Failed to write {}", pubspec_path))?;
    Ok(())
}

fn update_nfpm_version(repo_root: &Utf8Path, new_version: &Version, dry_run: bool) -> Result<()> {
    let nfpm_config_path = repo_root.join("app/linux/nfpm.yaml");
    ensure!(
        nfpm_config_path.exists(),
        "nfpm.yaml not found at {}",
        nfpm_config_path
    );

    let content = fs::read_to_string(&nfpm_config_path)
        .with_context(|| format!("Failed to read {}", nfpm_config_path))?;
    let regex = Regex::new(r"(?m)^version:\s*.+$").expect("valid regex");
    ensure!(
        regex.is_match(&content),
        "Could not locate version line in nfpm.yaml"
    );

    let replacement = format!("version: {}", new_version);
    if dry_run {
        println!("[dry-run] would write {nfpm_config_path} with `{replacement}`");
        return Ok(());
    }
    let updated = regex.replace(&content, replacement).to_string();
    fs::write(&nfpm_config_path, updated)
        .with_context(|| format!("Failed to write {}", nfpm_config_path))?;
    Ok(())
}
