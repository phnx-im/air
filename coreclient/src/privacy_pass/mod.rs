// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::HashMap;

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
use privacypass::{
    TokenType,
    amortized_tokens::{AmortizedBatchTokenRequest, AmortizedBatchTokenResponse},
    auth::authenticate::TokenChallenge,
    common::private::{PublicKey, deserialize_public_key},
    private_tokens::Ristretto255,
};
use sqlx::{SqliteExecutor, SqlitePool, SqliteTransaction};
use tls_codec::{Deserialize, Serialize};
use tracing::{error, info};

pub(crate) mod persistence;

#[derive(Debug, thiserror::Error)]
pub enum RequestTokensError {
    #[error("quota exceeded")]
    QuotaExceeded,
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error(transparent)]
    Generic(#[from] anyhow::Error),
}

/// Requests a batch of Privacy Pass tokens from the AS and stores them locally.
pub(crate) async fn request_and_store_tokens(
    txn: &mut SqliteTransaction<'_>,
    api_client: &ApiClient,
    user_id: UserId,
    signing_key: &ClientSigningKey,
    operation_type: OperationType,
    count: u16,
) -> Result<usize, RequestTokensError> {
    info!(%count, %operation_type, "requesting privacy pass tokens");

    let result =
        request_tokens_inner(txn, api_client, user_id, signing_key, operation_type, count).await;

    match &result {
        Ok(stored) => info!(%stored, "stored privacy pass tokens"),
        Err(error) => error!(%error, "failed to request privacy pass tokens"),
    }

    result
}

async fn request_tokens_inner(
    txn: &mut SqliteTransaction<'_>,
    api_client: &ApiClient,
    user_id: UserId,
    signing_key: &ClientSigningKey,
    operation_type: OperationType,
    count: u16,
) -> Result<usize, RequestTokensError> {
    let keys: Vec<(u8, Vec<u8>)> =
        persistence::load_batched_token_keys(txn.as_mut(), operation_type).await?;
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
    let response = api_client
        .as_issue_tokens(operation_type, user_id, signing_key, request_bytes)
        .await
        .map_err(|error| {
            if error.is_resource_exhausted() {
                RequestTokensError::QuotaExceeded
            } else {
                RequestTokensError::Generic(error.into())
            }
        })?;

    let token_response =
        AmortizedBatchTokenResponse::<Ristretto255>::tls_deserialize_exact(response.as_bytes())
            .context("failed to deserialize token response")?;

    let tokens = token_response
        .issue_tokens(&token_state)
        .context("failed to issue tokens")?;
    let stored = tokens.len();

    for token in tokens {
        let token_bytes = token
            .tls_serialize_detached()
            .context("failed to serialize issued tokens")?;
        persistence::store_token(txn.as_mut(), operation_type, &token_bytes).await?;
    }

    Ok(stored)
}

/// Consumes one token from local storage.
pub(crate) async fn consume_token(
    executor: impl SqliteExecutor<'_>,
    operation_type: OperationType,
) -> anyhow::Result<Option<SerializedToken>> {
    Ok(persistence::consume_token(executor, operation_type)
        .await?
        .map(SerializedToken::new))
}

/// Stores batched token keys received from the AS credentials response.
///
/// If the set of key IDs has changed (key rotation occurred), all cached
/// tokens are discarded because they were issued under an old key and are
/// no longer redeemable.
pub(crate) async fn store_batched_token_keys(
    txn: &mut SqliteTransaction<'_>,
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
        let existing = persistence::load_batched_token_keys(txn.as_mut(), operation_type).await?;
        let existing_ids: BTreeSet<u8> = existing.iter().map(|(id, _)| *id).collect();
        let new_ids: BTreeSet<u8> = keys.iter().map(|(token_key_id, _)| *token_key_id).collect();

        if existing_ids == new_ids {
            continue;
        }

        let discarded = persistence::token_count(txn.as_mut(), operation_type).await?;
        info!(
            ?existing_ids,
            ?new_ids,
            %discarded,
            "VOPRF key set changed, discarding cached tokens"
        );
        persistence::delete_all_tokens(txn.as_mut(), operation_type).await?;
        persistence::delete_all_batched_token_keys(txn.as_mut(), operation_type).await?;

        for (token_key_id, public_key) in keys {
            persistence::store_batched_token_key(
                txn.as_mut(),
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
    pool: &SqlitePool,
    domain: &Fqdn,
) -> anyhow::Result<Option<(SerializedTokenRequest, TokenState)>> {
    let keys = persistence::load_batched_token_keys(pool, OperationType::AddUsername).await?;
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
    pool: &SqlitePool,
    response: &SerializedTokenResponse,
    token_state: TokenState,
) -> anyhow::Result<()> {
    let token_response =
        AmortizedBatchTokenResponse::<Ristretto255>::tls_deserialize_exact(response.as_bytes())?;

    let tokens = token_response.issue_tokens(&token_state.0)?;
    for token in tokens {
        let token_bytes = token.tls_serialize_detached()?;
        persistence::store_token(pool, OperationType::AddUsername, &token_bytes).await?;
    }
    Ok(())
}

/// Fetches VOPRF keys from the server, detects key rotation, and requests
/// tokens if the local count is below [`OperationType::low_token_threshold`].
///
/// Returns the token count after replenishment.
pub(crate) async fn replenish_if_needed(
    txn: &mut SqliteTransaction<'_>,
    api_client: &ApiClient,
    user_id: UserId,
    signing_key: &ClientSigningKey,
    operation_type: OperationType,
) -> Result<u16, RequestTokensError> {
    // TODO: shouldn't the AS credentials be available somewhere instead of fetching them?
    // because we do that for each operation_type
    let credentials_response = api_client
        .as_as_credentials()
        .await
        .context("failed to fetch AS credentials")?;
    store_batched_token_keys(txn, &credentials_response.batched_token_keys).await?;

    let count = persistence::token_count(txn.as_mut(), operation_type).await?;
    let max_tokens = operation_type.max_tokens_allowance();
    if count < operation_type.low_tokens_threshold() {
        let needed = (max_tokens - count).min(max_tokens);
        request_and_store_tokens(
            txn,
            api_client,
            user_id,
            signing_key,
            operation_type,
            needed,
        )
        .await?;
    }

    Ok(persistence::token_count(txn.as_mut(), operation_type).await?)
}

/// Purges all cached tokens and keys, then replenishes from the server.
///
/// Called when the server reports that the token key has rotated and our
/// cached tokens are stale.
pub(crate) async fn purge_and_replenish(
    pool: &SqlitePool,
    api_client: &ApiClient,
    user_id: UserId,
    operation_type: OperationType,
    signing_key: &ClientSigningKey,
) -> Result<(), RequestTokensError> {
    // if is important that we lock the database here to avoid other parts to update the data
    // (note: if you have a better idea, go for it)
    let mut txn = pool.begin_with("BEGIN EXCLUSIVE").await?;
    let discarded = persistence::token_count(txn.as_mut(), operation_type).await?;
    info!(%discarded, "purging stale tokens after server rejected key");
    persistence::delete_all_tokens(txn.as_mut(), operation_type).await?;
    persistence::delete_all_batched_token_keys(txn.as_mut(), operation_type).await?;
    replenish_if_needed(
        &mut txn,
        api_client,
        user_id.clone(),
        signing_key,
        operation_type,
    )
    .await?;

    txn.commit().await?;
    Ok(())
}

/// Opaque wrapper around the privacypass `TokenState` needed to finalize
/// a token response.
pub(crate) struct TokenState(privacypass::amortized_tokens::TokenState<Ristretto255>);
