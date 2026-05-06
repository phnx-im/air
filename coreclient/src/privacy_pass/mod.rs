// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{collections::HashMap, time::Duration};

use airapiclient::ApiClient;
use aircommon::{
    credentials::keys::ClientSigningKey,
    identifiers::{Fqdn, UserId},
    messages::client_as::{
        BatchedTokenKeyResponse, SerializedToken, SerializedTokenRequest, SerializedTokenResponse,
    },
};
use airprotos::auth_service::v1::OperationType;
use anyhow::Context;
use chrono::{DateTime, Utc};
use privacypass::{
    TokenType,
    amortized_tokens::{AmortizedBatchTokenRequest, AmortizedBatchTokenResponse},
    auth::authenticate::TokenChallenge,
    common::private::{PublicKey, deserialize_public_key},
    private_tokens::Ristretto255,
};
use tls_codec::{Deserialize, Serialize};
use tokio::time;
use tracing::{debug, info, warn};

use crate::db_access::{DbAccess, ReadConnection, WriteConnection, WriteDbTransaction};

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct TokenId {
    pub id: i64,
    pub created_at: DateTime<Utc>,
}

pub(crate) mod persistence;

#[derive(Debug, thiserror::Error)]
pub enum RequestTokensError {
    #[error("quota exceeded: {tokens_available} tokens available in {retry_after:?}")]
    QuotaExceeded {
        /// How long until the quota resets.
        ///
        /// Zero means tokens are available now.
        retry_after: chrono::Duration,
        /// How many tokens will be available.
        ///
        /// Now if retry_after is zero, or after the reset if retry_after is non-zero.
        tokens_available: u16,
    },
}

/// Requests a batch of Privacy Pass tokens from the AS and stores them locally.
pub(crate) async fn request_and_store_tokens(
    db: &DbAccess,
    api_client: &ApiClient,
    user_id: UserId,
    signing_key: &ClientSigningKey,
    operation_type: OperationType,
    count: u16,
) -> anyhow::Result<Result<usize, RequestTokensError>> {
    debug!(%count, %operation_type, "requesting privacy pass tokens");

    let keys: Vec<(u8, Vec<u8>)> =
        persistence::load_batched_token_keys(db.write().await?, operation_type).await?;
    let (_, pk_bytes) = keys.first().context("no VOPRF public keys available")?;

    let public_key: PublicKey<Ristretto255> = deserialize_public_key::<Ristretto255>(pk_bytes)
        .context("failed to deserialize VOPRF public key")?;

    let domain = user_id.domain().to_string();
    let challenge = TokenChallenge::new(
        TokenType::PrivateRistretto255,
        &domain,
        None,
        std::slice::from_ref(&domain),
    );

    let (token_request, token_state) =
        AmortizedBatchTokenRequest::<Ristretto255>::new(public_key, &challenge, count)
            .context("failed to construct batched token request")?;

    let request_bytes = SerializedTokenRequest::new(
        token_request
            .tls_serialize_detached()
            .context("failed to serialize tokens request")?,
    );
    let response = match api_client
        .as_issue_tokens(operation_type, user_id, signing_key, request_bytes)
        .await
    {
        Ok(response) => response,
        Err(error) if error.is_resource_exhausted() => {
            let (retry_after, tokens_available) = error
                .token_quota_exceeded_detail()
                .and_then(|d| {
                    Some((
                        chrono::Duration::seconds(i64::try_from(d.retry_after_secs).ok()?),
                        u16::try_from(d.tokens_available).ok()?,
                    ))
                })
                .unwrap_or((chrono::Duration::zero(), 0));
            return Ok(Err(RequestTokensError::QuotaExceeded {
                retry_after,
                tokens_available,
            }));
        }
        Err(error) => return Err(error.into()),
    };

    let token_response =
        AmortizedBatchTokenResponse::<Ristretto255>::tls_deserialize_exact(response.as_bytes())
            .context("failed to deserialize token response")?;

    let tokens = token_response
        .issue_tokens(&token_state)
        .context("failed to issue tokens")?;
    let stored = tokens.len();

    let serialized_tokens: Vec<Vec<u8>> = tokens
        .into_iter()
        .map(|t| t.tls_serialize_detached())
        .collect::<Result<Vec<_>, _>>()?;

    // It is important to retry storing the tokens in case the database is locked. Otherwise, the
    // client will lose them and the server will not be able to issue more.
    //
    // TODO: Refactor and use a crate or an abstraction for this.
    const MAX_RETRIES: usize = 10;
    const RETRY_DELAY: Duration = Duration::from_secs(1);
    let mut retries = 0;
    loop {
        let res = db
            .with_write_transaction(async |txn| -> sqlx::Result<()> {
                for token_bytes in &serialized_tokens {
                    persistence::store_token(&mut *txn, operation_type, token_bytes).await?;
                }
                Ok(())
            })
            .await;
        match res {
            Ok(()) => {
                info!(%count, %operation_type, "stored privacy pass tokens");
                break;
            }
            Err(error) => {
                const DB_LOCKED_CODE: &str = "5"; // SQLITE_BUSY
                let is_db_locked = error
                    .as_database_error()
                    .is_some_and(|e| e.code().as_deref() == Some(DB_LOCKED_CODE));
                if is_db_locked {
                    retries += 1;
                    if retries >= MAX_RETRIES {
                        return Err(error.into());
                    }
                    warn!(
                        retries,
                        retry_in =? RETRY_DELAY,
                        "Database is locked when storing privacy pass tokens"
                    );
                } else {
                    return Err(error.into());
                }
            }
        }

        time::sleep(RETRY_DELAY).await;
    }

    Ok(Ok(stored))
}

/// Consumes one token from local storage.
pub(crate) async fn consume_token(
    connection: impl WriteConnection,
    operation_type: OperationType,
) -> anyhow::Result<Option<SerializedToken>> {
    Ok(persistence::consume_token(connection, operation_type)
        .await?
        .map(SerializedToken::new))
}

/// Stores batched token keys received from the AS credentials response.
///
/// If the set of key IDs has changed (key rotation occurred), all cached
/// tokens are discarded because they were issued under an old key and are
/// no longer redeemable.
pub(crate) async fn store_batched_token_keys(
    txn: &mut WriteDbTransaction<'_>,
    keys: &[BatchedTokenKeyResponse],
) -> anyhow::Result<()> {
    use std::collections::BTreeSet;

    let keys: HashMap<OperationType, Vec<(_, _)>> =
        keys.iter().fold(HashMap::new(), |mut keys, key| {
            if let Ok(operation_type) = OperationType::try_from(key.operation_type) {
                keys.entry(operation_type)
                    .or_default()
                    .push((key.token_key_id, key.public_key.as_slice()));
            }
            keys
        });

    for (operation_type, keys) in keys {
        let existing = persistence::load_batched_token_keys(&mut *txn, operation_type).await?;
        let existing_ids: BTreeSet<u8> = existing.iter().map(|(id, _)| *id).collect();
        let new_ids: BTreeSet<u8> = keys.iter().map(|(token_key_id, _)| *token_key_id).collect();

        if existing_ids == new_ids {
            continue;
        }

        let discarded = persistence::token_count(&mut *txn, operation_type).await?;
        info!(
            ?existing_ids,
            ?new_ids,
            %discarded,
            "VOPRF key set changed, discarding cached tokens"
        );
        persistence::delete_all_tokens(&mut *txn, operation_type).await?;
        persistence::delete_all_batched_token_keys(&mut *txn, operation_type).await?;

        for (token_key_id, public_key) in keys {
            persistence::store_batched_token_key(
                &mut *txn,
                token_key_id,
                operation_type,
                public_key,
            )
            .await?;
        }
    }
    Ok(())
}

/// Creates a single-token request for use in `DeleteHandle`.
///
/// Returns the serialized request bytes and the token state needed to finalize
/// the response.
pub(crate) async fn prepare_delete_token_request(
    connection: impl ReadConnection,
    domain: &Fqdn,
) -> anyhow::Result<Option<(SerializedTokenRequest, TokenState)>> {
    let keys = persistence::load_batched_token_keys(connection, OperationType::AddUsername).await?;
    let Some((_, pk_bytes)) = keys.first() else {
        return Ok(None);
    };

    let public_key: PublicKey<Ristretto255> = deserialize_public_key::<Ristretto255>(pk_bytes)
        .map_err(|_| anyhow::anyhow!("failed to deserialize VOPRF public key"))?;

    let domain = domain.to_string();
    let challenge = TokenChallenge::new(
        TokenType::PrivateRistretto255,
        &domain,
        None,
        std::slice::from_ref(&domain),
    );

    let (token_request, token_state) =
        AmortizedBatchTokenRequest::<Ristretto255>::new(public_key, &challenge, 1)?;

    let request_bytes = SerializedTokenRequest::new(token_request.tls_serialize_detached()?);
    Ok(Some((request_bytes, TokenState(token_state))))
}

/// Finalizes a token response from a `DeleteHandle` response and stores the
/// token locally.
pub(crate) async fn finalize_delete_token_response(
    db: &DbAccess,
    response: &SerializedTokenResponse,
    token_state: TokenState,
) -> anyhow::Result<()> {
    let token_response =
        AmortizedBatchTokenResponse::<Ristretto255>::tls_deserialize_exact(response.as_bytes())?;

    let tokens = token_response.issue_tokens(&token_state.0)?;
    for token in tokens {
        let token_bytes = token.tls_serialize_detached()?;
        persistence::store_token(db.write().await?, OperationType::AddUsername, &token_bytes)
            .await?;
    }
    Ok(())
}

pub(crate) async fn needs_replenishment(
    connection: impl ReadConnection,
    operation_type: OperationType,
) -> anyhow::Result<Option<u16>> {
    let count = persistence::token_count(connection, operation_type).await?;
    let max_tokens = operation_type.max_tokens_allowance();
    let replenish_count =
        (count < operation_type.low_tokens_threshold()).then_some(max_tokens.saturating_sub(count));
    Ok(replenish_count)
}

/// Purges all cached tokens and keys, then replenishes from the server.
///
/// Called when the server reports that the token key has rotated and our
/// cached tokens are stale.
pub(crate) async fn purge_and_replenish(
    db: &DbAccess,
    api_client: &ApiClient,
    user_id: UserId,
    operation_type: OperationType,
    signing_key: &ClientSigningKey,
) -> anyhow::Result<()> {
    let discarded = persistence::token_count(db.read().await?, operation_type).await?;
    info!(%discarded, "purging stale tokens after server rejected key");
    persistence::delete_all_tokens(db.write().await?, operation_type).await?;
    persistence::delete_all_batched_token_keys(db.write().await?, operation_type).await?;
    request_and_store_tokens(
        db,
        api_client,
        user_id.clone(),
        signing_key,
        operation_type,
        operation_type.max_tokens_allowance(),
    )
    .await??;
    Ok(())
}

/// Opaque wrapper around the privacypass `TokenState` needed to finalize
/// a token response.
pub(crate) struct TokenState(privacypass::amortized_tokens::TokenState<Ristretto255>);
