// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::fs;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use xshell::{Shell, cmd};

use crate::util::workspace_root;

/// Our own crates that are published and hence not caught by the `private` filter in `about.toml`.
const OWN_CRATES: &[&str] = &["apqmls", "mimi_content", "mimi-room-policy", "mls-assist"];

const OUTPUT: &str = "app/assets/licenses/rust_licenses.json";

#[derive(clap::Args)]
pub(crate) struct GenerateLicensesArgs {}

#[derive(Serialize, Deserialize)]
struct Licenses {
    licenses: Vec<License>,
}

#[derive(Serialize, Deserialize)]
struct License {
    id: String,
    name: String,
    text: String,
    used_by: Vec<Crate>,
}

#[derive(Serialize, Deserialize)]
struct Crate {
    name: String,
    version: String,
    repository: Option<String>,
}

/// Collects the licenses of the Rust dependencies shipped in the app with
/// `cargo about` and writes them as JSON asset.
///
/// Runs offline, so that the output does not depend on license files fetched
/// from the crates' git repositories and is reproducible in CI.
///
// Note: This command only exists because at the time of writing, the `cargo
// about` command does not support filtering specific crates.
pub(crate) fn run(_args: GenerateLicensesArgs) -> Result<()> {
    let repo_root = workspace_root();
    let shell = Shell::new()?;
    shell.change_dir(repo_root.as_std_path());

    cmd!(shell, "cargo fetch").run()?;
    let json = cmd!(
        shell,
        "cargo about generate --offline -c about.toml -m applogic/Cargo.toml about.hbs"
    )
    .read()
    .context("cargo about failed (is cargo-about installed?)")?;

    let mut licenses: Licenses =
        serde_json::from_str(&json).context("failed to parse cargo about output")?;
    for license in &mut licenses.licenses {
        license
            .used_by
            .retain(|krate| !OWN_CRATES.contains(&krate.name.as_str()));
    }
    licenses
        .licenses
        .retain(|license| !license.used_by.is_empty());

    let mut out = serde_json::to_string_pretty(&licenses)?;
    out.push('\n');
    let path = repo_root.join(OUTPUT);
    fs::write(&path, out).with_context(|| format!("failed to write {path}"))?;
    println!("Wrote {path}");
    Ok(())
}
