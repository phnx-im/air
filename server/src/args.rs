// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::identifiers::UserId;

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
    InvitationCodes(InvitationCodeArgs),
}

#[derive(clap::Args)]
pub struct InvitationCodeArgs {
    #[command(subcommand)]
    pub cmd: Option<CodeCommand>,
}

#[derive(Default, clap::Subcommand)]
pub enum CodeCommand {
    #[default]
    Stats,
    /// List the global or user specific invitation codes
    List {
        /// User ID in user@fqdn format
        #[arg(long)]
        user_id: Option<UserId>,
        /// Include redeemed codes
        #[arg(long, default_value_t = false)]
        include_redeemed: bool,
    },
    /// Delete the invite code of a user
    Delete {
        /// User ID in user@fqdn format
        #[arg(long)]
        user_id: UserId,
    },
    /// Replenish the invite code of a user
    Replenish {
        /// User ID in user@fqdn format
        #[arg(long)]
        user_id: UserId,
    },
}
