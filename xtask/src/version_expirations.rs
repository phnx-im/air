// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::fs;

use anyhow::{Context, Result};
use askama::Template;
use camino::Utf8Path;
use chrono::{DateTime, NaiveDate, NaiveTime, Utc};
use semver::Version;
use serde::Deserialize;

use crate::util::workspace_root;

const EXPIRATIONS_FILE: &str = "server/configuration/version_expirations.yaml";

/// Render context for `templates/version_expirations.yaml.jinja`.
#[derive(Template)]
#[template(path = "version_expirations.yaml.jinja", escape = "none")]
struct ExpirationsTemplate {
    entries: Vec<TemplateEntry>,
}

struct TemplateEntry {
    older_than: String,
    expires_on: String,
}

#[derive(Debug, Clone, Deserialize)]
struct Entry {
    older_than: Version,
    #[serde(deserialize_with = "date_or_datetime")]
    expires_on: DateTime<Utc>,
}

/// Accepts a date `YYYY-MM-DD` (midnight UTC) or an RFC 3339 timestamp.
///
/// Mirrors the deserializer in `airbackend::settings`.
fn date_or_datetime<'de, D>(d: D) -> Result<DateTime<Utc>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s: String = serde::Deserialize::deserialize(d)?;
    if let Ok(date) = s.parse::<NaiveDate>() {
        return Ok(date.and_time(NaiveTime::MIN).and_utc());
    }
    DateTime::parse_from_rfc3339(&s)
        .map(|dt| dt.to_utc())
        .map_err(serde::de::Error::custom)
}

#[derive(Deserialize)]
struct ConfigFile {
    application: Application,
}

#[derive(Deserialize)]
struct Application {
    #[serde(default)]
    version_expirations: Vec<Entry>,
}

/// Schedules all client versions below `older_than` to expire at
/// `expires_on`.
///
/// If an entry for `older_than` already exists, the earlier expiry wins, so
/// re-runs never extend an expiry. Returns the effective expiry.
pub(crate) fn schedule_expiry(
    older_than: &Version,
    expires_on: DateTime<Utc>,
) -> Result<DateTime<Utc>> {
    let path = workspace_root().join(EXPIRATIONS_FILE);
    schedule_expiry_at(&path, older_than, expires_on)
}

fn schedule_expiry_at(
    path: &Utf8Path,
    older_than: &Version,
    expires_on: DateTime<Utc>,
) -> Result<DateTime<Utc>> {
    let mut entries = read_entries(path)?;
    let effective = match entries
        .iter_mut()
        .find(|entry| &entry.older_than == older_than)
    {
        Some(entry) => {
            entry.expires_on = entry.expires_on.min(expires_on);
            entry.expires_on
        }
        None => {
            entries.push(Entry {
                older_than: older_than.clone(),
                expires_on,
            });
            expires_on
        }
    };
    entries.sort_unstable_by(|a, b| a.older_than.cmp(&b.older_than));
    write_entries(path, &entries)?;
    Ok(effective)
}

fn read_entries(path: &Utf8Path) -> Result<Vec<Entry>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let file: ConfigFile = config::Config::builder()
        .add_source(config::File::from(path.as_std_path()))
        .build()
        .and_then(config::Config::try_deserialize)
        .with_context(|| format!("failed to read {path}"))?;
    Ok(file.application.version_expirations)
}

fn write_entries(path: &Utf8Path, entries: &[Entry]) -> Result<()> {
    let template = ExpirationsTemplate {
        entries: entries
            .iter()
            .map(|entry| TemplateEntry {
                older_than: entry.older_than.to_string(),
                // date-only: a hand-edited timestamp is truncated to midnight
                // UTC of its day, which only ever tightens an expiry
                expires_on: entry.expires_on.format("%Y-%m-%d").to_string(),
            })
            .collect(),
    };
    // askama drops the template's trailing newline
    let mut out = template.render()?;
    out.push('\n');
    fs::write(path, out).with_context(|| format!("failed to write {path}"))
}

#[cfg(test)]
mod tests {
    use camino::Utf8PathBuf;

    use super::*;

    fn temp_path(name: &str) -> Utf8PathBuf {
        let dir = Utf8PathBuf::from_path_buf(std::env::temp_dir()).unwrap();
        let path = dir.join(format!(
            "xtask-version-expirations-{name}-{}.yaml",
            std::process::id()
        ));
        let _ = fs::remove_file(&path);
        path
    }

    fn version(s: &str) -> Version {
        Version::parse(s).unwrap()
    }

    fn time(s: &str) -> DateTime<Utc> {
        s.parse().unwrap()
    }

    #[test]
    fn creates_missing_file() {
        let path = temp_path("creates-missing-file");
        let expires_on = time("2026-09-02T00:00:00Z");
        let effective = schedule_expiry_at(&path, &version("0.20.0"), expires_on).unwrap();
        assert_eq!(effective, expires_on);

        let entries = read_entries(&path).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].older_than, version("0.20.0"));
        assert_eq!(entries[0].expires_on, expires_on);
    }

    #[test]
    fn earlier_expiry_wins() {
        let path = temp_path("earlier-expiry-wins");
        let earlier = time("2026-09-02T00:00:00Z");
        let later = time("2026-09-10T00:00:00Z");

        // a re-run with a later date never extends
        schedule_expiry_at(&path, &version("0.20.0"), earlier).unwrap();
        let effective = schedule_expiry_at(&path, &version("0.20.0"), later).unwrap();
        assert_eq!(effective, earlier);

        // an earlier date tightens
        let earliest = time("2026-08-30T00:00:00Z");
        let effective = schedule_expiry_at(&path, &version("0.20.0"), earliest).unwrap();
        assert_eq!(effective, earliest);

        let entries = read_entries(&path).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].expires_on, earliest);
    }

    #[test]
    fn reads_dates_and_timestamps() {
        let path = temp_path("reads-dates-and-timestamps");
        fs::write(
            &path,
            "application:\n  version_expirations:\n    \
             - older_than: \"0.19.0\"\n      expires_on: \"2026-08-01\"\n    \
             - older_than: \"0.20.0\"\n      expires_on: \"2026-09-02T15:30:00Z\"\n",
        )
        .unwrap();

        let entries = read_entries(&path).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].expires_on, time("2026-08-01T00:00:00Z"));
        assert_eq!(entries[1].expires_on, time("2026-09-02T15:30:00Z"));
    }

    #[test]
    fn entries_are_sorted_by_version() {
        let path = temp_path("entries-are-sorted-by-version");
        schedule_expiry_at(&path, &version("0.20.0"), time("2026-09-02T00:00:00Z")).unwrap();
        schedule_expiry_at(&path, &version("0.19.0"), time("2026-08-01T00:00:00Z")).unwrap();

        let entries = read_entries(&path).unwrap();
        let versions: Vec<_> = entries.iter().map(|e| e.older_than.clone()).collect();
        assert_eq!(versions, vec![version("0.19.0"), version("0.20.0")]);
    }

    #[test]
    fn output_is_stable() {
        let path = temp_path("output-is-stable");
        schedule_expiry_at(&path, &version("0.20.0"), time("2026-09-02T00:00:00Z")).unwrap();

        let contents = fs::read_to_string(&path).unwrap();
        let expected = r#"# SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
#
# SPDX-License-Identifier: AGPL-3.0-or-later

# Client version expirations. All client versions below `older_than` stop
# being accepted at `expires_on`, given as a date (YYYY-MM-DD, midnight UTC)
# or an RFC 3339 timestamp. Re-running the tooling never extends an existing
# entry.
#
# Managed by `cargo xtask cut-release`. Edit by hand only to postpone or
# emergency-expire a version.
application:
  version_expirations:
    - older_than: "0.20.0"
      expires_on: "2026-09-02"
"#;
        assert_eq!(contents, expected);
    }
}
