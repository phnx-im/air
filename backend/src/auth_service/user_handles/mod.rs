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
use persistence::UpdateExpirationDataResult;
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

        let expiration_data = ExpirationData::new(USER_HANDLE_VALIDITY_PERIOD);

        let record = UserHandleRecord {
            user_handle_hash: hash,
            verifying_key,
            expiration_data,
        };

        if !record.store(&mut txn).await? {
            return Err(CreateHandleError::UserHandleExists);
        }

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

        let expiration_data = ExpirationData::new(USER_HANDLE_VALIDITY_PERIOD);
        match UserHandleRecord::update_expiration_data(&mut txn, &hash, expiration_data).await? {
            UpdateExpirationDataResult::Updated => (),
            UpdateExpirationDataResult::Deleted => {
                return Err(RefreshHandleError::HandleAlreadyExpired);
            }
            UpdateExpirationDataResult::NotFound => return Err(RefreshHandleError::HandleNotFound),
        }

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
