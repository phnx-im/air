// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! The screenshots the Linux metainfo links to.
//!
//! They are served from the packages bucket as numbered sets
//! (`screenshots/v1/...`), and a published set is never changed: Flathub,
//! software centers and older releases keep pointing at it. The lock file
//! `SHA256SUMS` pins the current set version to the hashes of its files, so a
//! PR that changes a shot fails `check` until `fix` moves it to a new set. Two
//! PRs moving to the same version conflict on the lock file.
//!
//! `fix` numbers the new set after the one on the base branch, so running it
//! again in the same PR keeps the version instead of skipping one.

use std::{collections::BTreeMap, fs};

use anyhow::{Context, Result, ensure};
use camino::{Utf8Path, Utf8PathBuf};
use clap::{Args, Subcommand};
use regex::Regex;
use sha2::{Digest, Sha256};
use xshell::{Shell, cmd};

use crate::{publish_linux_packages::PACKAGES_CACHE_CONTROL, util::workspace_root};

const SCREENSHOTS_DIR: &str = "app/stores/linux/screenshots";
const LOCK_FILE: &str = "SHA256SUMS";
const METAINFO: &str = "app/linux/ms.air.Air.metainfo.xml";

const BASE_URL: &str = "https://packages.air.ms/screenshots/";
// Bucket key prefix the base URL is served from.
const S3_PREFIX: &str = "screenshots";

const LFS_POINTER_HEADER: &str = "version https://git-lfs.github.com/spec/v1";

#[derive(Args, Debug)]
pub(crate) struct ScreenshotsArgs {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Check that the lock file and the metainfo match the screenshots.
    Check,
    /// Move to a new screenshot set if the screenshots changed since the base
    /// branch, and point the metainfo at the current one.
    Fix(FixArgs),
    /// Upload the current screenshot set, unless it is already published.
    Publish(PublishArgs),
}

#[derive(Args, Debug)]
struct FixArgs {
    /// Git ref whose set the new one is numbered after. Without it, the set
    /// in the working tree is used.
    #[arg(long, default_value = "origin/main")]
    base: String,
}

#[derive(Args, Debug)]
struct PublishArgs {
    /// S3 bucket to upload to.
    #[arg(short = 'b', long = "s3-bucket", env = "S3_BUCKET")]
    s3_bucket: String,

    /// S3 endpoint URL (for MinIO, Cloudflare R2, ...).
    #[arg(long = "s3-endpoint", env = "S3_ENDPOINT")]
    s3_endpoint: Option<String>,

    /// Check what is published, but skip all uploads.
    #[arg(long)]
    dry_run: bool,
}

pub(crate) fn run(args: ScreenshotsArgs) -> Result<()> {
    let root = workspace_root();
    match args.command {
        Command::Check => check(&root),
        Command::Fix(args) => fix(&root, args),
        Command::Publish(args) => publish(&root, args),
    }
}

/// A screenshot set: its version and the hash of each file, keyed by its path
/// relative to the screenshots dir (e.g. `en-US/1_private_chat.png`).
#[derive(Debug, PartialEq, Eq)]
struct Lock {
    version: u32,
    files: BTreeMap<String, String>,
}

impl Lock {
    fn parse(content: &str) -> Result<Self> {
        let mut lines = content.lines();
        let version = lines
            .next()
            .and_then(|line| line.strip_prefix("# v"))
            .context("Lock file does not start with `# v<version>`")?
            .parse()
            .context("Invalid version in lock file")?;
        let mut files = BTreeMap::new();
        for line in lines.filter(|line| !line.is_empty()) {
            let (hash, path) = line
                .split_once("  ")
                .with_context(|| format!("Invalid lock file line: {line}"))?;
            files.insert(path.to_owned(), hash.to_owned());
        }
        Ok(Self { version, files })
    }

    fn render(&self) -> String {
        let mut out = format!("# v{}\n", self.version);
        for (path, hash) in &self.files {
            out.push_str(&format!("{hash}  {path}\n"));
        }
        out
    }
}

fn check(root: &Utf8Path) -> Result<()> {
    let lock = load_lock(root)?.context("No lock file, run `cargo xtask linux-screenshots fix`")?;
    let current = hash_screenshots(root)?;
    ensure!(
        lock.files == current,
        "The screenshots changed since set v{}, run `cargo xtask linux-screenshots fix` to move \
         to a new set",
        lock.version
    );
    check_metainfo(root, &lock)?;
    println!("Screenshots match set v{}", lock.version);
    Ok(())
}

fn fix(root: &Utf8Path, args: FixArgs) -> Result<()> {
    let files = hash_screenshots(root)?;
    let lock = match load_base_lock(root, &args.base)? {
        Some(lock) if lock.files == files => lock,
        Some(lock) => Lock {
            version: lock.version + 1,
            files,
        },
        None => Lock { version: 1, files },
    };
    fs::write(lock_path(root), lock.render())?;

    let path = root.join(METAINFO);
    let metainfo = fs::read_to_string(&path)?;
    let updated = url_regex()
        .replace_all(&metainfo, |caps: &regex::Captures| {
            format!("{BASE_URL}v{}/{}", lock.version, &caps["path"])
        })
        .into_owned();
    fs::write(&path, updated)?;

    check_metainfo(root, &lock)?;
    println!("Screenshots are set v{}", lock.version);
    Ok(())
}

fn publish(root: &Utf8Path, args: PublishArgs) -> Result<()> {
    check(root)?;
    let lock = load_lock(root)?.expect("checked above");

    let shell = Shell::new()?;
    // Same as in publish-packages: S3-compatible providers reject the newer
    // flow checksums AWS CLI v2 sends by default.
    if args.s3_endpoint.is_some() {
        for key in [
            "AWS_REQUEST_CHECKSUM_CALCULATION",
            "AWS_RESPONSE_CHECKSUM_VALIDATION",
        ] {
            if shell.var(key).is_err() {
                shell.set_var(key, "when_required");
            }
        }
    }
    let mut endpoint_args = Vec::new();
    if let Some(endpoint) = &args.s3_endpoint {
        endpoint_args.extend(["--endpoint-url", endpoint.as_str()]);
    }
    let bucket = &args.s3_bucket;

    for (path, hash) in &lock.files {
        let file = root.join(SCREENSHOTS_DIR).join(path);
        ensure!(
            !is_lfs_pointer(&fs::read(&file)?),
            "{file} is a Git LFS pointer, run `git lfs pull` first"
        );
        let key = format!("{S3_PREFIX}/v{}/{path}", lock.version);
        let aws_args = &endpoint_args;

        let head = cmd!(
            shell,
            "aws {aws_args...} s3api head-object --bucket {bucket} --key {key}"
        )
        .quiet()
        .ignore_status()
        .output()?;
        if head.status.success() {
            let head: serde_json::Value = serde_json::from_slice(&head.stdout)?;
            let published = head["Metadata"]["sha256"].as_str();
            ensure!(
                published == Some(hash.as_str()),
                "{key} is already published with different content, a published set must not \
                 change"
            );
            println!("{key} is already published");
            continue;
        }
        // Anything but a missing object, like a denied request, must not lead
        // to an upload that might replace it.
        let stderr = String::from_utf8_lossy(&head.stderr);
        ensure!(
            stderr.contains("Not Found") || stderr.contains("404"),
            "Failed to look up {key}: {stderr}"
        );

        let metadata = format!("sha256={hash}");
        let cmd = cmd!(
            shell,
            "aws {aws_args...} s3 cp --quiet {file} s3://{bucket}/{key} --content-type image/png --cache-control {PACKAGES_CACHE_CONTROL} --metadata {metadata} --acl public-read"
        );
        if args.dry_run {
            println!("Dry-run, skipping: {cmd}");
        } else {
            cmd.run()?;
            println!("Uploaded {key}");
        }
    }
    Ok(())
}

fn check_metainfo(root: &Utf8Path, lock: &Lock) -> Result<()> {
    let metainfo = fs::read_to_string(root.join(METAINFO))?;
    let mut found = false;
    for caps in url_regex().captures_iter(&metainfo) {
        found = true;
        let url = &caps[0];
        ensure!(
            caps["version"].parse::<u32>()? == lock.version,
            "{METAINFO} links {url}, but the current set is v{}, run `cargo xtask \
             linux-screenshots fix`",
            lock.version
        );
        ensure!(
            lock.files.contains_key(&caps["path"]),
            "{METAINFO} links {url}, which is not in {SCREENSHOTS_DIR}"
        );
    }
    ensure!(found, "{METAINFO} links no screenshots under {BASE_URL}");
    Ok(())
}

fn url_regex() -> Regex {
    let base = regex::escape(BASE_URL);
    Regex::new(&format!(r"{base}v(?P<version>\d+)/(?P<path>[^<\s]+)")).expect("valid regex")
}

fn lock_path(root: &Utf8Path) -> Utf8PathBuf {
    root.join(SCREENSHOTS_DIR).join(LOCK_FILE)
}

/// The lock file at [base], or the one in the working tree if [base] does not
/// exist (e.g. a checkout without `origin`).
fn load_base_lock(root: &Utf8Path, base: &str) -> Result<Option<Lock>> {
    let shell = Shell::new()?;
    shell.change_dir(root);
    let commit = format!("{base}^{{commit}}");
    let base_exists = cmd!(shell, "git rev-parse --verify --quiet {commit}")
        .quiet()
        .ignore_stdout()
        .run()
        .is_ok();
    if !base_exists {
        eprintln!("{base} does not exist, numbering after the set in the working tree");
        return load_lock(root);
    }
    let spec = format!("{base}:{SCREENSHOTS_DIR}/{LOCK_FILE}");
    // Missing on the base branch, so this is the first set.
    let Ok(content) = cmd!(shell, "git show {spec}")
        .quiet()
        .ignore_stderr()
        .read()
    else {
        return Ok(None);
    };
    Lock::parse(&content)
        .with_context(|| format!("Failed to parse {spec}"))
        .map(Some)
}

fn load_lock(root: &Utf8Path) -> Result<Option<Lock>> {
    let path = lock_path(root);
    if !path.exists() {
        return Ok(None);
    }
    Lock::parse(&fs::read_to_string(&path)?)
        .with_context(|| format!("Failed to parse {path}"))
        .map(Some)
}

/// Hashes every PNG in the language dirs of the screenshots dir.
fn hash_screenshots(root: &Utf8Path) -> Result<BTreeMap<String, String>> {
    let dir = root.join(SCREENSHOTS_DIR);
    let mut files = BTreeMap::new();
    for lang in dir.read_dir_utf8()? {
        let lang = lang?;
        if !lang.file_type()?.is_dir() {
            continue;
        }
        for entry in lang.path().read_dir_utf8()? {
            let entry = entry?;
            if entry.path().extension() != Some("png") {
                continue;
            }
            // Follows the symlinks into the goldens.
            let content = fs::read(entry.path())
                .with_context(|| format!("Failed to read {}", entry.path()))?;
            let path = format!("{}/{}", lang.file_name(), entry.file_name());
            files.insert(path, content_hash(&content)?);
        }
    }
    ensure!(!files.is_empty(), "No screenshots in {dir}");
    Ok(files)
}

/// The SHA-256 of a file's content. A Git LFS pointer carries it as its oid,
/// so checking works on a checkout without LFS.
fn content_hash(content: &[u8]) -> Result<String> {
    if is_lfs_pointer(content) {
        let pointer = std::str::from_utf8(content)?;
        let oid = pointer
            .lines()
            .find_map(|line| line.strip_prefix("oid sha256:"))
            .context("Git LFS pointer without a sha256 oid")?;
        return Ok(oid.to_owned());
    }
    Ok(hex::encode(Sha256::digest(content)))
}

fn is_lfs_pointer(content: &[u8]) -> bool {
    content.starts_with(LFS_POINTER_HEADER.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lock_round_trips() {
        let lock = Lock {
            version: 3,
            files: BTreeMap::from([
                ("en-US/1_a.png".to_owned(), "aa".to_owned()),
                ("en-US/2_b.png".to_owned(), "bb".to_owned()),
            ]),
        };
        assert_eq!(Lock::parse(&lock.render()).unwrap(), lock);
    }

    #[test]
    fn lfs_pointer_hash_is_its_oid() {
        let content = b"some png";
        let hash = content_hash(content).unwrap();
        let pointer = format!("{LFS_POINTER_HEADER}\noid sha256:{hash}\nsize 8\n");
        assert_eq!(content_hash(pointer.as_bytes()).unwrap(), hash);
    }

    #[test]
    fn url_regex_matches_set_urls() {
        let caps = url_regex()
            .captures("<image>https://packages.air.ms/screenshots/v12/en-US/1_a.png</image>")
            .unwrap();
        assert_eq!(&caps["version"], "12");
        assert_eq!(&caps["path"], "en-US/1_a.png");
    }
}
