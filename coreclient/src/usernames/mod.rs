// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::{
    credentials::keys::UsernameSigningKey,
    crypto::ConnectionDecryptionKey,
    identifiers::{Username, UsernameHash},
    messages::{
        client_as::SerializedToken,
        client_as_out::UsernameDeleteResponse,
        connection_package::{ConnectionPackage, ConnectionPackageMetadata},
    },
};
use airprotos::auth_service::v1::{OperationType, SignedConnectionPackage};
use anyhow::Context;
pub use persistence::UsernameRecord;
use tokio::task::spawn_blocking;
use tracing::{error, info, warn};

use airapiclient::ApiClient;

use crate::{
    clients::{CONNECTION_PACKAGES, CoreUser},
    db::access::{WriteConnection, WriteDbConnection},
    privacy_pass,
    usernames::connection_packages::ConnectionPackageRecord,
};

pub(crate) mod connection_packages;
mod persistence;

impl CoreUser {
    /// Check whether a username exists on the AS. Relatively expensive operation, as it
    /// requires computation of a username hash.
    ///
    /// Returns the computed hash of the username if it exists, otherwise `None`.
    pub async fn check_username_exists(
        &self,
        username: Username,
    ) -> anyhow::Result<Option<UsernameHash>> {
        let hash = spawn_blocking(move || username.calculate_hash()).await??;
        let username_exists = self.api_client()?.as_check_username_exists(hash).await?;
        Ok(username_exists.then_some(hash))
    }

    pub async fn usernames(&self) -> anyhow::Result<Vec<Username>> {
        Ok(UsernameRecord::load_all_usernames(self.db().read().await?).await?)
    }

    pub async fn username_records(&self) -> anyhow::Result<Vec<UsernameRecord>> {
        Ok(UsernameRecord::load_all(self.db().read().await?).await?)
    }

    /// Registers a new username on the server and adds it locally.
    ///
    /// Returns a username record on success, or `None` if the username was already present.
    pub async fn add_username(&self, username: Username) -> anyhow::Result<Option<UsernameRecord>> {
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

        let rollback = async |mut connection: WriteDbConnection, delete_locally: bool| {
            let domain = self.user_id().domain();
            if let Ok(Some((token_req, _))) =
                privacy_pass::prepare_delete_token_request(&mut connection, domain).await
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
                UsernameRecord::delete(&mut connection, &record.username)
                    .await
                    .inspect_err(|error| {
                        error!(%error, "failed to delete username locally in rollback");
                    })
                    .ok();
            }
        };

        let mut write = self.db().write().await?;
        let mut txn = write.begin().await?;
        if let Err(error) = record.store(&mut txn).await {
            error!(%error, "failed to store username; rollback");
            drop(txn);
            rollback(write, false).await;
            return Err(error.into());
        }

        // Publish connection packages
        let connection_package_bundles =
            generate_connection_packages(&record.signing_key, record.hash)?;

        // Store connection packages in the database
        for (decryption_key, metadata) in connection_package_bundles.decryption_keys {
            ConnectionPackageRecord::from(metadata)
                .store_for_username(&mut txn, &username, &decryption_key)
                .await?;
        }
        txn.commit().await?;

        if let Err(error) = api_client
            .as_publish_connection_packages_for_username(
                hash,
                connection_package_bundles.legacy,
                connection_package_bundles.signed,
                &record.signing_key,
            )
            .await
        {
            error!(%error, "failed to publish connection packages; rollback");
            rollback(write, true).await;
            return Err(error.into());
        }

        Ok(Some(record))
    }

    /// Deletes the username on the server and removes it locally.
    pub async fn remove_username(
        &self,
        username: &Username,
    ) -> anyhow::Result<UsernameDeleteResponse> {
        let record = UsernameRecord::load(self.db().read().await?, username)
            .await?
            .context("no username found")?;

        let domain = self.user_id().domain();
        let (token_request_bytes, token_state) =
            privacy_pass::prepare_delete_token_request(self.db().write().await?, domain)
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
                privacy_pass::finalize_delete_token_response(self.db(), &response, token_state)
                    .await
        {
            warn!("failed to finalize delete refund token: {e}");
        }

        self.remove_username_locally(username).await?;
        Ok(res)
    }

    pub(crate) async fn remove_username_locally(&self, username: &Username) -> anyhow::Result<()> {
        UsernameRecord::delete(self.db().write().await?, username).await?;
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
        if let Some(token) =
            privacy_pass::consume_token(self.db().write().await?, operation_type).await?
        {
            return Ok(token);
        }

        let credentials_response = api_client.as_as_credentials().await?;

        self.db()
            .with_write_transaction(async |txn| {
                privacy_pass::store_batched_token_keys(
                    txn,
                    &credentials_response.batched_token_keys,
                )
                .await
            })
            .await?;

        // Cache empty — replenish for future attempts but don't consume
        // immediately. The caller should propagate this error and retry,
        // providing a natural timing gap between issuance and redemption.
        let outcome = privacy_pass::replenish(
            self.db(),
            api_client,
            self.user_id().clone(),
            self.signing_key(),
            operation_type,
        )
        .await?;
        info!(?outcome, %operation_type, "replenished tokens on an empty cache");

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
            self.db(),
            api_client,
            self.user_id().clone(),
            operation_type,
            self.signing_key(),
        )
        .await
    }
}

struct GeneratedConnectionPackages {
    legacy: Vec<ConnectionPackage>,
    signed: Vec<SignedConnectionPackage>,
    decryption_keys: Vec<(ConnectionDecryptionKey, ConnectionPackageMetadata)>,
}

fn generate_connection_packages(
    signing_key: &UsernameSigningKey,
    hash: UsernameHash,
) -> anyhow::Result<GeneratedConnectionPackages> {
    // A single last resort legacy package is enough to keep old clients connecting until the legacy
    // fetch is retired.
    let (key, package, metadata) = ConnectionPackage::generate(hash, signing_key, true)?;
    let legacy = vec![package];

    let SignedConnectionPackages {
        packages: signed,
        mut decryption_keys,
    } = generate_signed_connection_packages(signing_key, hash)?;
    decryption_keys.push((key, metadata));

    Ok(GeneratedConnectionPackages {
        legacy,
        signed,
        decryption_keys,
    })
}

pub(crate) struct SignedConnectionPackages {
    pub(crate) packages: Vec<SignedConnectionPackage>,
    pub(crate) decryption_keys: Vec<(ConnectionDecryptionKey, ConnectionPackageMetadata)>,
}

/// Generates a full pool of signed connection packages, the last one being a
/// last resort package.
pub(crate) fn generate_signed_connection_packages(
    signing_key: &UsernameSigningKey,
    hash: UsernameHash,
) -> anyhow::Result<SignedConnectionPackages> {
    let mut packages = Vec::with_capacity(CONNECTION_PACKAGES);
    let mut decryption_keys = Vec::with_capacity(CONNECTION_PACKAGES);
    for i in 0..CONNECTION_PACKAGES {
        let is_last_resort = i + 1 == CONNECTION_PACKAGES;
        let (key, package, metadata) =
            SignedConnectionPackage::generate(hash, signing_key, is_last_resort)?;
        decryption_keys.push((key, metadata));
        packages.push(package);
    }
    Ok(SignedConnectionPackages {
        packages,
        decryption_keys,
    })
}
