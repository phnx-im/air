// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::{
    crypto::{
        RatchetEncryptionKey,
        kdf::keys::RatchetSecret,
        signatures::keys::{QsClientVerifyingKey, QsUserVerifyingKey},
    },
    identifiers::QsUserId,
    messages::{
        FriendshipToken,
        client_qs::{CreateClientRecordResponse, CreateUserRecordResponse},
        push_token::EncryptedPushToken,
    },
};

use crate::{
    errors::qs::{QsCreateUserError, QsDeleteUserError, QsUpdateUserError},
    qs::{Qs, user_record::UserRecord},
};

impl Qs {
    /// Update the info of a given queue. Requires a valid signature by the
    /// owner of the queue.
    #[tracing::instrument(skip_all, err)]
    pub(crate) async fn qs_create_user_record(
        &self,
        user_record_auth_key: QsUserVerifyingKey,
        friendship_token: FriendshipToken,
        client_record_auth_key: QsClientVerifyingKey,
        queue_encryption_key: RatchetEncryptionKey,
        encrypted_push_token: Option<EncryptedPushToken>,
        initial_ratchet_secret: RatchetSecret,
    ) -> Result<CreateUserRecordResponse, QsCreateUserError> {
        let user_record =
            UserRecord::new_and_store(&self.db_pool, user_record_auth_key, friendship_token)
                .await
                .map_err(|e| {
                    tracing::error!("Error creating and storing new user record: {:?}", e);
                    QsCreateUserError::StorageError
                })?;

        let CreateClientRecordResponse { qs_client_id } = self
            .qs_create_client_record(
                user_record.user_id,
                client_record_auth_key,
                queue_encryption_key,
                encrypted_push_token,
                initial_ratchet_secret,
            )
            .await
            .map_err(|_| QsCreateUserError::StorageError)?;

        let response = CreateUserRecordResponse {
            user_id: user_record.user_id,
            qs_client_id,
        };

        Ok(response)
    }

    /// Update a user record.
    #[tracing::instrument(skip_all, err)]
    pub(crate) async fn qs_update_user_record(
        &self,
        sender: QsUserId,
        user_record_auth_key: QsUserVerifyingKey,
        friendship_token: FriendshipToken,
    ) -> Result<(), QsUpdateUserError> {
        let mut transaction = self.db_pool.begin().await.map_err(|e| {
            tracing::error!("Error starting transaction: {:?}", e);
            QsUpdateUserError::StorageError
        })?;
        let mut user_record = UserRecord::load(&mut *transaction, &sender)
            .await
            .map_err(|e| {
                tracing::error!("Error loading user record: {:?}", e);
                QsUpdateUserError::StorageError
            })?
            .ok_or(QsUpdateUserError::UnknownUser)?;

        user_record.friendship_token = friendship_token;
        user_record.verifying_key = user_record_auth_key;

        user_record.update(&mut *transaction).await.map_err(|e| {
            tracing::error!("Error updating user record: {:?}", e);
            QsUpdateUserError::StorageError
        })?;

        transaction.commit().await.map_err(|e| {
            tracing::error!("Error committing transaction: {:?}", e);
            QsUpdateUserError::StorageError
        })?;
        Ok(())
    }

    /// Delete a user record.
    #[tracing::instrument(skip_all, err)]
    pub(crate) async fn qs_delete_user_record(
        &self,
        sender: QsUserId,
    ) -> Result<(), QsDeleteUserError> {
        UserRecord::delete(&self.db_pool, sender)
            .await
            .map_err(|e| {
                tracing::error!("Error deleting user record: {:?}", e);
                QsDeleteUserError::StorageError
            })?;

        Ok(())
    }
}
