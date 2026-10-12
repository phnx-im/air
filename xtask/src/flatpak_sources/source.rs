// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! flatpak-builder sources and modules, and the download helpers.

use std::{
    collections::BTreeMap,
    fs, io,
    process::{Command, Stdio},
};

use anyhow::{Context, Result, ensure};
use camino::{Utf8Path, Utf8PathBuf};
use digest_io::IoWrapper;
use serde::Serialize;
use sha2::{Digest, Sha256};
use xshell::cmd;

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub(super) enum Source {
    Archive(Archive),
    File(File),
    Git(Git),
    Inline(Inline),
    Shell(Shell),
}

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(super) struct Archive {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) path: Option<String>,
    pub(super) sha256: String,
    pub(super) dest: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) archive_type: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) strip_components: Option<u32>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(super) only_arches: Vec<String>,
}

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(super) struct File {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) sha256: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) dest: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) dest_filename: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(super) only_arches: Vec<String>,
}

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(super) struct Git {
    pub(super) url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) tag: Option<String>,
    pub(super) commit: String,
    pub(super) dest: String,
}

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(super) struct Inline {
    pub(super) contents: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) dest: Option<String>,
    pub(super) dest_filename: String,
}

#[derive(Debug, Default, Serialize)]
pub(super) struct Shell {
    pub(super) commands: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(super) struct Module {
    pub(super) name: &'static str,
    pub(super) buildsystem: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) build_options: Option<BuildOptions>,
    pub(super) build_commands: Vec<String>,
    pub(super) sources: Vec<Source>,
}

impl Module {
    pub(super) fn simple(
        name: &'static str,
        build_commands: Vec<String>,
        sources: Vec<Source>,
    ) -> Self {
        Self {
            name,
            buildsystem: "simple",
            build_options: None,
            build_commands,
            sources,
        }
    }
}

#[derive(Debug, Serialize)]
pub(super) struct BuildOptions {
    pub(super) env: BTreeMap<&'static str, &'static str>,
}

pub(super) fn write_json(path: &Utf8Path, value: &impl Serialize) -> Result<()> {
    // Through a Value, which sorts the keys.
    let value = serde_json::to_value(value)?;
    let mut content = serde_json::to_string_pretty(&value)?;
    content.push('\n');
    fs::write(path, content).with_context(|| format!("Failed to write {path}"))
}

// sha256 of remote files, keyed by URL. Only immutable URLs go in here.
pub(super) struct HashCache {
    path: Utf8PathBuf,
    hashes: BTreeMap<String, String>,
}

impl HashCache {
    pub(super) fn load(path: Utf8PathBuf) -> Result<Self> {
        let hashes = if path.exists() {
            serde_json::from_str(&fs::read_to_string(&path)?)
                .with_context(|| format!("Invalid {path}"))?
        } else {
            BTreeMap::new()
        };
        Ok(Self { path, hashes })
    }

    pub(super) fn sha256(&mut self, url: &str) -> Result<String> {
        if let Some(hash) = self.hashes.get(url) {
            return Ok(hash.clone());
        }
        let hash = sha256_url(url)?;
        self.insert(url, hash.clone())?;
        Ok(hash)
    }

    // From the `<url>.sha256` file the vendor publishes next to the file.
    pub(super) fn published_sha256(&mut self, url: &str) -> Result<String> {
        if let Some(hash) = self.hashes.get(url) {
            return Ok(hash.clone());
        }
        let sha_url = format!("{url}.sha256");
        let output = Command::new("curl")
            .args(["-fsSL", "--retry", "3", &sha_url])
            .output()
            .context("Failed to run curl")?;
        ensure!(
            output.status.success(),
            "Failed to download {sha_url}: {}",
            output.status
        );
        let content = String::from_utf8(output.stdout)?;
        // "<hash>" or "<hash>  <file name>"
        let hash = content.split_whitespace().next().unwrap_or_default();
        ensure!(
            hash.len() == 64 && hash.chars().all(|c| c.is_ascii_hexdigit()),
            "No sha256 in {sha_url}: {content:?}"
        );
        let hash = hash.to_ascii_lowercase();
        self.insert(url, hash.clone())?;
        Ok(hash)
    }

    pub(super) fn insert(&mut self, url: &str, hash: String) -> Result<()> {
        self.hashes.insert(url.to_owned(), hash);
        let mut content = serde_json::to_string_pretty(&self.hashes)?;
        content.push('\n');
        fs::write(&self.path, content).with_context(|| format!("Failed to write {}", self.path))
    }
}

pub(super) fn download(shell: &xshell::Shell, url: &str, dest: &Utf8Path) -> Result<()> {
    println!("Downloading {url}...");
    cmd!(shell, "curl -fsSL --retry 3 -o {dest} {url}")
        .quiet()
        .run()
        .with_context(|| format!("Failed to download {url}"))
}

pub(super) fn sha256_file(path: &Utf8Path) -> Result<String> {
    let mut file = fs::File::open(path).with_context(|| format!("Failed to open {path}"))?;
    sha256_reader(&mut file)
}

// Streams the download into the hasher, nothing is written to disk.
fn sha256_url(url: &str) -> Result<String> {
    println!("Hashing {url}...");
    let mut curl = Command::new("curl")
        .args(["-fsSL", "--retry", "3", url])
        .stdout(Stdio::piped())
        .spawn()
        .context("Failed to run curl")?;
    let stdout = curl.stdout.as_mut().context("No curl stdout")?;
    let hash = sha256_reader(stdout);
    let status = curl.wait()?;
    ensure!(status.success(), "Failed to download {url}: {status}");
    hash
}

fn sha256_reader(reader: &mut impl io::Read) -> Result<String> {
    let mut hasher = IoWrapper(Sha256::new());
    io::copy(reader, &mut hasher)?;
    let IoWrapper(hasher) = hasher;
    Ok(hex::encode(hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use super::{super::Arch, *};

    #[test]
    fn archive_serializes_flatpak_keys() {
        let source = Source::Archive(Archive {
            url: Some("https://example.com/a.zip".to_owned()),
            sha256: "abc".to_owned(),
            dest: "dir".to_owned(),
            strip_components: Some(0),
            only_arches: vec![Arch::Aarch64.to_string()],
            ..Default::default()
        });
        let json = serde_json::to_string(&serde_json::to_value(&source).unwrap()).unwrap();
        assert_eq!(
            json,
            r#"{"dest":"dir","only-arches":["aarch64"],"sha256":"abc","strip-components":0,"type":"archive","url":"https://example.com/a.zip"}"#
        );
    }
}
