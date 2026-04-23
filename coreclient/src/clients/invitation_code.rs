// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::identifiers::Fqdn;
use airprotos::auth_service::v1::OperationType;
use anyhow::Context;
use chrono::{DateTime, Utc};
use tracing::warn;

use crate::{
    TokenId,
    clients::{CoreUser, api_clients::ApiClients},
    privacy_pass,
    utils::connection_ext::StoreExt,
};

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct InvitationCode {
    pub code: String,
    pub copied: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, thiserror::Error)]
pub enum RequestInvitationCodeError {
    #[error("global quota exceeded")]
    GlobalQuotaExceeded,
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
        token_id: TokenId,
    ) -> anyhow::Result<Result<InvitationCode, RequestInvitationCodeError>> {
        let api_client = self.api_client()?;

        let token = TokenId::load(self.pool(), &token_id)
            .await?
            .context("no token found")?;

        let result = api_client.as_get_invitation_codes([token]).await;
        let codes = match result {
            Ok(codes) => codes,
            Err(e) if e.is_network_error() => {
                // Token is not burned, but the request failed
                return Err(e.into());
            }
            Err(e) if e.is_resource_exhausted() => {
                // Token is not burned, but the global quota is exceeded
                return Ok(Err(RequestInvitationCodeError::GlobalQuotaExceeded));
            }
            Err(e) => {
                // Token is burned
                if let Err(error) = TokenId::delete(self.pool(), &token_id).await {
                    warn!(%error, "failed to delete burned token");
                }
                return Err(e.into());
            }
        };

        let invitation_code = InvitationCode {
            code: codes
                .into_iter()
                .next()
                .context("no invitation code received in response")?
                .code,
            copied: false,
            created_at: Utc::now(),
        };

        self.with_transaction(async |txn| -> sqlx::Result<()> {
            invitation_code.store(txn.as_mut()).await?;
            TokenId::delete(txn.as_mut(), &token_id).await?;
            Ok(())
        })
        .await?;

        Ok(Ok(invitation_code))
    }

    pub async fn load_invitation_codes(&self) -> anyhow::Result<Vec<InvitationCode>> {
        Ok(InvitationCode::load_all(self.pool()).await?)
    }

    pub async fn load_invitation_token_ids(&self) -> anyhow::Result<Vec<TokenId>> {
        privacy_pass::persistence::load_token_ids(self.pool(), OperationType::GetInviteCode)
            .await
            .map_err(Into::into)
    }

    pub async fn mark_invitation_code_as_copied(&self, code: &str) -> anyhow::Result<()> {
        Ok(InvitationCode::mark_as_copied(self.pool(), code).await?)
    }

    pub async fn clear_copied_codes(&self) -> anyhow::Result<()> {
        Ok(InvitationCode::delete_all_copied(self.pool()).await?)
    }
}

mod persistence {
    use crate::db_access::{ReadConnection, WriteConnection};

    use super::InvitationCode;

    use sqlx::{SqliteExecutor, query, query_as};

    impl InvitationCode {
        pub(crate) async fn store(&self, mut connection: impl WriteConnection) -> sqlx::Result<()> {
            query!(
                "INSERT INTO invitation_code (
                    code, created_at, copied
                ) VALUES (?, ?, ?)",
                self.code,
                self.created_at,
                self.copied
            )
            .execute(connection.as_mut())
            .await?;
            Ok(())
        }

        pub async fn load_all(
            mut connection: impl ReadConnection,
        ) -> sqlx::Result<Vec<InvitationCode>> {
            query_as!(
                InvitationCode,
                r#"SELECT code, copied, created_at AS "created_at: _"
                FROM invitation_code"#
            )
            .fetch_all(connection.as_mut())
            .await
        }

        pub async fn mark_as_copied(
            mut connection: impl WriteConnection,
            code: &str,
        ) -> sqlx::Result<()> {
            query!(
                "UPDATE invitation_code SET copied = TRUE WHERE code = ?",
                code
            )
            .execute(connection.as_mut())
            .await?;
            Ok(())
        }

        pub async fn delete_all_copied(mut connection: impl WriteConnection) -> sqlx::Result<()> {
            query!("DELETE FROM invitation_code WHERE copied = TRUE",)
                .execute(connection.as_mut())
                .await?;
            Ok(())
        }
    }
}
