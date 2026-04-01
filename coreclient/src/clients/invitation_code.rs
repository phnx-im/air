// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::identifiers::{Fqdn, UserId};
use airprotos::auth_service::v1::InvitationCode;

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

    pub async fn replenish_invitation_codes(
        user_id: UserId,
    ) -> anyhow::Result<Vec<InvitationCode>> {
        let api_clients = ApiClients::new(user_id.domain().clone(), None);
        let api_client = api_clients.default_client()?;
        Ok(api_client.as_replenish_invitation_codes(user_id).await?)
    }
}
