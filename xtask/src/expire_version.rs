// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::fs;

use anyhow::{Context, Result, ensure};
use camino::Utf8Path;
use chrono::{DateTime, Duration, SecondsFormat, Utc};
use semver::Version;
use serde::Deserialize;

use crate::util::workspace_root;

const EXPIRATIONS_FILE: &str = "server/configuration/version_expirations.yaml";

/// Entries expired for longer than this are pruned, provided a newer expired
/// entry still covers their versions.
const PRUNE_AFTER_DAYS: i64 = 90;

const FILE_HEADER: &str = "\
# SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
#
# SPDX-License-Identifier: AGPL-3.0-or-later

# Client version expirations. All client versions below `before` expire at
# `expires_at` (the earliest matching entry wins); expired clients are
# blocked by the server.
#
# Managed by `just expire-version` and the version-expiry CI workflow.
";

#[derive(clap::Args)]
pub(crate) struct ExpireArgs {
    /// All client versions below this version expire (plain X.Y.Z).
    version: Version,
    /// Number of days from now until the versions expire.
    #[arg(long, default_value_t = 15)]
    in_days: u16,
}

#[derive(Deserialize)]
struct ConfigFile {
    application: ApplicationSection,
}

#[derive(Deserialize)]
struct ApplicationSection {
    #[serde(default)]
    version_expirations: Vec<Entry>,
}

#[derive(Deserialize, Clone)]
struct Entry {
    before: Version,
    expires_at: DateTime<Utc>,
}

pub(crate) fn run(args: ExpireArgs) -> Result<()> {
    ensure!(
        args.version.pre.is_empty() && args.version.build.is_empty(),
        "version must be a plain X.Y.Z version"
    );

    let path = workspace_root().join(EXPIRATIONS_FILE);
    let now = Utc::now();
    let expires_at = now + Duration::days(args.in_days.into());

    let mut entries = read_entries(&path)?;
    let expires_at = upsert(&mut entries, &args.version, expires_at);
    prune(&mut entries, now);
    entries.sort_by(|a, b| a.before.cmp(&b.before));
    write_entries(&path, &entries)?;

    println!(
        "Client versions below {} expire at {} ({})",
        args.version,
        format_time(&expires_at),
        path,
    );
    Ok(())
}

/// Adds an entry for `before` and returns the effective expiry. If an entry
/// already exists, the earlier `expires_at` wins, so that re-runs never
/// extend an expiry.
fn upsert(entries: &mut Vec<Entry>, before: &Version, expires_at: DateTime<Utc>) -> DateTime<Utc> {
    match entries.iter_mut().find(|entry| &entry.before == before) {
        Some(entry) => {
            if entry.expires_at <= expires_at {
                println!(
                    "Versions below {} already expire at {}; keeping the earlier expiry",
                    before,
                    format_time(&entry.expires_at)
                );
            } else {
                entry.expires_at = expires_at;
            }
            entry.expires_at
        }
        None => {
            entries.push(Entry {
                before: before.clone(),
                expires_at,
            });
            expires_at
        }
    }
}

/// Removes entries that expired more than [`PRUNE_AFTER_DAYS`] ago and whose
/// versions are still covered by another already-expired entry.
fn prune(entries: &mut Vec<Entry>, now: DateTime<Utc>) {
    let prune_before = now - Duration::days(PRUNE_AFTER_DAYS);
    let snapshot = entries.clone();
    entries.retain(|entry| {
        entry.expires_at >= prune_before
            || !snapshot
                .iter()
                .any(|other| other.before > entry.before && other.expires_at <= now)
    });
}

fn read_entries(path: &Utf8Path) -> Result<Vec<Entry>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let config = config::Config::builder()
        .add_source(config::File::from(path.as_std_path()))
        .build()
        .with_context(|| format!("Failed to read {path}"))?;
    let file: ConfigFile = config
        .try_deserialize()
        .with_context(|| format!("Failed to parse {path}"))?;
    Ok(file.application.version_expirations)
}

fn write_entries(path: &Utf8Path, entries: &[Entry]) -> Result<()> {
    let mut out = String::from(FILE_HEADER);
    if entries.is_empty() {
        out.push_str("application:\n  version_expirations: []\n");
        return fs::write(path, out).with_context(|| format!("Failed to write {path}"));
    }
    out.push_str("application:\n  version_expirations:\n");
    for entry in entries {
        out.push_str(&format!(
            "    - before: \"{}\"\n      expires_at: \"{}\"\n",
            entry.before,
            format_time(&entry.expires_at),
        ));
    }
    fs::write(path, out).with_context(|| format!("Failed to write {path}"))?;
    Ok(())
}

fn format_time(time: &DateTime<Utc>) -> String {
    time.to_rfc3339_opts(SecondsFormat::Secs, true)
}
