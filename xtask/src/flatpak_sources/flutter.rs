// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! The Flutter SDK module with the engine artifacts of the Linux build.

use std::fs;

use anyhow::{Context, Result};
use camino::Utf8Path;
use serde::{Deserialize, Serialize};
use xshell::{Shell, cmd};

use super::{
    Arch,
    source::{Archive, File, Git, HashCache, Module, Source, download, sha256_file},
};

const FLUTTER_GIT_URL: &str = "https://github.com/flutter/flutter.git";
const FLUTTER_STORAGE_URL: &str = "https://storage.googleapis.com";

#[derive(Serialize, Deserialize)]
pub(super) struct FlutterInfo {
    commit: String,
    engine: String,
    material_fonts: String,
    gradle_wrapper: String,
    // Not in the Flutter repo, resolved once per Flutter version.
    pub(super) flutter_tools_lock: String,
}

impl FlutterInfo {
    pub(super) fn load(
        shell: &Shell,
        cache_dir: &Utf8Path,
        tag: &str,
        hashes: &mut HashCache,
    ) -> Result<Self> {
        let info_path = cache_dir.join(format!("flutter-{tag}.json"));
        if info_path.exists() {
            return serde_json::from_str(&fs::read_to_string(&info_path)?)
                .with_context(|| format!("Invalid {info_path}"));
        }

        let work_dir = cache_dir.join(format!("flutter-{tag}"));
        if work_dir.exists() {
            fs::remove_dir_all(&work_dir)?;
        }
        let checkout = work_dir.join("flutter");
        println!("Cloning Flutter {tag}...");
        cmd!(
            shell,
            "git clone --quiet --depth 1 --branch {tag} {FLUTTER_GIT_URL} {checkout}"
        )
        .run()?;
        let commit = cmd!(shell, "git -C {checkout} rev-parse HEAD").read()?;
        let read_version = |name: &str| -> Result<String> {
            let path = checkout
                .join("bin/internal")
                .join(format!("{name}.version"));
            let content =
                fs::read_to_string(&path).with_context(|| format!("Failed to read {path}"))?;
            Ok(content.trim().to_owned())
        };
        let engine = read_version("engine")?;
        let material_fonts = read_version("material_fonts")?;
        let gradle_wrapper = read_version("gradle_wrapper")?;

        // Resolve flutter_tools with the SDK's own Dart, as the flutter tool
        // does on its first run.
        let host = Arch::host()?;
        let dart_url = format!(
            "{FLUTTER_STORAGE_URL}/flutter_infra_release/flutter/{engine}/dart-sdk-linux-{}.zip",
            host.flutter()
        );
        let dart_zip = work_dir.join("dart-sdk.zip");
        download(shell, &dart_url, &dart_zip)?;
        hashes.insert(&dart_url, sha256_file(&dart_zip)?)?;
        cmd!(shell, "unzip -q {dart_zip} -d {work_dir}").run()?;
        let dart = work_dir.join("dart-sdk/bin/dart");
        let pub_cache = work_dir.join("pub-cache");
        {
            let _dir = shell.push_dir(checkout.join("packages/flutter_tools"));
            cmd!(shell, "{dart} pub get --suppress-analytics")
                .env("PUB_CACHE", &pub_cache)
                .run()?;
        }
        let lock_path = checkout.join("packages/flutter_tools/pubspec.lock");
        let flutter_tools_lock = fs::read_to_string(&lock_path)
            .with_context(|| format!("Failed to read {lock_path}"))?;

        let info = Self {
            commit,
            engine,
            material_fonts,
            gradle_wrapper,
            flutter_tools_lock,
        };
        fs::write(&info_path, serde_json::to_string_pretty(&info)?)?;
        fs::remove_dir_all(&work_dir)?;
        Ok(info)
    }
}

pub(super) fn flutter_sdk_module(
    flutter: &FlutterInfo,
    tag: &str,
    arches: &[Arch],
    hashes: &mut HashCache,
) -> Result<Module> {
    let engine = format!(
        "{FLUTTER_STORAGE_URL}/flutter_infra_release/flutter/{}",
        flutter.engine
    );
    let cache = "flutter/bin/cache";
    let engine_dir = format!("{cache}/artifacts/engine");
    let mut archive = |url: String, dest: String| -> Result<Archive> {
        let sha256 = hashes.sha256(&url)?;
        Ok(Archive {
            url: Some(url),
            sha256,
            dest,
            ..Default::default()
        })
    };

    let mut sources = vec![Source::Git(Git {
        url: FLUTTER_GIT_URL.to_owned(),
        tag: Some(tag.to_owned()),
        commit: flutter.commit.clone(),
        dest: "flutter".to_owned(),
    })];
    for &arch in arches {
        let a = arch.flutter();
        sources.push(Source::Archive(Archive {
            strip_components: Some(0),
            only_arches: vec![arch.to_string()],
            ..archive(format!("{engine}/dart-sdk-linux-{a}.zip"), cache.to_owned())?
        }));
    }
    sources.push(Source::Archive(archive(
        format!("{FLUTTER_STORAGE_URL}/{}", flutter.material_fonts),
        format!("{cache}/artifacts/material_fonts"),
    )?));
    sources.push(Source::Archive(Archive {
        strip_components: Some(0),
        ..archive(
            format!("{FLUTTER_STORAGE_URL}/{}", flutter.gradle_wrapper),
            format!("{cache}/artifacts/gradle_wrapper"),
        )?
    }));
    for pkg in ["sky_engine", "flutter_gpu"] {
        sources.push(Source::Archive(archive(
            format!("{engine}/{pkg}.zip"),
            format!("{cache}/pkg/{pkg}"),
        )?));
    }
    for sdk in ["flutter_patched_sdk", "flutter_patched_sdk_product"] {
        sources.push(Source::Archive(archive(
            format!("{engine}/{sdk}.zip"),
            format!("{engine_dir}/common/{sdk}"),
        )?));
    }
    for &arch in arches {
        let a = arch.flutter();
        sources.push(Source::Archive(Archive {
            strip_components: Some(0),
            only_arches: vec![arch.to_string()],
            ..archive(
                format!("{engine}/linux-{a}/artifacts.zip"),
                format!("{engine_dir}/linux-{a}"),
            )?
        }));
        sources.push(Source::Archive(Archive {
            only_arches: vec![arch.to_string()],
            ..archive(
                format!("{engine}/linux-{a}/font-subset.zip"),
                format!("{engine_dir}/linux-{a}"),
            )?
        }));
        for mode in ["profile", "release"] {
            sources.push(Source::Archive(Archive {
                strip_components: Some(0),
                only_arches: vec![arch.to_string()],
                ..archive(
                    format!("{engine}/linux-{a}-{mode}/linux-{a}-flutter-gtk.zip"),
                    format!("{engine_dir}/linux-{a}-{mode}"),
                )?
            }));
        }
    }

    let engine_stamp = format!("{engine}/engine_stamp.json");
    let sha256 = hashes.sha256(&engine_stamp)?;
    sources.push(Source::File(File {
        url: Some(engine_stamp),
        sha256: Some(sha256),
        dest: Some(cache.to_owned()),
        ..Default::default()
    }));
    sources.push(Source::File(File {
        path: Some("flutter_tools-pubspec.lock".to_owned()),
        dest: Some("flutter/packages/flutter_tools".to_owned()),
        dest_filename: Some("pubspec.lock".to_owned()),
        ..Default::default()
    }));

    let mut commands: Vec<String> = [
        ("engine", "engine-dart-sdk"),
        ("material_fonts", "material_fonts"),
        ("gradle_wrapper", "gradle_wrapper"),
        ("engine", "engine_stamp"),
        ("engine", "flutter_sdk"),
        ("engine", "font-subset"),
        ("engine", "linux-sdk"),
    ]
    .iter()
    .map(|(version, stamp)| {
        format!("cp flutter/bin/internal/{version}.version {cache}/{stamp}.stamp")
    })
    .collect();
    commands.extend([
        // The tool builds itself on the first run, which resolves its
        // dependencies online. Resolve against the lock offline instead.
        "sed -i 's/pub upgrade --suppress-analytics/pub get --offline --enforce-lockfile --suppress-analytics/' flutter/bin/internal/shared.sh".to_owned(),
        "grep -q 'pub get --offline --enforce-lockfile' flutter/bin/internal/shared.sh".to_owned(),
        // Otherwise the tool takes the lock as outdated and rebuilds itself on
        // every run.
        "touch flutter/packages/flutter_tools/pubspec.lock".to_owned(),
        "mkdir -p /var/lib && cp -a flutter /var/lib/".to_owned(),
    ]);

    Ok(Module::simple("flutter", commands, sources))
}
