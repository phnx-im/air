// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::messages::client_qs::{
    CreateClientRecordParams, CreateClientRecordResponse, CreateUserRecordParams,
    CreateUserRecordResponse, DeleteUserRecordParams, UpdateUserRecordParams,
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
        params: CreateUserRecordParams,
    ) -> Result<CreateUserRecordResponse, QsCreateUserError> {
        let CreateUserRecordParams {
            user_record_auth_key,
            friendship_token,
            client_record_auth_key,
            queue_encryption_key,
            encrypted_push_token,
            initial_ratchet_secret,
        } = params;

        let user_record =
            UserRecord::new_and_store(&self.db_pool, user_record_auth_key, friendship_token)
                .await
                .map_err(|error| {
                    tracing::error!(%error, "Error creating and storing new user record");
                    QsCreateUserError::StorageError
                })?;

        let create_client_params = CreateClientRecordParams {
            sender: user_record.user_id,
            client_record_auth_key,
            queue_encryption_key,
            encrypted_push_token,
            initial_ratchet_secret,
        };

        let CreateClientRecordResponse { qs_client_id } = self
            .qs_create_client_record(create_client_params)
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
        params: UpdateUserRecordParams,
    ) -> Result<(), QsUpdateUserError> {
        let UpdateUserRecordParams {
            sender,
            user_record_auth_key,
            friendship_token,
        } = params;

        let user_record = UserRecord {
            user_id: sender,
            verifying_key: user_record_auth_key,
            friendship_token,
        };
        let is_updated = user_record.update(&self.db_pool).await.map_err(|error| {
            tracing::error!(%error, "Error updating user record");
            QsUpdateUserError::StorageError
        })?;
        if !is_updated {
            return Err(QsUpdateUserError::UnknownUser);
        }
        Ok(())
    }

    /// Marks a user record as deleted.
    #[tracing::instrument(skip_all, err)]
    pub(crate) async fn qs_delete_user_record(
        &self,
        params: DeleteUserRecordParams,
    ) -> Result<(), QsDeleteUserError> {
        let DeleteUserRecordParams { sender } = params;

        UserRecord::soft_delete(&self.db_pool, sender)
            .await
            .map_err(|error| {
                tracing::error!(%error, "Error deleting user record");
                QsDeleteUserError::StorageError
            })?;

        Ok(())
    }
}
