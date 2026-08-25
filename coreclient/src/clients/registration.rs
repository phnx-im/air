// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::{
    identifiers::Fqdn,
    messages::push_token::PushToken,
    registration::{ChallengeKind, NewAdmissionSession, RegistrationInfo},
};

use crate::clients::{CoreUser, api_clients::ApiClients};

/// A registration the server would not admit as it stands.
///
/// Anything else a registration can run into stays an ordinary error.
#[derive(Debug, thiserror::Error)]
pub enum RegistrationError {
    /// The server wants a challenge answered first, in one of these kinds.
    #[error("registration requires a challenge")]
    ChallengeRequired(Vec<ChallengeKind>),
    /// The server turned down the challenge response the request carried.
    #[error("the challenge response was rejected")]
    ChallengeRejected,
}

impl CoreUser {
    /// Asks a server whether registering with it needs a challenge right now.
    ///
    /// Note: This function creates a new API client for each call, because
    /// there is no CoreUser instance yet.
    pub async fn get_registration_info(domain: Fqdn) -> anyhow::Result<RegistrationInfo> {
        let api_clients = ApiClients::new(domain, None);
        let api_client = api_clients.default_client()?;
        Ok(api_client.as_get_registration_info().await?)
    }

    /// Opens an admission session, which has the server send a challenge to
    /// this device's push endpoint.
    ///
    /// Note: This function creates a new API client for each call, because
    /// there is no CoreUser instance yet.
    pub async fn create_admission_session(
        domain: Fqdn,
        push_token: PushToken,
    ) -> anyhow::Result<NewAdmissionSession> {
        let api_clients = ApiClients::new(domain, None);
        let session = api_clients
            .default_client()?
            .as_create_admission_session(&push_token)
            .await?;
        Ok(session)
    }
}
