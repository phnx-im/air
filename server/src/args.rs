// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use chrono::{DateTime, Utc};

#[derive(clap::Parser)]
pub struct Args {
    #[command(subcommand)]
    pub cmd: Option<Command>,
}

#[derive(Default, clap::Subcommand)]
pub enum Command {
    /// Run the server
    #[default]
    Run,
    /// Invitation codes subcommands
    Code(CodeArgs),
    /// Usernames subcommands
    Username(UsernameArgs),
}

#[derive(clap::Args)]
pub struct CodeArgs {
    #[command(subcommand)]
    pub cmd: Option<CodeCommand>,
}

#[derive(Default, clap::Subcommand)]
pub enum CodeCommand {
    /// Calculate basic invitation codes statistics
    #[default]
    Stats,
    /// List stored invitation codes
    List {
        /// Number of codes to list
        #[arg(default_value_t = 1000)]
        n: usize,
        /// Include redeemed codes
        #[arg(long, default_value_t = false)]
        include_redeemed: bool,
    },
    /// Generate invitation codes
    Generate {
        /// Number of codes to generate
        #[arg(default_value_t = 1)]
        n: usize,
    },
}

#[derive(clap::Args)]
pub struct UsernameArgs {
    #[command(subcommand)]
    pub cmd: Option<UsernameCommand>,
}

#[derive(Default, clap::Subcommand)]
pub enum UsernameCommand {
    /// Lists all hashes of usernames
    ///
    /// Note: The server does not have access to plaintext usernames.
    #[default]
    List,
    /// Refreshes usernames that are about to expire before the given date.
    RefreshExpiring { before: DateTime<Utc> },
}
