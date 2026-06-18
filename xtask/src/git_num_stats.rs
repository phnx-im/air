// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::path::Path;

use anyhow::Context;
use clap::Parser;
use git2::{AttrCheckFlags, DiffOptions, Patch, Repository};

#[derive(Parser)]
#[command(about = "Count diff lines excluding generated files marked in .gitattributes")]
pub(crate) struct GitNumStatsArgs {
    #[arg(long)]
    exclude: Vec<String>,
    #[arg(long, default_value = "HEAD")]
    commit: String,
    #[arg(long, default_value = "origin/main")]
    base: String,
}

pub(crate) fn run(args: GitNumStatsArgs) -> anyhow::Result<()> {
    let repo = Repository::discover(".").context("failed to open git repository")?;

    let base = repo
        .revparse_single(&args.base)
        .context("failed to resolve base revision")?
        .peel_to_tree()?;

    let commit = repo
        .revparse_single(&args.commit)
        .context("failed to resolve target revision")?
        .peel_to_tree()?;

    let mut diff_opts = DiffOptions::new();
    let diff = repo
        .diff_tree_to_tree(Some(&base), Some(&commit), Some(&mut diff_opts))
        .context("failed to compute diff")?;

    let mut total_added: u64 = 0;
    let mut total_removed: u64 = 0;

    for (idx, delta) in diff.deltas().enumerate() {
        let Some(path) = delta.new_file().path().or_else(|| delta.old_file().path()) else {
            continue;
        };
        let file = path.to_string_lossy();

        if args
            .exclude
            .iter()
            .any(|filter| file.contains(filter.as_str()))
        {
            continue;
        }

        if is_excluded(&repo, path)? {
            continue;
        }

        let Some(patch) = Patch::from_diff(&diff, idx).context("failed to read diff patch")? else {
            continue;
        };

        let (_context, added, removed) = patch.line_stats().context("failed to read line stats")?;
        total_added += added as u64;
        total_removed += removed as u64;
    }

    println!("+{total_added} -{total_removed}");

    Ok(())
}

fn is_excluded(repo: &Repository, path: &Path) -> anyhow::Result<bool> {
    for attr in ["linguist-generated", "diff"] {
        let value = repo
            .get_attr(path, attr, AttrCheckFlags::empty())
            .context("failed to read git attribute")?;
        if value == Some("true") {
            return Ok(true);
        }
    }

    Ok(false)
}
