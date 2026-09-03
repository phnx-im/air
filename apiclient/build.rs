// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Collects the git metadata that `src/metadata.rs` reports to the server.

use std::process::Command;

/// Changing this variable forces the metadata to be collected again.
const REFRESH_ENV: &str = "AIR_REFRESH_BUILD_METADATA";

/// Build number of the app, set by CI to the store build number. Defaults to 0
/// when unset (local builds).
const BUILD_NUMBER_ENV: &str = "AIR_BUILD_NUMBER";

/// Used when the git state cannot be determined, e.g. when building from a
/// source archive.
const UNKNOWN_COMMIT_HASH: &str = "0000000000000000000000000000000000000000";

fn main() {
    // Deliberately no `rerun-if-changed` on the sources or on the git state:
    // keeping the metadata in sync with every commit would recompile this
    // crate -- and everything depending on it -- on every build. Set
    // `AIR_REFRESH_BUILD_METADATA` to a value that changes (a commit hash, a
    // build number) to collect it again.
    println!("cargo::rerun-if-changed=build.rs");
    println!("cargo::rerun-if-env-changed={REFRESH_ENV}");
    println!("cargo::rerun-if-env-changed={BUILD_NUMBER_ENV}");

    let commit_hash = git(&["rev-parse", "HEAD"]).unwrap_or_else(|| UNKNOWN_COMMIT_HASH.to_owned());
    let dirty = git(&["status", "--porcelain"]).is_some_and(|status| !status.is_empty());
    let build_number = std::env::var(BUILD_NUMBER_ENV)
        .ok()
        .filter(|value| !value.is_empty())
        .map(|value| {
            value
                .parse::<u64>()
                .unwrap_or_else(|_| panic!("{BUILD_NUMBER_ENV} must be an integer, got {value:?}"))
        })
        .unwrap_or(0);

    println!("cargo::rustc-env=AIR_COMMIT_HASH={commit_hash}");
    println!("cargo::rustc-env=AIR_GIT_DIRTY={dirty}");
    println!("cargo::rustc-env=AIR_BUILD_NUMBER={build_number}");
}

/// Runs `git` with `args` and returns its trimmed stdout, or `None` if git is
/// unavailable or the command fails.
fn git(args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .args(args)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8(output.stdout).ok()?;
    Some(stdout.trim().to_owned())
}
