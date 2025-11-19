// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

mod bump_version;
mod prune_unused_l10n;
mod util;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    author,
    version,
    about = "Developer automation tasks for the Air workspace"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Bump the workspace crate versions, update Flutter metadata, add a changelog entry, and tag the commit.
    #[command(name = "bump-version")]
    BumpVersion,
    /// Scan Flutter / mobile sources for unused localization keys and prune them from ARB files.
    #[command(name = "prune-unused-l10n")]
    PruneUnusedL10n(prune_unused_l10n::PruneArgs),
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::BumpVersion => bump_version::run(),
        Commands::PruneUnusedL10n(args) => prune_unused_l10n::run(args),
    }
}
