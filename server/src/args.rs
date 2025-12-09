// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

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
    /// Invitation codes subcommand
    Code(CodeArgs),
}

#[derive(clap::Args)]
pub struct CodeArgs {
    #[command(subcommand)]
    pub cmd: Option<CodeCommand>,
}

#[derive(Default, clap::Subcommand)]
pub enum CodeCommand {
    #[default]
    Stats,
    List {
        /// Number of codes to list
        #[arg(default_value_t = 1000)]
        n: usize,
        /// Include redeemed codes
        #[arg(long, default_value_t = false)]
        include_redeemed: bool,
    },
    Generate {
        /// Number of codes to generate
        n: usize,
    },
}
