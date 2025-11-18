// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::fs;

use anyhow::{Context, Result, anyhow, bail, ensure};
use camino::Utf8Path;
use cargo_metadata::MetadataCommand;
use regex::Regex;
use semver::Version;
use xshell::{Shell, cmd};

use crate::util::workspace_root;

pub(crate) fn run() -> Result<()> {
    let repo_root = workspace_root();
    let shell = Shell::new()?;
    shell.change_dir(repo_root.as_std_path());

    let current = determine_current_version(repo_root.as_ref())?;
    let next = increment_minor(&current);
    println!("Bumping version {} -> {}", current, next);

    cmd!(shell, "cargo set-version --version").quiet().run()?;
    cmd!(shell, "git-cliff --version").quiet().run()?;

    let next_string = next.to_string();
    cmd!(shell, "cargo set-version --workspace {next_string}").run()?;

    update_flutter_version(repo_root.as_ref(), &next)?;
    println!("Updated Flutter version to {}+1", next);

    let changelog_section = cmd!(shell, "git-cliff --unreleased --tag v{next_string}").read()?;
    let trimmed = changelog_section.trim_end();
    if trimmed.is_empty() {
        bail!("git-cliff produced empty output for v{next}");
    }
    prepend_changelog(repo_root.as_ref(), trimmed)?;
    println!("Prepended changelog section for v{next}");

    create_tag(&shell, &next)?;
    println!("Created git tag v{next}");

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

fn update_flutter_version(repo_root: &Utf8Path, new_version: &Version) -> Result<()> {
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
    let updated = regex.replace(&content, replacement).to_string();
    fs::write(&pubspec_path, updated)
        .with_context(|| format!("Failed to write {}", pubspec_path))?;
    Ok(())
}

fn prepend_changelog(repo_root: &Utf8Path, new_section: &str) -> Result<()> {
    let changelog_path = repo_root.join("CHANGELOG.md");
    ensure!(
        changelog_path.exists(),
        "CHANGELOG.md not found at {}",
        changelog_path
    );

    let previous = fs::read_to_string(&changelog_path)?;
    let mut buffer = String::new();
    buffer.push_str(new_section);
    buffer.push_str("\n\n");
    buffer.push_str(&previous);
    fs::write(&changelog_path, buffer)
        .with_context(|| format!("Failed to write {}", changelog_path))?;
    Ok(())
}

fn create_tag(shell: &Shell, version: &Version) -> Result<()> {
    let tag_name = format!("v{}", version);
    let existing = cmd!(shell, "git tag --list {tag_name}").read()?;
    ensure!(
        existing.trim().is_empty(),
        "Tag {} already exists",
        tag_name
    );

    cmd!(shell, "git tag {tag_name}").run()?;
    Ok(())
}
