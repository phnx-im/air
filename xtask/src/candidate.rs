// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Picks the release candidate that `submit-release` and `publish-release` act
//! on.

use std::io::{BufRead, Write};

use anyhow::{Result, ensure};
use xshell::{Shell, cmd};

const SUGGESTED_BRANCHES: usize = 3;
pub(crate) const SUGGESTED_BUILDS: usize = 5;

/// Returns the given release branch, or asks for one when none is given.
pub(crate) fn choose_branch(shell: &Shell, branch: Option<String>) -> Result<String> {
    let branch = match branch {
        Some(branch) => branch,
        None => prompt_branch(shell)?,
    };
    ensure!(
        release_version(&branch).is_some(),
        "{branch} is not a release branch, expected release/X.Y"
    );
    Ok(branch)
}

/// Asks for the release branch, suggesting the newest ones on origin.
fn prompt_branch(shell: &Shell) -> Result<String> {
    let heads = cmd!(shell, "git ls-remote --heads origin refs/heads/release/*").read()?;
    let branches = newest_release_branches(&heads);
    ensure!(!branches.is_empty(), "origin has no release branches");

    println!("Release branch of the build:");
    for (i, branch) in branches.iter().enumerate() {
        println!("  {}) {branch}", i + 1);
    }
    println!("or enter another branch.");

    let default = &branches[0];
    loop {
        let input = read_answer(&format!("Choice [{default}]: "))?;
        let branch = match pick(&input, &branches) {
            Some(branch) => branch.clone(),
            None if input.is_empty() => default.clone(),
            None => input,
        };
        if release_version(&branch).is_some() {
            return Ok(branch);
        }
        println!("{branch} is not a release branch, expected release/X.Y");
    }
}

/// Fetches the branch from origin and returns its tip.
pub(crate) fn fetch_branch(shell: &Shell, branch: &str) -> Result<String> {
    let head = format!("refs/heads/{branch}");
    cmd!(shell, "git fetch --quiet origin {head}").run()?;
    let tip = cmd!(shell, "git rev-parse --verify FETCH_HEAD").read()?;
    Ok(tip.trim().to_owned())
}

/// Extracts the release branches from `git ls-remote` output, newest first.
fn newest_release_branches(heads: &str) -> Vec<String> {
    let mut branches = Vec::new();
    for line in heads.lines() {
        let Some((_, head)) = line.split_once("refs/heads/") else {
            continue;
        };
        if let Some(version) = release_version(head) {
            branches.push((version, head.to_owned()));
        }
    }
    branches.sort_by_key(|(version, _)| std::cmp::Reverse(*version));
    branches.truncate(SUGGESTED_BRANCHES);
    branches.into_iter().map(|(_, branch)| branch).collect()
}

/// Parses the major and minor version of a `release/X.Y` branch.
fn release_version(branch: &str) -> Option<(u64, u64)> {
    let (major, minor) = branch.strip_prefix("release/")?.split_once('.')?;
    Some((major.parse().ok()?, minor.parse().ok()?))
}

/// Returns the selected items in the order of `all`, without duplicates.
pub(crate) fn in_order<T: Copy + PartialEq>(all: &[T], selected: &[T]) -> Vec<T> {
    let mut ordered = Vec::new();
    for item in all {
        if selected.contains(item) {
            ordered.push(*item);
        }
    }
    ordered
}

/// Asks for a build number, as a number of the listed `builds` or typed out.
///
/// An empty answer takes the default, when there is one.
pub(crate) fn read_build(builds: &[u64], default: Option<u64>) -> Result<u64> {
    println!("or enter another build number.");
    let question = match default {
        Some(build) => format!("Choice [{build}]: "),
        None => "Choice: ".to_owned(),
    };
    loop {
        let input = read_answer(&question)?;
        if input.is_empty()
            && let Some(build) = default
        {
            return Ok(build);
        }
        if let Some(build) = pick(&input, builds) {
            return Ok(*build);
        }
        match input.parse() {
            Ok(build) => return Ok(build),
            Err(_) => println!("Invalid choice: {input}"),
        }
    }
}

/// Resolves a list number typed at a prompt to its choice.
fn pick<'a, T>(input: &str, choices: &'a [T]) -> Option<&'a T> {
    let number: usize = input.parse().ok()?;
    choices.get(number.checked_sub(1)?)
}

pub(crate) fn confirm(question: &str) -> Result<bool> {
    let answer = read_answer(&format!("{question} [y/N]: "))?;
    Ok(answer.eq_ignore_ascii_case("y") || answer.eq_ignore_ascii_case("yes"))
}

pub(crate) fn read_answer(question: &str) -> Result<String> {
    print!("{question}");
    std::io::stdout().flush()?;
    let mut line = String::new();
    let read = std::io::stdin().lock().read_line(&mut line)?;
    ensure!(read > 0, "stdin closed");
    Ok(line.trim().to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn release_branches_are_sorted_by_version_and_limited() {
        let heads = "\
a610\trefs/heads/release/0.9
19fd\trefs/heads/release/0.23
b02d\trefs/heads/release/0.100
ce95\trefs/heads/release/1.0
bf93\trefs/heads/release/next
";
        assert_eq!(
            newest_release_branches(heads),
            ["release/1.0", "release/0.100", "release/0.23"]
        );
    }

    #[test]
    fn no_release_branches() {
        assert!(newest_release_branches("").is_empty());
        assert!(newest_release_branches("a610\trefs/heads/main\n").is_empty());
    }

    #[test]
    fn release_version_rejects_other_branches() {
        assert_eq!(release_version("release/0.26"), Some((0, 26)));
        assert_eq!(release_version("main"), None);
        assert_eq!(release_version("release/0.26.1"), None);
        assert_eq!(release_version("merge-release/0.26"), None);
    }

    #[test]
    fn choose_branch_rejects_other_branches() {
        let shell = Shell::new().expect("shell");
        assert_eq!(
            choose_branch(&shell, Some("release/0.26".to_owned())).expect("release branch"),
            "release/0.26"
        );
        assert!(choose_branch(&shell, Some("main".to_owned())).is_err());
    }

    #[test]
    fn in_order_follows_the_full_list_and_drops_duplicates() {
        assert_eq!(in_order(&[1, 2, 3], &[3, 1, 3]), [1, 3]);
        assert_eq!(in_order(&[1, 2, 3], &[]), [] as [i32; 0]);
    }

    #[test]
    fn pick_resolves_list_numbers_only() {
        let builds = [1567, 1560];
        assert_eq!(pick("1", &builds), Some(&1567));
        assert_eq!(pick("2", &builds), Some(&1560));
        assert_eq!(pick("0", &builds), None);
        assert_eq!(pick("3", &builds), None);
        assert_eq!(pick("1567", &builds), None);
        assert_eq!(pick("", &builds), None);
    }
}
