// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::{
    identifiers::Fqdn,
    registration::{ChallengeKind, RegistrationInfo},
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
    /// Note: This function creates a new API client for each call. Therefore,
    /// the TCP/TLS/HTTP connection is not reused.
    pub async fn get_registration_info(domain: Fqdn) -> anyhow::Result<RegistrationInfo> {
        let api_clients = ApiClients::new(domain, None);
        let api_client = api_clients.default_client()?;
        match api_client.as_get_registration_info().await {
            Ok(info) => Ok(info),
            // A server from before this RPC existed only registers users who
            // bring an invitation code, which is what it would have answered.
            Err(error) if error.is_unimplemented() => Ok(RegistrationInfo {
                challenge_required: true,
                accepted_challenges: vec![ChallengeKind::InvitationCode],
            }),
            Err(error) => Err(error.into()),
        }
    }
}
