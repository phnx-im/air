// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use airbackend::{air_service::BackendService, auth_service::AuthService, settings::Settings};
use aircommon::identifiers::Fqdn;
use anyhow::Context;

use crate::args::{CodeArgs, CodeCommand};

pub async fn run_code_command(
    args: CodeArgs,
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
            let stats = auth_service.invitation_code_stats().await?;
            println!("Total codes: {}", stats.count);
            println!("Redeemed codes: {}", stats.redeemed);
        }
        CodeCommand::List {
            n,
            include_redeemed,
        } => {
            let codes = auth_service.invitation_codes_list(n, false).await?;
            for (code, redeemed) in codes {
                if include_redeemed {
                    println!("{}{}", code, if redeemed { " x" } else { "" });
                } else {
                    println!("{}", code);
                }
            }
        }
        CodeCommand::Generate { n } => {
            auth_service.invitation_codes_generate(n).await?;
            println!("Generated {} codes", n);
        }
    }

    Ok(())
}
