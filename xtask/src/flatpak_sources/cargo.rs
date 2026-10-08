// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! The vendored crates of Cargo.lock, from crates.io and git.

use std::{collections::BTreeMap, fmt, fs};

use anyhow::{Context, Result, bail, ensure};
use camino::Utf8Path;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use xshell::{Shell, cmd};

use super::{
    BUILD_DIR,
    source::{self, Archive, Git, Inline, Source},
};

const CRATES_IO_URL: &str = "https://static.crates.io/crates";
const CRATES_IO_SOURCES: [&str; 2] = [
    "registry+https://github.com/rust-lang/crates.io-index",
    "sparse+https://index.crates.io/",
];

#[derive(Deserialize)]
struct CargoLock {
    package: Vec<LockPackage>,
}

#[derive(Deserialize)]
struct LockPackage {
    name: String,
    version: String,
    // None for workspace members
    source: Option<String>,
    checksum: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct GitPackage {
    name: String,
    version: String,
    source: String,
}

// Also the cache key input, keep it stable.
impl fmt::Display for GitPackage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Self {
            name,
            version,
            source,
        } = self;
        write!(f, "{name} {version} {source}")
    }
}

#[derive(Serialize, Deserialize)]
struct GitCrate {
    #[serde(flatten)]
    package: GitPackage,
    // Package dir in the repo, empty for the root.
    subpath: String,
    // Normalized by cargo vendor, with workspace keys and path deps resolved.
    manifest: String,
}

pub(super) fn cargo_sources(
    shell: &Shell,
    root: &Utf8Path,
    cache_dir: &Utf8Path,
) -> Result<Vec<Source>> {
    let lock: CargoLock = toml::from_str(&fs::read_to_string(root.join("Cargo.lock"))?)
        .context("Invalid Cargo.lock")?;

    let mut crates = Vec::new();
    let mut git_packages = Vec::new();
    for LockPackage {
        name,
        version,
        source,
        checksum,
    } in lock.package
    {
        let Some(source) = source else {
            // Workspace member
            continue;
        };
        if CRATES_IO_SOURCES.contains(&source.as_str()) {
            let checksum =
                checksum.with_context(|| format!("No checksum of crate {name} {version}"))?;
            crates.push((name, version, checksum));
        } else if source.starts_with("git+") {
            git_packages.push(GitPackage {
                name,
                version,
                source,
            });
        } else {
            bail!("Unsupported source of crate {name} {version}: {source}");
        }
    }

    let git_crates = git_crates(shell, root, cache_dir, git_packages)?;

    let vendor_dir = "cargo-vendor";
    let mut sources = Vec::new();
    let mut git_repos: BTreeMap<String, Source> = BTreeMap::new();
    // Keyed like cargo vendor does, the source id without the commit.
    let mut replacements: BTreeMap<&str, GitSource> = BTreeMap::new();
    let mut git_crate_sources = Vec::new();
    for GitCrate {
        package: GitPackage {
            name,
            version,
            source,
        },
        subpath,
        manifest,
    } in &git_crates
    {
        let git_source = GitSource::parse(source)?;
        let GitSource { url, commit, .. } = &git_source;
        let repo_name = url
            .rsplit('/')
            .next()
            .unwrap_or("repo")
            .trim_end_matches(".git");
        let repo_dir = format!("cargo-git/{repo_name}-{}", &commit[..7]);
        git_repos.entry(repo_dir.clone()).or_insert_with(|| {
            Source::Git(Git {
                url: url.clone(),
                tag: None,
                commit: commit.clone(),
                dest: repo_dir.clone(),
            })
        });

        let crate_dir = format!("{vendor_dir}/{name}-{version}-{}", &commit[..7]);
        let package_dir = if subpath.is_empty() {
            format!("{repo_dir}/.")
        } else {
            format!("{repo_dir}/{subpath}")
        };
        // Dereferenced, packages link files elsewhere in the repo (README).
        git_crate_sources.push(Source::Shell(source::Shell {
            commands: vec![format!(
                "mkdir -p {vendor_dir} && cp -rL --reflink=auto \"{package_dir}\" \"{crate_dir}\""
            )],
        }));
        git_crate_sources.push(Source::Inline(Inline {
            contents: manifest.clone(),
            dest: Some(crate_dir.clone()),
            dest_filename: "Cargo.toml".to_owned(),
        }));
        git_crate_sources.push(Source::Inline(Inline {
            contents: r#"{"package": null, "files": {}}"#.to_owned(),
            dest: Some(crate_dir),
            dest_filename: ".cargo-checksum.json".to_owned(),
        }));

        let source_id = source.split('#').next().unwrap_or(source);
        replacements.insert(source_id, git_source);
    }
    sources.extend(git_repos.into_values());
    sources.extend(git_crate_sources);

    for (name, version, checksum) in crates {
        let crate_dir = format!("{vendor_dir}/{name}-{version}");
        sources.push(Source::Archive(Archive {
            url: Some(format!("{CRATES_IO_URL}/{name}/{name}-{version}.crate")),
            sha256: checksum.clone(),
            dest: crate_dir.clone(),
            archive_type: Some("tar-gzip"),
            ..Default::default()
        }));
        sources.push(Source::Inline(Inline {
            contents: format!(r#"{{"package": "{checksum}", "files": {{}}}}"#),
            dest: Some(crate_dir),
            dest_filename: ".cargo-checksum.json".to_owned(),
        }));
    }

    let mut config = String::from("[source.crates-io]\nreplace-with = \"vendored-sources\"\n");
    for (source_id, GitSource { url, reference, .. }) in &replacements {
        let (ref_key, ref_value) = reference;
        config.push_str(&format!(
            "\n[source.\"{source_id}\"]\ngit = \"{url}\"\n{ref_key} = \"{ref_value}\"\nreplace-with = \"vendored-sources\"\n"
        ));
    }
    config.push_str(&format!(
        "\n[source.vendored-sources]\ndirectory = \"{BUILD_DIR}/{vendor_dir}\"\n"
    ));
    config.push_str("\n[net]\noffline = true\n");
    sources.push(Source::Inline(Inline {
        contents: config,
        dest: None,
        dest_filename: "cargo-config.toml".to_owned(),
    }));

    Ok(sources)
}

#[derive(Debug, PartialEq, Eq)]
struct GitSource {
    url: String,
    // rev, tag or branch from the query
    reference: (String, String),
    commit: String,
}

impl GitSource {
    // "git+https://github.com/o/r?rev=abc#<commit>"
    fn parse(source: &str) -> Result<Self> {
        let rest = source
            .strip_prefix("git+")
            .with_context(|| format!("Not a git source: {source}"))?;
        let (rest, commit) = rest
            .split_once('#')
            .with_context(|| format!("No commit in git source: {source}"))?;
        let (url, query) = rest.split_once('?').unwrap_or((rest, ""));
        let reference = query
            .split('&')
            .filter_map(|pair| pair.split_once('='))
            .find(|(key, _)| ["rev", "tag", "branch"].contains(key))
            .map(|(key, value)| (key.to_owned(), value.to_owned()))
            .with_context(|| format!("No rev, tag or branch in git source: {source}"))?;
        ensure!(
            commit.len() == 40 && commit.chars().all(|c| c.is_ascii_hexdigit()),
            "Invalid commit in git source: {source}"
        );
        Ok(Self {
            url: url.to_owned(),
            reference,
            commit: commit.to_owned(),
        })
    }
}

// Runs cargo vendor once per set of git packages, which normalizes their
// manifests like a crates.io upload.
fn git_crates(
    shell: &Shell,
    root: &Utf8Path,
    cache_dir: &Utf8Path,
    git_packages: Vec<GitPackage>,
) -> Result<Vec<GitCrate>> {
    let mut hasher = Sha256::new();
    for package in &git_packages {
        hasher.update(format!("{package}\n").as_bytes());
    }
    let key = hex::encode(hasher.finalize());
    let cache_path = cache_dir.join(format!("cargo-git-{}.json", &key[..16]));
    if cache_path.exists() {
        return serde_json::from_str(&fs::read_to_string(&cache_path)?)
            .with_context(|| format!("Invalid {cache_path}"));
    }

    let vendor_dir = cache_dir.join("cargo-vendor");
    if vendor_dir.exists() {
        fs::remove_dir_all(&vendor_dir)?;
    }
    println!("Vendoring crates to normalize the git crates...");
    let manifest = root.join("Cargo.toml");
    // The log is parsed below, so no color codes (CI sets CARGO_TERM_COLOR).
    let output = cmd!(
        shell,
        "cargo vendor --color never --locked --versioned-dirs --manifest-path {manifest} {vendor_dir}"
    )
    .quiet()
    .ignore_status()
    .output()?;
    let log = String::from_utf8_lossy(&output.stderr);
    ensure!(
        output.status.success(),
        "cargo vendor failed with {}:\n{log}",
        output.status
    );

    let mut crates = Vec::new();
    for package in git_packages {
        let GitPackage { name, version, .. } = &package;
        // "Vendoring <name> v<version> (<path>) to <dest>"
        let prefix = format!("Vendoring {name} v{version} (");
        let path = log
            .lines()
            .find_map(|line| line.trim().strip_prefix(&prefix))
            .and_then(|rest| rest.split_once(") to "))
            .map(|(path, _dest)| path)
            .with_context(|| format!("cargo vendor did not vendor {name} {version}"))?;
        // <CARGO_HOME>/git/checkouts/<repo>-<hash>/<short commit>/<subpath>
        let (_, checkout) = path
            .split_once("/git/checkouts/")
            .with_context(|| format!("{name} {version} is not from a git checkout: {path}"))?;
        let subpath: Vec<&str> = checkout.split('/').skip(2).collect();
        let crate_manifest = vendor_dir.join(format!("{name}-{version}/Cargo.toml"));
        let manifest = fs::read_to_string(&crate_manifest)
            .with_context(|| format!("Failed to read {crate_manifest}"))?;
        crates.push(GitCrate {
            package,
            subpath: subpath.join("/"),
            manifest,
        });
    }
    fs::remove_dir_all(&vendor_dir)?;
    fs::write(&cache_path, serde_json::to_string_pretty(&crates)?)?;
    Ok(crates)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_git_source() {
        let commit = "0123456789abcdef0123456789abcdef01234567";
        let source = GitSource::parse(&format!(
            "git+https://github.com/o/r?branch=main&rev=abc#{commit}"
        ))
        .unwrap();
        assert_eq!(
            source,
            GitSource {
                url: "https://github.com/o/r".to_owned(),
                reference: ("branch".to_owned(), "main".to_owned()),
                commit: commit.to_owned(),
            }
        );
        assert!(GitSource::parse("git+https://github.com/o/r#abc").is_err());
    }
}
