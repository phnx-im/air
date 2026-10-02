// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{
    collections::{BTreeMap, HashMap},
    env, fmt, fs,
};

use anyhow::{Context, Result, bail, ensure};
use camino::{Utf8Path, Utf8PathBuf};
use clap::{Args, ValueEnum};
use xshell::{Shell, cmd};

// APT requires a component in the path/Release file. Hardcoded to "main"
// single-component repos are standard for small projects.
const APT_COMPONENT: &str = "main";

// Keep only the N most recent versions of each package per architecture so the
// pool doesn't grow unbounded across releases. Files are removed from the local
// working tree; the subsequent `aws s3 sync --delete` propagates removals.
const KEEP_VERSIONS: usize = 10;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PkgType {
    Deb,
    Rpm,
}

impl fmt::Display for PkgType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Deb => write!(f, "deb"),
            Self::Rpm => write!(f, "rpm"),
        }
    }
}

// Name of the package in the repositories, as set in the nFPM config.
const PACKAGE_NAME: &str = "air";

// Architectures every release is built for.
const DEB_ARCHS: [&str; 2] = ["amd64", "arm64"];
const RPM_ARCHS: [&str; 2] = ["x86_64", "aarch64"];

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub(crate) enum Track {
    Nightly,
    Beta,
    Stable,
    /// Legacy layout, a transitional mirror of nightly.
    Unstable,
}

impl Track {
    fn name(self) -> &'static str {
        match self {
            Self::Nightly => "nightly",
            Self::Beta => "beta",
            Self::Stable => "stable",
            Self::Unstable => "unstable",
        }
    }

    // Pool dir relative to the deb root. Legacy layout uses "pool/main".
    fn deb_pool_dir(self) -> String {
        match self {
            Self::Unstable => format!("pool/{APT_COMPONENT}"),
            _ => format!("pool/{self}/{APT_COMPONENT}"),
        }
    }

    // Origin and Label of the Release file. Legacy layout keeps its original
    // value, apt rejects a change.
    fn deb_origin(self) -> &'static str {
        match self {
            Self::Unstable => "Custom",
            _ => "Air",
        }
    }

    // Repo dir relative to the rpm root. Legacy layout uses "main".
    fn rpm_dir(self) -> &'static str {
        match self {
            Self::Unstable => "main",
            _ => self.name(),
        }
    }

    // Section and file name of the .repo file. Legacy layout has none.
    fn rpm_repo_id(self) -> Option<String> {
        match self {
            Self::Unstable => None,
            Self::Stable => Some(PACKAGE_NAME.to_owned()),
            _ => Some(format!("{PACKAGE_NAME}-{self}")),
        }
    }

    fn rpm_repo_name(self) -> String {
        match self {
            Self::Stable => "Air".to_owned(),
            _ => format!("Air ({self})"),
        }
    }
}

impl fmt::Display for Track {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

#[derive(Args, Debug)]
pub(crate) struct RepoArgs {
    /// S3 bucket to operate on.
    #[arg(short = 'b', long = "s3-bucket", env = "S3_BUCKET")]
    s3_bucket: String,

    /// Optional key prefix (e.g. "releases").
    #[arg(short = 'p', long = "prefix", env = "S3_PREFIX")]
    prefix: Option<String>,

    /// GPG key fingerprint/email to sign with.
    #[arg(short = 'k', long = "gpg-key-id", env = "GPG_KEY_ID")]
    gpg_key_id: String,

    /// S3 endpoint URL (for MinIO, Cloudflare R2, ...).
    #[arg(long = "s3-endpoint", env = "S3_ENDPOINT")]
    s3_endpoint: Option<String>,

    /// Public download base URL (bucket root) shown in client-setup
    /// instructions. The task appends "/deb" or "/rpm" automatically.
    #[arg(long = "repository-base-url", env = "REPOSITORY_BASE_URL")]
    repository_base_url: String,

    /// Build and sign the repository locally, but skip all uploads.
    #[arg(long, action = clap::ArgAction::SetTrue)]
    dry_run: bool,
}

#[derive(Args, Debug)]
pub(crate) struct PublishArgs {
    /// Package files (.deb or .rpm, all of one type) to publish. May span
    /// several architectures.
    #[arg(required = true, num_args = 1..)]
    package_files: Vec<Utf8PathBuf>,

    /// Release track to publish to.
    #[arg(long, value_enum, default_value_t = Track::Nightly, env = "TRACK")]
    track: Track,

    #[command(flatten)]
    repo: RepoArgs,
}

#[derive(Args, Debug)]
pub(crate) struct PromoteArgs {
    /// Track to take the packages from.
    #[arg(long, value_enum)]
    from: Track,

    /// Track to publish the packages to.
    #[arg(long, value_enum)]
    to: Track,

    /// Package version to promote (e.g. "0.23.0-1490").
    #[arg(long)]
    version: String,

    #[command(flatten)]
    repo: RepoArgs,
}

struct Config {
    bucket: String,
    prefix: Option<String>,
    gpg_key_id: String,
    s3_endpoint: Option<String>,
    repository_base_url: String,
    dry_run: bool,
    workdir: Utf8PathBuf,
}

impl Config {
    fn s3_path(&self, suffix: &str) -> String {
        if let Some(prefix) = self.prefix.as_deref() {
            let prefix = prefix.trim_end_matches('/');
            format!("s3://{}/{prefix}/{suffix}", self.bucket)
        } else {
            format!("s3://{}/{suffix}", self.bucket)
        }
    }

    fn aws_args(&self) -> Vec<&str> {
        let mut args = Vec::new();
        if let Some(endpoint) = self.s3_endpoint.as_deref() {
            args.push("--endpoint-url");
            args.push(endpoint);
        }
        args
    }

    fn workdir(&self, path: impl AsRef<Utf8Path>) -> Utf8PathBuf {
        self.workdir.join(path)
    }

    fn sync(&self, shell: &Shell, upload: &Upload, delete: bool) -> Result<()> {
        let Upload {
            local,
            remote,
            cache_control,
            exclude,
            ..
        } = upload;
        let aws_args = self.aws_args();
        let mut opts = Vec::new();
        if delete {
            opts.push("--delete");
        }
        if let Some(exclude) = exclude {
            opts.extend(["--exclude", exclude]);
        }
        let cmd = cmd!(
            shell,
            "aws {aws_args...} s3 sync --quiet {local} {remote} {opts...} --cache-control {cache_control} --acl public-read"
        );
        if self.dry_run {
            println!("Dry-run, skipping: {cmd}");
        } else {
            cmd.run()?;
        }
        Ok(())
    }

    // Packages go up before any index, and pruned packages are removed only
    // after no index refers to them anymore.
    fn upload(&self, shell: &Shell, uploads: &Uploads) -> Result<()> {
        println!("Uploading packages (immutable, long TTL)...");
        for upload in &uploads.packages {
            self.sync(shell, upload, false)?;
        }
        println!("Uploading indexes and keys (short TTL)...");
        for upload in &uploads.metadata {
            self.sync(shell, upload, upload.delete)?;
        }
        println!("Removing pruned packages...");
        for upload in &uploads.packages {
            self.sync(shell, upload, upload.delete)?;
        }
        Ok(())
    }

    fn repo_url(&self, pkg_type: PkgType) -> String {
        format!("{}/{pkg_type}", self.repository_base_url)
    }
}

// A local dir that is synced to S3.
struct Upload {
    local: Utf8PathBuf,
    remote: String,
    cache_control: &'static str,
    exclude: Option<&'static str>,
    // Removes remote files that are missing locally.
    delete: bool,
}

#[derive(Default)]
struct Uploads {
    packages: Vec<Upload>,
    metadata: Vec<Upload>,
}

const PACKAGES_CACHE_CONTROL: &str = "public, max-age=31536000, immutable";
const INDEX_CACHE_CONTROL: &str = "public, max-age=300";
const KEY_CACHE_CONTROL: &str = "public, max-age=86400";

pub(crate) fn run(args: PublishArgs) -> Result<()> {
    let shell = Shell::new()?;

    let (package_files, pkg_type) = check_package_files(&args.package_files)?;
    let cfg = setup(&shell, args.repo)?;

    for file in &package_files {
        println!("Package\t: {}", file.file_name().unwrap_or(file.as_str()));
    }
    println!("Track\t: {}", args.track);

    let mut uploads = Uploads::default();
    match pkg_type {
        PkgType::Deb => build_deb(&shell, &cfg, args.track, &package_files, &mut uploads)?,
        PkgType::Rpm => build_rpm(&shell, &cfg, args.track, &package_files, true, &mut uploads)?,
    }
    cfg.upload(&shell, &uploads)
}

pub(crate) fn promote(args: PromoteArgs) -> Result<()> {
    let PromoteArgs {
        from,
        to,
        version,
        repo,
    } = args;
    ensure!(from != to, "--from and --to must differ, both are {from}");
    ensure!(
        !version.is_empty()
            && version
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || ".+~-".contains(c)),
        "Invalid version: {version:?}"
    );

    let shell = Shell::new()?;
    let cfg = setup(&shell, repo)?;
    println!("Promote\t: {PACKAGE_NAME} {version} from {from} to {to}");

    let promote_dir = cfg.workdir("promote");
    if promote_dir.exists() {
        fs::remove_dir_all(&promote_dir)
            .with_context(|| format!("Failed to remove {promote_dir}"))?;
    }
    fs::create_dir_all(&promote_dir)?;

    // Download and build everything first, so a missing file or a failed
    // build leaves `to` untouched.
    let download = |remote: String, name: String| -> Result<Utf8PathBuf> {
        let src = cfg.s3_path(&remote);
        let dst = promote_dir.join(name);
        println!("Downloading {src}...");
        let aws_args = cfg.aws_args();
        cmd!(shell, "aws {aws_args...} s3 cp --quiet {src} {dst}")
            .run()
            .with_context(|| {
                format!(
                    "Failed to download {src} (version {version} must exist in {from} for all arches)"
                )
            })?;
        Ok(dst)
    };
    let mut debs = Vec::new();
    for arch in DEB_ARCHS {
        let name = format!("{PACKAGE_NAME}_{version}_{arch}.deb");
        let remote = format!("deb/{}/{name}", from.deb_pool_dir());
        debs.push(download(remote, name)?);
    }
    let mut rpms = Vec::new();
    for arch in RPM_ARCHS {
        let name = format!("{PACKAGE_NAME}-{version}.{arch}.rpm");
        let remote = format!("rpm/{}/{arch}/{name}", from.rpm_dir());
        rpms.push(download(remote, name)?);
    }

    let mut uploads = Uploads::default();
    build_deb(&shell, &cfg, to, &debs, &mut uploads)?;
    // Already signed in `from`, re-signing would change the bytes.
    build_rpm(&shell, &cfg, to, &rpms, false, &mut uploads)?;
    cfg.upload(&shell, &uploads)
}

fn check_package_files(files: &[Utf8PathBuf]) -> Result<(Vec<Utf8PathBuf>, PkgType)> {
    let mut package_files = Vec::with_capacity(files.len());
    let mut pkg_type = None;
    for file in files {
        ensure!(file.exists(), "File not found: {file}");
        let file = file.canonicalize_utf8()?;
        let file_type = match file.extension() {
            Some("deb") => PkgType::Deb,
            Some("rpm") => PkgType::Rpm,
            _ => bail!("Cannot detect package type from filename: {file}"),
        };
        ensure!(
            *pkg_type.get_or_insert(file_type) == file_type,
            "All package files must have the same type, got mixed: {file}",
        );
        package_files.push(file);
    }
    let pkg_type = pkg_type.context("No package files given")?;
    Ok((package_files, pkg_type))
}

fn setup(shell: &Shell, args: RepoArgs) -> Result<Config> {
    // Some S3-compatible providers (Upcloud, some MinIO versions, ...) reject
    // the newer flow checksums AWS CLI v2 sends by default. "when_required"
    // only emits checksums when the server asks for them.
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

    // Trim trailing slash so client-setup snippets don't end up with "//".
    let repository_base_url = args.repository_base_url.trim_end_matches('/').to_owned();

    let workdir = Utf8PathBuf::from(cmd!(shell, "git rev-parse --show-toplevel").read()?)
        .join("app/linux/package-builds");

    let cfg = Config {
        bucket: args.s3_bucket,
        prefix: args.prefix,
        gpg_key_id: args.gpg_key_id,
        s3_endpoint: args.s3_endpoint,
        repository_base_url,
        dry_run: args.dry_run,
        workdir,
    };

    println!("URL\t: {}", cfg.repository_base_url);
    println!("Workdir\t: {}", cfg.workdir);
    println!("GPG key\t: {}", cfg.gpg_key_id);
    if cfg.dry_run {
        eprintln!("Dry-run mode: the repository is built locally but not uploaded.");
    }

    Ok(cfg)
}

fn write_text(path: &Utf8Path, mut content: String) -> Result<()> {
    if !content.ends_with('\n') {
        content.push('\n');
    }
    fs::write(path, content).with_context(|| format!("Failed to write {path}"))
}

fn dpkg_field(shell: &Shell, deb: &Utf8Path, field: &str) -> Result<String> {
    let deb_str = deb.as_str();
    Ok(cmd!(shell, "dpkg-deb -f {deb_str} {field}")
        .quiet()
        .read()?
        .trim()
        .to_string())
}

fn dpkg_version_gt(shell: &Shell, a: &str, b: &str) -> bool {
    cmd!(shell, "dpkg --compare-versions {a} gt {b}")
        .quiet()
        .ignore_stdout()
        .ignore_stderr()
        .run()
        .is_ok()
}

fn prune_deb_pool(shell: &Shell, pool: &Utf8Path, keep: usize) -> Result<()> {
    let mut debs: Vec<Utf8PathBuf> = Vec::new();
    for entry in pool.read_dir_utf8()? {
        let path = entry?.path().to_path_buf();
        if path.extension() == Some("deb") {
            debs.push(path);
        }
    }
    if debs.is_empty() {
        return Ok(());
    }

    // Group by "<Package>_<Architecture>" and sort each group newest-first
    // using dpkg's version comparison (handles epochs, ~rc/~beta, etc.).
    let mut groups: HashMap<String, Vec<(String, Utf8PathBuf)>> = HashMap::new();
    for deb in debs {
        let name = dpkg_field(shell, &deb, "Package")?;
        let arch = dpkg_field(shell, &deb, "Architecture")?;
        let version = dpkg_field(shell, &deb, "Version")?;
        groups
            .entry(format!("{name}_{arch}"))
            .or_default()
            .push((version, deb));
    }

    let mut removed = 0usize;
    for (_, mut entries) in groups {
        // Selection sort newest first. O(n²) is fine — counts per group stay small.
        let mut sorted: Vec<(String, Utf8PathBuf)> = Vec::with_capacity(entries.len());
        while !entries.is_empty() {
            let mut max_idx = 0;
            for i in 1..entries.len() {
                if dpkg_version_gt(shell, &entries[i].0, &entries[max_idx].0) {
                    max_idx = i;
                }
            }
            sorted.push(entries.remove(max_idx));
        }
        for (_, file) in sorted.into_iter().skip(keep) {
            let name = file.file_name().unwrap_or_else(|| file.as_str());
            println!("Pruning old package: {name}");
            fs::remove_file(&file).with_context(|| format!("Failed to remove {file}"))?;
            removed += 1;
        }
    }
    if removed > 0 {
        println!("Pruned {removed} old .deb(s); keeping last {keep} per package/arch.");
    }
    Ok(())
}

fn prune_rpm_packages(shell: &Shell, repo_dir: &Utf8Path, keep: usize) -> Result<()> {
    // repomanage fails on a dir without rpms, e.g. the first publish of an arch.
    let mut has_rpms = false;
    for entry in repo_dir.read_dir_utf8()? {
        has_rpms |= entry?.path().extension() == Some("rpm");
    }
    if !has_rpms {
        return Ok(());
    }

    // repomanage only lists the old packages, one path per line.
    let keep_arg = format!("--keep={keep}");
    let repo_dir_str = repo_dir.as_str();
    let old = cmd!(shell, "dnf repomanage --old {keep_arg} {repo_dir_str}").read()?;
    let mut removed = 0usize;
    for line in old.lines().map(str::trim).filter(|l| l.ends_with(".rpm")) {
        let file = repo_dir.join(line);
        ensure!(
            file.starts_with(repo_dir),
            "repomanage listed a file outside {repo_dir}: {file}"
        );
        let name = file.file_name().unwrap_or(line);
        println!("Pruning old package: {name}");
        fs::remove_file(&file).with_context(|| format!("Failed to remove {file}"))?;
        removed += 1;
    }
    if removed > 0 {
        println!("Pruned {removed} old .rpm(s); keeping last {keep} per package.");
    }
    Ok(())
}

// createrepo_c only replaces the files of the previous repomd.xml. Older
// generations hydrated from S3 stay unless removed here.
fn prune_repodata(repodata: &Utf8Path) -> Result<()> {
    let repomd_path = repodata.join("repomd.xml");
    let repomd = fs::read_to_string(&repomd_path)
        .with_context(|| format!("Failed to read {repomd_path}"))?;
    let referenced: Vec<&str> = repomd
        .split("href=\"repodata/")
        .skip(1)
        .filter_map(|rest| rest.split('"').next())
        .collect();
    ensure!(!referenced.is_empty(), "No metadata files in {repomd_path}");

    for entry in repodata.read_dir_utf8()? {
        let entry = entry?;
        let name = entry.file_name();
        if name == "repomd.xml" || name == "repomd.xml.asc" || referenced.contains(&name) {
            continue;
        }
        fs::remove_file(entry.path())
            .with_context(|| format!("Failed to remove {}", entry.path()))?;
    }
    Ok(())
}

fn stage_file(file: &Utf8Path, dir: &Utf8Path) -> Result<Utf8PathBuf> {
    let name = file.file_name().context("package file has no filename")?;
    let staged = dir.join(name);
    fs::copy(file, &staged).with_context(|| format!("Failed to copy {file} to {staged}"))?;
    Ok(staged)
}

fn build_deb(
    shell: &Shell,
    cfg: &Config,
    track: Track,
    files: &[Utf8PathBuf],
    uploads: &mut Uploads,
) -> Result<()> {
    let deb_root = cfg.workdir("deb");
    let pool_rel = track.deb_pool_dir();
    let pool_dir = deb_root.join(&pool_rel);
    let key_dir = deb_root.join("keys");
    fs::create_dir_all(&pool_dir)?;
    fs::create_dir_all(&key_dir)?;

    let s3_deb = cfg.s3_path("deb");
    let dists_track_local = deb_root.join("dists").join(track.name());

    // Hydrate this track's pool and dists from S3 so the regenerated Packages
    // and Release files describe everything that was there before, plus the
    // new package. Other tracks are never touched.
    let pool_remote = format!("{s3_deb}/{pool_rel}");
    println!("Syncing existing pool from {pool_remote}...");
    let aws_args = cfg.aws_args();
    cmd!(
        shell,
        "aws {aws_args...} s3 sync --quiet {pool_remote} {pool_dir}"
    )
    .run()?;

    let dists_remote_track = format!("{s3_deb}/dists/{track}");
    println!("Syncing existing dists from {dists_remote_track}...");
    let aws_args = cfg.aws_args();
    cmd!(
        shell,
        "aws {aws_args...} s3 sync --quiet {dists_remote_track} {dists_track_local}"
    )
    .run()?;

    prune_deb_pool(shell, &pool_dir, KEEP_VERSIONS)?;

    println!("Staging packages into pool...");
    for file in files {
        stage_file(file, &pool_dir)?;
    }

    // Architectures of everything in the pool, including earlier releases.
    let mut archs = Vec::new();
    for entry in pool_dir.read_dir_utf8()? {
        let path = entry?.path().to_path_buf();
        if path.extension() == Some("deb") {
            archs.push(dpkg_field(shell, &path, "Architecture")?);
        }
    }
    archs.sort();
    archs.dedup();
    println!("Archs\t: {}", archs.join(" "));
    println!();

    // Release file signature provides repo-level integrity; per-package
    // signatures are intentionally omitted for DEB.
    let gpg_key_id = &cfg.gpg_key_id;
    let armored = cmd!(shell, "gpg --batch --yes --export --armor {gpg_key_id}").read()?;
    write_text(&key_dir.join("gpg-key.asc"), armored)?;

    println!("Running apt-ftparchive packages...");
    // cd into deb_root so apt-ftparchive embeds the pool path relative to the
    // deb root in the Filename: field, matching the URL clients construct.
    for arch in &archs {
        let dists_dir = dists_track_local
            .join(APT_COMPONENT)
            .join(format!("binary-{arch}"));
        fs::create_dir_all(&dists_dir)?;
        let packages_path = dists_dir.join("Packages");
        {
            let _pd = shell.push_dir(deb_root.as_std_path());
            let packages_out =
                cmd!(shell, "apt-ftparchive --arch {arch} packages {pool_rel}").read()?;
            write_text(&packages_path, packages_out)?;
        }
        let packages_str = packages_path.as_str();
        cmd!(shell, "gzip -9 -f -k {packages_str}").run()?;
        cmd!(shell, "bzip2 -9 -f -k {packages_str}").run()?;
        cmd!(shell, "xz -9 -f -k {packages_str}").run()?;
    }

    println!("Running apt-ftparchive release...");
    let release_dir = dists_track_local.clone();
    let release_dir_str = release_dir.as_str();
    let suite = track.name();
    let origin = track.deb_origin();
    let archs_opt = format!(
        "APT::FTPArchive::Release::Architectures={}",
        archs.join(" ")
    );
    let release_output = cmd!(
        shell,
        "apt-ftparchive
         -o APT::FTPArchive::Release::Origin={origin}
         -o APT::FTPArchive::Release::Label={origin}
         -o APT::FTPArchive::Release::Suite={suite}
         -o APT::FTPArchive::Release::Codename={suite}
         -o APT::FTPArchive::Release::Components={APT_COMPONENT}
         -o {archs_opt}
         -o APT::FTPArchive::Release::MD5=false
         -o APT::FTPArchive::Release::SHA1=false
         release {release_dir_str}"
    )
    .read()?;
    let release_path = release_dir.join("Release");
    write_text(&release_path, release_output)?;

    println!("Signing Release file...");
    let gpg_key_id = &cfg.gpg_key_id;
    let release_gpg = release_dir.join("Release.gpg");
    cmd!(
        shell,
        "gpg --batch --yes --default-key {gpg_key_id} --armor --detach-sign --output {release_gpg} {release_path}"
    ).run()?;

    let inrelease = release_dir.join("InRelease");
    cmd!(
        shell,
        "gpg --batch --yes --default-key {gpg_key_id} --armor --clearsign --output {inrelease} {release_path}"
    ).run()?;

    // Only this track's pool and dists, a sync of all of them with --delete
    // would wipe the other tracks, which were never hydrated.
    uploads.packages.push(Upload {
        local: pool_dir,
        remote: pool_remote,
        cache_control: PACKAGES_CACHE_CONTROL,
        exclude: None,
        delete: true,
    });
    uploads.metadata.push(Upload {
        local: dists_track_local,
        remote: dists_remote_track,
        cache_control: INDEX_CACHE_CONTROL,
        exclude: None,
        delete: true,
    });
    uploads.metadata.push(Upload {
        local: key_dir,
        remote: s3_deb.clone(),
        cache_control: KEY_CACHE_CONTROL,
        exclude: None,
        delete: false,
    });

    let repo_url = cfg.repo_url(PkgType::Deb);
    println!(
        r#"
DEB repository for {s3_deb} (suite {suite})

Client setup:
  curl -fsSL {repo_url}/gpg-key.asc \
    | sudo gpg --dearmor -o /usr/share/keyrings/air-keyring.gpg
  echo "deb [signed-by=/usr/share/keyrings/air-keyring.gpg] {repo_url} {suite} {APT_COMPONENT}" \
    | sudo tee /etc/apt/sources.list.d/air.list
  sudo apt update"#
    );

    Ok(())
}

// Restores ~/.rpmmacros to its original state when the guard drops, ensuring
// the user's macros file survives an `rpm --addsign` failure.
struct RpmMacrosGuard {
    path: Utf8PathBuf,
    original: Option<String>,
    restored: bool,
}

impl RpmMacrosGuard {
    fn install(path: Utf8PathBuf, contents: &str) -> Result<Self> {
        let original = if path.exists() {
            Some(fs::read_to_string(&path).with_context(|| format!("Failed to read {path}"))?)
        } else {
            None
        };
        fs::write(&path, contents).with_context(|| format!("Failed to write {path}"))?;
        Ok(Self {
            path,
            original,
            restored: false,
        })
    }
}

impl Drop for RpmMacrosGuard {
    fn drop(&mut self) {
        if self.restored {
            return;
        }
        self.restored = true;
        let result = match &self.original {
            Some(content) => fs::write(&self.path, content),
            None => fs::remove_file(&self.path),
        };
        if let Err(e) = result {
            eprintln!("warning: failed to restore {}: {e}", self.path);
        }
    }
}

// Without `sign` the packages are staged as is, e.g. already signed ones
// that are promoted between tracks.
fn build_rpm(
    shell: &Shell,
    cfg: &Config,
    track: Track,
    files: &[Utf8PathBuf],
    sign: bool,
    uploads: &mut Uploads,
) -> Result<()> {
    // Group the files by rpm arch.
    let queryformat = "%{ARCH}";
    let mut by_arch: BTreeMap<String, Vec<&Utf8Path>> = BTreeMap::new();
    for file in files {
        let pkg = file.as_str();
        let arch = cmd!(shell, "rpm -qp --queryformat {queryformat} {pkg}")
            .quiet()
            .ignore_stderr()
            .read()?
            .trim()
            .to_string();
        by_arch.entry(arch).or_default().push(file);
    }
    let archs: Vec<&str> = by_arch.keys().map(String::as_str).collect();
    println!("Archs\t: {}", archs.join(" "));
    println!();

    let rpm_root = cfg.workdir("rpm");
    // Recreated so no stale .repo file of another track gets uploaded.
    let key_dir = rpm_root.join("keys");
    if key_dir.exists() {
        fs::remove_dir_all(&key_dir).with_context(|| format!("Failed to remove {key_dir}"))?;
    }
    fs::create_dir_all(&key_dir)?;

    let s3_rpm = cfg.s3_path("rpm");
    let repo_url = cfg.repo_url(PkgType::Rpm);
    let track_dir = track.rpm_dir();

    let home_str = env::var("HOME").context("HOME is not set")?;
    let home = Utf8PathBuf::from(home_str);
    let macros_path = home.join(".rpmmacros");
    let macros_content = format!("%_signature gpg\n%_gpg_name  {key}\n", key = cfg.gpg_key_id,);
    let gpg_key_id = &cfg.gpg_key_id;

    for (arch, files) in &by_arch {
        println!("Publishing arch {arch}...");
        let repo_dir = rpm_root.join(track_dir).join(arch);
        fs::create_dir_all(&repo_dir)?;
        let s3_arch = format!("{s3_rpm}/{track_dir}/{arch}");

        // Hydrate the track/arch dir (existing .rpms + repodata/) so
        // createrepo_c --update can incrementally extend the previous metadata.
        println!("Syncing existing repo from {s3_arch}...");
        let aws_args = cfg.aws_args();
        cmd!(
            shell,
            "aws {aws_args...} s3 sync --quiet {s3_arch} {repo_dir}"
        )
        .run()?;

        prune_rpm_packages(shell, &repo_dir, KEEP_VERSIONS)?;

        if sign {
            println!("Staging and signing .rpm with GPG key: {gpg_key_id}");
            let _guard = RpmMacrosGuard::install(macros_path.clone(), &macros_content)?;
            for file in files {
                let staged = stage_file(file, &repo_dir)?;
                let staged_str = staged.as_str();
                cmd!(shell, "rpm --addsign {staged_str}").run()?;
            }
        } else {
            println!("Staging already signed .rpm...");
            for file in files {
                stage_file(file, &repo_dir)?;
            }
        }

        println!("Running createrepo_c...");
        let repo_dir_str = repo_dir.as_str();
        cmd!(shell, "createrepo_c --update {repo_dir_str}").run()?;
        prune_repodata(&repo_dir.join("repodata"))?;

        println!("Signing repomd.xml...");
        let repomd = repo_dir.join("repodata/repomd.xml");
        let repomd_asc = repo_dir.join("repodata/repomd.xml.asc");
        cmd!(
            shell,
            "gpg --batch --yes --default-key {gpg_key_id} --armor --detach-sign --output {repomd_asc} {repomd}"
        ).run()?;

        // createrepo_c replaces the metadata files, --delete drops the old
        // ones on S3.
        uploads.metadata.push(Upload {
            local: repo_dir.join("repodata"),
            remote: format!("{s3_arch}/repodata"),
            cache_control: INDEX_CACHE_CONTROL,
            exclude: None,
            delete: true,
        });
        uploads.packages.push(Upload {
            local: repo_dir,
            remote: s3_arch,
            cache_control: PACKAGES_CACHE_CONTROL,
            exclude: Some("repodata/*"),
            delete: true,
        });
    }

    let armored = cmd!(shell, "gpg --batch --yes --export --armor {gpg_key_id}").read()?;
    write_text(&key_dir.join("gpg-key.asc"), armored)?;

    // Generate a .repo file so clients can install via
    // `dnf config-manager addrepo --from-repofile <url>`. dnf expands
    // $basearch itself.
    let repo_id = track.rpm_repo_id();
    if let Some(repo_id) = &repo_id {
        let repo_file = key_dir.join(format!("{repo_id}.repo"));
        let repo_contents = format!(
            r#"[{repo_id}]
name={name}
baseurl={repo_url}/{track_dir}/$basearch
enabled=1
gpgcheck=1
repo_gpgcheck=1
gpgkey={repo_url}/gpg-key.asc
"#,
            name = track.rpm_repo_name(),
        );
        fs::write(&repo_file, repo_contents)
            .with_context(|| format!("Failed to write {repo_file}"))?;
    }

    uploads.metadata.push(Upload {
        local: key_dir,
        remote: s3_rpm.clone(),
        cache_control: KEY_CACHE_CONTROL,
        exclude: None,
        delete: false,
    });

    println!("RPM repository for {s3_rpm}/{track_dir}");

    match repo_id {
        Some(repo_id) => println!(
            "Client setup: sudo dnf config-manager addrepo --from-repofile {repo_url}/{repo_id}.repo"
        ),
        None => println!("Legacy layout, no .repo file: baseurl={repo_url}/{track_dir}/$basearch"),
    }

    Ok(())
}
