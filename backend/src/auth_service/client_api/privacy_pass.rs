// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::identifiers::UserId;
use airprotos::auth_service::v1::OperationType;
use chrono::{DateTime, TimeDelta, Utc};
use privacypass::{
    amortized_tokens::{
        AmortizedBatchTokenRequest, AmortizedBatchTokenResponse, AmortizedToken, server::Server,
    },
    common::{private::serialize_public_key, store::PrivateKeyStore},
    private_tokens::Ristretto255,
};
use sha2::{Digest, Sha256};
use sqlx::PgConnection;
use tls_codec::Deserialize;
use tracing::error;

use tokio::sync::Mutex;

use crate::{
    auth_service::{
        AuthService,
        privacy_pass::{
            AuthServiceBatchedKeyStoreProvider, AuthServiceNonceStore, TokenAllowance,
            TokenIssuanceKey,
        },
    },
    errors::auth_service::{IssueTokenBatchError, IssueTokensError, RedeemTokenError},
};

/// How far off the server clock a claimed allowance epoch may be. A client
/// requesting right around an epoch boundary must not be rejected for landing
/// on the adjacent bucket.
const EPOCH_BOUNDARY_TOLERANCE: TimeDelta = TimeDelta::minutes(10);

/// Whether `claimed` is the allowance epoch of `operation_type` at `now` or an
/// adjacent one within the boundary tolerance.
fn epoch_is_acceptable(operation_type: OperationType, now: DateTime<Utc>, claimed: u32) -> bool {
    [
        now,
        now - EPOCH_BOUNDARY_TOLERANCE,
        now + EPOCH_BOUNDARY_TOLERANCE,
    ]
    .into_iter()
    .any(|at| operation_type.allowance_epoch_at(at) == claimed)
}

/// Outcome of a token batch issuance request.
///
/// Conflicts and stale epochs are expected outcomes of the multi device seed
/// agreement protocol, so they are data rather than errors.
#[derive(Debug)]
pub(crate) enum TokenBatchIssuance {
    Issued(AmortizedBatchTokenResponse<Ristretto255>),
    Conflict,
    InvalidEpoch { current_epoch: u32 },
}

impl AuthService {
    pub(crate) async fn as_issue_tokens(
        &self,
        user_id: &UserId,
        operation_type: OperationType,
        token_request: AmortizedBatchTokenRequest<Ristretto255>,
        now: DateTime<Utc>,
    ) -> Result<AmortizedBatchTokenResponse<Ristretto255>, IssueTokensError> {
        if OperationType::Unspecified == operation_type {
            return Err(IssueTokensError::BadRequest("unknown operation type"));
        }

        let tokens_requested = token_request.nr() as u16;
        if tokens_requested == 0 {
            return Err(IssueTokensError::BadRequest("zero tokens requested"));
        }

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

        if let OperationType::AddUsername = operation_type {
            // Token allowance is not yet enforced for AddUsername
        } else if token_allowance.remaining < tokens_requested {
            let (retry_after_secs, tokens_available) = if token_allowance.remaining > 0 {
                (0, token_allowance.remaining)
            } else {
                let secs = token_allowance
                    .valid_until
                    .signed_duration_since(now)
                    .num_seconds()
                    .max(0) as u64;
                (secs, operation_type.max_tokens_allowance())
            };
            return Err(IssueTokensError::TooManyTokensRequested {
                retry_after_secs,
                tokens_available,
            });
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
        token_allowance.remaining = token_allowance.remaining.saturating_sub(tokens_requested);
        token_allowance.update(&mut *txn, user_id).await?;

        txn.commit().await?;

        Ok(token_response)
    }

    /// Issues one token batch per (user, operation type, allowance epoch, key),
    /// answering any repeat of the same request for free.
    ///
    /// The request bytes are taken raw so that the hash the idempotency row is
    /// keyed on is over exactly what the client sent. No allowance counter is
    /// read or written on this path.
    pub(crate) async fn as_issue_token_batch(
        &self,
        user_id: &UserId,
        operation_type: OperationType,
        allowance_epoch: u32,
        token_request_bytes: &[u8],
        now: DateTime<Utc>,
    ) -> Result<TokenBatchIssuance, IssueTokenBatchError> {
        if OperationType::Unspecified == operation_type {
            return Err(IssueTokenBatchError::BadRequest("unknown operation type"));
        }

        let token_request =
            AmortizedBatchTokenRequest::<Ristretto255>::tls_deserialize_exact(token_request_bytes)
                .map_err(|_| IssueTokenBatchError::BadRequest("invalid token request"))?;

        // Uniform batch sizes are what makes requests byte-identical across a
        // user's devices, and they stop the request leaking cache state.
        if token_request.nr() != operation_type.max_tokens_allowance() as usize {
            return Err(IssueTokenBatchError::BadRequest(
                "batch size must be the full allowance",
            ));
        }

        if !epoch_is_acceptable(operation_type, now, allowance_epoch) {
            return Ok(TokenBatchIssuance::InvalidEpoch {
                current_epoch: operation_type.allowance_epoch_at(now),
            });
        }
        let allowance_epoch = i32::try_from(allowance_epoch)
            .map_err(|_| IssueTokenBatchError::BadRequest("allowance epoch out of range"))?;

        let truncated_token_key_id = token_request.truncated_token_key_id();
        let request_hash = Sha256::digest(token_request_bytes);

        let mut txn = self.db_pool.begin().await?;

        // The row is keyed on the full key fingerprint: the one-byte truncated
        // ID recurs across key generations while older issuance rows are still
        // live, which would fabricate conflicts or grants.
        let key_fingerprint = {
            let conn_mutex = Mutex::new(&mut *txn);
            let key_store = AuthServiceBatchedKeyStoreProvider::new(&conn_mutex, operation_type);
            let voprf_server = key_store
                .get(&truncated_token_key_id)
                .await
                .ok_or(IssueTokenBatchError::BadRequest("unknown token key id"))?;
            Sha256::digest(serialize_public_key::<Ristretto255>(
                voprf_server.get_public_key(),
            ))
        };

        let issuance_key = TokenIssuanceKey {
            user_id,
            operation_type,
            allowance_epoch,
            key_fingerprint: key_fingerprint.as_slice(),
        };
        if let Some(stored_hash) = issuance_key
            .claim(&mut txn, request_hash.as_slice())
            .await?
            && stored_hash != request_hash.as_slice()
        {
            return Ok(TokenBatchIssuance::Conflict);
        }

        let pp_server = Server::<Ristretto255>::new();
        let token_response = {
            let conn_mutex = Mutex::new(&mut *txn);
            let key_store = AuthServiceBatchedKeyStoreProvider::new(&conn_mutex, operation_type);
            pp_server
                .issue_token_response(&key_store, token_request)
                .await?
        };

        txn.commit().await?;

        Ok(TokenBatchIssuance::Issued(token_response))
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
            return Err(IssueTokensError::BadRequest(
                "single token endpoint requires exactly one token",
            ));
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
    use chrono::{TimeDelta, TimeZone, Utc};
    use privacypass::{
        amortized_tokens::{AmortizedBatchTokenRequest, AmortizedToken, TokenState},
        auth::authenticate::TokenChallenge,
        common::private::PrivateCipherSuite,
        private_tokens::Ristretto255,
    };
    use sqlx::{PgPool, postgres::PgPoolOptions};
    use tls_codec::{Deserialize, Serialize};
    use tokio_util::sync::CancellationToken;

    use crate::{
        auth_service::{
            AuthService, client_record::persistence::tests::store_random_client_record,
            privacy_pass::TokenAllowance,
        },
        errors::auth_service::{IssueTokenBatchError, IssueTokensError, RedeemTokenError},
    };

    use crate::air_service::BackendService;
    use crate::auth_service::user_record::persistence::tests::store_random_user_record;

    use privacypass::common::private::PublicKey;

    use super::{TokenBatchIssuance, epoch_is_acceptable};

    /// Helper: creates an AuthService (which bootstraps a VOPRF key) and
    /// returns the public key of the current key.
    async fn setup_with_keypair(
        pool: &PgPool,
    ) -> anyhow::Result<(AuthService, HashMap<OperationType, PublicKey<Ristretto255>>)> {
        // initialize() calls rotate_keys_if_needed() which creates the first key.
        let service = AuthService::initialize(
            pool.clone(),
            "example.com".parse()?,
            Default::default(),
            CancellationToken::new(),
        )
        .await?;

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

    /// Backdates the keys of an operation type, the only way to reach a
    /// rotation from a test: `created_at` is `now()` on insert.
    async fn age_keys(
        pool: &PgPool,
        operation_type: OperationType,
        days: i32,
    ) -> anyhow::Result<()> {
        sqlx::query!(
            "UPDATE as_batched_key \
             SET created_at = now() - make_interval(days => $2) \
             WHERE operation_type = $1",
            operation_type as i16,
            days
        )
        .execute(pool)
        .await?;
        Ok(())
    }

    /// Issues a single token for `operation_type` under the given public key.
    async fn issue_one_token(
        service: &AuthService,
        pool: &PgPool,
        operation_type: OperationType,
        public_key: PublicKey<Ristretto255>,
    ) -> anyhow::Result<privacypass::amortized_tokens::AmortizedToken<Ristretto255>> {
        let user_record = store_random_user_record(pool).await?;
        let _client_record =
            store_random_client_record(pool, user_record.user_id().clone()).await?;

        let challenge = build_challenge();
        let (token_request, token_state) =
            AmortizedBatchTokenRequest::<Ristretto255>::new(public_key, &challenge, 1)?;
        let token_response = service
            .as_issue_tokens(
                user_record.user_id(),
                operation_type,
                token_request,
                Utc::now(),
            )
            .await?;
        let token = token_response
            .issue_tokens(&token_state)?
            .into_iter()
            .next()
            .expect("one token was requested");
        Ok(token)
    }

    fn build_challenge() -> TokenChallenge {
        TokenChallenge::new(
            Ristretto255::token_type(),
            "example.com",
            None,
            &["example.com".to_string()],
        )
    }

    /// Public key of the newest key for `operation_type`.
    async fn current_public_key(
        pool: &PgPool,
        operation_type: OperationType,
    ) -> anyhow::Result<PublicKey<Ristretto255>> {
        let record = crate::auth_service::privacy_pass::load_batched_token_keys(pool)
            .await?
            .into_iter()
            .find(|record| record.operation_type == operation_type)
            .expect("no key for operation type");
        Ok(privacypass::common::private::deserialize_public_key::<
            Ristretto255,
        >(&record.public_key)?)
    }

    /// A full-allowance batch request, serialized as the gRPC layer hands it to
    /// the handler.
    fn batch_request(
        public_key: PublicKey<Ristretto255>,
        operation_type: OperationType,
    ) -> anyhow::Result<(Vec<u8>, TokenState<Ristretto255>)> {
        let challenge = build_challenge();
        let (request, state) = AmortizedBatchTokenRequest::<Ristretto255>::new(
            public_key,
            &challenge,
            operation_type.max_tokens_allowance(),
        )?;
        Ok((request.tls_serialize_detached()?, state))
    }

    fn serialize_tokens(tokens: &[AmortizedToken<Ristretto255>]) -> anyhow::Result<Vec<Vec<u8>>> {
        let mut serialized = Vec::with_capacity(tokens.len());
        for token in tokens {
            serialized.push(token.tls_serialize_detached()?);
        }
        Ok(serialized)
    }

    async fn count_issuance_rows(pool: &PgPool) -> anyhow::Result<i64> {
        Ok(
            sqlx::query_scalar!("SELECT COUNT(*) FROM as_token_issuance")
                .fetch_one(pool)
                .await?
                .unwrap_or_default(),
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
                Utc::now(),
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
                Utc::now(),
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
                Utc::now(),
            )
            .await;
        assert!(err.is_err());

        Ok(())
    }

    /// When the quota is fully exhausted the error carries a non-zero retry_after_secs and the full
    /// allowance as tokens_available.
    #[sqlx::test]
    async fn quota_exhausted_returns_retry_after(pool: PgPool) -> anyhow::Result<()> {
        let (service, public_keys) = setup_with_keypair(&pool).await?;
        let public_key = *public_keys.get(&OperationType::GetInviteCode).unwrap();

        let user_record = store_random_user_record(&pool).await?;
        let _client_record =
            store_random_client_record(&pool, user_record.user_id().clone()).await?;

        let max = OperationType::GetInviteCode.max_tokens_allowance();
        let challenge = build_challenge();

        // Exhaust the full allowance.
        let (token_request, _) =
            AmortizedBatchTokenRequest::<Ristretto255>::new(public_key, &challenge, max)?;
        service
            .as_issue_tokens(
                user_record.user_id(),
                OperationType::GetInviteCode,
                token_request,
                Utc::now(),
            )
            .await?;

        // Now request even one more token.
        let (token_request, _) =
            AmortizedBatchTokenRequest::<Ristretto255>::new(public_key, &challenge, 1)?;
        let err = service
            .as_issue_tokens(
                user_record.user_id(),
                OperationType::GetInviteCode,
                token_request,
                Utc::now(),
            )
            .await
            .unwrap_err();

        let IssueTokensError::TooManyTokensRequested {
            retry_after_secs,
            tokens_available,
        } = err
        else {
            panic!("expected TooManyTokensRequested, got {err:?}");
        };
        assert!(retry_after_secs > 0, "should have a non-zero retry delay");
        assert_eq!(
            tokens_available, max,
            "should advertise the full allowance after reset"
        );

        Ok(())
    }

    /// When some tokens remain but fewer than requested, the error signals they are available
    /// immediately with retry_after_secs == 0.
    #[sqlx::test]
    async fn quota_partial_returns_remaining_tokens_now(pool: PgPool) -> anyhow::Result<()> {
        let (service, public_keys) = setup_with_keypair(&pool).await?;
        let public_key = *public_keys.get(&OperationType::GetInviteCode).unwrap();

        let user_record = store_random_user_record(&pool).await?;
        let _client_record =
            store_random_client_record(&pool, user_record.user_id().clone()).await?;

        let challenge = build_challenge();

        // Fresh user has 1 token remaining (the full GetInviteCode allowance). Requesting 2 exceeds
        // it, triggering the partial-quota path.
        let (token_request, _) =
            AmortizedBatchTokenRequest::<Ristretto255>::new(public_key, &challenge, 2)?;
        let err = service
            .as_issue_tokens(
                user_record.user_id(),
                OperationType::GetInviteCode,
                token_request,
                Utc::now(),
            )
            .await
            .unwrap_err();

        let IssueTokensError::TooManyTokensRequested {
            retry_after_secs,
            tokens_available,
        } = err
        else {
            panic!("expected TooManyTokensRequested, got {err:?}");
        };
        assert_eq!(
            retry_after_secs, 0,
            "token is available now, no wait needed"
        );
        assert_eq!(
            tokens_available, 1,
            "one token remains in the current epoch"
        );

        Ok(())
    }

    /// Once the allowance window ends the counter starts over, and until then
    /// the error tells the client how long is left of it.
    #[sqlx::test]
    async fn issue_tokens_resets_allowance_after_rollover(pool: PgPool) -> anyhow::Result<()> {
        let (service, public_keys) = setup_with_keypair(&pool).await?;
        let operation_type = OperationType::GetInviteCode;
        let public_key = *public_keys.get(&operation_type).unwrap();

        let user_record = store_random_user_record(&pool).await?;
        let _client_record =
            store_random_client_record(&pool, user_record.user_id().clone()).await?;

        let challenge = build_challenge();
        let request = || {
            AmortizedBatchTokenRequest::<Ristretto255>::new(public_key, &challenge, 1)
                .map(|(request, _)| request)
        };

        // Draws the single token of the window starting here.
        let opened_at = Utc.with_ymd_and_hms(2026, 8, 4, 12, 0, 0).unwrap();
        service
            .as_issue_tokens(user_record.user_id(), operation_type, request()?, opened_at)
            .await?;

        // An hour before the window ends there is nothing left to draw.
        let err = service
            .as_issue_tokens(
                user_record.user_id(),
                operation_type,
                request()?,
                opened_at + TimeDelta::hours(23),
            )
            .await
            .unwrap_err();
        let IssueTokensError::TooManyTokensRequested {
            retry_after_secs, ..
        } = err
        else {
            panic!("expected TooManyTokensRequested, got {err:?}");
        };
        assert_eq!(retry_after_secs, TimeDelta::hours(1).num_seconds() as u64);

        // Past the end of the window the allowance is back.
        let rolled_over_at = opened_at + TimeDelta::days(1) + TimeDelta::seconds(1);
        service
            .as_issue_tokens(
                user_record.user_id(),
                operation_type,
                request()?,
                rolled_over_at,
            )
            .await?;

        let allowance = TokenAllowance::load(&pool, user_record.user_id(), operation_type)
            .await?
            .expect("allowance record missing");
        assert_eq!(
            allowance.remaining, 0,
            "the token of the new window was drawn"
        );
        assert_eq!(
            allowance.valid_until,
            rolled_over_at + TimeDelta::days(1),
            "the window now runs from the request that reset it"
        );

        Ok(())
    }

    /// Token allowance is not yet enforced for `AddUsername`
    #[sqlx::test]
    async fn add_username_bypasses_allowance(pool: PgPool) -> anyhow::Result<()> {
        let (service, public_keys) = setup_with_keypair(&pool).await?;
        let public_key = *public_keys.get(&OperationType::AddUsername).unwrap();

        let user_record = store_random_user_record(&pool).await?;
        let _client_record =
            store_random_client_record(&pool, user_record.user_id().clone()).await?;

        let challenge = build_challenge();
        let requested = OperationType::AddUsername.max_tokens_allowance() + 1;
        let (token_request, token_state) =
            AmortizedBatchTokenRequest::<Ristretto255>::new(public_key, &challenge, requested)?;

        // Exceeds the allowance, but AddUsername skips the check for now.
        let token_response = service
            .as_issue_tokens(
                user_record.user_id(),
                OperationType::AddUsername,
                token_request,
                Utc::now(),
            )
            .await?;

        let tokens = token_response.issue_tokens(&token_state)?;
        assert_eq!(tokens.len(), requested as usize);

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
                Utc::now(),
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

    /// A token issued under the outgoing key is still redeemable while the AS
    /// keeps that key around for the overlap window.
    #[sqlx::test]
    async fn token_of_outgoing_key_redeems_during_overlap(pool: PgPool) -> anyhow::Result<()> {
        use crate::auth_service::privacy_pass::rotate_keys_if_needed;

        let (service, public_keys) = setup_with_keypair(&pool).await?;
        let public_key = *public_keys.get(&OperationType::AddUsername).unwrap();
        let token =
            issue_one_token(&service, &pool, OperationType::AddUsername, public_key).await?;

        // Past the rotation period, inside the overlap window.
        age_keys(&pool, OperationType::AddUsername, 91).await?;
        let rotated = rotate_keys_if_needed(&pool).await?;
        assert!(rotated.contains(&OperationType::AddUsername));

        service
            .as_redeem_token(
                pool.acquire().await?.as_mut(),
                token,
                OperationType::AddUsername,
            )
            .await?;

        Ok(())
    }

    /// A token issued under a key that was removed past the overlap window is
    /// rejected as having an unknown key ID.
    #[sqlx::test]
    async fn token_of_removed_key_is_rejected(pool: PgPool) -> anyhow::Result<()> {
        use crate::auth_service::privacy_pass::rotate_keys_if_needed;

        let (service, public_keys) = setup_with_keypair(&pool).await?;
        let public_key = *public_keys.get(&OperationType::AddUsername).unwrap();
        let token =
            issue_one_token(&service, &pool, OperationType::AddUsername, public_key).await?;

        // Past the rotation period plus the overlap window.
        age_keys(&pool, OperationType::AddUsername, 98).await?;
        rotate_keys_if_needed(&pool).await?;

        let error = service
            .as_redeem_token(
                pool.acquire().await?.as_mut(),
                token,
                OperationType::AddUsername,
            )
            .await
            .expect_err("the issuing key is gone");

        let RedeemTokenError::UnknownKeyId = error else {
            panic!("expected UnknownKeyId, got {error:?}");
        };

        Ok(())
    }

    /// `load_batched_token_keys` marks exactly the newest key of each operation
    /// type as current.
    #[sqlx::test]
    async fn load_batched_token_keys_marks_newest_as_current(pool: PgPool) -> anyhow::Result<()> {
        use crate::auth_service::privacy_pass::{load_batched_token_keys, rotate_keys_if_needed};

        setup_with_keypair(&pool).await?;

        // Rotate AddUsername only, leaving it with an outgoing and an incoming
        // key while GetInviteCode keeps its single key.
        age_keys(&pool, OperationType::AddUsername, 91).await?;
        rotate_keys_if_needed(&pool).await?;

        let keys = load_batched_token_keys(&pool).await?;

        let newest_id = sqlx::query_scalar!(
            "SELECT token_key_id FROM as_batched_key \
             WHERE operation_type = $1 \
             ORDER BY created_at DESC \
             LIMIT 1",
            OperationType::AddUsername as i16
        )
        .fetch_one(&pool)
        .await?;

        let add_username: Vec<_> = keys
            .iter()
            .filter(|key| key.operation_type == OperationType::AddUsername)
            .collect();
        assert_eq!(add_username.len(), 2);
        let current: Vec<_> = add_username.iter().filter(|key| key.is_current).collect();
        assert_eq!(current.len(), 1);
        assert_eq!(i16::from(current[0].token_key_id), newest_id);

        let invite_code: Vec<_> = keys
            .iter()
            .filter(|key| key.operation_type == OperationType::GetInviteCode)
            .collect();
        assert_eq!(invite_code.len(), 1);
        assert!(invite_code[0].is_current);

        Ok(())
    }

    /// A request built right before an epoch boundary may claim either side of
    /// it, but nothing further out.
    #[test]
    fn epoch_tolerance_spans_the_boundary() {
        let operation_type = OperationType::GetInviteCode;
        let accepts = |now, claimed| epoch_is_acceptable(operation_type, now, claimed);

        let near_boundary = Utc.with_ymd_and_hms(2026, 8, 4, 23, 55, 0).unwrap();
        let epoch = operation_type.allowance_epoch_at(near_boundary);
        assert!(accepts(near_boundary, epoch));
        assert!(accepts(near_boundary, epoch + 1));
        assert!(!accepts(near_boundary, epoch - 1));
        assert!(!accepts(near_boundary, epoch + 2));

        let mid_epoch = Utc.with_ymd_and_hms(2026, 8, 4, 12, 0, 0).unwrap();
        let epoch = operation_type.allowance_epoch_at(mid_epoch);
        assert!(accepts(mid_epoch, epoch));
        assert!(!accepts(mid_epoch, epoch + 1));
        assert!(!accepts(mid_epoch, epoch - 1));
    }

    /// The key ID a request carries matches the ID the key store filed the
    /// key under, so the handler's key lookup resolves the right key.
    #[sqlx::test]
    async fn truncated_token_key_id_matches_the_stored_key(pool: PgPool) -> anyhow::Result<()> {
        setup_with_keypair(&pool).await?;
        let operation_type = OperationType::AddUsername;
        let public_key = current_public_key(&pool, operation_type).await?;
        let (request_bytes, _) = batch_request(public_key, operation_type)?;

        let stored_id = sqlx::query_scalar!(
            "SELECT token_key_id FROM as_batched_key WHERE operation_type = $1",
            operation_type as i16,
        )
        .fetch_one(&pool)
        .await?;

        let request =
            AmortizedBatchTokenRequest::<Ristretto255>::tls_deserialize_exact(&request_bytes)?;
        assert_eq!(i16::from(request.truncated_token_key_id()), stored_id);

        Ok(())
    }

    /// Replaying the same batch request is free: one issuance row, identical
    /// tokens, and the allowance counter is never touched.
    #[sqlx::test]
    async fn issue_token_batch_replay_is_idempotent(pool: PgPool) -> anyhow::Result<()> {
        let (service, _) = setup_with_keypair(&pool).await?;
        let operation_type = OperationType::AddUsername;
        let public_key = current_public_key(&pool, operation_type).await?;

        let user_record = store_random_user_record(&pool).await?;
        let _client_record =
            store_random_client_record(&pool, user_record.user_id().clone()).await?;

        let (request_bytes, token_state) = batch_request(public_key, operation_type)?;
        let now = Utc::now();
        let epoch = operation_type.allowance_epoch_at(now);

        let first = service
            .as_issue_token_batch(
                user_record.user_id(),
                operation_type,
                epoch,
                &request_bytes,
                now,
            )
            .await?;
        let second = service
            .as_issue_token_batch(
                user_record.user_id(),
                operation_type,
                epoch,
                &request_bytes,
                now,
            )
            .await?;

        let TokenBatchIssuance::Issued(first) = first else {
            panic!("expected Issued, got {first:?}");
        };
        let TokenBatchIssuance::Issued(second) = second else {
            panic!("expected Issued, got {second:?}");
        };

        let first_tokens = serialize_tokens(&first.issue_tokens(&token_state)?)?;
        let second_tokens = serialize_tokens(&second.issue_tokens(&token_state)?)?;
        assert_eq!(
            first_tokens.len(),
            operation_type.max_tokens_allowance() as usize
        );
        assert_eq!(first_tokens, second_tokens);

        assert_eq!(count_issuance_rows(&pool).await?, 1);
        assert!(
            TokenAllowance::load(&pool, user_record.user_id(), operation_type)
                .await?
                .is_none(),
            "the batch path must not touch the allowance counter"
        );

        Ok(())
    }

    /// A second, different request for the same (user, operation type, epoch,
    /// key) is a seed divergence and is rejected.
    #[sqlx::test]
    async fn issue_token_batch_rejects_conflicting_request(pool: PgPool) -> anyhow::Result<()> {
        let (service, _) = setup_with_keypair(&pool).await?;
        let operation_type = OperationType::AddUsername;
        let public_key = current_public_key(&pool, operation_type).await?;

        let user_record = store_random_user_record(&pool).await?;
        let _client_record =
            store_random_client_record(&pool, user_record.user_id().clone()).await?;

        let (first_bytes, _) = batch_request(public_key, operation_type)?;
        let (second_bytes, _) = batch_request(public_key, operation_type)?;
        assert_ne!(first_bytes, second_bytes);

        let now = Utc::now();
        let epoch = operation_type.allowance_epoch_at(now);
        service
            .as_issue_token_batch(
                user_record.user_id(),
                operation_type,
                epoch,
                &first_bytes,
                now,
            )
            .await?;

        let outcome = service
            .as_issue_token_batch(
                user_record.user_id(),
                operation_type,
                epoch,
                &second_bytes,
                now,
            )
            .await?;
        let TokenBatchIssuance::Conflict = outcome else {
            panic!("expected Conflict, got {outcome:?}");
        };
        assert_eq!(count_issuance_rows(&pool).await?, 1);

        Ok(())
    }

    /// An epoch the server does not consider current is rejected, and the
    /// outcome carries the server's value so the client can converge.
    #[sqlx::test]
    async fn issue_token_batch_rejects_wrong_epoch(pool: PgPool) -> anyhow::Result<()> {
        let (service, _) = setup_with_keypair(&pool).await?;
        let operation_type = OperationType::GetInviteCode;
        let public_key = current_public_key(&pool, operation_type).await?;

        let user_record = store_random_user_record(&pool).await?;
        let _client_record =
            store_random_client_record(&pool, user_record.user_id().clone()).await?;

        let (request_bytes, _) = batch_request(public_key, operation_type)?;
        let now = Utc::now();
        let epoch = operation_type.allowance_epoch_at(now);

        let outcome = service
            .as_issue_token_batch(
                user_record.user_id(),
                operation_type,
                epoch + 5,
                &request_bytes,
                now,
            )
            .await?;

        let TokenBatchIssuance::InvalidEpoch { current_epoch } = outcome else {
            panic!("expected InvalidEpoch, got {outcome:?}");
        };
        assert_eq!(current_epoch, epoch);
        assert_eq!(count_issuance_rows(&pool).await?, 0);

        Ok(())
    }

    /// The next allowance epoch is a fresh batch under the same key, and the
    /// epoch it replaced stops being claimable.
    #[sqlx::test]
    async fn issue_token_batch_grants_a_batch_per_epoch(pool: PgPool) -> anyhow::Result<()> {
        let (service, _) = setup_with_keypair(&pool).await?;
        let operation_type = OperationType::GetInviteCode;
        let public_key = current_public_key(&pool, operation_type).await?;

        let user_record = store_random_user_record(&pool).await?;
        let _client_record =
            store_random_client_record(&pool, user_record.user_id().clone()).await?;

        let now = Utc.with_ymd_and_hms(2026, 8, 4, 12, 0, 0).unwrap();
        let next_day = now + TimeDelta::days(1);
        let epoch = operation_type.allowance_epoch_at(now);
        let next_epoch = operation_type.allowance_epoch_at(next_day);
        assert_eq!(next_epoch, epoch + 1);

        let (first_bytes, _) = batch_request(public_key, operation_type)?;
        service
            .as_issue_token_batch(
                user_record.user_id(),
                operation_type,
                epoch,
                &first_bytes,
                now,
            )
            .await?;

        let (second_bytes, token_state) = batch_request(public_key, operation_type)?;
        let outcome = service
            .as_issue_token_batch(
                user_record.user_id(),
                operation_type,
                next_epoch,
                &second_bytes,
                next_day,
            )
            .await?;
        let TokenBatchIssuance::Issued(response) = outcome else {
            panic!("expected Issued, got {outcome:?}");
        };
        assert_eq!(
            response.issue_tokens(&token_state)?.len(),
            operation_type.max_tokens_allowance() as usize
        );
        assert_eq!(
            count_issuance_rows(&pool).await?,
            2,
            "one row per epoch, both under the same key"
        );

        // A device that never learned of the rollover is told which epoch is
        // current rather than served the old one again.
        let (stale_bytes, _) = batch_request(public_key, operation_type)?;
        let outcome = service
            .as_issue_token_batch(
                user_record.user_id(),
                operation_type,
                epoch,
                &stale_bytes,
                next_day,
            )
            .await?;
        let TokenBatchIssuance::InvalidEpoch { current_epoch } = outcome else {
            panic!("expected InvalidEpoch, got {outcome:?}");
        };
        assert_eq!(current_epoch, next_epoch);
        assert_eq!(count_issuance_rows(&pool).await?, 2);

        Ok(())
    }

    /// A retry that crosses an epoch boundary replays the batch it was already
    /// given instead of drawing a second one under the new epoch.
    #[sqlx::test]
    async fn issue_token_batch_replays_across_the_epoch_boundary(
        pool: PgPool,
    ) -> anyhow::Result<()> {
        let (service, _) = setup_with_keypair(&pool).await?;
        let operation_type = OperationType::GetInviteCode;
        let public_key = current_public_key(&pool, operation_type).await?;

        let user_record = store_random_user_record(&pool).await?;
        let _client_record =
            store_random_client_record(&pool, user_record.user_id().clone()).await?;

        let before_boundary = Utc.with_ymd_and_hms(2026, 8, 4, 23, 55, 0).unwrap();
        let after_boundary = Utc.with_ymd_and_hms(2026, 8, 5, 0, 2, 0).unwrap();
        let claimed_epoch = operation_type.allowance_epoch_at(before_boundary);
        assert_eq!(
            operation_type.allowance_epoch_at(after_boundary),
            claimed_epoch + 1
        );

        let (request_bytes, token_state) = batch_request(public_key, operation_type)?;
        let issue = |now| {
            service.as_issue_token_batch(
                user_record.user_id(),
                operation_type,
                claimed_epoch,
                &request_bytes,
                now,
            )
        };

        let outcome = issue(before_boundary).await?;
        let TokenBatchIssuance::Issued(response) = outcome else {
            panic!("expected Issued, got {outcome:?}");
        };
        let tokens = serialize_tokens(&response.issue_tokens(&token_state)?)?;

        // The client keeps claiming the epoch it built the request in. The
        // tolerance accepts it, and it resolves to the row that already exists
        // rather than opening one for the epoch the server has moved to.
        let outcome = issue(after_boundary).await?;
        let TokenBatchIssuance::Issued(response) = outcome else {
            panic!("expected Issued, got {outcome:?}");
        };
        assert_eq!(
            serialize_tokens(&response.issue_tokens(&token_state)?)?,
            tokens
        );
        assert_eq!(count_issuance_rows(&pool).await?, 1);

        Ok(())
    }

    /// Batches are all-or-nothing: a partial batch never becomes the request the
    /// other devices have to reproduce.
    #[sqlx::test]
    async fn issue_token_batch_rejects_partial_batch(pool: PgPool) -> anyhow::Result<()> {
        let (service, _) = setup_with_keypair(&pool).await?;
        let operation_type = OperationType::AddUsername;
        let public_key = current_public_key(&pool, operation_type).await?;

        let user_record = store_random_user_record(&pool).await?;
        let _client_record =
            store_random_client_record(&pool, user_record.user_id().clone()).await?;

        let challenge = build_challenge();
        let (request, _) = AmortizedBatchTokenRequest::<Ristretto255>::new(
            public_key,
            &challenge,
            operation_type.max_tokens_allowance() - 1,
        )?;
        let request_bytes = request.tls_serialize_detached()?;
        let now = Utc::now();
        let epoch = operation_type.allowance_epoch_at(now);

        let err = service
            .as_issue_token_batch(
                user_record.user_id(),
                operation_type,
                epoch,
                &request_bytes,
                now,
            )
            .await
            .unwrap_err();
        assert!(
            matches!(err, IssueTokenBatchError::BadRequest(_)),
            "expected BadRequest, got {err:?}"
        );

        Ok(())
    }

    /// A key rotation inside an allowance epoch grants a fresh batch under the
    /// new key, refunding the tokens the rotation invalidated.
    #[sqlx::test]
    async fn issue_token_batch_after_rotation_grants_second_batch(
        pool: PgPool,
    ) -> anyhow::Result<()> {
        use crate::auth_service::privacy_pass::rotate_keys_if_needed;

        let (service, _) = setup_with_keypair(&pool).await?;
        let operation_type = OperationType::AddUsername;

        let user_record = store_random_user_record(&pool).await?;
        let _client_record =
            store_random_client_record(&pool, user_record.user_id().clone()).await?;

        let now = Utc::now();
        let epoch = operation_type.allowance_epoch_at(now);
        let old_key = current_public_key(&pool, operation_type).await?;
        let (old_bytes, _) = batch_request(old_key, operation_type)?;
        service
            .as_issue_token_batch(
                user_record.user_id(),
                operation_type,
                epoch,
                &old_bytes,
                now,
            )
            .await?;

        age_keys(&pool, operation_type, 91).await?;
        let rotated = rotate_keys_if_needed(&pool).await?;
        assert!(rotated.contains(&operation_type));

        let new_key = current_public_key(&pool, operation_type).await?;
        let (new_bytes, token_state) = batch_request(new_key, operation_type)?;
        let outcome = service
            .as_issue_token_batch(
                user_record.user_id(),
                operation_type,
                epoch,
                &new_bytes,
                now,
            )
            .await?;

        let TokenBatchIssuance::Issued(response) = outcome else {
            panic!("expected Issued, got {outcome:?}");
        };
        assert_eq!(
            response.issue_tokens(&token_state)?.len(),
            operation_type.max_tokens_allowance() as usize
        );
        assert_eq!(
            count_issuance_rows(&pool).await?,
            2,
            "one row per key, both in the same allowance epoch"
        );

        Ok(())
    }

    /// Concurrent rotation runs (as two replicas would) create exactly one key.
    #[sqlx::test]
    async fn concurrent_rotation_creates_one_key(pool: PgPool) -> anyhow::Result<()> {
        use crate::auth_service::privacy_pass::{load_batched_token_keys, rotate_keys_if_needed};

        let operation_type = OperationType::AddUsername;
        rotate_keys_if_needed(&pool).await?;
        age_keys(&pool, operation_type, 91).await?;

        // A second pool stands in for a second replica. Racing two runs over
        // one pool does not exercise anything: the pool hands out its
        // connections one after the other, so the runs never overlap.
        let replica_pool = PgPoolOptions::new()
            .max_connections(1)
            .connect_with((*pool.connect_options()).clone())
            .await?;

        let (first, second) = tokio::join!(
            rotate_keys_if_needed(&pool),
            rotate_keys_if_needed(&replica_pool)
        );
        let rotations = [
            first?.contains(&operation_type),
            second?.contains(&operation_type),
        ]
        .into_iter()
        .filter(|rotated| *rotated)
        .count();
        assert_eq!(rotations, 1, "only one run may rotate");

        let keys = load_batched_token_keys(&pool)
            .await?
            .into_iter()
            .filter(|key| key.operation_type == operation_type)
            .count();
        assert_eq!(keys, 2, "the aged key plus exactly one new key");

        Ok(())
    }

    /// The legacy counter path is unaffected by an issuance row for the same
    /// user, which is the accepted double budget while both endpoints are live.
    #[sqlx::test]
    async fn legacy_issue_tokens_decrements_counter_independently(
        pool: PgPool,
    ) -> anyhow::Result<()> {
        let (service, _) = setup_with_keypair(&pool).await?;
        let operation_type = OperationType::GetInviteCode;
        let public_key = current_public_key(&pool, operation_type).await?;

        let user_record = store_random_user_record(&pool).await?;
        let _client_record =
            store_random_client_record(&pool, user_record.user_id().clone()).await?;

        let now = Utc::now();
        let epoch = operation_type.allowance_epoch_at(now);
        let (batch_bytes, _) = batch_request(public_key, operation_type)?;
        service
            .as_issue_token_batch(
                user_record.user_id(),
                operation_type,
                epoch,
                &batch_bytes,
                now,
            )
            .await?;
        assert_eq!(count_issuance_rows(&pool).await?, 1);

        let challenge = build_challenge();
        let (token_request, _) =
            AmortizedBatchTokenRequest::<Ristretto255>::new(public_key, &challenge, 1)?;
        service
            .as_issue_tokens(
                user_record.user_id(),
                operation_type,
                token_request,
                Utc::now(),
            )
            .await?;

        let allowance = TokenAllowance::load(&pool, user_record.user_id(), operation_type)
            .await?
            .expect("allowance record missing");
        assert_eq!(
            allowance.remaining, 0,
            "the full allowance was drawn on the legacy path"
        );

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
                Utc::now(),
            )
            .await;

        assert!(matches!(
            token_response,
            Err(IssueTokensError::PrivacyPassError(_))
        ));

        Ok(())
    }
}
