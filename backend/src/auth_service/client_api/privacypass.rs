// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::identifiers::UserId;
use airprotos::common::v1::OperationType;
use privacypass::{
    amortized_tokens::{
        AmortizedBatchTokenRequest, AmortizedBatchTokenResponse, AmortizedToken, server::Server,
    },
    private_tokens::Ristretto255,
};
use tracing::error;

use tokio::sync::Mutex;

use crate::{
    auth_service::{
        AuthService,
        client_record::ClientRecord,
        privacy_pass::{
            AuthServiceBatchedKeyStoreProvider, AuthServiceNonceStore, load_current_key_id,
        },
    },
    errors::auth_service::{IssueTokensError, RedeemTokenError},
};

pub(crate) const MAX_TOKENS_PER_REQUEST: i32 = 100;

impl AuthService {
    pub(crate) async fn as_issue_tokens(
        &self,
        user_id: &UserId,
        operation_type: OperationType,
        token_request: AmortizedBatchTokenRequest<Ristretto255>,
    ) -> Result<AmortizedBatchTokenResponse<Ristretto255>, IssueTokensError> {
        let tokens_requested = token_request.nr() as i32;

        // Start a transaction
        let mut transaction = self
            .db_pool
            .begin()
            .await
            .map_err(|_| IssueTokensError::StorageError)?;

        // Lock the row to prevent concurrent over-issuance.
        let mut client_record = ClientRecord::load_for_update(&mut *transaction, user_id)
            .await
            .map_err(|error| {
                error!(%error, "Error loading client record");
                IssueTokensError::StorageError
            })?
            .ok_or(IssueTokensError::UnknownUser)?;

        // Reset allowance if the epoch (current key) has changed.
        let current_epoch = load_current_key_id(&mut *transaction)
            .await
            .map_err(|_| IssueTokensError::StorageError)?
            .unwrap_or(0);
        if client_record.allowance_epoch != current_epoch {
            client_record.token_allowance = DEFAULT_TOKEN_ALLOWANCE;
            client_record.allowance_epoch = current_epoch;
        }

        if tokens_requested > client_record.token_allowance
            || tokens_requested > MAX_TOKENS_PER_REQUEST
        {
            return Err(IssueTokensError::TooManyTokens);
        }

        let pp_server = Server::<Ristretto255>::new();
        let token_response = {
            let conn_mutex = Mutex::new(&mut *transaction);
            let key_store = AuthServiceBatchedKeyStoreProvider::new(&conn_mutex);
            pp_server
                .issue_token_response(&key_store, token_request)
                .await
                .map_err(|_| IssueTokensError::PrivacyPassError)?
        };

        // Reduce the token allowance by the number of tokens issued.
        client_record.token_allowance -= tokens_requested;
        client_record.update(&mut *transaction).await.map_err(|e| {
            error!("Error updating client record: {:?}", e);
            IssueTokensError::StorageError
        })?;

        transaction
            .commit()
            .await
            .map_err(|_| IssueTokensError::StorageError)?;

        Ok(token_response)
    }

    /// Redeems a single Privacy Pass token, verifying its validity and
    /// preventing double-spending.
    ///
    /// Both the key store and nonce store share a single connection behind
    /// one mutex. This is safe because `Server::redeem_token` accesses them
    /// sequentially (reserve nonce → lookup key + verify → commit/release
    /// nonce), never holding borrows on both at the same time.
    pub(crate) async fn as_redeem_token(
        &self,
        token: AmortizedToken<Ristretto255>,
    ) -> Result<(), RedeemTokenError> {
        let mut conn = self
            .db_pool
            .acquire()
            .await
            .map_err(|_| RedeemTokenError::StorageError)?;

        let conn_mutex = Mutex::new(&mut *conn);
        let key_store = AuthServiceBatchedKeyStoreProvider::new(&conn_mutex);
        let nonce_store = AuthServiceNonceStore::new(&conn_mutex);
        let server = Server::<Ristretto255>::new();

        server
            .redeem_token(&key_store, &nonce_store, token)
            .await
            .map_err(|e| {
                error!("Token redemption failed: {e}");
                match e {
                    privacypass::common::errors::RedeemTokenError::KeyIdNotFound => {
                        RedeemTokenError::UnknownKeyId
                    }
                    _ => RedeemTokenError::InvalidToken,
                }
            })
    }

    /// Issues a single replacement token (used during handle deletion).
    /// Currently unused: refunds are disabled during gradual rollout.
    #[allow(dead_code)]
    pub(crate) async fn as_issue_single_token(
        &self,
        token_request: AmortizedBatchTokenRequest<Ristretto255>,
    ) -> Result<AmortizedBatchTokenResponse<Ristretto255>, IssueTokensError> {
        if token_request.nr() != 1 {
            return Err(IssueTokensError::TooManyTokens);
        }

        let mut transaction = self
            .db_pool
            .begin()
            .await
            .map_err(|_| IssueTokensError::StorageError)?;

        let pp_server = Server::<Ristretto255>::new();
        let token_response = {
            let conn_mutex = Mutex::new(&mut *transaction);
            let key_store = AuthServiceBatchedKeyStoreProvider::new(&conn_mutex);
            pp_server
                .issue_token_response(&key_store, token_request)
                .await
                .map_err(|_| IssueTokensError::PrivacyPassError)?
        };

        transaction
            .commit()
            .await
            .map_err(|_| IssueTokensError::StorageError)?;

        Ok(token_response)
    }
}

#[cfg(test)]
mod tests {
    use privacypass::{
        amortized_tokens::{AmortizedBatchTokenRequest, server::Server},
        auth::authenticate::TokenChallenge,
        common::private::PrivateCipherSuite,
        private_tokens::Ristretto255,
    };
    use sqlx::PgPool;
    use tls_codec::{Deserialize, Serialize};

    use crate::auth_service::{
        AuthService, client_record::ClientRecord,
        client_record::persistence::tests::store_random_client_record,
    };

    use crate::air_service::BackendService;
    use crate::auth_service::user_record::persistence::tests::store_random_user_record;

    use privacypass::common::private::PublicKey;

    /// Helper: creates an AuthService (which bootstraps a VOPRF key) and
    /// returns the public key of the current key.
    async fn setup_with_keypair(
        pool: &PgPool,
    ) -> anyhow::Result<(AuthService, PublicKey<Ristretto255>)> {
        // initialize() calls rotate_keys_if_needed() which creates the first key.
        let service = AuthService::initialize(pool.clone(), "example.com".parse()?, None).await?;

        let keys = crate::auth_service::privacy_pass::load_batched_token_keys(pool).await?;
        let first = keys.first().expect("no VOPRF key after init");
        let public_key = privacypass::common::private::deserialize_public_key::<Ristretto255>(
            &first.public_key,
        )?;

        Ok((service, public_key))
    }

    fn build_challenge() -> TokenChallenge {
        TokenChallenge::new(
            Ristretto255::token_type(),
            "example.com",
            None,
            &["example.com".to_string()],
        )
    }

    /// Issue a batch of tokens, redeem each one, and verify the allowance is decremented.
    #[sqlx::test]
    async fn issue_and_redeem_tokens(pool: PgPool) -> anyhow::Result<()> {
        let (service, public_key) = setup_with_keypair(&pool).await?;

        // Register a user + client record so we have a token allowance.
        let user_record = store_random_user_record(&pool).await?;
        let _client_record =
            store_random_client_record(&pool, user_record.user_id().clone()).await?;

        let challenge = build_challenge();
        let nr = 5u16;

        // Client: create a token request.
        let (token_request, token_state) =
            AmortizedBatchTokenRequest::<Ristretto255>::new(public_key, &challenge, nr)?;

        // Server: issue tokens.
        let token_response = service
            .as_issue_tokens(user_record.user_id(), token_request)
            .await?;

        // Client: finalize tokens.
        let tokens = token_response.issue_tokens(&token_state)?;
        assert_eq!(tokens.len(), nr as usize);

        // Server: redeem each token.
        for token in &tokens {
            service.as_redeem_token(token.clone()).await?;
        }

        // Server: token allowance was decremented.
        let loaded = ClientRecord::load(&pool, user_record.user_id())
            .await?
            .expect("client record missing");
        // Epoch reset gives 10 tokens; 10 - 5 = 5 remaining.
        assert_eq!(loaded.token_allowance, 10 - nr as i32);

        Ok(())
    }

    /// Redeeming the same token twice is rejected (double-spend protection).
    #[sqlx::test]
    async fn double_spend_rejected(pool: PgPool) -> anyhow::Result<()> {
        let (service, public_key) = setup_with_keypair(&pool).await?;

        let user_record = store_random_user_record(&pool).await?;
        let _client_record =
            store_random_client_record(&pool, user_record.user_id().clone()).await?;

        let challenge = build_challenge();
        let (token_request, token_state) =
            AmortizedBatchTokenRequest::<Ristretto255>::new(public_key, &challenge, 1)?;

        let token_response = service
            .as_issue_tokens(user_record.user_id(), token_request)
            .await?;

        let tokens = token_response.issue_tokens(&token_state)?;
        let token = tokens.into_iter().next().unwrap();

        // First redemption succeeds.
        service.as_redeem_token(token.clone()).await?;

        // Second redemption of the same token fails.
        let err = service.as_redeem_token(token).await;
        assert!(err.is_err());

        Ok(())
    }

    /// Requesting more tokens than the per-epoch allowance is rejected.
    #[sqlx::test]
    async fn issue_tokens_exceeds_allowance(pool: PgPool) -> anyhow::Result<()> {
        let (service, public_key) = setup_with_keypair(&pool).await?;

        let user_record = store_random_user_record(&pool).await?;
        let _client_record =
            store_random_client_record(&pool, user_record.user_id().clone()).await?;

        let challenge = build_challenge();
        // Request more tokens than the per-epoch allowance of 10.
        let (token_request, _token_state) =
            AmortizedBatchTokenRequest::<Ristretto255>::new(public_key, &challenge, 11)?;

        let err = service
            .as_issue_tokens(user_record.user_id(), token_request)
            .await;
        assert!(err.is_err());

        Ok(())
    }

    /// `as_issue_single_token` issues exactly one token without checking allowance.
    #[sqlx::test]
    async fn issue_single_token_for_delete(pool: PgPool) -> anyhow::Result<()> {
        let (service, public_key) = setup_with_keypair(&pool).await?;

        let challenge = build_challenge();
        let (token_request, token_state) =
            AmortizedBatchTokenRequest::<Ristretto255>::new(public_key, &challenge, 1)?;

        // issue_single_token does NOT check user allowance — it's for handle
        // delete refunds.
        let token_response = service.as_issue_single_token(token_request).await?;

        let tokens = token_response.issue_tokens(&token_state)?;
        assert_eq!(tokens.len(), 1);

        // The issued token is redeemable.
        service
            .as_redeem_token(tokens.into_iter().next().unwrap())
            .await?;

        Ok(())
    }

    /// `as_issue_single_token` rejects requests for more than one token.
    #[sqlx::test]
    async fn issue_single_token_rejects_batch(pool: PgPool) -> anyhow::Result<()> {
        let (service, public_key) = setup_with_keypair(&pool).await?;

        let challenge = build_challenge();
        let (token_request, _token_state) =
            AmortizedBatchTokenRequest::<Ristretto255>::new(public_key, &challenge, 5)?;

        let err = service.as_issue_single_token(token_request).await;
        assert!(err.is_err());

        Ok(())
    }

    /// End-to-end: issue a token, serialize/deserialize through TLS codec
    /// (as gRPC would), then redeem.
    #[sqlx::test]
    async fn token_roundtrip_through_tls_codec(pool: PgPool) -> anyhow::Result<()> {
        use privacypass::amortized_tokens::AmortizedToken;

        let (service, public_key) = setup_with_keypair(&pool).await?;
        let user_record = store_random_user_record(&pool).await?;
        let _client_record =
            store_random_client_record(&pool, user_record.user_id().clone()).await?;

        let challenge = build_challenge();
        let (token_request, token_state) =
            AmortizedBatchTokenRequest::<Ristretto255>::new(public_key, &challenge, 1)?;

        // Serialize token request as the gRPC handler would.
        let request_bytes = token_request.tls_serialize_detached()?;
        let deserialized_request =
            AmortizedBatchTokenRequest::<Ristretto255>::tls_deserialize_exact(&request_bytes)?;

        let token_response = service
            .as_issue_tokens(user_record.user_id(), deserialized_request)
            .await?;

        // Serialize token response.
        let response_bytes = token_response.tls_serialize_detached()?;
        let deserialized_response = privacypass::amortized_tokens::AmortizedBatchTokenResponse::<
            Ristretto255,
        >::tls_deserialize_exact(&response_bytes)?;

        let tokens = deserialized_response.issue_tokens(&token_state)?;
        let token = tokens.into_iter().next().unwrap();

        // Serialize/deserialize the token itself.
        let token_bytes = token.tls_serialize_detached()?;
        let deserialized_token =
            AmortizedToken::<Ristretto255>::tls_deserialize_exact(&token_bytes)?;

        service.as_redeem_token(deserialized_token).await?;

        Ok(())
    }

    /// Key rotation changes the epoch, resetting the user's token allowance.
    #[sqlx::test]
    async fn epoch_change_resets_allowance(pool: PgPool) -> anyhow::Result<()> {
        let (service, public_key) = setup_with_keypair(&pool).await?;

        let user_record = store_random_user_record(&pool).await?;
        let _client_record =
            store_random_client_record(&pool, user_record.user_id().clone()).await?;

        let challenge = build_challenge();

        // Issue all 10 tokens (epoch resets allowance from 0→10).
        let (token_request, _token_state) =
            AmortizedBatchTokenRequest::<Ristretto255>::new(public_key, &challenge, 10)?;
        service
            .as_issue_tokens(user_record.user_id(), token_request)
            .await?;

        // Allowance is now 0 — requesting more should fail.
        let (token_request, _token_state) =
            AmortizedBatchTokenRequest::<Ristretto255>::new(public_key, &challenge, 1)?;
        assert!(
            service
                .as_issue_tokens(user_record.user_id(), token_request)
                .await
                .is_err()
        );

        // Simulate key rotation: insert a new key with a different ID.
        // This changes the "current epoch" so the next issuance resets.
        let new_public_key = {
            let mut conn = pool.acquire().await?;
            let conn_mutex = tokio::sync::Mutex::new(&mut *conn);
            let key_store =
                crate::auth_service::privacy_pass::AuthServiceBatchedKeyStoreProvider::new(
                    &conn_mutex,
                );
            let server = Server::<Ristretto255>::new();
            server.create_keypair(&key_store).await?
        };

        // Now issuing against the new key should succeed (epoch changed → reset to 10).
        let (token_request, token_state) =
            AmortizedBatchTokenRequest::<Ristretto255>::new(new_public_key, &challenge, 5)?;
        let token_response = service
            .as_issue_tokens(user_record.user_id(), token_request)
            .await?;

        let tokens = token_response.issue_tokens(&token_state)?;
        assert_eq!(tokens.len(), 5);

        // Verify allowance is 5 (10 - 5).
        let loaded = ClientRecord::load(&pool, user_record.user_id())
            .await?
            .expect("missing client record");
        assert_eq!(loaded.token_allowance, 5);

        Ok(())
    }

    /// `rotate_keys_if_needed` creates the first key and skips when one is fresh.
    #[sqlx::test]
    async fn rotate_keys_creates_first_key(pool: PgPool) -> anyhow::Result<()> {
        use crate::auth_service::privacy_pass::{load_batched_token_keys, rotate_keys_if_needed};

        // No keys exist yet.
        let keys_before = load_batched_token_keys(&pool).await?;
        assert!(keys_before.is_empty());

        // Rotation should create a key.
        let rotated = rotate_keys_if_needed(&pool).await?;
        assert!(rotated);

        let keys_after = load_batched_token_keys(&pool).await?;
        assert_eq!(keys_after.len(), 1);

        // Second call: key is fresh, no rotation needed.
        let rotated = rotate_keys_if_needed(&pool).await?;
        assert!(!rotated);

        Ok(())
    }
}
