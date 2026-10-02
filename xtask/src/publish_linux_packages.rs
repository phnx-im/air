// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{
    collections::{BTreeMap, HashMap},
    env, fmt, fs,
};

use anyhow::{Context, Result, bail, ensure};
use camino::{Utf8Path, Utf8PathBuf};
use clap::Args;
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

#[derive(Args, Debug)]
pub(crate) struct PublishArgs {
    /// Package files (.deb or .rpm, all of one type) to publish. May span
    /// several architectures.
    #[arg(required = true, num_args = 1..)]
    package_files: Vec<Utf8PathBuf>,

    /// S3 bucket to operate on.
    #[arg(short = 'b', long = "s3-bucket", env = "S3_BUCKET")]
    s3_bucket: String,

    /// Optional key prefix (e.g. "releases").
    #[arg(short = 'p', long = "prefix", env = "S3_PREFIX")]
    prefix: Option<String>,

    /// Release track / APT suite name (e.g. "testing", "stable").
    /// Falls back to $TRACK; defaults to "testing".
    #[arg(long, default_value = "unstable", env = "TRACK")]
    track: String,

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

    /// Pass --dryrun to aws commands.
    #[arg(long, action = clap::ArgAction::SetTrue)]
    dry_run: bool,
}

struct Config {
    package_files: Vec<Utf8PathBuf>,
    pkg_type: PkgType,
    bucket: String,
    prefix: Option<String>,
    track: String,
    gpg_key_id: String,
    s3_endpoint: Option<String>,
    repo_url: String,
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
}

pub(crate) fn run(args: PublishArgs) -> Result<()> {
    let shell = Shell::new()?;

    let cfg = build_config(&shell, args)?;

    // Some S3-compatible providers (Upcloud, some MinIO versions, ...) reject
    // the newer flow checksums AWS CLI v2 sends by default. "when_required"
    // only emits checksums when the server asks for them.
    if cfg.s3_endpoint.is_some() {
        for key in [
            "AWS_REQUEST_CHECKSUM_CALCULATION",
            "AWS_RESPONSE_CHECKSUM_VALIDATION",
        ] {
            if shell.var(key).is_err() {
                shell.set_var(key, "when_required");
            }
        }
    }

    for file in &cfg.package_files {
        println!("Package\t: {}", file.file_name().unwrap_or(file.as_str()));
    }
    println!("URL\t: {}", cfg.repo_url);
    println!("Workdir\t: {}", cfg.workdir);
    println!("GPG key\t: {}", cfg.gpg_key_id);

    if cfg.dry_run {
        eprintln!("Dry-run mode: no changes will be made.");
    }

    match cfg.pkg_type {
        PkgType::Deb => publish_deb(&shell, &cfg),
        PkgType::Rpm => publish_rpm(&shell, &cfg),
    }
}

fn build_config(shell: &Shell, args: PublishArgs) -> Result<Config> {
    let mut package_files = Vec::with_capacity(args.package_files.len());
    let mut pkg_type = None;
    for file in &args.package_files {
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

    // Trim trailing slash so client-setup snippets don't end up with "//".
    let repository_base_url = args.repository_base_url.trim_end_matches('/');

    let workdir = Utf8PathBuf::from(cmd!(shell, "git rev-parse --show-toplevel").read()?)
        .join("app/linux/package-builds");

    Ok(Config {
        package_files,
        pkg_type,
        bucket: args.s3_bucket,
        prefix: args.prefix,
        track: args.track,
        gpg_key_id: args.gpg_key_id,
        s3_endpoint: args.s3_endpoint,
        repo_url: format!("{repository_base_url}/{pkg_type}"),
        dry_run: args.dry_run,
        workdir,
    })
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

fn publish_deb(shell: &Shell, cfg: &Config) -> Result<()> {
    let deb_root = cfg.workdir("deb");
    let pool_dir = deb_root.join("pool").join(APT_COMPONENT);
    let key_dir = deb_root.join("keys");
    fs::create_dir_all(&pool_dir)?;
    fs::create_dir_all(&key_dir)?;

    let s3_deb = cfg.s3_path("deb");
    let pool_local = deb_root.join("pool");
    let dists_local = deb_root.join("dists");
    let dists_track_local = dists_local.join(&cfg.track);

    // Hydrate pool/ and dists/${TRACK}/ from S3 so the regenerated Packages
    // and Release files describe everything that was there before, plus the
    // new package.
    let pool_remote = format!("{s3_deb}/pool");
    println!("Syncing existing pool from {pool_remote}...");
    let aws_args = cfg.aws_args();
    cmd!(
        shell,
        "aws {aws_args...} s3 sync --quiet {pool_remote} {pool_local}"
    )
    .run()?;

    let dists_remote_track = format!("{s3_deb}/dists/{}", cfg.track);
    println!("Syncing existing dists from {dists_remote_track}...");
    let aws_args = cfg.aws_args();
    cmd!(
        shell,
        "aws {aws_args...} s3 sync --quiet {dists_remote_track} {dists_track_local}"
    )
    .run()?;

    prune_deb_pool(shell, &pool_dir, KEEP_VERSIONS)?;

    println!("Staging packages into pool...");
    for file in &cfg.package_files {
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
    // cd into deb_root so apt-ftparchive embeds "pool/main/..." (relative to
    // dists/) in the Filename: field, matching the URL clients construct.
    let pool_rel = format!("pool/{APT_COMPONENT}");
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
    let track = &cfg.track;
    let archs_opt = format!(
        "APT::FTPArchive::Release::Architectures={}",
        archs.join(" ")
    );
    let release_output = cmd!(
        shell,
        "apt-ftparchive
         -o APT::FTPArchive::Release::Origin=Custom
         -o APT::FTPArchive::Release::Label=Custom
         -o APT::FTPArchive::Release::Suite={track}
         -o APT::FTPArchive::Release::Codename={track}
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

    println!("Uploading pool (immutable, long TTL)...");
    let aws_args = cfg.aws_args();
    cmd!(
        shell,
        "aws {aws_args...} s3 sync --quiet {pool_local} {pool_remote} --delete --cache-control 'public, max-age=31536000' --acl public-read"
    ).run()?;

    // Only this track's dists, a sync of all dists with --delete would wipe
    // the other tracks, which were never hydrated.
    println!("Uploading dists (index files, short TTL)...");
    let aws_args = cfg.aws_args();
    cmd!(
        shell,
        "aws {aws_args...} s3 sync --delete --quiet {dists_track_local} {dists_remote_track} --cache-control 'public, max-age=300' --acl public-read"
    ).run()?;

    println!("Uploading public GPG key...");
    let aws_args = cfg.aws_args();
    cmd!(
        shell,
        "aws {aws_args...} s3 sync --quiet {key_dir} {s3_deb} --cache-control 'public, max-age=86400' --acl public-read"
    ).run()?;

    let repo_url = &cfg.repo_url;
    let track = &cfg.track;
    println!(
        r#"
DEB repository published to {s3_deb}

Client setup:
  curl -fsSL {repo_url}/gpg-key.asc \
    | sudo gpg --dearmor -o /usr/share/keyrings/air-keyring.gpg
  echo "deb [signed-by=/usr/share/keyrings/air-keyring.gpg] {repo_url} {track} {APT_COMPONENT}" \
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

fn publish_rpm(shell: &Shell, cfg: &Config) -> Result<()> {
    // Group the files by rpm arch.
    let queryformat = "%{ARCH}";
    let mut by_arch: BTreeMap<String, Vec<&Utf8Path>> = BTreeMap::new();
    for file in &cfg.package_files {
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
    let key_dir = rpm_root.join("keys");
    fs::create_dir_all(&key_dir)?;

    let s3_rpm = cfg.s3_path("rpm");

    let home_str = env::var("HOME").context("HOME is not set")?;
    let home = Utf8PathBuf::from(home_str);
    let macros_path = home.join(".rpmmacros");
    let macros_content = format!("%_signature gpg\n%_gpg_name  {key}\n", key = cfg.gpg_key_id,);
    let gpg_key_id = &cfg.gpg_key_id;

    for (arch, files) in &by_arch {
        println!("Publishing arch {arch}...");
        let repo_dir = rpm_root.join(APT_COMPONENT).join(arch);
        fs::create_dir_all(&repo_dir)?;
        let s3_arch = format!("{s3_rpm}/{APT_COMPONENT}/{arch}");

        // Hydrate the component/arch dir (existing .rpms + repodata/) so
        // createrepo_c --update can incrementally extend the previous metadata.
        println!("Syncing existing repo from {s3_arch}...");
        let aws_args = cfg.aws_args();
        cmd!(
            shell,
            "aws {aws_args...} s3 sync --quiet {s3_arch} {repo_dir}"
        )
        .run()?;

        prune_rpm_packages(shell, &repo_dir, KEEP_VERSIONS)?;

        println!("Staging and signing .rpm with GPG key: {gpg_key_id}");
        {
            let _guard = RpmMacrosGuard::install(macros_path.clone(), &macros_content)?;
            for file in files {
                let staged = stage_file(file, &repo_dir)?;
                let staged_str = staged.as_str();
                cmd!(shell, "rpm --addsign {staged_str}").run()?;
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

        println!("Uploading .rpm packages (immutable, long TTL)...");
        let aws_args = cfg.aws_args();
        cmd!(
            shell,
            "aws {aws_args...} s3 sync --quiet {repo_dir} {s3_arch} --delete --exclude repodata/* --cache-control 'public, max-age=31536000, immutable' --acl public-read"
        ).run()?;

        // createrepo_c replaces the metadata files, --delete drops the old
        // ones on S3.
        println!("Uploading repodata (short TTL)...");
        let local_repodata = repo_dir.join("repodata");
        let s3_repodata = format!("{s3_arch}/repodata");
        let aws_args = cfg.aws_args();
        cmd!(
            shell,
            "aws {aws_args...} s3 sync --quiet --delete {local_repodata} {s3_repodata} --cache-control 'public, max-age=300' --acl public-read"
        ).run()?;
    }

    let armored = cmd!(shell, "gpg --batch --yes --export --armor {gpg_key_id}").read()?;
    write_text(&key_dir.join("gpg-key.asc"), armored)?;

    // Generate a .repo file so clients can install via
    // `dnf config-manager addrepo --from-repofile <url>`. dnf expands
    // $basearch itself.
    let repo_file = key_dir.join("air.repo");
    let repo_contents = format!(
        r#"[air]
name=Air Messenger builds
baseurl={url}/{APT_COMPONENT}/$basearch
enabled=1
gpgcheck=1
repo_gpgcheck=1
gpgkey={url}/gpg-key.asc
        "#,
        url = cfg.repo_url
    );
    fs::write(&repo_file, repo_contents).with_context(|| format!("Failed to write {repo_file}"))?;

    println!("Uploading GPG key and .repo descriptor...");
    let aws_args = cfg.aws_args();
    cmd!(
        shell,
        "aws {aws_args...} s3 sync --quiet {key_dir} {s3_rpm} --cache-control 'public, max-age=86400' --acl public-read"
    ).run()?;

    println!("RPM repository published to {s3_rpm}");

    println!(
        "Client setup: sudo dnf config-manager addrepo --from-repofile {}/air.repo",
        cfg.repo_url
    );

    Ok(())
}
