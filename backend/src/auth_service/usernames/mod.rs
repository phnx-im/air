// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later
use aircommon::{
    credentials::keys::UsernameVerifyingKey,
    identifiers::{
        USERNAME_REFRESH_THRESHOLD, USERNAME_VALIDITY_PERIOD, Username, UsernameHash,
        UsernameHashError, UsernameValidationError,
    },
    time::{Duration, ExpirationData, TimeStamp},
};
use airprotos::auth_service::v1::OperationType;
use displaydoc::Display;
use privacypass::{
    amortized_tokens::{AmortizedBatchTokenRequest, AmortizedBatchTokenResponse, AmortizedToken},
    private_tokens::Ristretto255,
};
use thiserror::Error;
use tokio::task::spawn_blocking;
use tonic::Status;
use tracing::{error, warn};

use crate::errors::auth_service::{IssueTokensError, RedeemTokenError};

use super::AuthService;

pub(crate) use connect::ConnectUsernameProtocol;
pub(crate) use persistence::UsernameRecord;
pub(crate) use queue::UsernameQueues;

mod connect;
mod persistence;
mod queue;

impl AuthService {
    pub(crate) async fn as_check_username_exists(
        &self,
        hash: &UsernameHash,
    ) -> Result<bool, CheckUsernameExistsError> {
        let exists = UsernameRecord::check_exists(&self.db_pool, hash).await?;
        Ok(exists)
    }

    /// Token is optional during gradual rollout: old clients omit it, new
    /// clients provide one. Once all clients support tokens, make it required.
    pub(crate) async fn as_create_username(
        &self,
        verifying_key: UsernameVerifyingKey,
        username_plaintext: String,
        hash: UsernameHash,
        token: Option<AmortizedToken<Ristretto255>>,
    ) -> Result<(), CreateUsernameError> {
        let mut txn = self.db_pool.begin().await?;

        if let Some(token) = token {
            self.as_redeem_token(&mut txn, token, OperationType::AddUsername)
                .await?;
        }

        let username = Username::new(username_plaintext)?;

        let local_hash = spawn_blocking(move || username.calculate_hash()).await??;
        if local_hash != hash {
            return Err(CreateUsernameError::HashMismatch);
        }

        let exists = if let Some(expiration_data) =
            UsernameRecord::load_expiration_data_for_update(txn.as_mut(), &hash).await?
        {
            if expiration_data.validate() {
                return Err(CreateUsernameError::UsernameExists);
            }
            true
        } else {
            false
        };

        let expiration_data = ExpirationData::new(USERNAME_VALIDITY_PERIOD);

        let record = UsernameRecord {
            username_hash: hash,
            verifying_key,
            expiration_data,
        };
        if exists {
            record.update(txn.as_mut()).await?;
        } else if !record.store(txn.as_mut()).await? {
            // Race condition: another process created the username before we did.
            return Err(CreateUsernameError::UsernameExists);
        }

        txn.commit().await?;
        Ok(())
    }

    /// Token refunds are disabled during gradual rollout to prevent token
    /// farming (create-without-token + delete-with-refund = free tokens).
    /// Re-enable when token redemption becomes mandatory for all username ops.
    pub(crate) async fn as_delete_username(
        &self,
        hash: UsernameHash,
        _token_request: Option<AmortizedBatchTokenRequest<Ristretto255>>,
    ) -> Result<Option<AmortizedBatchTokenResponse<Ristretto255>>, DeleteUsernameError> {
        if !UsernameRecord::delete(&self.db_pool, &hash).await? {
            return Err(DeleteUsernameError::UsernameNotFound);
        }

        Ok(None)
    }

    /// Token is optional during gradual rollout (see `as_create_username`).
    pub(crate) async fn as_refresh_username(
        &self,
        hash: UsernameHash,
        token: Option<AmortizedToken<Ristretto255>>,
    ) -> Result<UsernameRefresh, RefreshUsernameError> {
        let mut txn = self.db_pool.begin().await?;

        let Some(expiration_data) =
            UsernameRecord::load_expiration_data_for_update(txn.as_mut(), &hash).await?
        else {
            return Err(RefreshUsernameError::UsernameNotFound);
        };

        if !expiration_data.validate() {
            return Err(RefreshUsernameError::UsernameAlreadyExpired);
        }

        // We only refresh the username and consume a token if it is due
        if !is_refresh_due(&expiration_data) {
            return Ok(UsernameRefresh::Skipped {
                expires_at: expiration_data.not_after(),
            });
        }

        if let Some(token) = token {
            self.as_redeem_token(&mut txn, token, OperationType::AddUsername)
                .await?;
        }

        let new_expiration_data = ExpirationData::new(USERNAME_VALIDITY_PERIOD);
        let expires_at = new_expiration_data.not_after();

        UsernameRecord::update_expiration_data(txn.as_mut(), &hash, new_expiration_data).await?;

        txn.commit().await?;
        Ok(UsernameRefresh::Refreshed { expires_at })
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum UsernameRefresh {
    Refreshed { expires_at: TimeStamp },
    Skipped { expires_at: TimeStamp },
}

/// Leeway on top of the client's refresh threshold.
const REFRESH_LEEWAY: Duration = Duration::weeks(1);

/// Whether a client following [`USERNAME_REFRESH_THRESHOLD`] could have a
/// reason to refresh the username.
fn is_refresh_due(expiration_data: &ExpirationData) -> bool {
    let due_within = USERNAME_VALIDITY_PERIOD - USERNAME_REFRESH_THRESHOLD + REFRESH_LEEWAY;
    *expiration_data.not_after() <= *TimeStamp::now() + due_within
}

#[derive(Debug, Error, Display)]
pub(crate) enum CheckUsernameExistsError {
    /// Storage provider error
    StorageError(#[from] sqlx::Error),
}

impl From<CheckUsernameExistsError> for Status {
    fn from(error: CheckUsernameExistsError) -> Self {
        let msg = error.to_string();
        match error {
            CheckUsernameExistsError::StorageError(error) => {
                error!(%error, "Error checking username existence");
                Status::internal(msg)
            }
        }
    }
}

#[derive(Debug, Error, Display)]
pub(crate) enum CreateUsernameError {
    /// Storage provider error
    StorageError(#[from] sqlx::Error),
    /// Failed to hash the username
    HashError(#[from] UsernameHashError),
    /// Failed to hash the username
    HashTaskError(#[from] tokio::task::JoinError),
    /// Invalid username
    UsernameValidation(#[from] UsernameValidationError),
    /// Hash does not match the hash of the username
    HashMismatch,
    /// Username already exists
    UsernameExists,
    /// Token redemption failed
    TokenRedemption(#[from] RedeemTokenError),
}

impl From<CreateUsernameError> for Status {
    fn from(error: CreateUsernameError) -> Self {
        let msg = error.to_string();
        match error {
            CreateUsernameError::StorageError(error) => {
                error!(%error, "Error creating username");
                Status::internal(msg)
            }
            CreateUsernameError::HashError(error) => {
                error!(%error, "Error creating username");
                Status::internal(msg)
            }
            CreateUsernameError::HashTaskError(error) => {
                error!(%error, "Error creating username");
                Status::internal(msg)
            }
            CreateUsernameError::UsernameValidation(_) => {
                // This is not an error, but shows that a client might be faulty.
                warn!(%error, "Username validation failed");
                Status::invalid_argument(msg)
            }
            CreateUsernameError::HashMismatch => Status::invalid_argument(msg),
            CreateUsernameError::UsernameExists => Status::already_exists(msg),
            CreateUsernameError::TokenRedemption(e) => e.into(),
        }
    }
}

#[derive(Debug, Error, Display)]
pub(crate) enum DeleteUsernameError {
    /// Storage provider error
    StorageError(#[from] sqlx::Error),
    /// Username not found
    UsernameNotFound,
    /// Token issuance failed
    TokenIssuance(#[from] IssueTokensError),
}

impl From<DeleteUsernameError> for Status {
    fn from(error: DeleteUsernameError) -> Self {
        let msg = error.to_string();
        match error {
            DeleteUsernameError::StorageError(error) => {
                error!(%error, "Error deleting username");
                Status::internal(msg)
            }
            DeleteUsernameError::UsernameNotFound => Status::not_found(msg),
            DeleteUsernameError::TokenIssuance(e) => e.into(),
        }
    }
}

#[derive(Debug, Error, Display)]
pub(crate) enum RefreshUsernameError {
    /// Storage provider error
    StorageError(#[from] sqlx::Error),
    /// Username not found
    UsernameNotFound,
    /// Username is already expired
    UsernameAlreadyExpired,
    /// Token redemption failed
    TokenRedemption(#[from] RedeemTokenError),
}

impl From<RefreshUsernameError> for Status {
    fn from(error: RefreshUsernameError) -> Self {
        let msg = error.to_string();
        match error {
            RefreshUsernameError::StorageError(error) => {
                error!(%error, "Error refreshing username");
                Status::internal(msg)
            }
            RefreshUsernameError::UsernameNotFound => Status::not_found(msg),
            RefreshUsernameError::UsernameAlreadyExpired => Status::failed_precondition(msg),
            RefreshUsernameError::TokenRedemption(e) => e.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use aircommon::{
        identifiers::UsernameHash,
        time::{Duration, ExpirationData},
    };
    use airprotos::auth_service::v1::OperationType;
    use chrono::Utc;
    use privacypass::{
        amortized_tokens::{AmortizedBatchTokenRequest, AmortizedToken},
        auth::authenticate::TokenChallenge,
        common::private::{PrivateCipherSuite, PublicKey, deserialize_public_key},
        private_tokens::Ristretto255,
    };
    use sqlx::PgPool;
    use tokio_util::sync::CancellationToken;

    use crate::{
        air_service::BackendService,
        auth_service::{
            AuthService, client_record::persistence::tests::store_random_client_record,
            user_record::persistence::tests::store_random_user_record,
        },
    };

    use super::*;

    async fn setup(pool: &PgPool) -> anyhow::Result<AuthService> {
        Ok(AuthService::initialize(
            pool.clone(),
            "example.com".parse()?,
            Default::default(),
            0,
            CancellationToken::new(),
        )
        .await?)
    }

    fn make_verifying_key() -> UsernameVerifyingKey {
        UsernameVerifyingKey::from_bytes(vec![1, 2, 3, 4, 5])
    }

    const USERNAME: &str = "test-username";

    // Pre-computed hash of "test-username"
    const HASH: UsernameHash = UsernameHash::new([
        222, 173, 3, 115, 219, 79, 226, 238, 144, 239, 100, 203, 87, 27, 122, 68, 108, 137, 203,
        52, 45, 2, 134, 87, 81, 168, 75, 225, 173, 118, 108, 250,
    ]);

    async fn issue_token(
        service: &AuthService,
        pool: &PgPool,
    ) -> anyhow::Result<AmortizedToken<Ristretto255>> {
        let public_keys: HashMap<OperationType, PublicKey<Ristretto255>> =
            crate::auth_service::privacy_pass::load_batched_token_keys(pool)
                .await?
                .into_iter()
                .filter_map(|btr| {
                    let public_key =
                        deserialize_public_key::<Ristretto255>(&btr.public_key).ok()?;
                    Some((btr.operation_type, public_key))
                })
                .collect();

        let public_key = *public_keys.get(&OperationType::AddUsername).unwrap();
        let user_record = store_random_user_record(pool).await?;
        let _client_record =
            store_random_client_record(pool, user_record.user_id().clone()).await?;

        let challenge = TokenChallenge::new(
            Ristretto255::token_type(),
            "example.com",
            None,
            &["example.com".to_string()],
        );

        let (token_request, token_state) =
            AmortizedBatchTokenRequest::<Ristretto255>::new(public_key, &challenge, 1)?;

        let token_response = service
            .as_issue_tokens(
                user_record.user_id(),
                OperationType::AddUsername,
                token_request,
                Utc::now(),
            )
            .await?;

        let mut tokens = token_response.issue_tokens(&token_state)?;
        Ok(tokens.remove(0))
    }

    // as_check_username_exists

    #[sqlx::test]
    async fn check_username_exists_true(pool: PgPool) -> anyhow::Result<()> {
        let service = setup(&pool).await?;

        UsernameRecord {
            username_hash: HASH,
            verifying_key: make_verifying_key(),
            expiration_data: ExpirationData::new(Duration::days(1)),
        }
        .store(&pool)
        .await?;

        assert!(service.as_check_username_exists(&HASH).await?);
        Ok(())
    }

    #[sqlx::test]
    async fn check_username_exists_false(pool: PgPool) -> anyhow::Result<()> {
        let service = setup(&pool).await?;

        assert!(!service.as_check_username_exists(&HASH).await?);
        Ok(())
    }

    // as_create_username

    #[sqlx::test]
    async fn create_username_success(pool: PgPool) -> anyhow::Result<()> {
        let service = setup(&pool).await?;

        service
            .as_create_username(make_verifying_key(), USERNAME.to_owned(), HASH, None)
            .await?;
        Ok(())
    }

    #[sqlx::test]
    async fn create_username_success_with_token(pool: PgPool) -> anyhow::Result<()> {
        let service = setup(&pool).await?;
        let token = issue_token(&service, &pool).await?;

        service
            .as_create_username(make_verifying_key(), USERNAME.to_owned(), HASH, Some(token))
            .await?;
        Ok(())
    }

    #[sqlx::test]
    async fn create_username_token_redemption_fails(pool: PgPool) -> anyhow::Result<()> {
        let service = setup(&pool).await?;
        let token = issue_token(&service, &pool).await?;

        // Spend the token first so it cannot be reused.
        service
            .as_redeem_token(
                pool.acquire().await?.as_mut(),
                token.clone(),
                OperationType::AddUsername,
            )
            .await?;

        let result = service
            .as_create_username(make_verifying_key(), USERNAME.to_owned(), HASH, Some(token))
            .await;
        assert!(matches!(
            result,
            Err(CreateUsernameError::TokenRedemption(_))
        ));
        Ok(())
    }

    #[sqlx::test]
    async fn create_username_invalid_username(pool: PgPool) -> anyhow::Result<()> {
        let service = setup(&pool).await?;

        let result = service
            .as_create_username(
                make_verifying_key(),
                "INVALID_UPPER".to_string(),
                HASH,
                None,
            )
            .await;
        assert!(matches!(
            result,
            Err(CreateUsernameError::UsernameValidation(_))
        ));
        Ok(())
    }

    #[sqlx::test]
    async fn create_username_hash_mismatch(pool: PgPool) -> anyhow::Result<()> {
        let service = setup(&pool).await?;
        let wrong_hash = UsernameHash::new([0; 32]);

        let result = service
            .as_create_username(make_verifying_key(), USERNAME.to_owned(), wrong_hash, None)
            .await;
        assert!(matches!(result, Err(CreateUsernameError::HashMismatch)));
        Ok(())
    }

    #[sqlx::test]
    async fn create_username_already_exists(pool: PgPool) -> anyhow::Result<()> {
        let service = setup(&pool).await?;

        service
            .as_create_username(make_verifying_key(), USERNAME.to_owned(), HASH, None)
            .await?;

        let result = service
            .as_create_username(make_verifying_key(), USERNAME.to_owned(), HASH, None)
            .await;
        assert!(matches!(result, Err(CreateUsernameError::UsernameExists)));
        Ok(())
    }

    #[sqlx::test]
    async fn create_username_concurrent(pool: PgPool) -> anyhow::Result<()> {
        let service = setup(&pool).await?;

        let (r1, r2) = tokio::join!(
            service.as_create_username(make_verifying_key(), USERNAME.to_owned(), HASH, None),
            service.as_create_username(make_verifying_key(), USERNAME.to_owned(), HASH, None),
        );

        let ok_count = [r1.is_ok(), r2.is_ok()].iter().filter(|&&ok| ok).count();
        assert_eq!(ok_count, 1, "Exactly one concurrent create should succeed");

        let err = if r1.is_err() {
            r1.unwrap_err()
        } else {
            r2.unwrap_err()
        };
        assert!(matches!(err, CreateUsernameError::UsernameExists));

        Ok(())
    }

    #[sqlx::test]
    async fn create_username_replaces_expired(pool: PgPool) -> anyhow::Result<()> {
        let service = setup(&pool).await?;

        // Insert an expired record directly.
        UsernameRecord {
            username_hash: HASH,
            verifying_key: make_verifying_key(),
            expiration_data: ExpirationData::new(Duration::zero()),
        }
        .store(&pool)
        .await?;

        service
            .as_create_username(make_verifying_key(), USERNAME.to_owned(), HASH, None)
            .await?;
        Ok(())
    }

    // as_delete_username

    #[sqlx::test]
    async fn delete_username_success(pool: PgPool) -> anyhow::Result<()> {
        let service = setup(&pool).await?;

        service
            .as_create_username(make_verifying_key(), USERNAME.to_owned(), HASH, None)
            .await?;

        let result = service.as_delete_username(HASH, None).await?;
        assert!(result.is_none());
        assert!(!service.as_check_username_exists(&HASH).await?);
        Ok(())
    }

    #[sqlx::test]
    async fn delete_username_not_found(pool: PgPool) -> anyhow::Result<()> {
        let service = setup(&pool).await?;

        let result = service.as_delete_username(HASH, None).await;
        assert!(matches!(result, Err(DeleteUsernameError::UsernameNotFound)));
        Ok(())
    }

    // as_refresh_username

    /// Stores a username that expires in `remaining`.
    async fn store_username_expiring_in(
        pool: &PgPool,
        hash: UsernameHash,
        remaining: Duration,
    ) -> anyhow::Result<()> {
        UsernameRecord {
            username_hash: hash,
            verifying_key: make_verifying_key(),
            expiration_data: ExpirationData::new(remaining),
        }
        .store(pool)
        .await?;
        Ok(())
    }

    async fn expires_at(pool: &PgPool, hash: &UsernameHash) -> anyhow::Result<TimeStamp> {
        let expiration_data = UsernameRecord::load_expiration_data_for_update(pool, hash)
            .await?
            .expect("the username is stored");
        Ok(expiration_data.not_after())
    }

    #[sqlx::test]
    async fn refresh_username_success(pool: PgPool) -> anyhow::Result<()> {
        let service = setup(&pool).await?;
        store_username_expiring_in(&pool, HASH, Duration::days(30)).await?;

        let refresh = service.as_refresh_username(HASH, None).await?;

        let expires_at = expires_at(&pool, &HASH).await?;
        assert_eq!(refresh, UsernameRefresh::Refreshed { expires_at });
        assert!(*expires_at > Utc::now() + Duration::days(179));
        Ok(())
    }

    #[sqlx::test]
    async fn refresh_username_success_with_token(pool: PgPool) -> anyhow::Result<()> {
        let service = setup(&pool).await?;
        store_username_expiring_in(&pool, HASH, Duration::days(30)).await?;

        let token = issue_token(&service, &pool).await?;
        let refresh = service.as_refresh_username(HASH, Some(token)).await?;
        assert!(matches!(refresh, UsernameRefresh::Refreshed { .. }));
        Ok(())
    }

    #[sqlx::test]
    async fn refresh_username_token_redemption_fails(pool: PgPool) -> anyhow::Result<()> {
        let service = setup(&pool).await?;
        store_username_expiring_in(&pool, HASH, Duration::days(30)).await?;
        let expires_before = expires_at(&pool, &HASH).await?;

        // Spend the token first so it cannot be reused.
        let token = issue_token(&service, &pool).await?;
        service
            .as_redeem_token(
                pool.acquire().await?.as_mut(),
                token.clone(),
                OperationType::AddUsername,
            )
            .await?;

        let result = service.as_refresh_username(HASH, Some(token)).await;
        assert!(matches!(
            result,
            Err(RefreshUsernameError::TokenRedemption(_))
        ));
        assert_eq!(expires_at(&pool, &HASH).await?, expires_before);
        Ok(())
    }

    #[sqlx::test]
    async fn a_refresh_that_is_not_due_keeps_the_username_and_the_token(
        pool: PgPool,
    ) -> anyhow::Result<()> {
        let service = setup(&pool).await?;
        service
            .as_create_username(make_verifying_key(), USERNAME.to_owned(), HASH, None)
            .await?;
        let expires_before = expires_at(&pool, &HASH).await?;

        let token = issue_token(&service, &pool).await?;
        let refresh = service
            .as_refresh_username(HASH, Some(token.clone()))
            .await?;

        assert_eq!(
            refresh,
            UsernameRefresh::Skipped {
                expires_at: expires_before
            }
        );
        assert_eq!(expires_at(&pool, &HASH).await?, expires_before);
        // The token was not redeemed, so it is still good.
        service
            .as_redeem_token(
                pool.acquire().await?.as_mut(),
                token,
                OperationType::AddUsername,
            )
            .await?;
        Ok(())
    }

    #[sqlx::test]
    async fn a_refresh_is_due_from_a_week_before_the_client_threshold(
        pool: PgPool,
    ) -> anyhow::Result<()> {
        let service = setup(&pool).await?;
        let early = UsernameHash::new([2; 32]);

        // Refreshed 83 days ago, the first day the leeway allows.
        store_username_expiring_in(&pool, HASH, Duration::days(97)).await?;
        // Refreshed 82 days ago, by a sibling the requesting device missed.
        store_username_expiring_in(&pool, early, Duration::days(98)).await?;

        let refresh = service.as_refresh_username(HASH, None).await?;
        assert!(matches!(refresh, UsernameRefresh::Refreshed { .. }));
        let refresh = service.as_refresh_username(early, None).await?;
        assert!(matches!(refresh, UsernameRefresh::Skipped { .. }));
        Ok(())
    }

    #[sqlx::test]
    async fn concurrent_refreshes_extend_the_username_once(pool: PgPool) -> anyhow::Result<()> {
        let service = setup(&pool).await?;
        store_username_expiring_in(&pool, HASH, Duration::days(30)).await?;

        let (first, second) = tokio::join!(
            service.as_refresh_username(HASH, None),
            service.as_refresh_username(HASH, None),
        );
        let mut refreshes = [first?, second?];
        refreshes.sort_by_key(|refresh| matches!(refresh, UsernameRefresh::Skipped { .. }));

        let expires_at = expires_at(&pool, &HASH).await?;
        assert_eq!(
            refreshes,
            [
                UsernameRefresh::Refreshed { expires_at },
                UsernameRefresh::Skipped { expires_at },
            ]
        );
        Ok(())
    }

    #[sqlx::test]
    async fn refresh_username_not_found(pool: PgPool) -> anyhow::Result<()> {
        let service = setup(&pool).await?;

        let result = service.as_refresh_username(HASH, None).await;
        assert!(matches!(
            result,
            Err(RefreshUsernameError::UsernameNotFound)
        ));
        Ok(())
    }

    #[sqlx::test]
    async fn refresh_username_already_expired(pool: PgPool) -> anyhow::Result<()> {
        let service = setup(&pool).await?;

        UsernameRecord {
            username_hash: HASH,
            verifying_key: make_verifying_key(),
            expiration_data: ExpirationData::new(Duration::zero()),
        }
        .store(&pool)
        .await?;

        let result = service.as_refresh_username(HASH, None).await;
        assert!(matches!(
            result,
            Err(RefreshUsernameError::UsernameAlreadyExpired)
        ));
        Ok(())
    }
}
