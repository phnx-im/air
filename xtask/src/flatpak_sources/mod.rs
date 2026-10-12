// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Generates the Flatpak manifest and the sources for an offline build.
//!
//! Ports the source generators of flatpak-flutter and flatpak-builder-tools
//! (both MIT). The output lives under `target/flatpak/` and is regenerated on
//! every build.

mod cargo;
mod flutter;
mod pub_deps;
mod rustup;
mod source;

use std::{fmt, fs};

use anyhow::{Context, Result, anyhow, bail, ensure};
use askama::Template;
use camino::Utf8Path;
use clap::{Args, ValueEnum};
use serde::Deserialize;
use xshell::{Shell, cmd};

use self::source::{Archive, File, Git, HashCache, Source, sha256_file, write_json};
use crate::util::workspace_root;

const FLATPAK_DIR: &str = "target/flatpak";

const AIR_GIT_URL: &str = "https://github.com/phnx-im/air.git";
const AIR_LFS_MEDIA_URL: &str = "https://media.githubusercontent.com/media/phnx-im/air";

/// Build dir of the app module in the sandbox. The module is named "air".
const BUILD_DIR: &str = "/run/build/air";
/// Checkout of the app inside the build dir.
const SRC_DIR: &str = "src";

/// LFS objects needed by the Linux build. The rest (tests, mobile, store assets) is not downloaded.
const LFS_PREFIXES: [&str; 3] = ["app/assets/", "app/linux/", "app/fonts/"];

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub(crate) enum Flavor {
    Production,
    Staging,
}

impl Flavor {
    pub(crate) const fn app_id(self) -> &'static str {
        match self {
            Self::Production => "ms.air.Air",
            Self::Staging => "ms.air.Air.Staging",
        }
    }
}

impl fmt::Display for Flavor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = self
            .to_possible_value()
            .expect("no skipped Flavor variants");
        f.write_str(value.get_name())
    }
}

// Displays the Flatpak and Rust name.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
pub(crate) enum Arch {
    #[value(name = "x86_64")]
    X86_64,
    #[value(name = "aarch64")]
    Aarch64,
}

impl Arch {
    // Flutter name
    fn flutter(self) -> &'static str {
        match self {
            Self::X86_64 => "x64",
            Self::Aarch64 => "arm64",
        }
    }

    fn host() -> Result<Self> {
        let arch = std::env::consts::ARCH;
        Self::from_str(arch, true).map_err(|_| anyhow!("Unsupported host arch: {arch}"))
    }
}

impl fmt::Display for Arch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = self.to_possible_value().expect("no skipped Arch variants");
        f.write_str(value.get_name())
    }
}

#[derive(Args, Debug)]
pub(crate) struct FlatpakSourcesArgs {
    /// App flavor to build.
    #[arg(long, value_enum, default_value_t = Flavor::Production)]
    flavor: Flavor,

    /// Build number baked into the app.
    #[arg(long, env = "AIR_BUILD_NUMBER", default_value_t = 0)]
    build_number: u64,

    /// Architectures to generate sources for.
    #[arg(long = "arch", value_enum, default_values_t = [Arch::X86_64, Arch::Aarch64])]
    arches: Vec<Arch>,

    /// Build the working tree, incl. uncommitted changes, instead of the HEAD
    /// commit from GitHub.
    #[arg(long)]
    local: bool,
}

pub(crate) fn run(args: FlatpakSourcesArgs) -> Result<()> {
    let FlatpakSourcesArgs {
        flavor,
        build_number,
        mut arches,
        local,
    } = args;
    arches.sort();
    arches.dedup();

    let root = workspace_root();
    let shell = Shell::new()?;
    shell.change_dir(root.as_std_path());

    let out_dir = root.join(FLATPAK_DIR);
    let generated_dir = out_dir.join("generated");
    let cache_dir = out_dir.join("cache");
    fs::create_dir_all(&generated_dir)?;
    fs::create_dir_all(&cache_dir)?;
    let mut hashes = HashCache::load(cache_dir.join("sha256.json"))?;

    let commit = cmd!(shell, "git rev-parse HEAD").read()?;
    let version = app_version(&root)?;
    let rust_channel = rust_channel(&root)?;
    let flutter_tag = flutter_tag(&root)?;

    println!("App\t: {} {version}-{build_number}", flavor.app_id());
    println!(
        "Commit\t: {commit}{}",
        if local { " (working tree)" } else { "" }
    );
    println!("Flutter\t: {flutter_tag}");
    println!("Rust\t: {rust_channel}");
    let arch_names: Vec<String> = arches.iter().map(Arch::to_string).collect();
    println!("Archs\t: {}", arch_names.join(" "));

    // The checkout and its LFS objects. In one file because flatpak-builder
    // rejects an empty source list. A working tree build takes the LFS objects
    // from the checkout.
    let mut air_sources = Vec::new();
    if local {
        air_sources.push(local_air_source(&shell, &root, &generated_dir)?);
    } else {
        let stale = generated_dir.join("air-src.tar");
        if stale.exists() {
            fs::remove_file(&stale)?;
        }
        air_sources.push(Source::Git(Git {
            url: AIR_GIT_URL.to_owned(),
            tag: None,
            commit: commit.clone(),
            dest: SRC_DIR.to_owned(),
        }));
        air_sources.extend(lfs_sources(&shell, &commit)?);
    }
    write_json(&generated_dir.join("air-sources.json"), &air_sources)?;

    let flutter = flutter::FlutterInfo::load(&shell, &cache_dir, &flutter_tag, &mut hashes)?;
    let flutter_tools_lock = generated_dir.join("flutter_tools-pubspec.lock");
    fs::write(&flutter_tools_lock, &flutter.flutter_tools_lock)
        .with_context(|| format!("Failed to write {flutter_tools_lock}"))?;
    let flutter_sdk = flutter::flutter_sdk_module(&flutter, &flutter_tag, &arches, &mut hashes)?;
    write_json(&generated_dir.join("flutter-sdk.json"), &flutter_sdk)?;

    let app_lock = fs::read_to_string(root.join("app/pubspec.lock"))?;
    let pub_sources = pub_deps::pubspec_sources(&[&app_lock, &flutter.flutter_tools_lock])?;
    write_json(&generated_dir.join("pubspec-sources.json"), &pub_sources)?;

    let cargo_sources = cargo::cargo_sources(&shell, &root, &cache_dir)?;
    write_json(&generated_dir.join("cargo-sources.json"), &cargo_sources)?;

    let rustup = rustup::rustup_module(&shell, &cache_dir, &rust_channel, &arches, &mut hashes)?;
    write_json(&generated_dir.join("rustup.json"), &rustup)?;

    let manifest = ManifestTemplate {
        app_id: flavor.app_id(),
        flavor,
        build_number,
        commit: &commit,
        rust_channel: &rust_channel,
    }
    .render()?;
    let manifest_path = out_dir.join(format!("{}.yml", flavor.app_id()));
    fs::write(&manifest_path, manifest)
        .with_context(|| format!("Failed to write {manifest_path}"))?;

    // For the commit subject, which promotion looks up.
    fs::write(
        out_dir.join("version"),
        format!("{version}-{build_number}\n"),
    )?;

    println!("Manifest: {manifest_path}");
    Ok(())
}

// Symlink to app/linux/flatpak/ms.air.Air.yml.jinja.
#[derive(Template)]
#[template(path = "flatpak_manifest.yml.jinja", escape = "none")]
struct ManifestTemplate<'a> {
    app_id: &'a str,
    flavor: Flavor,
    build_number: u64,
    commit: &'a str,
    rust_channel: &'a str,
}

// Version without the build number, as in the deb/rpm packages.
fn app_version(root: &Utf8Path) -> Result<String> {
    let pubspec = fs::read_to_string(root.join("app/pubspec.yaml"))?;
    let version = pubspec
        .lines()
        .find_map(|line| line.strip_prefix("version:"))
        .context("No version in app/pubspec.yaml")?
        .trim();
    let (version, _build) = version.split_once('+').unwrap_or((version, ""));
    Ok(version.to_owned())
}

#[derive(Deserialize)]
struct ToolchainFile {
    toolchain: Toolchain,
}

#[derive(Deserialize)]
struct Toolchain {
    channel: String,
}

// The build hook runs rustup in applogic/, so its toolchain file decides.
fn rust_channel(root: &Utf8Path) -> Result<String> {
    let path = root.join("applogic/rust-toolchain.toml");
    let content = fs::read_to_string(&path)?;
    let ToolchainFile {
        toolchain: Toolchain { channel },
    } = toml::from_str(&content).with_context(|| format!("Invalid {path}"))?;
    ensure!(
        channel.split('.').count() == 3 && channel.split('.').all(|p| p.parse::<u32>().is_ok()),
        "Expected an exact Rust version in {path}, got {channel:?}"
    );
    Ok(channel)
}

#[derive(Deserialize)]
struct Fvmrc {
    flutter: String,
}

fn flutter_tag(root: &Utf8Path) -> Result<String> {
    let Fvmrc { flutter } = serde_json::from_str(&fs::read_to_string(root.join("app/.fvmrc"))?)
        .context("No flutter version in app/.fvmrc")?;
    Ok(flutter)
}

// The working tree minus everything git ignores, as archive. A sandboxed
// build only reads sources next to the manifest. Without the history, the
// git metadata in the app falls back to unknown.
fn local_air_source(shell: &Shell, root: &Utf8Path, generated_dir: &Utf8Path) -> Result<Source> {
    let files = cmd!(
        shell,
        "git ls-files --cached --others --exclude-standard -z"
    )
    .read()?;
    let mut list: Vec<&str> = files
        .split('\0')
        .filter(|file| !file.is_empty() && root.join(file).symlink_metadata().is_ok())
        .collect();
    list.sort();
    list.dedup();
    let archive = generated_dir.join("air-src.tar");
    cmd!(
        shell,
        "tar -cf {archive} -C {root} --null --no-recursion -T -"
    )
    .stdin(list.join("\0"))
    .run()?;
    Ok(Source::Archive(Archive {
        // Relative to the manifest, not to this file.
        path: Some("generated/air-src.tar".to_owned()),
        // flatpak-builder checks a local file only by path, a changed hash
        // invalidates the module cache.
        sha256: sha256_file(&archive)?,
        dest: SRC_DIR.to_owned(),
        archive_type: Some("tar"),
        strip_components: Some(0),
        ..Default::default()
    }))
}

fn lfs_sources(shell: &Shell, commit: &str) -> Result<Vec<Source>> {
    let listing = cmd!(shell, "git lfs ls-files --long")
        .read()
        .context("git lfs ls-files failed (is git-lfs installed?)")?;
    let mut sources = Vec::new();
    for line in listing.lines() {
        // "<oid> <*|-> <path>"
        let mut parts = line.splitn(3, ' ');
        let (Some(oid), Some(_), Some(path)) = (parts.next(), parts.next(), parts.next()) else {
            bail!("Unexpected git lfs output: {line}");
        };
        if !LFS_PREFIXES.iter().any(|prefix| path.starts_with(prefix)) {
            continue;
        }
        let path = Utf8Path::new(path);
        let (Some(dir), Some(name)) = (path.parent(), path.file_name()) else {
            bail!("Unexpected LFS path: {path}");
        };
        sources.push(Source::File(File {
            url: Some(format!("{AIR_LFS_MEDIA_URL}/{commit}/{path}")),
            sha256: Some(oid.to_owned()),
            dest: Some(format!("{SRC_DIR}/{dir}")),
            dest_filename: Some(name.to_owned()),
            ..Default::default()
        }));
    }
    Ok(sources)
}

#[cfg(test)]
mod tests {
    use yaml_rust2::YamlLoader;

    use super::*;

    #[test]
    fn manifest_template_renders_valid_yaml() {
        let manifest = ManifestTemplate {
            app_id: Flavor::Staging.app_id(),
            flavor: Flavor::Staging,
            build_number: 1490,
            commit: "0123456789abcdef",
            rust_channel: "1.96.0",
        }
        .render()
        .unwrap();
        let docs = YamlLoader::load_from_str(&manifest).unwrap();
        let [doc] = docs.as_slice() else {
            panic!("expected one YAML document");
        };
        assert_eq!(doc["id"].as_str(), Some("ms.air.Air.Staging"));
        assert!(manifest.contains("--flavor staging --no-pub --build-number=1490"));
        assert!(manifest.contains(r#""AIR_REFRESH_BUILD_METADATA": "0123456789abcdef""#));
        assert!(!manifest.contains("{{"));
        assert_eq!(doc["rename-icon"].as_str(), Some("ms.air.Air"));
    }

    #[test]
    fn production_manifest_renames_nothing() {
        let manifest = ManifestTemplate {
            app_id: Flavor::Production.app_id(),
            flavor: Flavor::Production,
            build_number: 1490,
            commit: "0123456789abcdef",
            rust_channel: "1.96.0",
        }
        .render()
        .unwrap();
        let docs = YamlLoader::load_from_str(&manifest).unwrap();
        let [doc] = docs.as_slice() else {
            panic!("expected one YAML document");
        };
        assert_eq!(doc["id"].as_str(), Some("ms.air.Air"));
        assert!(doc["rename-desktop-file"].is_badvalue());
        assert!(doc["rename-icon"].is_badvalue());
    }

    #[test]
    fn arch_names() {
        assert_eq!(Arch::X86_64.to_string(), "x86_64");
        assert_eq!(Arch::Aarch64.to_string(), "aarch64");
    }
}
