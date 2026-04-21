// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::{
    credentials::keys::UsernameSigningKey,
    crypto::ConnectionDecryptionKey,
    identifiers::{Username, UsernameHash},
    messages::{
        client_as::SerializedToken, client_as_out::UsernameDeleteResponse,
        connection_package::ConnectionPackage,
    },
};
use airprotos::auth_service::v1::OperationType;
use anyhow::{Context, bail};
pub use persistence::UsernameRecord;
use tokio::task::spawn_blocking;
use tracing::{error, warn};

use airapiclient::ApiClient;

use crate::{
    clients::{CONNECTION_PACKAGES, CoreUser},
    privacy_pass,
    store::StoreResult,
    usernames::connection_packages::StorableConnectionPackage,
    utils::connection_ext::StoreExt,
};

pub(crate) mod connection_packages;
mod persistence;

impl CoreUser {
    /// Registers a new username on the server and adds it locally.
    ///
    /// Returns a username record on success, or `None` if the username was already present.
    pub(crate) async fn add_username(
        &self,
        username: Username,
    ) -> StoreResult<Option<UsernameRecord>> {
        let signing_key = UsernameSigningKey::generate()?;
        let username_inner = username.clone();
        let hash = spawn_blocking(move || username_inner.calculate_hash()).await??;

        let api_client = self.api_client()?;

        let token: SerializedToken = self
            .consume_or_replenish_token(&api_client, OperationType::AddUsername)
            .await
            .inspect_err(|e| warn!(%e, "no privacy pass token available for username creation"))?;

        let result = api_client
            .as_create_username(&username, hash, &signing_key, token)
            .await;

        // If the server says our token key is stale, purge and replenish
        // but don't retry immediately — the caller should retry later to
        // maintain timing decorrelation between issuance and redemption.
        let created = match result {
            Err(e) if e.is_unknown_token_key_id() => {
                warn!("unknown token key ID, purging stale tokens");
                self.purge_and_replenish_tokens(&api_client, OperationType::AddUsername)
                    .await?;
                anyhow::bail!("token key rotated; replenished — retry to use decorrelated tokens")
            }
            other => other?,
        };
        if !created {
            return Ok(None);
        }

        let record = UsernameRecord::new(username.clone(), hash, signing_key);

        let rollback = async |delete_locally: bool| {
            let domain = self.user_id().domain();
            if let Ok(Some((token_req, _))) =
                privacy_pass::prepare_delete_token_request(self.pool(), domain).await
            {
                api_client
                    .as_delete_username(record.hash, &record.signing_key, token_req)
                    .await
                    .inspect_err(|error| {
                        error!(%error, "failed to delete username on the server in rollback");
                    })
                    .ok();
            } else {
                error!("failed to prepare token request for rollback delete");
            }
            if delete_locally {
                UsernameRecord::delete(self.pool(), &record.username)
                    .await
                    .inspect_err(|error| {
                        error!(%error, "failed to delete username locally in rollback");
                    })
                    .ok();
            }
        };

        let mut txn = self.pool().begin().await?;
        if let Err(error) = record.store(&mut *txn).await {
            error!(%error, "failed to store username; rollback");
            rollback(false).await;
            return Err(error.into());
        }

        // Publish connection packages
        let connection_package_bundles =
            generate_connection_packages(&record.signing_key, record.hash)?;

        // Store connection packages in the database
        let mut connection_packages = Vec::with_capacity(connection_package_bundles.len());
        for (decryption_key, connection_package) in connection_package_bundles {
            connection_package
                .store_for_username(&mut txn, &username, &decryption_key)
                .await?;
            connection_packages.push(connection_package);
        }
        txn.commit().await?;

        if let Err(error) = api_client
            .as_publish_connection_packages_for_username(
                hash,
                connection_packages,
                &record.signing_key,
            )
            .await
        {
            error!(%error, "failed to publish connection packages; rollback");
            rollback(true).await;
            return Err(error.into());
        }

        Ok(Some(record))
    }

    /// Deletes the username on the server and removes it locally.
    pub(crate) async fn remove_username(
        &self,
        username: &Username,
    ) -> StoreResult<UsernameDeleteResponse> {
        let record = UsernameRecord::load(self.pool(), username)
            .await?
            .context("no username found")?;

        let domain = self.user_id().domain();
        let (token_request_bytes, token_state) =
            privacy_pass::prepare_delete_token_request(self.pool(), domain)
                .await
                .inspect_err(
                    |e| warn!(%e, "failed to prepare privacy pass token for username deletion"),
                )?
                .context("no VOPRF keys available for delete token request")?;

        let api_client = self.api_client()?;
        let (res, token_response_bytes) = api_client
            .as_delete_username(record.hash, &record.signing_key, token_request_bytes)
            .await?;

        // Finalize the refund token if we got one back.
        if let Some(response) = token_response_bytes
            && let Err(e) =
                privacy_pass::finalize_delete_token_response(self.pool(), &response, token_state)
                    .await
        {
            warn!("failed to finalize delete refund token: {e}");
        }

        self.remove_username_locally(username).await?;
        Ok(res)
    }

    pub(crate) async fn remove_username_locally(&self, username: &Username) -> StoreResult<()> {
        let mut txn = self.pool().begin().await?;
        UsernameRecord::delete(txn.as_mut(), username).await?;
        txn.commit().await?;
        Ok(())
    }

    /// Consumes a token from the local cache.
    ///
    /// Returns an error if the cache is empty. Callers must NOT replenish
    /// and consume in the same request chain — doing so lets the server
    /// correlate the authenticated issuance with the anonymous redemption
    /// by timing. The background `TokenReplenishment` task keeps the cache
    /// warm; if the cache is empty, replenish and let the caller retry
    /// later.
    pub(crate) async fn consume_or_replenish_token(
        &self,
        api_client: &ApiClient,
        operation_type: OperationType,
    ) -> anyhow::Result<SerializedToken> {
        if let Some(token) = privacy_pass::consume_token(self.pool(), operation_type).await? {
            return Ok(token);
        }

        let Some(replenish_count) =
            privacy_pass::needs_replenishment(self.pool(), operation_type).await?
        else {
            bail!("no tokens available to replenish");
        };

        let credentials_response = api_client.as_as_credentials().await?;
        self.with_transaction(async move |txn| {
            privacy_pass::store_batched_token_keys(txn, &credentials_response.batched_token_keys)
                .await
        })
        .await?;

        // Cache empty — replenish for future attempts but don't consume
        // immediately. The caller should propagate this error and retry,
        // providing a natural timing gap between issuance and redemption.
        privacy_pass::request_and_store_tokens(
            self.pool(),
            api_client,
            self.user_id().clone(),
            self.signing_key(),
            operation_type,
            replenish_count,
        )
        .await??;

        anyhow::bail!(
            "privacy pass token cache was empty; \
             replenished — retry to use decorrelated tokens"
        )
    }

    /// Purges all cached tokens (key rotation) and replenishes.
    ///
    /// Does NOT consume a token immediately — the caller should retry later
    /// to maintain timing decorrelation between issuance and redemption.
    pub(crate) async fn purge_and_replenish_tokens(
        &self,
        api_client: &ApiClient,
        operation_type: OperationType,
    ) -> anyhow::Result<()> {
        privacy_pass::purge_and_replenish(
            self.pool(),
            api_client,
            self.user_id().clone(),
            operation_type,
            self.signing_key(),
        )
        .await
    }
}

fn generate_connection_packages(
    signing_key: &UsernameSigningKey,
    hash: UsernameHash,
) -> anyhow::Result<Vec<(ConnectionDecryptionKey, ConnectionPackage)>> {
    let mut connection_packages = Vec::with_capacity(CONNECTION_PACKAGES);
    for _ in 0..CONNECTION_PACKAGES - 1 {
        let connection_package = ConnectionPackage::new(hash, signing_key, false)?;
        connection_packages.push(connection_package);
    }
    // Last resort connection package
    let connection_package = ConnectionPackage::new(hash, signing_key, true)?;
    connection_packages.push(connection_package);
    Ok(connection_packages)
}
