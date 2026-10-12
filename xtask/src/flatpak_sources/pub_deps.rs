// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! The pub cache of the hosted Dart packages.

use std::collections::BTreeMap;

use anyhow::{Context, Result, bail, ensure};
use yaml_rust2::{Yaml, YamlLoader};

use super::source::{Archive, Inline, Source};

const PUB_DEV_URL: &str = "https://pub.dev";

// pub.dev archives of all hosted packages in the given locks.
pub(super) fn pubspec_sources(locks: &[&str]) -> Result<Vec<Source>> {
    let mut packages: BTreeMap<(String, String), String> = BTreeMap::new();
    for lock in locks {
        let docs = YamlLoader::load_from_str(lock)?;
        let doc = docs.first().context("Empty pubspec.lock")?;
        let Yaml::Hash(entries) = &doc["packages"] else {
            bail!("No packages in pubspec.lock");
        };
        for (name, package) in entries {
            let name = name.as_str().context("Invalid package name")?;
            let source = package["source"].as_str().unwrap_or_default();
            match source {
                "hosted" => {}
                // Part of the Flutter SDK or the app checkout.
                "sdk" | "path" => continue,
                _ => bail!("Unsupported source {source:?} of package {name}"),
            }
            let description = &package["description"];
            let url = description["url"].as_str().unwrap_or_default();
            ensure!(
                url == PUB_DEV_URL,
                "Package {name} is not from pub.dev: {url}"
            );
            let version = package["version"]
                .as_str()
                .with_context(|| format!("No version of package {name}"))?;
            let sha256 = description["sha256"]
                .as_str()
                .with_context(|| format!("No sha256 of package {name}"))?;
            let key = (name.to_owned(), version.to_owned());
            if let Some(existing) = packages.insert(key, sha256.to_owned()) {
                ensure!(
                    existing == sha256,
                    "Conflicting sha256 of package {name} {version}"
                );
            }
        }
    }

    let mut sources = Vec::new();
    for ((name, version), sha256) in packages {
        sources.push(Source::Archive(Archive {
            url: Some(format!(
                "{PUB_DEV_URL}/api/archives/{name}-{version}.tar.gz"
            )),
            sha256: sha256.clone(),
            dest: format!("pub-cache/hosted/pub.dev/{name}-{version}"),
            archive_type: Some("tar-gzip"),
            strip_components: Some(0),
            ..Default::default()
        }));
        sources.push(Source::Inline(Inline {
            contents: sha256,
            dest: Some("pub-cache/hosted-hashes/pub.dev".to_owned()),
            dest_filename: format!("{name}-{version}.sha256"),
        }));
    }
    Ok(sources)
}
