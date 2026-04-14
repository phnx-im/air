// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::identifiers::Fqdn;
use airprotos::common::v1::OperationType;
use tracing::warn;

use crate::clients::{CoreUser, api_clients::ApiClients};

impl CoreUser {
    /// Checks if the invitation code is valid.
    ///
    /// Note: This function creates a new API client for each call. Therefore, the TCP/TLS/HTTP
    /// connection is not reused.
    pub async fn check_invitation_code(
        domain: Fqdn,
        invitation_code: String,
    ) -> anyhow::Result<bool> {
        let api_clients = ApiClients::new(domain, None);
        let api_client = api_clients.default_client()?;
        Ok(api_client.as_check_invitation_code(invitation_code).await?)
    }

    pub async fn get_invitation_code(&self) -> anyhow::Result<String> {
        let api_client = self.api_client()?;
        let token = self
            .consume_or_replenish_token(&api_client, OperationType::GetInviteCode)
            .await
            .inspect_err(|e| warn!(%e, "no privacy pass token available for handle creation"))?;
        Ok(api_client
            .as_get_invitation_codes([token])
            .await?
            .into_iter()
            .next()
            .unwrap()
            .code) // TODO: don't do this
    }
}
