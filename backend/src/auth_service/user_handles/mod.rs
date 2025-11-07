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
use displaydoc::Display;
use persistence::UpdateExpirationDataResult;
use thiserror::Error;
use tokio::task::spawn_blocking;
use tonic::Status;
use tracing::{error, warn};

use super::AuthService;

pub(crate) use connect::ConnectHandleProtocol;
pub(crate) use persistence::UserHandleRecord;
pub(crate) use queue::UserHandleQueues;

mod connect;
mod persistence;
mod queue;

impl AuthService {
    pub(crate) async fn as_create_handle(
        &self,
        verifying_key: HandleVerifyingKey,
        handle_plaintext: String,
        hash: UserHandleHash,
    ) -> Result<(), CreateHandleError> {
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

        if record.store(&self.db_pool).await? {
            Ok(())
        } else {
            Err(CreateHandleError::UserHandleExists)
        }
    }

    pub(crate) async fn as_delete_handle(
        &self,
        hash: UserHandleHash,
    ) -> Result<(), DeleteHandleError> {
        if UserHandleRecord::delete(&self.db_pool, &hash).await? {
            Ok(())
        } else {
            Err(DeleteHandleError::UserHandleNotFound)
        }
    }

    pub(crate) async fn as_refresh_handle(
        &self,
        hash: UserHandleHash,
    ) -> Result<(), RefreshHandleError> {
        let expiration_data = ExpirationData::new(USER_HANDLE_VALIDITY_PERIOD);
        match UserHandleRecord::update_expiration_data(&self.db_pool, &hash, expiration_data)
            .await?
        {
            UpdateExpirationDataResult::Updated => Ok(()),
            UpdateExpirationDataResult::Deleted => Err(RefreshHandleError::HandleAlreadyExpired),
            UpdateExpirationDataResult::NotFound => Err(RefreshHandleError::HandleNotFound),
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
        }
    }
}

#[derive(Debug, Error, Display)]
pub(crate) enum DeleteHandleError {
    /// Storage provider error
    StorageError(#[from] sqlx::Error),
    /// User handle not found
    UserHandleNotFound,
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
        }
    }
}
