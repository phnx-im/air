// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use airbackend::{
    air_service::BackendService,
    auth_service::{AuthService, cli::InvitationCodeStats},
    settings::Settings,
};
use aircommon::identifiers::Fqdn;
use anyhow::Context;

use crate::args::{CodeCommand, InvitationCodeArgs};

pub async fn run_invitation_code_command(
    args: InvitationCodeArgs,
    configuration: Settings,
    domain: Fqdn,
) -> anyhow::Result<()> {
    let auth_service = AuthService::new(
        &configuration.database,
        domain,
        configuration.application.versionreq,
    )
    .await
    .context("Failed to connect to database")?;

    match args.cmd.unwrap_or_default() {
        CodeCommand::Stats => {
            let InvitationCodeStats { count, redeemed } =
                auth_service.invitation_code_stats().await?;
            println!("Total codes: {count}");
            println!("Redeemed codes: {redeemed}");
        }
        CodeCommand::List {
            user_id,
            include_redeemed,
        } => {
            let codes = auth_service
                .invitation_codes_list(user_id.as_ref(), include_redeemed)
                .await?;

            for code in codes {
                if include_redeemed {
                    println!("{}{}", code.code(), if code.redeemed() { " x" } else { "" });
                } else {
                    println!("{}", code.code());
                }
            }
        }
        CodeCommand::Delete { user_id } => {
            let codes_deleted = auth_service.invitation_codes_delete_all(&user_id).await?;
            println!("💣 Deleted {codes_deleted} invitation codes!");
        }
        CodeCommand::Replenish { user_id } => {
            auth_service.invitation_codes_replenish(&user_id).await?;
        }
    }

    Ok(())
}
