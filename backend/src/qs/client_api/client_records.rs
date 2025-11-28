// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::{
    messages::client_qs::{
        CreateClientRecordParams, CreateClientRecordResponse, DeleteClientRecordParams,
        UpdateClientRecordParams,
    },
    time::TimeStamp,
};
use rand::rngs::OsRng;
use tracing::error;

use crate::{
    errors::qs::{QsCreateClientRecordError, QsUpdateClientRecordError},
    qs::{Qs, client_record::QsClientRecord},
};

impl Qs {
    /// Create a new client record.
    #[tracing::instrument(skip_all, err)]
    pub(crate) async fn qs_create_client_record(
        &self,
        params: CreateClientRecordParams,
    ) -> Result<CreateClientRecordResponse, QsCreateClientRecordError> {
        let CreateClientRecordParams {
            sender,
            client_record_auth_key,
            queue_encryption_key,
            encrypted_push_token,
            initial_ratchet_secret,
        } = params;

        let ratchet_key = initial_ratchet_secret
            .try_into()
            .map_err(|_| QsCreateClientRecordError::LibraryError)?;
        let mut rng = OsRng;
        let mut connection = self.db_pool.acquire().await.map_err(|error| {
            error!(%error, "Error acquiring connection from pool");
            QsCreateClientRecordError::StorageError
        })?;
        let client_record = QsClientRecord::new_and_store(
            &mut connection,
            &mut rng,
            TimeStamp::now(),
            sender,
            encrypted_push_token,
            queue_encryption_key,
            client_record_auth_key,
            ratchet_key,
        )
        .await
        .map_err(|error| {
            error!(%error, "Error creating and storing new client record");
            QsCreateClientRecordError::StorageError
        })?;

        let response = CreateClientRecordResponse {
            qs_client_id: client_record.client_id,
        };

        Ok(response)
    }

    /// Update a client record.
    #[tracing::instrument(skip_all, err)]
    pub(crate) async fn qs_update_client_record(
        &self,
        params: UpdateClientRecordParams,
    ) -> Result<(), QsUpdateClientRecordError> {
        let UpdateClientRecordParams {
            sender,
            client_record_auth_key,
            queue_encryption_key,
            encrypted_push_token,
        } = params;

        let mut transaction = self.db_pool.begin().await.map_err(|error| {
            error!(%error, "Error starting transaction");
            QsUpdateClientRecordError::StorageError
        })?;
        let mut client_record = QsClientRecord::load_for_update(&mut *transaction, &sender)
            .await
            .map_err(|error| {
                error!(%error, "Error loading client record");
                QsUpdateClientRecordError::StorageError
            })?
            .ok_or(QsUpdateClientRecordError::UnknownClient)?;

        client_record.auth_key = client_record_auth_key;
        client_record.queue_encryption_key = queue_encryption_key;
        client_record.encrypted_push_token = encrypted_push_token;

        client_record
            .update(&mut *transaction)
            .await
            .map_err(|error| {
                error!(%error, "Error updating client record");
                QsUpdateClientRecordError::StorageError
            })?;

        transaction.commit().await.map_err(|error| {
            error!(%error, "Error committing transaction");
            QsUpdateClientRecordError::StorageError
        })?;

        Ok(())
    }

    /// Delete a client record.
    #[tracing::instrument(skip_all, err)]
    pub(crate) async fn qs_delete_client_record(
        &self,
        params: DeleteClientRecordParams,
    ) -> Result<(), QsUpdateClientRecordError> {
        QsClientRecord::delete(&self.db_pool, &params.sender)
            .await
            .map_err(|e| {
                error!("Error deleting client record: {:?}", e);
                QsUpdateClientRecordError::StorageError
            })?;

        Ok(())
    }
}
