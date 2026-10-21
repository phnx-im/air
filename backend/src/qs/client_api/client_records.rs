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
use tracing::error;

use crate::{
    errors::qs::{QsCreateClientRecordError, QsUpdateClientRecordError},
    qs::{Qs, client_record::QsClientRecord, user_record::UserRecord},
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
        let mut txn = self.db_pool.begin().await.map_err(|error| {
            error!(%error, "Error starting transaction");
            QsCreateClientRecordError::StorageError
        })?;
        let is_locked = UserRecord::lock_active(txn.as_mut(), sender)
            .await
            .map_err(|error| {
                error!(%error, "Error locking user record");
                QsCreateClientRecordError::StorageError
            })?;
        if !is_locked {
            return Err(QsCreateClientRecordError::UnknownUser);
        }
        let client_record = QsClientRecord::new_and_store(
            txn.as_mut(),
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
        txn.commit().await.map_err(|error| {
            error!(%error, "Error committing transaction");
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
        UserRecord::lock_active_owner(&mut *transaction, &sender)
            .await
            .map_err(|error| {
                error!(%error, "Error locking user record");
                QsUpdateClientRecordError::StorageError
            })?
            .ok_or(QsUpdateClientRecordError::UnknownClient)?;
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
        params: &DeleteClientRecordParams,
    ) -> Result<(), QsUpdateClientRecordError> {
        let is_deleted = QsClientRecord::soft_delete(&self.db_pool, &params.sender)
            .await
            .map_err(|error| {
                error!(%error, "Error deleting client record");
                QsUpdateClientRecordError::StorageError
            })?;
        if !is_deleted {
            return Err(QsUpdateClientRecordError::UnknownClient);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use aircommon::crypto::{
        RatchetEncryptionKey, kdf::keys::RatchetSecret, signatures::keys::QsClientVerifyingKey,
    };
    use sqlx::PgPool;

    use crate::qs::{
        client_api::tests::qs,
        client_record::persistence::tests::store_random_client_record,
        user_record::persistence::tests::{
            count_rows, run_during_soft_delete, store_random_user_record,
        },
    };

    use super::*;

    #[sqlx::test]
    async fn create_client_waits_for_soft_delete(pool: PgPool) -> anyhow::Result<()> {
        let qs = qs(&pool).await?;
        let user = store_random_user_record(&pool).await?;
        let user_id = user.user_id;

        let created = run_during_soft_delete(&pool, user_id, async move {
            qs.qs_create_client_record(CreateClientRecordParams {
                sender: user_id,
                client_record_auth_key: QsClientVerifyingKey::new_for_test(b"key".to_vec()),
                queue_encryption_key: RatchetEncryptionKey::new_for_test(b"key".to_vec()),
                encrypted_push_token: None,
                initial_ratchet_secret: RatchetSecret::random().unwrap(),
            })
            .await
        })
        .await?;

        assert!(matches!(
            created,
            Err(QsCreateClientRecordError::UnknownUser)
        ));
        assert_eq!(count_rows(&pool, "qs_client_record", user_id).await?, 0);

        Ok(())
    }

    #[sqlx::test]
    async fn delete_client_rejects_deleted_user(pool: PgPool) -> anyhow::Result<()> {
        let qs = qs(&pool).await?;
        let user = store_random_user_record(&pool).await?;
        let client = store_random_client_record(&pool, user.user_id).await?;
        UserRecord::soft_delete(&pool, user.user_id).await?;
        let sender = client.client_id;

        let deleted = qs
            .qs_delete_client_record(&DeleteClientRecordParams { sender })
            .await;

        assert!(matches!(
            deleted,
            Err(QsUpdateClientRecordError::UnknownClient)
        ));
        assert_eq!(
            QsClientRecord::load(&pool, &sender).await?,
            Some(QsClientRecord {
                encrypted_push_token: None,
                ..client
            })
        );

        Ok(())
    }

    #[sqlx::test]
    async fn update_client_waits_for_soft_delete(pool: PgPool) -> anyhow::Result<()> {
        let qs = qs(&pool).await?;
        let user = store_random_user_record(&pool).await?;
        let client = store_random_client_record(&pool, user.user_id).await?;
        let sender = client.client_id;

        let updated = run_during_soft_delete(&pool, user.user_id, async move {
            qs.qs_update_client_record(UpdateClientRecordParams {
                sender,
                client_record_auth_key: QsClientVerifyingKey::new_for_test(b"key".to_vec()),
                queue_encryption_key: RatchetEncryptionKey::new_for_test(b"key".to_vec()),
                encrypted_push_token: None,
            })
            .await
        })
        .await?;

        assert!(matches!(
            updated,
            Err(QsUpdateClientRecordError::UnknownClient)
        ));
        assert_eq!(
            QsClientRecord::load(&pool, &sender).await?,
            Some(QsClientRecord {
                encrypted_push_token: None,
                ..client
            })
        );

        Ok(())
    }
}
