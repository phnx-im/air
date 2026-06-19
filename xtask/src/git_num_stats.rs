// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use anyhow::Context;
use clap::Parser;
use gix::{
    AttributeStack,
    attrs::StateRef,
    bstr::{BStr, ByteSlice},
    prelude::TreeDiffChangeExt,
    worktree::stack::state::attributes::Source,
};

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
    let repo = gix::discover(".").context("failed to open git repository")?;

    let base = repo
        .rev_parse_single(args.base.as_str())
        .context("failed to resolve base revision")?
        .object()?
        .peel_to_tree()?;

    let commit = repo
        .rev_parse_single(args.commit.as_str())
        .context("failed to resolve target revision")?
        .object()?
        .peel_to_tree()?;

    let changes = repo
        .diff_tree_to_tree(Some(&base), Some(&commit), None)
        .context("failed to compute diff")?;

    // Cache that stores the diffable blobs while computing per-file line stats.
    let mut resource_cache = repo
        .diff_resource_cache_for_tree_diff()
        .context("failed to create diff resource cache")?;

    // Attribute lookup, reading `.gitattributes` from the worktree and falling
    // back to the index for paths that aren't checked out.
    let index = repo.index_or_empty().context("failed to open git index")?;
    let mut attributes = repo
        .attributes_only(&index, Source::WorktreeThenIdMapping)
        .context("failed to configure git attributes")?;
    let mut outcome = attributes.selected_attribute_matches(["linguist-generated", "diff"]);

    let mut total_added: u32 = 0;
    let mut total_removed: u32 = 0;

    for change in &changes {
        let change = change.attach(&repo, &repo);
        let path = change.location();
        let file = path.to_str_lossy();

        if args
            .exclude
            .iter()
            .any(|filter| file.contains(filter.as_str()))
        {
            continue;
        }

        if is_excluded(&mut attributes, &mut outcome, path)? {
            continue;
        }

        if let Some(counts) = change
            .diff(&mut resource_cache)
            .ok()
            .and_then(|mut platform| platform.line_counts().ok())
            .flatten()
        {
            total_added += counts.insertions;
            total_removed += counts.removals;
        }

        resource_cache.clear_resource_cache_keep_allocation();
    }

    println!("+{total_added} -{total_removed}");

    Ok(())
}

fn is_excluded(
    attributes: &mut AttributeStack<'_>,
    outcome: &mut gix::attrs::search::Outcome,
    path: &BStr,
) -> anyhow::Result<bool> {
    outcome.reset();
    let platform = attributes
        .at_entry(path, None)
        .context("failed to read git attribute")?;
    platform.matching_attributes(outcome);

    Ok(outcome.iter_selected().any(|m| match m.assignment.state {
        StateRef::Set => true, // for attributes like -diff
        StateRef::Value(value) => value.as_bstr() == "true", // for attributes like linguist-generated=true
        _ => false,
    }))
}
