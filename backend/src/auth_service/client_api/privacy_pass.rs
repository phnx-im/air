// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::identifiers::UserId;
use airprotos::auth_service::v1::OperationType;
use chrono::Utc;
use privacypass::{
    amortized_tokens::{
        AmortizedBatchTokenRequest, AmortizedBatchTokenResponse, AmortizedToken, server::Server,
    },
    private_tokens::Ristretto255,
};
use sqlx::PgConnection;
use tracing::error;

use tokio::sync::Mutex;

use crate::{
    auth_service::{
        AuthService,
        privacy_pass::{AuthServiceBatchedKeyStoreProvider, AuthServiceNonceStore, TokenAllowance},
    },
    errors::auth_service::{IssueTokensError, RedeemTokenError},
};

impl AuthService {
    pub(crate) async fn as_issue_tokens(
        &self,
        user_id: &UserId,
        operation_type: OperationType,
        token_request: AmortizedBatchTokenRequest<Ristretto255>,
    ) -> Result<AmortizedBatchTokenResponse<Ristretto255>, IssueTokensError> {
        let tokens_requested = token_request.nr() as u16;
        if tokens_requested == 0 {
            return Err(IssueTokensError::BadRequest("zero tokens requested"));
        }

        let now = Utc::now();

        // Make sure the record immediately exists for any further request (preventing a first-issuance race)
        TokenAllowance::ensure_exists(self.db_pool(), user_id, operation_type, now).await?;

        // Start a transaction
        let mut txn = self.db_pool.begin().await?;

        // Lock the row to prevent concurrent over-issuance.
        let mut token_allowance =
            TokenAllowance::load_for_update(&mut txn, user_id, operation_type).await?;

        if !token_allowance.is_valid_at(now) {
            token_allowance.remaining = operation_type.max_tokens_allowance();
            token_allowance.valid_until = operation_type.valid_until_starting_at(now);
        }

        // NB: we might want to switch to returning the maximum amount of possible remaining tokens
        // instead of rejecting the request
        if token_allowance.remaining < tokens_requested {
            return Err(IssueTokensError::TooManyTokensRequested);
        }

        let pp_server = Server::<Ristretto255>::new();
        let token_response = {
            let conn_mutex = Mutex::new(&mut *txn);
            let key_store = AuthServiceBatchedKeyStoreProvider::new(&conn_mutex, operation_type);
            pp_server
                .issue_token_response(&key_store, token_request)
                .await?
        };

        // Reduce the token allowance by the number of tokens issued.
        token_allowance.remaining -= tokens_requested;
        token_allowance.update(&mut *txn, user_id).await?;

        txn.commit().await?;

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
        conn: &mut PgConnection,
        token: AmortizedToken<Ristretto255>,
        operation_type: OperationType,
    ) -> Result<(), RedeemTokenError> {
        let conn_mutex = Mutex::new(&mut *conn);
        let key_store = AuthServiceBatchedKeyStoreProvider::new(&conn_mutex, operation_type);
        let nonce_store = AuthServiceNonceStore::new(&conn_mutex, operation_type);
        let server = Server::<Ristretto255>::new();

        server
            .redeem_token(&key_store, &nonce_store, token)
            .await
            .map_err(|error| {
                error!(%error, "Token redemption failed");
                match error {
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
        operation_type: OperationType,
    ) -> Result<AmortizedBatchTokenResponse<Ristretto255>, IssueTokensError> {
        if token_request.nr() != 1 {
            return Err(IssueTokensError::TooManyTokensRequested);
        }

        let mut transaction = self.db_pool.begin().await?;

        let pp_server = Server::<Ristretto255>::new();
        let token_response = {
            let conn_mutex = Mutex::new(&mut *transaction);
            let key_store = AuthServiceBatchedKeyStoreProvider::new(&conn_mutex, operation_type);
            pp_server
                .issue_token_response(&key_store, token_request)
                .await?
        };

        transaction.commit().await?;

        Ok(token_response)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use airprotos::auth_service::v1::OperationType;
    use privacypass::{
        amortized_tokens::AmortizedBatchTokenRequest, auth::authenticate::TokenChallenge,
        common::private::PrivateCipherSuite, private_tokens::Ristretto255,
    };
    use sqlx::PgPool;
    use tls_codec::{Deserialize, Serialize};

    use crate::{
        auth_service::{
            AuthService, client_record::persistence::tests::store_random_client_record,
            privacy_pass::TokenAllowance,
        },
        errors::auth_service::IssueTokensError,
    };

    use crate::air_service::BackendService;
    use crate::auth_service::user_record::persistence::tests::store_random_user_record;

    use privacypass::common::private::PublicKey;

    /// Helper: creates an AuthService (which bootstraps a VOPRF key) and
    /// returns the public key of the current key.
    async fn setup_with_keypair(
        pool: &PgPool,
    ) -> anyhow::Result<(AuthService, HashMap<OperationType, PublicKey<Ristretto255>>)> {
        // initialize() calls rotate_keys_if_needed() which creates the first key.
        let service = AuthService::initialize(pool.clone(), "example.com".parse()?, None).await?;

        let public_keys = crate::auth_service::privacy_pass::load_batched_token_keys(pool)
            .await?
            .into_iter()
            .filter_map(|btr| {
                let public_key =
                    privacypass::common::private::deserialize_public_key::<Ristretto255>(
                        &btr.public_key,
                    )
                    .ok()?;
                Some((btr.operation_type, public_key))
            })
            .collect();

        Ok((service, public_keys))
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
        let (service, public_keys) = setup_with_keypair(&pool).await?;
        let public_key = *public_keys.get(&OperationType::AddUsername).unwrap();

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
            .as_issue_tokens(
                user_record.user_id(),
                OperationType::AddUsername,
                token_request,
            )
            .await?;

        // Client: finalize tokens.
        let tokens = token_response.issue_tokens(&token_state)?;
        assert_eq!(tokens.len(), nr as usize);

        // Server: redeem each token.
        for token in &tokens {
            service
                .as_redeem_token(
                    pool.acquire().await?.as_mut(),
                    token.clone(),
                    OperationType::AddUsername,
                )
                .await?;
        }

        // Server: token allowance was decremented.
        let loaded: TokenAllowance =
            TokenAllowance::load(&pool, user_record.user_id(), OperationType::AddUsername)
                .await?
                .expect("client record missing");
        // Epoch reset gives 10 tokens; 10 - 5 = 5 remaining.
        assert_eq!(loaded.remaining, 10 - nr);

        Ok(())
    }

    /// Redeeming the same token twice is rejected (double-spend protection).
    #[sqlx::test]
    async fn double_spend_rejected(pool: PgPool) -> anyhow::Result<()> {
        let (service, public_keys) = setup_with_keypair(&pool).await?;
        let public_key = *public_keys.get(&OperationType::GetInviteCode).unwrap();

        let user_record = store_random_user_record(&pool).await?;
        let _client_record =
            store_random_client_record(&pool, user_record.user_id().clone()).await?;

        let challenge = build_challenge();
        let (token_request, token_state) =
            AmortizedBatchTokenRequest::<Ristretto255>::new(public_key, &challenge, 1)?;

        let token_response = service
            .as_issue_tokens(
                user_record.user_id(),
                OperationType::GetInviteCode,
                token_request,
            )
            .await?;

        let tokens = token_response.issue_tokens(&token_state)?;
        let token = tokens.into_iter().next().unwrap();

        // First redemption succeeds.
        service
            .as_redeem_token(
                pool.acquire().await?.as_mut(),
                token.clone(),
                OperationType::GetInviteCode,
            )
            .await?;

        // Second redemption of the same token fails.
        let err = service
            .as_redeem_token(
                pool.acquire().await?.as_mut(),
                token,
                OperationType::GetInviteCode,
            )
            .await;
        assert!(err.is_err());

        Ok(())
    }

    /// Requesting more tokens than the per-epoch allowance is rejected.
    #[sqlx::test]
    async fn issue_tokens_exceeds_allowance(pool: PgPool) -> anyhow::Result<()> {
        let (service, public_keys) = setup_with_keypair(&pool).await?;
        let public_key = *public_keys.get(&OperationType::AddUsername).unwrap();

        let user_record = store_random_user_record(&pool).await?;
        let _client_record =
            store_random_client_record(&pool, user_record.user_id().clone()).await?;

        let challenge = build_challenge();
        // Request more tokens than the per-epoch allowance of 10.
        let (token_request, _token_state) = AmortizedBatchTokenRequest::<Ristretto255>::new(
            public_key,
            &challenge,
            OperationType::GetInviteCode.max_tokens_allowance() + 1,
        )?;

        let err = service
            .as_issue_tokens(
                user_record.user_id(),
                OperationType::GetInviteCode,
                token_request,
            )
            .await;
        assert!(err.is_err());

        Ok(())
    }

    /// `as_issue_single_token` issues exactly one token without checking allowance.
    #[sqlx::test]
    async fn issue_single_token_for_delete(pool: PgPool) -> anyhow::Result<()> {
        let (service, public_keys) = setup_with_keypair(&pool).await?;
        let public_key = *public_keys.get(&OperationType::GetInviteCode).unwrap();

        let challenge = build_challenge();
        let (token_request, token_state) =
            AmortizedBatchTokenRequest::<Ristretto255>::new(public_key, &challenge, 1)?;

        // issue_single_token does NOT check user allowance — it's for handle
        // delete refunds.
        let token_response = service
            .as_issue_single_token(token_request, OperationType::GetInviteCode)
            .await?;

        let tokens = token_response.issue_tokens(&token_state)?;
        assert_eq!(tokens.len(), 1);

        // The issued token is redeemable.
        service
            .as_redeem_token(
                pool.acquire().await?.as_mut(),
                tokens.into_iter().next().unwrap(),
                OperationType::GetInviteCode,
            )
            .await?;

        Ok(())
    }

    /// `as_issue_single_token` rejects requests for more than one token.
    #[sqlx::test]
    async fn issue_single_token_rejects_batch(pool: PgPool) -> anyhow::Result<()> {
        let (service, public_keys) = setup_with_keypair(&pool).await?;
        let public_key = *public_keys.get(&OperationType::GetInviteCode).unwrap();

        let challenge = build_challenge();
        let (token_request, _token_state) =
            AmortizedBatchTokenRequest::<Ristretto255>::new(public_key, &challenge, 5)?;

        let err = service
            .as_issue_single_token(token_request, OperationType::GetInviteCode)
            .await;

        assert!(err.is_err());

        Ok(())
    }

    /// End-to-end: issue a token, serialize/deserialize through TLS codec
    /// (as gRPC would), then redeem.
    #[sqlx::test]
    async fn token_roundtrip_through_tls_codec(pool: PgPool) -> anyhow::Result<()> {
        use privacypass::amortized_tokens::AmortizedToken;

        let (service, public_keys) = setup_with_keypair(&pool).await?;
        let public_key = *public_keys.get(&OperationType::AddUsername).unwrap();

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
            .as_issue_tokens(
                user_record.user_id(),
                OperationType::AddUsername,
                deserialized_request,
            )
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

        service
            .as_redeem_token(
                pool.acquire().await?.as_mut(),
                deserialized_token,
                OperationType::AddUsername,
            )
            .await?;

        Ok(())
    }

    /// `rotate_keys_if_needed` creates the first key and skips when one is fresh.
    #[sqlx::test]
    async fn rotate_keys_creates_first_keys(pool: PgPool) -> anyhow::Result<()> {
        use crate::auth_service::privacy_pass::{load_batched_token_keys, rotate_keys_if_needed};

        // No keys exist yet.
        let keys_before = load_batched_token_keys(&pool).await?;
        assert!(keys_before.is_empty());

        // Rotation should create a key.
        let rotated = rotate_keys_if_needed(&pool).await?;
        assert!(rotated.contains(&OperationType::AddUsername));
        assert!(rotated.contains(&OperationType::GetInviteCode));

        let keys_after = load_batched_token_keys(&pool).await?;
        assert_eq!(keys_after.len(), 2);

        // Second call: key is fresh, no rotation needed.
        let rotated = rotate_keys_if_needed(&pool).await?;
        assert!(rotated.is_empty());

        Ok(())
    }

    /// Issuing a token with the wrong operation type should fail
    #[sqlx::test]
    async fn public_key_operation_type_mismatch(pool: PgPool) -> anyhow::Result<()> {
        let (service, public_keys) = setup_with_keypair(&pool).await?;
        let public_key = *public_keys.get(&OperationType::AddUsername).unwrap();

        let user_record = store_random_user_record(&pool).await?;
        let _client_record =
            store_random_client_record(&pool, user_record.user_id().clone()).await?;

        let challenge = build_challenge();
        let (token_request, _token_state) =
            AmortizedBatchTokenRequest::<Ristretto255>::new(public_key, &challenge, 1)?;

        let token_response = service
            .as_issue_tokens(
                user_record.user_id(),
                OperationType::GetInviteCode,
                token_request,
            )
            .await;

        assert!(matches!(
            token_response,
            Err(IssueTokensError::PrivacyPassError(_))
        ));

        Ok(())
    }
}
