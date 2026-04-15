// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later
use aircommon::{
    credentials::keys::UsernameVerifyingKey,
    identifiers::{
        USERNAME_VALIDITY_PERIOD, Username, UsernameHash, UsernameHashError,
        UsernameValidationError,
    },
    time::ExpirationData,
};
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
        if let Some(token) = token {
            self.as_redeem_token(token).await?;
        }

        let username = Username::new(username_plaintext)?;

        let local_hash = spawn_blocking(move || username.calculate_hash()).await??;
        if local_hash != hash {
            return Err(CreateUsernameError::HashMismatch);
        }

        let expiration_data = ExpirationData::new(USERNAME_VALIDITY_PERIOD);

        let record = UsernameRecord {
            username_hash: hash,
            verifying_key,
            expiration_data,
        };

        if record.store(&self.db_pool).await? {
            Ok(())
        } else {
            Err(CreateUsernameError::UsernameExists)
        }
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
    ) -> Result<(), RefreshUsernameError> {
        if let Some(token) = token {
            self.as_redeem_token(token).await?;
        }

        let expiration_data = ExpirationData::new(USERNAME_VALIDITY_PERIOD);
        match UsernameRecord::update_expiration_data(&self.db_pool, &hash, expiration_data).await? {
            UpdateExpirationDataResult::Updated => Ok(()),
            UpdateExpirationDataResult::Deleted => {
                Err(RefreshUsernameError::UsernameAlreadyExpired)
            }
            UpdateExpirationDataResult::NotFound => Err(RefreshUsernameError::UsernameNotFound),
        }
    }
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
