// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{collections::HashMap, env, fmt, fs};

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
    /// Package file (.deb or .rpm) to publish.
    package_file: Utf8PathBuf,

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

    /// Architecture override. Auto-detected from the package when omitted.
    #[arg(short = 'a', long = "arch")]
    arch: Option<String>,

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

    /// Print aws commands without executing them.
    #[arg(long, action = clap::ArgAction::SetTrue)]
    dry_run: bool,
}

struct Config {
    package_file: Utf8PathBuf,
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

    fn workdir(&self, path: impl AsRef<Utf8Path>) -> Utf8PathBuf {
        self.workdir.join(path)
    }
}

pub(crate) fn run(args: PublishArgs) -> Result<()> {
    let cfg = build_config(args)?;

    // Some S3-compatible providers (Upcloud, some MinIO versions, ...) reject
    // the newer flow checksums AWS CLI v2 sends by default. "when_required"
    // only emits checksums when the server asks for them.
    if cfg.s3_endpoint.is_some() {
        for key in [
            "AWS_REQUEST_CHECKSUM_CALCULATION",
            "AWS_RESPONSE_CHECKSUM_VALIDATION",
        ] {
            if env::var_os(key).is_none() {
                // SAFETY: no other threads have been spawned at this point.
                unsafe {
                    env::set_var(key, "when_required");
                }
            }
        }
    }

    let shell = Shell::new()?;

    let display_name = cfg
        .package_file
        .file_name()
        .unwrap_or_else(|| cfg.package_file.as_str());

    println!("Package\t: {display_name}");
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

fn build_config(args: PublishArgs) -> Result<Config> {
    ensure!(
        args.package_file.exists(),
        "File not found: {}",
        args.package_file,
    );
    let package_file = args.package_file.canonicalize_utf8()?;

    let pkg_type = match package_file.extension() {
        Some("deb") => PkgType::Deb,
        Some("rpm") => PkgType::Rpm,
        _ => bail!("Cannot detect package type from filename."),
    };

    // Trim trailing slash so client-setup snippets don't end up with "//".
    let repository_base_url = args.repository_base_url.trim_end_matches('/');

    let cwd = env::current_dir().context("Failed to read current directory")?;
    let workdir = Utf8PathBuf::try_from(cwd)
        .context("Current directory is not valid UTF-8")?
        .join("app/linux/package-builds");

    Ok(Config {
        package_file,
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

fn aws_cmd(shell: &Shell, cfg: &Config, args: &[&str]) -> Result<()> {
    let endpoint: Vec<&str> = match cfg.s3_endpoint.as_deref() {
        Some(ep) => vec!["--endpoint-url", ep],
        None => Vec::new(),
    };
    if cfg.dry_run {
        let joined: Vec<&str> = endpoint.iter().chain(args.iter()).copied().collect();
        println!("[dry-run] aws {}", joined.join(" "));
        return Ok(());
    }
    cmd!(shell, "aws {endpoint...} {args...}").run()?;
    Ok(())
}

fn run_gpg_read(shell: &Shell, args: &[&str]) -> Result<String> {
    Ok(cmd!(shell, "gpg --batch --yes {args...}").read()?)
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
    let keep_arg = format!("--keep={keep}");
    let repo_dir_str = repo_dir.as_str();
    let output = cmd!(shell, "repomanage --old {keep_arg} {repo_dir_str}")
        .quiet()
        .ignore_stderr()
        .ignore_status()
        .read()
        .unwrap_or_default();

    let mut removed = 0usize;
    for line in output.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let name = Utf8Path::new(line).file_name().unwrap_or(line);
        println!("Pruning old package: {name}");
        if let Err(e) = fs::remove_file(line) {
            eprintln!("Failed to remove {line}: {e}");
        } else {
            removed += 1;
        }
    }
    if removed > 0 {
        println!("Pruned {removed} old .rpm(s); keeping last {keep} per package.");
    }
    Ok(())
}

fn publish_deb(shell: &Shell, cfg: &Config) -> Result<()> {
    let arch = dpkg_field(shell, &cfg.package_file, "Architecture")?;
    println!("Arch\t: {arch}");
    println!();

    let deb_root = cfg.workdir("deb");
    let pool_dir = deb_root.join("pool").join(APT_COMPONENT);
    let dists_dir = deb_root
        .join("dists")
        .join(&cfg.track)
        .join(APT_COMPONENT)
        .join(format!("binary-{arch}"));
    let key_dir = deb_root.join("keys");
    fs::create_dir_all(&pool_dir)?;
    fs::create_dir_all(&dists_dir)?;
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
    aws_cmd(
        shell,
        cfg,
        &["s3", "sync", "--quiet", &pool_remote, pool_local.as_str()],
    )?;

    let dists_remote_track = format!("{s3_deb}/dists/{}", cfg.track);
    println!("Syncing existing dists from {dists_remote_track}...");
    aws_cmd(
        shell,
        cfg,
        &[
            "s3",
            "sync",
            "--quiet",
            &dists_remote_track,
            dists_track_local.as_str(),
        ],
    )?;

    prune_deb_pool(shell, &pool_dir, KEEP_VERSIONS)?;

    println!("Staging package into pool...");
    let staged_name = cfg
        .package_file
        .file_name()
        .context("package file has no filename")?;
    let staged = pool_dir.join(staged_name);
    fs::copy(&cfg.package_file, &staged)
        .with_context(|| format!("Failed to copy {} to {}", cfg.package_file, staged))?;

    // Release file signature provides repo-level integrity; per-package
    // signatures are intentionally omitted for DEB.
    let armored = run_gpg_read(shell, &["--export", "--armor", &cfg.gpg_key_id])?;
    write_text(&key_dir.join("gpg-key.asc"), armored)?;

    println!("Running apt-ftparchive packages...");
    // cd into deb_root so apt-ftparchive embeds "pool/main/..." (relative to
    // dists/) in the Filename: field, matching the URL clients construct.
    let pool_rel = format!("pool/{APT_COMPONENT}");
    let packages_path = dists_dir.join("Packages");
    {
        let _pd = shell.push_dir(deb_root.as_std_path());
        let packages_out = cmd!(shell, "apt-ftparchive packages {pool_rel}").read()?;
        write_text(&packages_path, packages_out)?;
    }
    let packages_str = packages_path.as_str();
    cmd!(shell, "gzip -9 -f -k {packages_str}").run()?;
    cmd!(shell, "bzip2 -9 -f -k {packages_str}").run()?;
    cmd!(shell, "xz -9 -f -k {packages_str}").run()?;

    println!("Running apt-ftparchive release...");
    let release_dir = dists_track_local.clone();
    let release_dir_str = release_dir.as_str();
    let track = &cfg.track;
    let release_output = cmd!(
        shell,
        "apt-ftparchive
         -o APT::FTPArchive::Release::Origin=Custom
         -o APT::FTPArchive::Release::Label=Custom
         -o APT::FTPArchive::Release::Suite={track}
         -o APT::FTPArchive::Release::Codename={track}
         -o APT::FTPArchive::Release::Components={APT_COMPONENT}
         -o APT::FTPArchive::Release::Architectures={arch}
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
    aws_cmd(
        shell,
        cfg,
        &[
            "s3",
            "sync",
            "--quiet",
            pool_local.as_str(),
            &pool_remote,
            "--delete",
            "--cache-control",
            "public, max-age=31536000, immutable",
            "--acl",
            "public-read",
        ],
    )?;

    println!("Uploading dists (index files, short TTL)...");
    let dists_remote = format!("{s3_deb}/dists");
    aws_cmd(
        shell,
        cfg,
        &[
            "s3",
            "sync",
            "--delete",
            "--quiet",
            dists_local.as_str(),
            &dists_remote,
            "--cache-control",
            "public, max-age=300",
            "--acl",
            "public-read",
        ],
    )?;

    println!("Uploading public GPG key...");
    aws_cmd(
        shell,
        cfg,
        &[
            "s3",
            "sync",
            "--quiet",
            key_dir.as_str(),
            s3_deb.as_str(),
            "--cache-control",
            "public, max-age=86400",
            "--acl",
            "public-read",
        ],
    )?;

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
    let pkg = cfg.package_file.as_str();
    let queryformat = "%{ARCH}";
    let arch = cmd!(shell, "rpm -qp --queryformat {queryformat} {pkg}")
        .quiet()
        .ignore_stderr()
        .read()?
        .trim()
        .to_string();
    println!("Arch\t: {arch}");
    println!();

    let rpm_root = cfg.workdir("rpm");
    let repo_dir = rpm_root.join(APT_COMPONENT).join(&arch);
    let key_dir = rpm_root.join("keys");
    fs::create_dir_all(&repo_dir)?;
    fs::create_dir_all(&key_dir)?;

    let s3_rpm = cfg.s3_path("rpm");
    let s3_arch = format!("{s3_rpm}/{APT_COMPONENT}/{arch}");

    // Hydrate the component/arch dir (existing .rpms + repodata/) so
    // createrepo_c --update can incrementally extend the previous metadata.
    println!("Syncing existing repo from {s3_arch}...");
    aws_cmd(
        shell,
        cfg,
        &["s3", "sync", "--quiet", &s3_arch, repo_dir.as_str()],
    )?;

    prune_rpm_packages(shell, &repo_dir, KEEP_VERSIONS)?;

    println!("Staging package...");
    let staged_name = cfg
        .package_file
        .file_name()
        .context("package file has no filename")?;
    let staged = repo_dir.join(staged_name);
    fs::copy(&cfg.package_file, &staged)
        .with_context(|| format!("Failed to copy {} to {}", cfg.package_file, staged))?;

    println!("Signing .rpm with GPG key: {}", cfg.gpg_key_id);

    let home_str = env::var("HOME").context("HOME is not set")?;
    let home = Utf8PathBuf::from(home_str);
    let macros_path = home.join(".rpmmacros");
    let macros_content = format!("%_signature gpg\n%_gpg_name  {key}\n", key = cfg.gpg_key_id,);
    {
        let _guard = RpmMacrosGuard::install(macros_path.clone(), &macros_content)?;
        let staged_str = staged.as_str();
        cmd!(shell, "rpm --addsign {staged_str}").run()?;
    }

    let armored = run_gpg_read(shell, &["--export", "--armor", &cfg.gpg_key_id])?;
    write_text(&key_dir.join("gpg-key.asc"), armored)?;

    println!("Running createrepo_c...");
    let repo_dir_str = repo_dir.as_str();
    cmd!(shell, "createrepo_c --update {repo_dir_str}").run()?;

    println!("Signing repomd.xml...");
    let gpg_key_id = &cfg.gpg_key_id;
    let repomd = repo_dir.join("repodata/repomd.xml");
    let repomd_asc = repo_dir.join("repodata/repomd.xml.asc");
    cmd!(
        shell,
        "gpg --batch --yes --default-key {gpg_key_id} --armor --detach-sign --output {repomd_asc} {repomd}"
    ).run()?;

    // Generate a .repo file so clients can install via
    // `dnf config-manager addrepo --from-repofile <url>`.
    let repo_file = key_dir.join("air.repo");
    let repo_contents = format!(
        r#"
        [air]
        name=Air Messenger builds
        baseurl={url}/{APT_COMPONENT}/{arch}
        enabled=1
        gpgcheck=1
        repo_gpgcheck=1
        gpgkey={url}/gpg-key.asc
        "#,
        url = cfg.repo_url
    );
    fs::write(&repo_file, repo_contents).with_context(|| format!("Failed to write {repo_file}"))?;

    println!("Uploading .rpm packages (immutable, long TTL)...");
    aws_cmd(
        shell,
        cfg,
        &[
            "s3",
            "sync",
            "--quiet",
            repo_dir.as_str(),
            &s3_arch,
            "--delete",
            "--exclude",
            "repodata/*",
            "--cache-control",
            "public, max-age=31536000, immutable",
            "--acl",
            "public-read",
        ],
    )?;

    println!("Uploading repodata (short TTL)...");
    let local_repodata = repo_dir.join("repodata");
    let s3_repodata = format!("{s3_arch}/repodata");
    aws_cmd(
        shell,
        cfg,
        &[
            "s3",
            "sync",
            "--quiet",
            local_repodata.as_str(),
            &s3_repodata,
            "--cache-control",
            "public, max-age=300",
            "--acl",
            "public-read",
        ],
    )?;

    println!("Uploading GPG key and .repo descriptor...");
    aws_cmd(
        shell,
        cfg,
        &[
            "s3",
            "sync",
            "--quiet",
            key_dir.as_str(),
            s3_rpm.as_str(),
            "--cache-control",
            "public, max-age=86400",
            "--acl",
            "public-read",
        ],
    )?;

    println!("RPM repository published to {s3_rpm}");

    println!(
        "Client setup: sudo dnf config-manager addrepo --from-repofile {}/air.repo",
        cfg.repo_url
    );

    Ok(())
}
