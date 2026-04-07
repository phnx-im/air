// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use airapiclient::ApiClient;
use aircommon::{
    credentials::keys::ClientSigningKey,
    identifiers::{Fqdn, UserId},
    messages::client_as::{
        BatchedTokenKeyResponse, SerializedToken, SerializedTokenRequest, SerializedTokenResponse,
    },
};
use privacypass::{
    TokenType,
    amortized_tokens::{AmortizedBatchTokenRequest, AmortizedBatchTokenResponse},
    auth::authenticate::TokenChallenge,
    common::private::{PublicKey, deserialize_public_key},
    private_tokens::Ristretto255,
};
use sqlx::SqlitePool;
use tls_codec::{Deserialize, Serialize};
use tracing::info;

pub(crate) mod persistence;

/// Requests a batch of Privacy Pass tokens from the AS and stores them locally.
pub(crate) async fn request_and_store_tokens(
    pool: &SqlitePool,
    api_client: &ApiClient,
    user_id: UserId,
    signing_key: &ClientSigningKey,
    count: u16,
) -> anyhow::Result<usize> {
    let keys = persistence::load_batched_token_keys(pool).await?;
    let (_, pk_bytes) = keys
        .first()
        .ok_or_else(|| anyhow::anyhow!("no VOPRF public keys available"))?;

    let public_key: PublicKey<Ristretto255> = deserialize_public_key::<Ristretto255>(pk_bytes)
        .map_err(|_| anyhow::anyhow!("failed to deserialize VOPRF public key"))?;

    let domain = user_id.domain().to_string();
    let challenge = TokenChallenge::new(
        TokenType::PrivateRistretto255,
        &domain,
        None,
        std::slice::from_ref(&domain),
    );

    let (token_request, token_state) =
        AmortizedBatchTokenRequest::<Ristretto255>::new(public_key, &challenge, count)?;

    let request_bytes = SerializedTokenRequest::new(token_request.tls_serialize_detached()?);
    let response = api_client
        .as_issue_tokens(user_id, signing_key, request_bytes)
        .await?;

    let token_response =
        AmortizedBatchTokenResponse::<Ristretto255>::tls_deserialize_exact(response.as_bytes())?;

    let tokens = token_response.issue_tokens(&token_state)?;
    let stored = tokens.len();

    let mut tx = pool.begin().await?;
    for token in tokens {
        let token_bytes = token.tls_serialize_detached()?;
        persistence::store_token(&mut *tx, &token_bytes).await?;
    }
    tx.commit().await?;

    info!(%stored, "stored privacy pass tokens");
    Ok(stored)
}

/// Consumes one token from local storage.
pub(crate) async fn consume_token(pool: &SqlitePool) -> anyhow::Result<Option<SerializedToken>> {
    Ok(persistence::consume_token(pool)
        .await?
        .map(SerializedToken::new))
}

/// Returns the number of locally stored tokens.
pub(crate) async fn token_count(pool: &SqlitePool) -> anyhow::Result<i64> {
    Ok(persistence::token_count(pool).await?)
}

/// Stores batched token keys received from the AS credentials response.
///
/// If the set of key IDs has changed (key rotation occurred), all cached
/// tokens are discarded because they were issued under an old key and are
/// no longer redeemable.
pub(crate) async fn store_batched_token_keys(
    pool: &SqlitePool,
    keys: &[BatchedTokenKeyResponse],
) -> anyhow::Result<()> {
    use std::collections::BTreeSet;

    let existing = persistence::load_batched_token_keys(pool).await?;
    let existing_ids: BTreeSet<u8> = existing.iter().map(|(id, _)| *id).collect();
    let new_ids: BTreeSet<u8> = keys.iter().map(|k| k.token_key_id).collect();

    if existing_ids == new_ids {
        return Ok(());
    }

    info!("VOPRF key set changed, discarding cached tokens");
    persistence::delete_all_tokens(pool).await?;
    persistence::delete_all_batched_token_keys(pool).await?;

    for key in keys {
        persistence::store_batched_token_key(pool, key.token_key_id, &key.public_key).await?;
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
    let keys = persistence::load_batched_token_keys(pool).await?;
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
        persistence::store_token(pool, &token_bytes).await?;
    }
    Ok(())
}

/// Minimum token count below which replenishment is triggered.
pub(crate) const LOW_TOKEN_THRESHOLD: i64 = 5;

/// Target number of tokens to hold locally.
const TARGET_TOKEN_COUNT: i64 = 10;

/// Fetches VOPRF keys from the server, detects key rotation, and requests
/// tokens if the local count is below [`LOW_TOKEN_THRESHOLD`].
///
/// Returns the token count after replenishment.
pub(crate) async fn replenish_if_needed(
    pool: &SqlitePool,
    api_client: &ApiClient,
    user_id: UserId,
    signing_key: &ClientSigningKey,
) -> anyhow::Result<i64> {
    let credentials_response = api_client.as_as_credentials().await?;
    store_batched_token_keys(pool, &credentials_response.batched_token_keys).await?;

    let count = token_count(pool).await?;
    if count < LOW_TOKEN_THRESHOLD {
        let needed = (TARGET_TOKEN_COUNT - count).min(TARGET_TOKEN_COUNT) as u16;
        request_and_store_tokens(pool, api_client, user_id, signing_key, needed).await?;
    }
    token_count(pool).await
}

/// Purges all cached tokens and keys, then replenishes from the server.
///
/// Called when the server reports that the token key has rotated and our
/// cached tokens are stale.
pub(crate) async fn purge_and_replenish(
    pool: &SqlitePool,
    api_client: &ApiClient,
    user_id: UserId,
    signing_key: &ClientSigningKey,
) -> anyhow::Result<()> {
    info!("purging stale tokens after key rotation");
    persistence::delete_all_tokens(pool).await?;
    persistence::delete_all_batched_token_keys(pool).await?;
    replenish_if_needed(pool, api_client, user_id, signing_key).await?;
    Ok(())
}

/// Opaque wrapper around the privacypass `TokenState` needed to finalize
/// a token response.
pub(crate) struct TokenState(privacypass::amortized_tokens::TokenState<Ristretto255>);
