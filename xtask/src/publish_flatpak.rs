// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Publishes Flatpak bundles to an OSTree repository on S3, one branch per
//! track.

use std::{collections::BTreeMap, fs};

use anyhow::{Context, Result, bail, ensure};
use askama::Template;
use base64::{Engine, prelude::BASE64_STANDARD};
use camino::{Utf8Path, Utf8PathBuf};
use clap::Args;
use xshell::{Shell, cmd};

use crate::{
    flatpak_sources::Flavor,
    publish_linux_packages::{
        Config, INDEX_CACHE_CONTROL, KEEP_VERSIONS, KEY_CACHE_CONTROL, PACKAGES_CACHE_CONTROL,
        RepoArgs, Track, Upload, Uploads, recreate_dir, setup, write_text,
    },
};

const APP_ID: &str = Flavor::Production.app_id();

// Branch of the bundles built by CI, see `just build-flatpak`.
const BUILD_BRANCH: &str = "build";

// Content addressed or written once, never changed in place.
const IMMUTABLE_DIRS: [&str; 3] = ["objects", "deltas", "summaries"];

// Left out of the index upload: the immutable dirs go up separately, the
// rest is local state.
const LOCAL_ONLY: [&str; 8] = [
    ".lock",
    "objects/*",
    "deltas/*",
    "summaries/*",
    "tmp/*",
    "state/*",
    "refs/remotes/*",
    "refs/mirrors/*",
];

#[derive(Args, Debug)]
pub(crate) struct PublishArgs {
    /// Flatpak bundles of the app (`flatpak build-bundle`), one per
    /// architecture, on the branch "build".
    #[arg(required = true, num_args = 1..)]
    bundles: Vec<Utf8PathBuf>,

    /// Release track to publish to.
    #[arg(long, value_enum, default_value_t = Track::Nightly, env = "TRACK")]
    track: Track,

    #[command(flatten)]
    repo: RepoArgs,
}

pub(crate) fn run(args: PublishArgs) -> Result<()> {
    let PublishArgs {
        bundles,
        track,
        repo,
    } = args;
    let branch = branch(track)?;
    for bundle in &bundles {
        ensure!(bundle.exists(), "File not found: {bundle}");
    }

    let shell = Shell::new()?;
    let cfg = setup(&shell, repo)?;
    for bundle in &bundles {
        println!(
            "Bundle\t: {}",
            bundle.file_name().unwrap_or(bundle.as_str())
        );
    }
    println!("Track\t: {track}");

    let repo = FlatpakRepo::hydrate(&shell, &cfg)?;

    // Bundles go to a separate repo first, so only the track branch ends up
    // in the published one.
    let staging = cfg.workdir("flatpak/staging");
    recreate_dir(&staging)?;
    cmd!(shell, "ostree --repo={staging} init --mode=archive-z2").run()?;
    for bundle in &bundles {
        let bundle = bundle.canonicalize_utf8()?;
        cmd!(
            shell,
            "flatpak build-import-bundle --no-update-summary {staging} {bundle}"
        )
        .run()?;
    }
    let staged_refs = cmd!(shell, "ostree --repo={staging} refs").read()?;
    let mut archs = Vec::new();
    for staged_ref in staged_refs.lines() {
        let Some(arch) = staged_ref
            .strip_prefix(&format!("app/{APP_ID}/"))
            .and_then(|rest| rest.strip_suffix(&format!("/{BUILD_BRANCH}")))
        else {
            bail!("Unexpected ref in the bundles: {staged_ref}");
        };
        archs.push(arch);
    }
    // Bundles of the same arch import to the same ref.
    ensure!(
        archs.len() == bundles.len(),
        "Several bundles for the same arch"
    );
    archs.sort();
    println!("Archs\t: {}", archs.join(" "));

    for arch in archs {
        repo.commit_from(
            &shell,
            &cfg,
            &staging,
            &app_ref(arch, BUILD_BRANCH),
            &app_ref(arch, branch),
        )?;
    }

    let mut uploads = Uploads::default();
    repo.finish(&shell, &cfg, &mut uploads)?;
    cfg.upload(&shell, &uploads)
}

/// Copies the build of `version` from one track to another for every arch.
/// The copy is a new commit of the same files, nothing is rebuilt.
pub(crate) fn promote(
    shell: &Shell,
    cfg: &Config,
    from: Track,
    to: Track,
    version: &str,
    uploads: &mut Uploads,
) -> Result<()> {
    let (from_branch, to_branch) = (branch(from)?, branch(to)?);
    let repo = FlatpakRepo::hydrate(shell, cfg)?;

    // Look up all arches first, so a missing build leaves `to` untouched.
    let subject = commit_subject(version);
    let archs = repo.archs(shell, from_branch)?;
    ensure!(!archs.is_empty(), "No Flatpak on branch {from_branch}");
    let mut commits = Vec::new();
    for arch in archs {
        let from_ref = app_ref(&arch, from_branch);
        let commit = repo
            .find_commit(shell, &from_ref, &subject)?
            .with_context(|| {
                format!("No commit {subject:?} in the last {KEEP_VERSIONS} of {from_ref}")
            })?;
        println!("Promote\t: {from_ref} {commit}");
        commits.push((arch, commit));
    }

    let path = &repo.path;
    for (arch, commit) in commits {
        // build-commit-from takes a ref, not a commit.
        let tmp_ref = app_ref(&arch, "promote");
        cmd!(
            shell,
            "ostree --repo={path} refs --create={tmp_ref} {commit}"
        )
        .run()?;
        let result = repo.commit_from(shell, cfg, path, &tmp_ref, &app_ref(&arch, to_branch));
        cmd!(shell, "ostree --repo={path} refs --delete {tmp_ref}").run()?;
        result?;
    }

    repo.finish(shell, cfg, uploads)
}

/// Subject of the commits of a build. The version is the package version,
/// e.g. "0.23.0-1490".
fn commit_subject(version: &str) -> String {
    format!("Air {version}")
}

fn branch(track: Track) -> Result<&'static str> {
    match track {
        Track::Nightly => Ok("nightly"),
        Track::Beta => Ok("beta"),
        Track::Stable => Ok("stable"),
        Track::Unstable => bail!("The legacy track {track} has no Flatpak branch"),
    }
}

fn app_ref(arch: &str, branch: &str) -> String {
    format!("app/{APP_ID}/{arch}/{branch}")
}

// Local OSTree repo with the recent history of the published one.
struct FlatpakRepo {
    path: Utf8PathBuf,
    // Holds the files next to the repo: flatpakrepo, flatpakrefs and key.
    keys_dir: Utf8PathBuf,
    url: String,
    remote: String,
}

impl FlatpakRepo {
    // Mirrors the last KEEP_VERSIONS commits of each ref, plus the deltas and
    // summaries that flatpak only regenerates when missing.
    fn hydrate(shell: &Shell, cfg: &Config) -> Result<Self> {
        let root = cfg.workdir("flatpak");
        let path = root.join("repo");
        let keys_dir = root.join("keys");
        recreate_dir(&path)?;
        recreate_dir(&keys_dir)?;
        let url = format!("{}/flatpak", cfg.repository_base_url);
        let remote = cfg.s3_path("flatpak/repo");

        let gpg_key_id = &cfg.gpg_key_id;
        let public_key = keys_dir.join("air.gpg");
        let key = cmd!(shell, "gpg --batch --export {gpg_key_id}")
            .quiet()
            .output()?
            .stdout;
        ensure!(!key.is_empty(), "No public key for {gpg_key_id}");
        fs::write(&public_key, key)?;

        cmd!(shell, "ostree --repo={path} init --mode=archive-z2").run()?;

        let repo_config = format!("{remote}/config");
        let aws_args = cfg.aws_args();
        let exists = cmd!(shell, "aws {aws_args...} s3 ls {repo_config}")
            .quiet()
            .ignore_stdout()
            .ignore_stderr()
            .run()
            .is_ok();
        if exists {
            let repo_url = format!("{url}/repo");
            println!("Mirroring {repo_url}...");
            let depth = KEEP_VERSIONS.to_string();
            cmd!(
                shell,
                "ostree --repo={path} remote add --gpg-import={public_key} origin {repo_url}"
            )
            .run()?;
            let pull = cmd!(
                shell,
                "ostree --repo={path} pull --mirror --depth={depth} origin"
            )
            .run();
            cmd!(shell, "ostree --repo={path} remote delete origin").run()?;
            pull?;

            println!("Syncing deltas and summaries from {remote}...");
            let aws_args = cfg.aws_args();
            cmd!(
                shell,
                "aws {aws_args...} s3 sync --quiet {remote} {path}
                 --exclude * --include deltas/* --include summaries/* --include summary.idx"
            )
            .run()?;
        } else {
            println!("No repository at {remote}, starting a new one.");
        }
        fs::remove_file(&public_key)?;

        Ok(Self {
            path,
            keys_dir,
            url,
            remote,
        })
    }

    // Commits the tree of `src_ref` in `src_repo` as new head of `dst_ref`.
    fn commit_from(
        &self,
        shell: &Shell,
        cfg: &Config,
        src_repo: &Utf8Path,
        src_ref: &str,
        dst_ref: &str,
    ) -> Result<()> {
        let path = &self.path;
        let gpg_key_id = &cfg.gpg_key_id;
        println!("Committing {src_ref} to {dst_ref}...");
        cmd!(
            shell,
            "flatpak build-commit-from --no-update-summary --gpg-sign={gpg_key_id}
             --src-repo={src_repo} --src-ref={src_ref} {path} {dst_ref}"
        )
        .run()?;
        Ok(())
    }

    // Arches of the app per branch.
    fn branches(&self, shell: &Shell) -> Result<BTreeMap<String, Vec<String>>> {
        let path = &self.path;
        let refs = cmd!(shell, "ostree --repo={path} refs").read()?;
        let mut branches: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for app_ref in refs.lines() {
            let parts: Vec<&str> = app_ref.split('/').collect();
            if let ["app", id, arch, branch] = parts[..]
                && id == APP_ID
            {
                branches
                    .entry(branch.to_owned())
                    .or_default()
                    .push(arch.to_owned());
            }
        }
        Ok(branches)
    }

    fn archs(&self, shell: &Shell, branch: &str) -> Result<Vec<String>> {
        Ok(self.branches(shell)?.remove(branch).unwrap_or_default())
    }

    // Commit with the given subject in the mirrored history of `app_ref`.
    fn find_commit(&self, shell: &Shell, app_ref: &str, subject: &str) -> Result<Option<String>> {
        let path = &self.path;
        let log = cmd!(shell, "ostree --repo={path} log {app_ref}").read()?;
        Ok(parse_log(&log)
            .into_iter()
            .find(|(_commit, s)| s == subject)
            .map(|(commit, _subject)| commit))
    }

    // Prunes old commits, signs the summary, writes the flatpakrepo and
    // flatpakref files, and queues the uploads.
    fn finish(&self, shell: &Shell, cfg: &Config, uploads: &mut Uploads) -> Result<()> {
        let Self {
            path,
            keys_dir,
            url,
            remote,
        } = self;
        let gpg_key_id = &cfg.gpg_key_id;
        let depth = KEEP_VERSIONS.to_string();
        println!("Updating repository...");
        cmd!(
            shell,
            "flatpak build-update-repo --gpg-sign={gpg_key_id} --title=Air --default-branch=stable
             --prune --prune-depth={depth} --generate-static-deltas {path}"
        )
        .run()?;
        for dir in IMMUTABLE_DIRS {
            fs::create_dir_all(path.join(dir))?;
        }

        let key = cmd!(shell, "gpg --batch --export {gpg_key_id}")
            .quiet()
            .output()?
            .stdout;
        let gpg_key = BASE64_STANDARD.encode(key);
        let armored = cmd!(shell, "gpg --batch --yes --export --armor {gpg_key_id}").read()?;
        write_text(&keys_dir.join("gpg-key.asc"), armored)?;

        let repo_url = format!("{url}/repo/");
        write_text(
            &keys_dir.join("air.flatpakrepo"),
            FlatpakRepoTemplate {
                repo_url: &repo_url,
                gpg_key: &gpg_key,
            }
            .render()?,
        )?;

        let branches = self.branches(shell)?;
        println!();
        println!("Flatpak repository {url}");
        println!("Client setup:");
        for (branch, archs) in &branches {
            let (name, title) = match branch.as_str() {
                "stable" => (APP_ID.to_owned(), "Air".to_owned()),
                _ => (format!("{APP_ID}-{branch}"), format!("Air ({branch})")),
            };
            write_text(
                &keys_dir.join(format!("{name}.flatpakref")),
                FlatpakRefTemplate {
                    title: &title,
                    app_id: APP_ID,
                    branch,
                    repo_url: &repo_url,
                    gpg_key: &gpg_key,
                }
                .render()?,
            )?;
            println!(
                "  {branch} ({}): flatpak install --user {url}/{name}.flatpakref",
                archs.join(" ")
            );
        }

        for dir in IMMUTABLE_DIRS {
            uploads.packages.push(Upload {
                local: path.join(dir),
                remote: format!("{remote}/{dir}"),
                cache_control: PACKAGES_CACHE_CONTROL,
                exclude: &[],
                delete: true,
            });
        }
        // Summaries, refs and config, which change with every publish.
        uploads.metadata.push(Upload {
            local: path.clone(),
            remote: remote.clone(),
            cache_control: INDEX_CACHE_CONTROL,
            exclude: &LOCAL_ONLY,
            delete: true,
        });
        uploads.metadata.push(Upload {
            local: keys_dir.clone(),
            remote: cfg.s3_path("flatpak"),
            cache_control: KEY_CACHE_CONTROL,
            exclude: &[],
            delete: false,
        });
        Ok(())
    }
}

// (commit, subject) of each commit in the output of `ostree log`.
fn parse_log(log: &str) -> Vec<(String, String)> {
    let mut commits = Vec::new();
    let mut current: Option<String> = None;
    for line in log.lines() {
        if let Some(commit) = line.strip_prefix("commit ") {
            current = Some(commit.trim().to_owned());
        } else if line.starts_with("    ")
            && !line.trim().is_empty()
            && let Some(commit) = current.take()
        {
            commits.push((commit, line.trim().to_owned()));
        }
    }
    commits
}

#[derive(Template)]
#[template(path = "flatpakrepo.jinja", escape = "none")]
struct FlatpakRepoTemplate<'a> {
    repo_url: &'a str,
    gpg_key: &'a str,
}

#[derive(Template)]
#[template(path = "flatpakref.jinja", escape = "none")]
struct FlatpakRefTemplate<'a> {
    title: &'a str,
    app_id: &'a str,
    branch: &'a str,
    repo_url: &'a str,
    gpg_key: &'a str,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flatpakref_and_flatpakrepo_render() {
        let repo = FlatpakRepoTemplate {
            repo_url: "https://packages.air.ms/flatpak/repo/",
            gpg_key: "S0VZ",
        }
        .render()
        .unwrap();
        assert_eq!(
            repo.trim_end(),
            "[Flatpak Repo]\nTitle=Air\nUrl=https://packages.air.ms/flatpak/repo/\n\
             Homepage=https://air.ms\nComment=Air, a secure messenger\n\
             DefaultBranch=stable\nGPGKey=S0VZ"
        );

        let flatpakref = FlatpakRefTemplate {
            title: "Air (beta)",
            app_id: APP_ID,
            branch: "beta",
            repo_url: "https://packages.air.ms/flatpak/repo/",
            gpg_key: "S0VZ",
        }
        .render()
        .unwrap();
        assert_eq!(
            flatpakref.trim_end(),
            "[Flatpak Ref]\nTitle=Air (beta)\nName=ms.air.Air\nBranch=beta\n\
             Url=https://packages.air.ms/flatpak/repo/\nSuggestRemoteName=air\n\
             Homepage=https://air.ms\n\
             RuntimeRepo=https://dl.flathub.org/repo/flathub.flatpakrepo\n\
             IsRuntime=false\nGPGKey=S0VZ"
        );
    }

    #[test]
    fn parses_ostree_log() {
        let log = "\
commit 1111
Parent:  2222
ContentChecksum:  aaaa
Date:  2026-10-05 12:00:00 +0000

    Air 0.23.0-1491

    Some body

commit 2222
ContentChecksum:  bbbb
Date:  2026-10-04 12:00:00 +0000

    Air 0.23.0-1490

<< History beyond this commit not fetched >>
";
        assert_eq!(
            parse_log(log),
            [
                ("1111".to_owned(), "Air 0.23.0-1491".to_owned()),
                ("2222".to_owned(), "Air 0.23.0-1490".to_owned()),
            ]
        );
    }
}
