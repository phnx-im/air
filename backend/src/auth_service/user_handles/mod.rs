// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later
use aircommon::{
    credentials::keys::HandleVerifyingKey,
    identifiers::{
        USER_HANDLE_VALIDITY_PERIOD, UserHandle, UserHandleHash, UserHandleHashError,
        UserHandleValidationError,
    },
    time::ExpirationData,
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

pub(crate) use connect::ConnectHandleProtocol;
pub(crate) use persistence::UserHandleRecord;
pub(crate) use queue::UserHandleQueues;

mod connect;
mod persistence;
mod queue;

impl AuthService {
    pub(crate) async fn as_check_handle_exists(
        &self,
        hash: &UserHandleHash,
    ) -> Result<bool, CheckHandleExistsError> {
        let exists = UserHandleRecord::check_exists(&self.db_pool, hash).await?;
        Ok(exists)
    }

    /// Token is optional during gradual rollout: old clients omit it, new
    /// clients provide one. Once all clients support tokens, make it required.
    pub(crate) async fn as_create_handle(
        &self,
        verifying_key: HandleVerifyingKey,
        handle_plaintext: String,
        hash: UserHandleHash,
        token: Option<AmortizedToken<Ristretto255>>,
    ) -> Result<(), CreateHandleError> {
        let mut txn = self.db_pool.begin().await?;

        if let Some(token) = token {
            self.as_redeem_token(&mut txn, token, OperationType::AddUsername)
                .await?;
        }

        let handle = UserHandle::new(handle_plaintext)?;

        let local_hash = spawn_blocking(move || handle.calculate_hash()).await??;
        if local_hash != hash {
            return Err(CreateHandleError::HashMismatch);
        }

        if let Some(expiration_data) =
            UserHandleRecord::load_expiration_data_for_update(txn.as_mut(), &hash).await?
            && expiration_data.validate()
        {
            return Err(CreateHandleError::UserHandleExists);
        }

        let expiration_data = ExpirationData::new(USER_HANDLE_VALIDITY_PERIOD);

        let record = UserHandleRecord {
            user_handle_hash: hash,
            verifying_key,
            expiration_data,
        };
        record.store(txn.as_mut()).await?;

        txn.commit().await?;
        Ok(())
    }

    /// Token refunds are disabled during gradual rollout to prevent token
    /// farming (create-without-token + delete-with-refund = free tokens).
    /// Re-enable when token redemption becomes mandatory for all handle ops.
    pub(crate) async fn as_delete_handle(
        &self,
        hash: UserHandleHash,
        _token_request: Option<AmortizedBatchTokenRequest<Ristretto255>>,
    ) -> Result<Option<AmortizedBatchTokenResponse<Ristretto255>>, DeleteHandleError> {
        if !UserHandleRecord::delete(&self.db_pool, &hash).await? {
            return Err(DeleteHandleError::UserHandleNotFound);
        }

        Ok(None)
    }

    /// Token is optional during gradual rollout (see `as_create_handle`).
    pub(crate) async fn as_refresh_handle(
        &self,
        hash: UserHandleHash,
        token: Option<AmortizedToken<Ristretto255>>,
    ) -> Result<(), RefreshHandleError> {
        let mut txn = self.db_pool.begin().await?;

        if let Some(token) = token {
            self.as_redeem_token(&mut txn, token, OperationType::AddUsername)
                .await?;
        }

        let Some(expiration_data) =
            UserHandleRecord::load_expiration_data_for_update(txn.as_mut(), &hash).await?
        else {
            return Err(RefreshHandleError::HandleNotFound);
        };

        if !expiration_data.validate() {
            return Err(RefreshHandleError::HandleAlreadyExpired);
        }

        let new_expiration_data = ExpirationData::new(USER_HANDLE_VALIDITY_PERIOD);

        UserHandleRecord::update_expiration_data(txn.as_mut(), &hash, new_expiration_data).await?;

        txn.commit().await?;
        Ok(())
    }
}

#[derive(Debug, Error, Display)]
pub(crate) enum CheckHandleExistsError {
    /// Storage provider error
    StorageError(#[from] sqlx::Error),
}

impl From<CheckHandleExistsError> for Status {
    fn from(error: CheckHandleExistsError) -> Self {
        let msg = error.to_string();
        match error {
            CheckHandleExistsError::StorageError(error) => {
                error!(%error, "Error checking user handle existence");
                Status::internal(msg)
            }
        }
    }
}

#[derive(Debug, Error, Display)]
pub(crate) enum CreateHandleError {
    /// Storage provider error
    StorageError(#[from] sqlx::Error),
    /// Failed to hash the user handle
    HashError(#[from] UserHandleHashError),
    /// Failed to hash the user handle
    HashTaskError(#[from] tokio::task::JoinError),
    /// Invalid user handle
    UserHandleValidation(#[from] UserHandleValidationError),
    /// Hash does not match the hash of the user handle
    HashMismatch,
    /// User handle already exists
    UserHandleExists,
    /// Token redemption failed
    TokenRedemption(#[from] RedeemTokenError),
}

impl From<CreateHandleError> for Status {
    fn from(error: CreateHandleError) -> Self {
        let msg = error.to_string();
        match error {
            CreateHandleError::StorageError(error) => {
                error!(%error, "Error creating user handle");
                Status::internal(msg)
            }
            CreateHandleError::HashError(error) => {
                error!(%error, "Error creating user handle");
                Status::internal(msg)
            }
            CreateHandleError::HashTaskError(error) => {
                error!(%error, "Error creating user handle");
                Status::internal(msg)
            }
            CreateHandleError::UserHandleValidation(_) => {
                // This is not an error, but shows that a client might be faulty.
                warn!(%error, "User handle validation failed");
                Status::invalid_argument(msg)
            }
            CreateHandleError::HashMismatch => Status::invalid_argument(msg),
            CreateHandleError::UserHandleExists => Status::already_exists(msg),
            CreateHandleError::TokenRedemption(e) => e.into(),
        }
    }
}

#[derive(Debug, Error, Display)]
pub(crate) enum DeleteHandleError {
    /// Storage provider error
    StorageError(#[from] sqlx::Error),
    /// User handle not found
    UserHandleNotFound,
    /// Token issuance failed
    TokenIssuance(#[from] IssueTokensError),
}

impl From<DeleteHandleError> for Status {
    fn from(error: DeleteHandleError) -> Self {
        let msg = error.to_string();
        match error {
            DeleteHandleError::StorageError(error) => {
                error!(%error, "Error deleting user handle");
                Status::internal(msg)
            }
            DeleteHandleError::UserHandleNotFound => Status::not_found(msg),
            DeleteHandleError::TokenIssuance(e) => e.into(),
        }
    }
}

#[derive(Debug, Error, Display)]
pub(crate) enum RefreshHandleError {
    /// Storage provider error
    StorageError(#[from] sqlx::Error),
    /// User handle not found
    HandleNotFound,
    /// User handle is already expired
    HandleAlreadyExpired,
    /// Token redemption failed
    TokenRedemption(#[from] RedeemTokenError),
}

impl From<RefreshHandleError> for Status {
    fn from(error: RefreshHandleError) -> Self {
        let msg = error.to_string();
        match error {
            RefreshHandleError::StorageError(error) => {
                error!(%error, "Error refreshing user handle");
                Status::internal(msg)
            }
            RefreshHandleError::HandleNotFound => Status::not_found(msg),
            RefreshHandleError::HandleAlreadyExpired => Status::failed_precondition(msg),
            RefreshHandleError::TokenRedemption(e) => e.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use aircommon::{
        credentials::keys::HandleVerifyingKey,
        identifiers::UserHandleHash,
        time::{Duration, ExpirationData},
    };
    use airprotos::auth_service::v1::OperationType;
    use privacypass::{
        amortized_tokens::{AmortizedBatchTokenRequest, AmortizedToken},
        auth::authenticate::TokenChallenge,
        common::private::{PrivateCipherSuite, PublicKey, deserialize_public_key},
        private_tokens::Ristretto255,
    };
    use sqlx::PgPool;

    use crate::{
        air_service::BackendService,
        auth_service::{
            AuthService, client_record::persistence::tests::store_random_client_record,
            user_record::persistence::tests::store_random_user_record,
        },
    };

    use super::*;

    async fn setup(pool: &PgPool) -> anyhow::Result<AuthService> {
        Ok(AuthService::initialize(pool.clone(), "example.com".parse()?, None).await?)
    }

    fn make_verifying_key() -> HandleVerifyingKey {
        HandleVerifyingKey::from_bytes(vec![1, 2, 3, 4, 5])
    }

    const HANDLE: &str = "test-handle";

    // Pre-computed hash of "test-handle"
    const HASH: UserHandleHash = UserHandleHash::new([
        228, 197, 147, 201, 246, 168, 193, 83, 177, 136, 204, 15, 104, 245, 88, 251, 198, 237, 118,
        196, 78, 25, 212, 45, 193, 60, 235, 36, 134, 37, 207, 17,
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
            )
            .await?;

        let mut tokens = token_response.issue_tokens(&token_state)?;
        Ok(tokens.remove(0))
    }

    // as_check_handle_exists

    #[sqlx::test]
    async fn check_handle_exists_true(pool: PgPool) -> anyhow::Result<()> {
        let service = setup(&pool).await?;

        UserHandleRecord {
            user_handle_hash: HASH,
            verifying_key: make_verifying_key(),
            expiration_data: ExpirationData::new(Duration::days(1)),
        }
        .store(&pool)
        .await?;

        assert!(service.as_check_handle_exists(&HASH).await?);
        Ok(())
    }

    #[sqlx::test]
    async fn check_handle_exists_false(pool: PgPool) -> anyhow::Result<()> {
        let service = setup(&pool).await?;

        assert!(!service.as_check_handle_exists(&HASH).await?);
        Ok(())
    }

    // as_create_handle

    #[sqlx::test]
    async fn create_handle_success(pool: PgPool) -> anyhow::Result<()> {
        let service = setup(&pool).await?;

        service
            .as_create_handle(make_verifying_key(), HANDLE.to_owned(), HASH, None)
            .await?;
        Ok(())
    }

    #[sqlx::test]
    async fn create_handle_success_with_token(pool: PgPool) -> anyhow::Result<()> {
        let service = setup(&pool).await?;
        let token = issue_token(&service, &pool).await?;

        service
            .as_create_handle(make_verifying_key(), HANDLE.to_owned(), HASH, Some(token))
            .await?;
        Ok(())
    }

    #[sqlx::test]
    async fn create_handle_token_redemption_fails(pool: PgPool) -> anyhow::Result<()> {
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
            .as_create_handle(make_verifying_key(), HANDLE.to_owned(), HASH, Some(token))
            .await;
        assert!(matches!(result, Err(CreateHandleError::TokenRedemption(_))));
        Ok(())
    }

    #[sqlx::test]
    async fn create_handle_invalid_handle(pool: PgPool) -> anyhow::Result<()> {
        let service = setup(&pool).await?;

        let result = service
            .as_create_handle(
                make_verifying_key(),
                "INVALID_UPPER".to_string(),
                HASH,
                None,
            )
            .await;
        assert!(matches!(
            result,
            Err(CreateHandleError::UserHandleValidation(_))
        ));
        Ok(())
    }

    #[sqlx::test]
    async fn create_handle_hash_mismatch(pool: PgPool) -> anyhow::Result<()> {
        let service = setup(&pool).await?;
        let wrong_hash = UserHandleHash::new([0; 32]);

        let result = service
            .as_create_handle(make_verifying_key(), HANDLE.to_owned(), wrong_hash, None)
            .await;
        assert!(matches!(result, Err(CreateHandleError::HashMismatch)));
        Ok(())
    }

    #[sqlx::test]
    async fn create_handle_already_exists(pool: PgPool) -> anyhow::Result<()> {
        let service = setup(&pool).await?;

        service
            .as_create_handle(make_verifying_key(), HANDLE.to_owned(), HASH, None)
            .await?;

        let result = service
            .as_create_handle(make_verifying_key(), HANDLE.to_owned(), HASH, None)
            .await;
        assert!(matches!(result, Err(CreateHandleError::UserHandleExists)));
        Ok(())
    }

    #[sqlx::test]
    async fn create_handle_replaces_expired(pool: PgPool) -> anyhow::Result<()> {
        let service = setup(&pool).await?;

        // Insert an expired record directly.
        UserHandleRecord {
            user_handle_hash: HASH,
            verifying_key: make_verifying_key(),
            expiration_data: ExpirationData::new(Duration::zero()),
        }
        .store(&pool)
        .await?;

        service
            .as_create_handle(make_verifying_key(), HANDLE.to_owned(), HASH, None)
            .await?;
        Ok(())
    }

    // as_delete_handle

    #[sqlx::test]
    async fn delete_handle_success(pool: PgPool) -> anyhow::Result<()> {
        let service = setup(&pool).await?;

        service
            .as_create_handle(make_verifying_key(), HANDLE.to_owned(), HASH, None)
            .await?;

        let result = service.as_delete_handle(HASH, None).await?;
        assert!(result.is_none());
        assert!(!service.as_check_handle_exists(&HASH).await?);
        Ok(())
    }

    #[sqlx::test]
    async fn delete_handle_not_found(pool: PgPool) -> anyhow::Result<()> {
        let service = setup(&pool).await?;

        let result = service.as_delete_handle(HASH, None).await;
        assert!(matches!(result, Err(DeleteHandleError::UserHandleNotFound)));
        Ok(())
    }

    // as_refresh_handle

    #[sqlx::test]
    async fn refresh_handle_success(pool: PgPool) -> anyhow::Result<()> {
        let service = setup(&pool).await?;

        service
            .as_create_handle(make_verifying_key(), HANDLE.to_owned(), HASH, None)
            .await?;

        service.as_refresh_handle(HASH, None).await?;
        Ok(())
    }

    #[sqlx::test]
    async fn refresh_handle_success_with_token(pool: PgPool) -> anyhow::Result<()> {
        let service = setup(&pool).await?;

        service
            .as_create_handle(make_verifying_key(), HANDLE.to_owned(), HASH, None)
            .await?;

        let token = issue_token(&service, &pool).await?;
        service.as_refresh_handle(HASH, Some(token)).await?;
        Ok(())
    }

    #[sqlx::test]
    async fn refresh_handle_token_redemption_fails(pool: PgPool) -> anyhow::Result<()> {
        let service = setup(&pool).await?;

        service
            .as_create_handle(make_verifying_key(), HANDLE.to_owned(), HASH, None)
            .await?;

        // Spend the token first so it cannot be reused.
        let token = issue_token(&service, &pool).await?;
        service
            .as_redeem_token(
                pool.acquire().await?.as_mut(),
                token.clone(),
                OperationType::AddUsername,
            )
            .await?;

        let result = service.as_refresh_handle(HASH, Some(token)).await;
        assert!(matches!(
            result,
            Err(RefreshHandleError::TokenRedemption(_))
        ));
        Ok(())
    }

    #[sqlx::test]
    async fn refresh_handle_not_found(pool: PgPool) -> anyhow::Result<()> {
        let service = setup(&pool).await?;

        let result = service.as_refresh_handle(HASH, None).await;
        assert!(matches!(result, Err(RefreshHandleError::HandleNotFound)));
        Ok(())
    }

    #[sqlx::test]
    async fn refresh_handle_already_expired(pool: PgPool) -> anyhow::Result<()> {
        let service = setup(&pool).await?;

        UserHandleRecord {
            user_handle_hash: HASH,
            verifying_key: make_verifying_key(),
            expiration_data: ExpirationData::new(Duration::zero()),
        }
        .store(&pool)
        .await?;

        let result = service.as_refresh_handle(HASH, None).await;
        assert!(matches!(
            result,
            Err(RefreshHandleError::HandleAlreadyExpired)
        ));
        Ok(())
    }
}
