// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::fmt;

use airbackend::{air_service::BackendService, auth_service::AuthService, settings::Settings};
use aircommon::identifiers::Fqdn;
use anyhow::Context;

use crate::args::{UsernameArgs, UsernameCommand};

pub async fn run_username_command(
    args: UsernameArgs,
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
        UsernameCommand::List => {
            let usernames = auth_service.usernames_list().await?;
            let mut is_first_record = true;
            for (hash, expiration_data) in usernames {
                if is_first_record {
                    is_first_record = false;
                    println!("Username Hash\tNot before\tNot after");
                }
                println!(
                    "{}\t{}\t{}",
                    hex_encode(hash.as_slice()),
                    expiration_data.not_before().format("%Y-%m-%dT%H:%M:%SZ"),
                    expiration_data.not_after().format("%Y-%m-%dT%H:%M:%SZ"),
                );
            }
            if is_first_record {
                println!("No usernames found");
            }
        }
        UsernameCommand::RefreshExpiring { before } => {
            let refreshed_count = auth_service.username_refresh_expiring(before).await?;
            println!("Refreshed {refreshed_count} usernames");
        }
    }

    Ok(())
}

fn hex_encode(bytes: &[u8]) -> impl std::fmt::Display {
    struct HexDisplay<'a>(&'a [u8]);

    impl<'a> fmt::Display for HexDisplay<'a> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            for byte in self.0 {
                write!(f, "{byte:02x}")?;
            }
            Ok(())
        }
    }

    HexDisplay(bytes)
}
