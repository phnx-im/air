// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use airapiclient::as_api::AsRequestError;
use aircommon::identifiers::Fqdn;
use airprotos::auth_service::v1::OperationType;
use anyhow::Context;

use crate::{
    clients::{CoreUser, api_clients::ApiClients},
    privacy_pass::RequestTokensError,
};

#[derive(Debug, Clone)]
pub struct InvitationCode {
    pub code: String,
    pub copied: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum RequestInvitationCodeError {
    #[error("user quota exceeded")]
    UserQuotaExceeded,
    #[error("global quota exceeded")]
    GlobalQuotaExceeded,
    #[error(transparent)]
    Database(#[from] sqlx::Error),
    #[error(transparent)]
    Generic(#[from] anyhow::Error),
}

impl From<RequestTokensError> for RequestInvitationCodeError {
    fn from(error: RequestTokensError) -> Self {
        match error {
            RequestTokensError::QuotaExceeded => Self::UserQuotaExceeded,
            RequestTokensError::Database(error) => Self::Database(error),
            RequestTokensError::Generic(error) => Self::Generic(error),
        }
    }
}

impl From<AsRequestError> for RequestInvitationCodeError {
    fn from(error: AsRequestError) -> Self {
        if error.is_resource_exhausted() {
            Self::GlobalQuotaExceeded
        } else {
            Self::Generic(error.into())
        }
    }
}

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

    /// Requests a new invitation code from the server (consuming a token in the process)
    pub async fn request_invitation_code(
        &self,
    ) -> Result<InvitationCode, RequestInvitationCodeError> {
        let api_client = self.api_client()?;
        let token = self
            .consume_or_replenish_token(&api_client, OperationType::GetInviteCode)
            .await?;

        let invitation_code = api_client
            .as_get_invitation_codes([token])
            .await?
            .into_iter()
            .next()
            .context("no invitation code received in response")?;

        let invitation_code = InvitationCode {
            code: invitation_code.code,
            copied: false,
        };

        invitation_code.store(self.pool()).await?;

        Ok(invitation_code)
    }

    pub async fn load_invitation_codes(&self) -> anyhow::Result<Vec<InvitationCode>> {
        Ok(InvitationCode::load_all(self.pool()).await?)
    }

    pub async fn mark_invitation_code_as_copied(&self, code: &str) -> anyhow::Result<bool> {
        Ok(InvitationCode::mark_as_copied(self.pool(), code).await?)
    }
}

mod persistence {
    use super::InvitationCode;

    use sqlx::{SqliteExecutor, query, query_as};

    impl InvitationCode {
        pub(crate) async fn store(&self, executor: impl SqliteExecutor<'_>) -> sqlx::Result<()> {
            query!(
                "INSERT INTO invitation_code (code) VALUES (?) ON CONFLICT DO NOTHING",
                self.code
            )
            .execute(executor)
            .await?;
            Ok(())
        }

        pub async fn load_all(
            executor: impl SqliteExecutor<'_>,
        ) -> sqlx::Result<Vec<InvitationCode>> {
            query_as!(
                InvitationCode,
                "SELECT code AS 'code!', copied FROM invitation_code"
            )
            .fetch_all(executor)
            .await
        }

        pub async fn mark_as_copied(
            executor: impl SqliteExecutor<'_>,
            code: &str,
        ) -> sqlx::Result<bool> {
            let result = query_as!(
                InvitationCode,
                "UPDATE invitation_code SET copied = TRUE WHERE code = ?",
                code
            )
            .execute(executor)
            .await?;
            Ok(result.rows_affected() > 0)
        }
    }
}
