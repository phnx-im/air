// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use anyhow::Context;
use url::Url;

use crate::clients::{CoreUser, api_clients::ApiClients};

impl CoreUser {
    /// Checks if the invitation code is valid.
    ///
    /// Note: This function creates a new API client for each call. Therefore, the TCP/TLS/HTTP
    /// connection is not reused.
    pub async fn check_invitation_code(
        server_url: Url,
        invitation_code: String,
    ) -> anyhow::Result<bool> {
        let domain = server_url.domain().context("missing domain")?.parse()?;
        let api_clients = ApiClients::new(domain, server_url.clone());
        let api_client = api_clients.default_client()?;
        Ok(api_client.as_check_invitation_code(invitation_code).await?)
    }
}
