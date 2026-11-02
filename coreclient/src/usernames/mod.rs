// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::{
    credentials::keys::UsernameSigningKey,
    crypto::ConnectionDecryptionKey,
    identifiers::{Username, UsernameHash},
    messages::{
        client_as_out::UsernameDeleteResponse,
        connection_package::{ConnectionPackage, ConnectionPackageHash, ConnectionPackageMetadata},
    },
};
use airprotos::auth_service::v1::{OperationType, SignedConnectionPackage};
use anyhow::Context;
pub use persistence::UsernameRecord;
use persistence::{PendingKey, PendingKeyOrigin};
use tokio::task::spawn_blocking;
use tracing::warn;

use airapiclient::{ApiClient, ClassifyRequestError, RequestErrorKind, as_api::AsRequestError};

use crate::{
    clients::{CONNECTION_PACKAGES, CoreUser},
    db::access::WriteDbTransaction,
    outbound_service::timed_tasks::{TimedTask, TimedTaskKind},
    privacy_pass::{self, ConsumedToken, TokenShortage},
    usernames::connection_packages::ConnectionPackageRecord,
};

pub(crate) mod connection_packages;
mod persistence;
pub(crate) mod sync;

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
        if UsernameRecord::load(self.db().read().await?, &username)
            .await?
            .is_some()
        {
            return Ok(None);
        }

        let username_inner = username.clone();
        let hash = spawn_blocking(move || username_inner.calculate_hash()).await??;

        let api_client = self.api_client()?;

        let consumed = self
            .consume_or_replenish_token(&api_client, OperationType::AddUsername)
            .await?
            .inspect_err(|error| {
                warn!(%error, "no privacy pass token available for username creation");
            })?;

        let pending_key = self
            .db()
            .with_write_transaction(async |txn| PendingKey::load_or_create(txn, &username).await)
            .await?;

        let result = api_client
            .as_create_username(
                &username,
                hash,
                &pending_key.signing_key,
                consumed.token.clone(),
            )
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
        if created {
            // Privacy Pass tokens are only redeemed when the username is successfully created.
            if let Some(position) = consumed.position
                && let Err(error) =
                    privacy_pass::mark_redeemed(self.db().write().await?, &position).await
            {
                warn!(%error, "failed to record a redeemed privacy pass token");
            }
        } else {
            // The AS rolls the redemption back when the username is taken.
            privacy_pass::restore_unredeemed_token(self.db(), OperationType::AddUsername, consumed)
                .await;
            match pending_key.origin {
                PendingKeyOrigin::New => {
                    // The AS has not seen the new key. The username belongs to
                    // someone else.
                    PendingKey::delete(self.db().write().await?, &username).await?;
                    return Ok(None);
                }
                // Publishing the packages below tells whether an earlier
                // attempt registered the username with this key.
                PendingKeyOrigin::Resumed => (),
            }
        }

        let record = UsernameRecord::new(username, hash, pending_key.signing_key);
        self.publish_new_username(&api_client, record).await
    }

    /// Stores a username with fresh connection packages and publishes them.
    async fn publish_new_username(
        &self,
        api_client: &ApiClient,
        record: UsernameRecord,
    ) -> anyhow::Result<Option<UsernameRecord>> {
        let GeneratedConnectionPackages {
            legacy,
            signed,
            decryption_keys,
        } = generate_connection_packages(&record.signing_key, record.hash)?;
        let hashes = self
            .db()
            .with_write_transaction(async |txn| {
                record.store(&mut *txn).await?;
                ConnectionPackageRecord::store_all_for_username(
                    txn,
                    &record.username,
                    decryption_keys,
                )
                .await
            })
            .await?;

        let published = api_client
            .as_publish_connection_packages_for_username(
                record.hash,
                legacy,
                signed,
                &record.signing_key,
            )
            .await;
        match published {
            Ok(()) => {
                let parked = self
                    .db()
                    .with_write_transaction(async |txn| {
                        finish_registration(txn, &record.username).await
                    })
                    .await?;
                if parked {
                    self.outbound_service().notify_pending_chat_operations();
                }
                Ok(Some(record))
            }
            Err(error) if error.is_unauthenticated() => {
                // The username is registered under another key. The AS refused
                // the packages, and their keys go with the username.
                self.db()
                    .with_write_transaction(async |txn| {
                        UsernameRecord::delete(txn, &record.username).await
                    })
                    .await?;
                Ok(None)
            }
            Err(error) => {
                warn!(%error, "failed to publish connection packages, retrying later");
                self.db()
                    .with_write_transaction(async |txn| -> sqlx::Result<()> {
                        discard_unpublished_package_keys(txn, &hashes, &error).await?;
                        TimedTask::schedule_now(txn, TimedTaskKind::PendingUsernameUpload).await
                    })
                    .await?;
                self.outbound_service().notify_pending_usernames();
                Ok(Some(record))
            }
        }
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

    /// Removes the username on this device and on its siblings.
    pub(crate) async fn remove_username_locally(&self, username: &Username) -> anyhow::Result<()> {
        let parked = self
            .db()
            .with_write_transaction(async |txn| -> anyhow::Result<bool> {
                let Some(record) = UsernameRecord::load(&mut *txn, username).await? else {
                    return Ok(false);
                };
                UsernameRecord::delete(&mut *txn, username).await?;
                Ok(sync::park_released(txn, username, record.signing_key.verifying_key()).await?)
            })
            .await?;
        if parked {
            self.outbound_service().notify_pending_chat_operations();
        }
        Ok(())
    }

    /// Consumes a token from the local cache.
    ///
    /// The background `TokenReplenishment` task keeps the cache warm. An empty
    /// cache starts a replenishment and reports the shortage, and the caller has
    /// to give up for now: replenishing and consuming in one request chain
    /// would let the server correlate the authenticated issuance with the
    /// anonymous redemption by timing.
    pub(crate) async fn consume_or_replenish_token(
        &self,
        api_client: &ApiClient,
        operation_type: OperationType,
    ) -> anyhow::Result<Result<ConsumedToken, TokenShortage>> {
        if let Some(consumed) =
            privacy_pass::consume_token(self.db().write().await?, operation_type).await?
        {
            return Ok(Ok(consumed));
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

        let shortage = privacy_pass::replenish_empty_cache(
            self.db(),
            api_client,
            self.user_id().clone(),
            self.signing_key(),
            operation_type,
        )
        .await?;
        Ok(Err(shortage))
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

/// FInal step of the username registration: removes the pending key and
/// schedules a self-group message to be sent to the siblings.
pub(crate) async fn finish_registration(
    txn: &mut WriteDbTransaction<'_>,
    username: &Username,
) -> sqlx::Result<bool> {
    PendingKey::delete(&mut *txn, username).await?;
    sync::park_claimed(txn, username).await
}

/// Deletes the keys of the connection packages whose publication failed with
/// `error`.
pub(crate) async fn discard_unpublished_package_keys(
    txn: &mut WriteDbTransaction<'_>,
    hashes: &[ConnectionPackageHash],
    error: &AsRequestError,
) -> sqlx::Result<()> {
    match error.kind() {
        RequestErrorKind::Network | RequestErrorKind::ServerError => return Ok(()),
        RequestErrorKind::RateLimited { .. }
        | RequestErrorKind::NotFound
        | RequestErrorKind::GroupClosed
        | RequestErrorKind::Rejected => (),
    }
    for hash in hashes {
        ConnectionPackageRecord::delete(&mut *txn, hash).await?;
    }
    Ok(())
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
