// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! The rustup module with an offline dist server and sccache.

use std::{collections::BTreeMap, fs};

use anyhow::{Context, Result};
use camino::Utf8Path;
use serde::Deserialize;
use xshell::Shell;

use super::{
    Arch,
    source::{Archive, BuildOptions, File, HashCache, Module, Source, download, sha256_file},
};

const RUST_DIST_URL: &str = "https://static.rust-lang.org";
const RUSTUP_VERSION: &str = "1.29.1";
const SCCACHE_VERSION: &str = "0.15.0";

// The Rust channel manifest, channel-rust-<version>.toml.
#[derive(Deserialize)]
struct Channel {
    date: String,
    pkg: BTreeMap<String, Pkg>,
}

#[derive(Deserialize)]
struct Pkg {
    target: BTreeMap<String, Target>,
}

// Unavailable targets lack the archive fields.
#[derive(Deserialize)]
struct Target {
    xz_url: Option<String>,
    xz_hash: Option<String>,
}

pub(super) fn rustup_module(
    shell: &Shell,
    cache_dir: &Utf8Path,
    channel: &str,
    arches: &[Arch],
    hashes: &mut HashCache,
) -> Result<Module> {
    let dist_dir = "dist-server/dist";
    let channel_url = format!("{RUST_DIST_URL}/dist/channel-rust-{channel}.toml");
    let channel_sha_url = format!("{channel_url}.sha256");

    // Immutable per version, so cached like the hashes.
    let channel_file = cache_dir.join(format!("channel-rust-{channel}.toml"));
    if !channel_file.exists() {
        download(shell, &channel_url, &channel_file)?;
    }
    let manifest: Channel = toml::from_str(&fs::read_to_string(&channel_file)?)
        .with_context(|| format!("Invalid {channel_file}"))?;

    let channel_sha = hashes.sha256(&channel_sha_url)?;
    let mut sources = vec![
        Source::File(File {
            url: Some(channel_url),
            sha256: Some(sha256_file(&channel_file)?),
            dest: Some(dist_dir.to_owned()),
            ..Default::default()
        }),
        Source::File(File {
            url: Some(channel_sha_url),
            sha256: Some(channel_sha),
            dest: Some(dist_dir.to_owned()),
            ..Default::default()
        }),
    ];
    for &arch in arches {
        let url = format!(
            "{RUST_DIST_URL}/rustup/archive/{RUSTUP_VERSION}/{arch}-unknown-linux-gnu/rustup-init"
        );
        let sha256 = hashes.published_sha256(&url)?;
        sources.push(Source::File(File {
            url: Some(url),
            sha256: Some(sha256),
            only_arches: vec![arch.to_string()],
            ..Default::default()
        }));
    }
    // The minimal profile
    for component in ["cargo", "rust-std", "rustc"] {
        for &arch in arches {
            let triple = format!("{arch}-unknown-linux-gnu");
            let Target { xz_url, xz_hash } = manifest
                .pkg
                .get(component)
                .and_then(|pkg| pkg.target.get(&triple))
                .with_context(|| format!("No {component} for {triple} in Rust {channel}"))?;
            let xz_url = xz_url
                .as_ref()
                .with_context(|| format!("No xz_url of {component} for {triple}"))?;
            let xz_hash = xz_hash
                .as_ref()
                .with_context(|| format!("No xz_hash of {component} for {triple}"))?;
            sources.push(Source::File(File {
                url: Some(xz_url.clone()),
                sha256: Some(xz_hash.clone()),
                dest: Some(format!("{dist_dir}/{}", manifest.date)),
                only_arches: vec![arch.to_string()],
                ..Default::default()
            }));
        }
    }
    for &arch in arches {
        let name = format!("sccache-v{SCCACHE_VERSION}-{arch}-unknown-linux-musl");
        let url = format!(
            "https://github.com/mozilla/sccache/releases/download/v{SCCACHE_VERSION}/{name}.tar.gz"
        );
        let sha256 = hashes.published_sha256(&url)?;
        sources.push(Source::Archive(Archive {
            url: Some(url),
            sha256,
            dest: "sccache".to_owned(),
            only_arches: vec![arch.to_string()],
            ..Default::default()
        }));
    }

    let mut module = Module::simple(
        "rustup",
        vec![
            format!(
                "chmod +x rustup-init && ./rustup-init -y --no-modify-path --profile minimal --default-toolchain {channel}"
            ),
            "install -Dm755 sccache/sccache /var/lib/cargo/bin/sccache".to_owned(),
        ],
        sources,
    );
    module.build_options = Some(BuildOptions {
        env: BTreeMap::from([
            ("RUSTUP_HOME", "/var/lib/rustup"),
            ("CARGO_HOME", "/var/lib/cargo"),
            ("RUSTUP_DIST_SERVER", "file:///run/build/rustup/dist-server"),
        ]),
    });
    Ok(module)
}
