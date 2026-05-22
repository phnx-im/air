// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use airbackend::{
    auth_service::{AsConnector, AuthService},
    qs::errors::AsConnectorError,
};
use aircommon::{credentials::keys::ClientVerifyingKey, identifiers::UserId};

#[derive(Debug, Clone)]
pub struct SimpleAsConnector {
    auth_service: AuthService,
}

impl SimpleAsConnector {
    pub fn new(auth_service: &AuthService) -> Self {
        Self {
            auth_service: auth_service.clone(),
        }
    }
}

impl AsConnector for SimpleAsConnector {
    type Error = AsConnectorError;

    async fn client_verifying_key(
        &self,
        user_id: &UserId,
    ) -> Result<Option<ClientVerifyingKey>, Self::Error> {
        self.auth_service
            .load_client_verifying_key(user_id)
            .await
            .map_err(Into::into)
    }
}
