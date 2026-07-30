// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use airprotos::{convert::RefInto, queue_service::v1::QueueEventPayload};
use serde::{Deserialize, Serialize};
use sqlx::{PgConnection, PgPool, PgTransaction};
use tls_codec::{TlsDeserializeBytes, TlsSerialize, TlsSize};

use aircommon::{
    crypto::{
        RatchetEncryptionKey, RatchetKeyUpdate,
        aead::{AeadDecryptable, keys::PushTokenEarKey},
        signatures::keys::QsClientVerifyingKey,
    },
    identifiers::{QsClientId, QsUserId},
    messages::{
        QueueMessage,
        client_ds::{DsEventMessage, QsQueueMessagePayload, QsQueueRatchet},
        push_token::{EncryptedPushToken, PushToken},
    },
    time::TimeStamp,
};
use tracing::{error, info, trace, warn};

use crate::{
    errors::StorageError,
    messages::intra_backend::DsFanOutPayload,
    qs::{PushNotificationError, queue::Queues},
};

use super::{PushNotificationProvider, errors::EnqueueError};

/// An enum defining the different kind of messages that are stored in an QS
/// queue.
/// TODO: This needs a codec that allows decoding to the proper type.
#[derive(Serialize, Deserialize, TlsSerialize, TlsDeserializeBytes, TlsSize, Debug)]
#[repr(u8)]
pub(super) enum QueueMessageType {
    #[tls_codec(discriminant = 1)]
    RatchetKeyUpdate(RatchetKeyUpdate),
    EnqueuedMessage(QueueMessage),
}

/// Info attached to a queue meant as a target for messages fanned out by a DS.
#[derive(Debug)]
#[cfg_attr(test, derive(PartialEq, Eq))]
pub(super) struct QsClientRecord<const UPDATABLE: bool = true> {
    pub(super) user_id: QsUserId,
    pub(super) client_id: QsClientId,
    pub(super) encrypted_push_token: Option<EncryptedPushToken>,
    pub(super) queue_encryption_key: RatchetEncryptionKey,
    pub(super) auth_key: QsClientVerifyingKey,
    pub(super) ratchet_key: QsQueueRatchet,
    pub(super) activity_time: TimeStamp,
}

impl QsClientRecord {
    pub(super) async fn new_and_store(
        connection: &mut PgConnection,
        now: TimeStamp,
        user_id: QsUserId,
        encrypted_push_token: Option<EncryptedPushToken>,
        queue_encryption_key: RatchetEncryptionKey,
        auth_key: QsClientVerifyingKey,
        ratchet_key: QsQueueRatchet,
    ) -> Result<Self, StorageError> {
        let client_id = QsClientId::random(&mut rand::rng());
        let record = Self {
            user_id,
            client_id,
            encrypted_push_token,
            queue_encryption_key,
            auth_key,
            ratchet_key,
            activity_time: now,
        };
        record.store(connection).await?;
        Ok(record)
    }
}

pub(crate) mod persistence {
    use aircommon::codec::{BlobDecoded, BlobEncoded};
    use sqlx::{PgExecutor, query};

    use super::*;

    use crate::errors::StorageError;

    impl QsClientRecord {
        pub(crate) async fn store(
            &self,
            connection: impl PgExecutor<'_>,
        ) -> Result<(), StorageError> {
            // Create and store the client record.
            let owner_public_key = BlobEncoded(&self.queue_encryption_key);
            let owner_signature_key = BlobEncoded(&self.auth_key);
            let ratchet = BlobEncoded(&self.ratchet_key);

            query!(
                "INSERT INTO
                    qs_client_record
                    (client_id, user_id, encrypted_push_token, owner_public_key,
                    owner_signature_key, ratchet, activity_time)
                VALUES
                    ($1, $2, $3, $4, $5, $6, $7)",
                &self.client_id as &QsClientId,
                &self.user_id as &QsUserId,
                self.encrypted_push_token.as_ref() as Option<&EncryptedPushToken>,
                owner_public_key as _,
                owner_signature_key as _,
                ratchet as _,
                &self.activity_time as _,
            )
            .execute(connection)
            .await?;

            Ok(())
        }

        #[cfg(test)]
        pub(in crate::qs) async fn load(
            connection: impl PgExecutor<'_>,
            client_id: &QsClientId,
        ) -> Result<Option<QsClientRecord>, StorageError> {
            let client_id = client_id.as_uuid();
            let record = sqlx::query!(
                r#"SELECT
                    user_id as "user_id: QsUserId",
                    encrypted_push_token as "encrypted_push_token: EncryptedPushToken",
                    owner_public_key AS "owner_public_key: BlobDecoded<RatchetEncryptionKey>",
                    owner_signature_key AS "owner_signature_key: BlobDecoded<QsClientVerifyingKey>",
                    ratchet AS "ratchet: BlobDecoded<QsQueueRatchet>",
                    activity_time AS "activity_time: TimeStamp"
                FROM
                    qs_client_record
                WHERE
                    client_id = $1 AND deleted_at IS NULL"#,
                client_id,
            )
            .fetch_optional(connection)
            .await?;
            Ok(record.map(|record| QsClientRecord {
                user_id: record.user_id,
                client_id: (*client_id).into(),
                encrypted_push_token: record.encrypted_push_token,
                queue_encryption_key: record.owner_public_key.into_inner(),
                auth_key: record.owner_signature_key.into_inner(),
                ratchet_key: record.ratchet.into_inner(),
                activity_time: record.activity_time,
            }))
        }

        /// Note: This function must lock the row exclusively with `FOR UPDATE`.
        pub(in crate::qs) async fn load_for_update(
            connection: impl PgExecutor<'_>,
            client_id: &QsClientId,
        ) -> Result<Option<QsClientRecord>, StorageError> {
            let client_id = client_id.as_uuid();
            let record = sqlx::query!(
                r#"SELECT
                    user_id as "user_id: QsUserId",
                    encrypted_push_token as "encrypted_push_token: EncryptedPushToken",
                    owner_public_key AS "owner_public_key: BlobDecoded<RatchetEncryptionKey>",
                    owner_signature_key AS "owner_signature_key: BlobDecoded<QsClientVerifyingKey>",
                    ratchet AS "ratchet: BlobDecoded<QsQueueRatchet>",
                    activity_time AS "activity_time: TimeStamp"
                FROM
                    qs_client_record
                WHERE
                    client_id = $1 AND deleted_at IS NULL
                FOR UPDATE"#,
                client_id,
            )
            .fetch_optional(connection)
            .await?;
            Ok(record.map(|record| QsClientRecord {
                user_id: record.user_id,
                client_id: (*client_id).into(),
                encrypted_push_token: record.encrypted_push_token,
                queue_encryption_key: record.owner_public_key.into_inner(),
                auth_key: record.owner_signature_key.into_inner(),
                ratchet_key: record.ratchet.into_inner(),
                activity_time: record.activity_time,
            }))
        }

        pub(in crate::qs) async fn load_verifying_key(
            connection: impl PgExecutor<'_>,
            client_id: &QsClientId,
        ) -> Result<Option<QsClientVerifyingKey>, StorageError> {
            let client_id = client_id.as_uuid();
            sqlx::query_scalar!(
                r#"SELECT
                    owner_signature_key as "verifying_key: BlobDecoded<QsClientVerifyingKey>"
                FROM
                    qs_client_record
                WHERE
                    client_id = $1
                    AND deleted_at IS NULL"#,
                client_id,
            )
            .fetch_optional(connection)
            .await
            .map(|key| key.map(|BlobDecoded(verifying_key)| verifying_key))
            .map_err(From::from)
        }

        pub(in crate::qs) async fn load_user_id(
            connection: impl PgExecutor<'_>,
            client_id: &QsClientId,
        ) -> sqlx::Result<Option<QsUserId>> {
            let client_id = client_id.as_uuid();
            sqlx::query_scalar!(
                r#"SELECT
                    user_id as "user_id: QsUserId"
                FROM
                    qs_client_record
                WHERE
                    client_id = $1
                    AND deleted_at IS NULL"#,
                client_id,
            )
            .fetch_optional(connection)
            .await
        }

        pub(in crate::qs) async fn update_activity_time(
            connection: impl PgExecutor<'_>,
            client_id: QsClientId,
            activity_time: TimeStamp,
        ) -> sqlx::Result<()> {
            query!(
                "UPDATE qs_client_record SET activity_time = $1
                WHERE client_id = $2 AND deleted_at IS NULL",
                &activity_time as &TimeStamp,
                client_id as _,
            )
            .execute(connection)
            .await?;
            Ok(())
        }

        pub(in crate::qs) async fn soft_delete(
            connection: impl PgExecutor<'_>,
            client_id: &QsClientId,
        ) -> Result<(), StorageError> {
            // Delete the client record to trigger cascading deletes, and reinsert a tombstone.
            query!(
                r#"WITH deleted_client_record AS (
                    DELETE FROM qs_client_record
                    WHERE client_id = $1
                    RETURNING client_id, user_id
                )
                INSERT INTO qs_client_record (
                    client_id,
                    user_id,
                    deleted_at,
                    activity_time,
                    encrypted_push_token,
                    owner_public_key,
                    owner_signature_key,
                    ratchet
                )
                SELECT
                    client_id,
                    user_id,
                    NOW(),
                    NOW(),
                    NULL,
                    '\x',
                    '\x',
                    '\x'
                FROM deleted_client_record;
                "#,
                client_id as &QsClientId
            )
            .execute(connection)
            .await?;
            Ok(())
        }

        /// Returns the active client ids for the user owning the given client id.
        ///
        /// - `Ok(None)` if no row with `client_id` exists.
        /// - `Ok(Some(vec![]))` if the row exists but no active client ids (e.g. tombstoned
        ///   representative with no remaining devices).
        /// - `Ok(Some(vec![...]))` otherwise. The given client id is included if it is itself
        ///   active.
        ///
        /// The client ids are ordered ascending. Callers lock the client records in that order
        /// (see `load_for_update`), which keeps the lock order consistent with the staged key
        /// package promote and avoids deadlocks between the two.
        pub(in crate::qs) async fn load_client_ids(
            connection: impl PgExecutor<'_>,
            client_id: &QsClientId,
        ) -> Result<Option<Vec<QsClientId>>, StorageError> {
            let rows: Vec<Option<QsClientId>> = sqlx::query_scalar!(
                r#"SELECT other.client_id AS "client_id?: QsClientId"
                FROM qs_client_record c
                LEFT JOIN qs_client_record AS other ON
                    other.user_id = c.user_id
                    AND other.deleted_at IS NULL
                WHERE c.client_id = $1
                ORDER BY other.client_id"#,
                client_id.as_uuid(),
            )
            .fetch_all(connection)
            .await?;
            if rows.is_empty() {
                // anchor row does not exist
                Ok(None)
            } else {
                // anchor exists
                Ok(Some(rows.into_iter().flatten().collect()))
            }
        }

        /// Deletes token from client's database record if it still set.
        pub(in crate::qs) async fn delete_push_token(
            &self,
            executor: impl PgExecutor<'_>,
        ) -> sqlx::Result<()> {
            if let Some(encrypted_push_token) = self.encrypted_push_token.as_ref() {
                query!(
                    "UPDATE qs_client_record
                    SET encrypted_push_token = NULL
                    WHERE client_id = $1
                        AND encrypted_push_token = $2
                        AND deleted_at IS NULL",
                    self.client_id as _,
                    encrypted_push_token as _,
                )
                .execute(executor)
                .await?;
            }
            Ok(())
        }
    }

    impl QsClientRecord<true> {
        pub(in crate::qs) async fn update(
            &self,
            connection: impl PgExecutor<'_>,
        ) -> Result<(), StorageError> {
            let owner_public_key = BlobEncoded(&self.queue_encryption_key);
            let owner_signature_key = BlobEncoded(&self.auth_key);
            let ratchet = BlobEncoded(&self.ratchet_key);

            query!(
                "UPDATE qs_client_record
                SET
                    encrypted_push_token = $1,
                    owner_public_key = $2,
                    owner_signature_key = $3,
                    ratchet = $4,
                    activity_time = $5
                WHERE
                    client_id = $6
                    AND deleted_at IS NULL",
                self.encrypted_push_token.as_ref() as Option<&EncryptedPushToken>,
                owner_public_key as _,
                owner_signature_key as _,
                ratchet as _,
                &self.activity_time as &TimeStamp,
                &self.client_id as &QsClientId,
            )
            .execute(connection)
            .await?;

            Ok(())
        }

        pub(crate) async fn update_queue_ratchet(
            &self,
            connection: impl PgExecutor<'_>,
        ) -> Result<(), StorageError> {
            let ratchet = BlobEncoded(&self.ratchet_key);
            query!(
                "UPDATE qs_client_record
                SET ratchet = $1
                WHERE client_id = $2
                AND deleted_at IS NULL",
                ratchet as _,
                &self.client_id as &QsClientId,
            )
            .execute(connection)
            .await?;
            Ok(())
        }
    }

    #[cfg(test)]
    pub(crate) mod tests {
        use aircommon::crypto::ratchet::QueueRatchet;
        use sqlx::PgPool;

        use crate::qs::user_record::persistence::tests::store_random_user_record;

        use super::*;

        fn random_client_record(user_id: QsUserId) -> QsClientRecord {
            QsClientRecord {
                user_id,
                client_id: QsClientId::random(&mut rand::rng()),
                encrypted_push_token: Some(EncryptedPushToken::dummy()),
                queue_encryption_key: RatchetEncryptionKey::new_for_test(
                    b"encryption_key_32_bytes".to_vec(),
                ),
                auth_key: QsClientVerifyingKey::new_for_test(b"auth_key".to_vec()),
                ratchet_key: QueueRatchet::random().unwrap(),
                activity_time: TimeStamp::now(),
            }
        }

        pub(crate) async fn store_random_client_record(
            pool: &PgPool,
            user_id: QsUserId,
        ) -> anyhow::Result<QsClientRecord> {
            let record = random_client_record(user_id);
            record.store(pool).await?;
            Ok(record)
        }

        #[sqlx::test]
        async fn store(pool: PgPool) -> anyhow::Result<()> {
            let user_record = store_random_user_record(&pool).await?;
            let client_record = store_random_client_record(&pool, user_record.user_id).await?;

            let loaded = QsClientRecord::load(&pool, &client_record.client_id)
                .await?
                .expect("missing client record");
            assert_eq!(loaded, client_record);

            let verifying_key = QsClientRecord::load_verifying_key(&pool, &client_record.client_id)
                .await?
                .expect("missing client verifying key");
            assert_eq!(verifying_key, client_record.auth_key);

            Ok(())
        }

        #[sqlx::test]
        async fn update(pool: PgPool) -> anyhow::Result<()> {
            let user_record = store_random_user_record(&pool).await?;
            let client_record = store_random_client_record(&pool, user_record.user_id).await?;

            let loaded = QsClientRecord::load_for_update(&pool, &client_record.client_id)
                .await?
                .expect("missing client record");
            assert_eq!(loaded, client_record);

            let updated_client_record = QsClientRecord {
                user_id: client_record.user_id,
                client_id: client_record.client_id,
                ..random_client_record(user_record.user_id)
            };

            updated_client_record.update(&pool).await?;
            let loaded = QsClientRecord::load(&pool, &client_record.client_id)
                .await?
                .expect("missing client record");
            assert_eq!(loaded, updated_client_record);

            Ok(())
        }

        #[sqlx::test]
        async fn soft_delete(pool: PgPool) -> anyhow::Result<()> {
            let user_record = store_random_user_record(&pool).await?;
            let client_record = store_random_client_record(&pool, user_record.user_id).await?;

            let loaded = QsClientRecord::load(&pool, &client_record.client_id)
                .await?
                .expect("missing client record");
            assert_eq!(loaded, client_record);

            QsClientRecord::soft_delete(&pool, &client_record.client_id).await?;

            let loaded = QsClientRecord::load(&pool, &client_record.client_id).await?;
            assert_eq!(loaded, None);

            let loaded = QsClientRecord::load_for_update(&pool, &client_record.client_id).await?;
            assert_eq!(loaded, None);

            let loaded =
                QsClientRecord::load_verifying_key(&pool, &client_record.client_id).await?;
            assert_eq!(loaded, None);

            let loaded = QsClientRecord::load_client_ids(&pool, &client_record.client_id).await?;
            assert_eq!(loaded, Some(vec![]));

            let tombstone_user_id: QsUserId = sqlx::query_scalar(
                "SELECT user_id FROM qs_client_record
                WHERE client_id = $1 AND deleted_at IS NOT NULL",
            )
            .bind(client_record.client_id)
            .fetch_one(&pool)
            .await?;
            assert_eq!(tombstone_user_id, user_record.user_id);

            Ok(())
        }

        #[sqlx::test]
        async fn delete_push_token(pool: PgPool) -> anyhow::Result<()> {
            let user_record = store_random_user_record(&pool).await?;
            let mut client_record = store_random_client_record(&pool, user_record.user_id).await?;

            let loaded = QsClientRecord::load(&pool, &client_record.client_id)
                .await?
                .expect("missing client record");
            assert_eq!(loaded, client_record);

            // push token is deleted
            client_record.delete_push_token(&pool).await?;
            let loaded = QsClientRecord::load_for_update(&pool, &client_record.client_id)
                .await?
                .expect("missing client record");
            assert_eq!(loaded.encrypted_push_token, None);

            // push token is not deleted because it is a different one
            let push_token = EncryptedPushToken::random();
            client_record.encrypted_push_token = Some(push_token.clone());
            client_record.update(&pool).await?;

            let loaded = QsClientRecord::load(&pool, &client_record.client_id)
                .await?
                .expect("missing client record");
            assert_eq!(loaded, client_record);

            client_record.encrypted_push_token = Some(EncryptedPushToken::random());
            client_record.delete_push_token(&pool).await?;
            let loaded = QsClientRecord::load(&pool, &client_record.client_id)
                .await?
                .expect("missing client record");
            assert_eq!(loaded.encrypted_push_token, Some(push_token.clone()));

            // push token is not deleted because it is not set
            client_record.encrypted_push_token = None;
            client_record.delete_push_token(&pool).await?;
            let loaded = QsClientRecord::load(&pool, &client_record.client_id)
                .await?
                .expect("missing client record");
            assert_eq!(loaded.encrypted_push_token, Some(push_token));

            Ok(())
        }
    }
}

impl QsClientRecord {
    /// Put a message into the queue.
    ///
    /// Returns a QS client record if a push notification should be sent to the client, otherwise
    /// `None`.
    pub(crate) async fn enqueue(
        txn: &mut PgTransaction<'_>,
        client_id: QsClientId,
        queues: &Queues,
        msg: &DsFanOutPayload,
    ) -> Result<Option<QsClientRecord>, EnqueueError> {
        match msg {
            // Enqueue a queue message.
            // Serialize the message so that we can put it in the queue.
            DsFanOutPayload::QueueMessage(queue_message) => {
                let (client_record, has_listener) =
                    Self::do_enqueue(txn, client_id, queues, queue_message).await?;
                // Only return the client record if the client does *not* have a listener.
                Ok((!has_listener).then_some(client_record))
            }
            // Dispatch an event message.
            DsFanOutPayload::EventMessage(DsEventMessage {
                group_id,
                sender_index,
                epoch,
                timestamp,
                payload,
            }) => {
                let payload = QueueEventPayload {
                    group_id: Some(group_id.ref_into()),
                    sender: Some((*sender_index).into()),
                    epoch: Some((*epoch).into()),
                    timestamp: Some((*timestamp).into()),
                    payload: payload.clone(),
                };
                queues.send_payload(client_id, payload).await?;
                Ok(None)
            }
        }
    }

    /// Tries to send a push notification to the client.
    ///
    /// If the token is invalid, the push notification token stored for the client is deleted.
    ///
    /// The operation is infallible, all errors are handled inside and logged.
    pub(crate) async fn send_push_notification(
        &self,
        pool: &PgPool,
        push_notification_provider: &impl PushNotificationProvider,
        push_token_key: Option<&PushTokenEarKey>,
    ) {
        trace!("Trying to send push notification");

        // Send a push notification under the following conditions:
        // - there is a push token associated with the queue
        // - there is a push token decryption key
        // - the decryption is successful

        let Some(encrypted_push_token) = self.encrypted_push_token.as_ref() else {
            return;
        };
        let Some(ear_key) = push_token_key else {
            return;
        };

        // Attempt to decrypt the push token.
        let push_token = match PushToken::decrypt(ear_key, encrypted_push_token) {
            Ok(push_token) => push_token,
            Err(error) => {
                error!(%error, "Push token decryption failed");
                return;
            }
        };

        trace!("Send push notification");
        if let Err(error) = push_notification_provider.push(push_token).await {
            match error {
                PushNotificationError::NetworkError(_) | PushNotificationError::InvalidToken(_) => {
                    info!(%error, "Failed to send push notification")
                }
                PushNotificationError::UnsupportedType => {
                    warn!(%error, "Failed to send push notification")
                }
                _ => {
                    error!(%error, "Failed to send push notification")
                }
            };
            if let PushNotificationError::InvalidToken(_) = error {
                self.delete_push_token(pool)
                    .await
                    .inspect_err(|error| {
                        error!(%error, "Failed to delete push token");
                    })
                    .ok();
            }
        }
    }

    async fn do_enqueue(
        txn: &mut PgTransaction<'_>,
        client_id: QsClientId,
        queues: &Queues,
        queue_message: &QsQueueMessagePayload,
    ) -> Result<(QsClientRecord, bool), EnqueueError> {
        let mut client_record = Self::load_for_update(txn.as_mut(), &client_id)
            .await?
            .ok_or(EnqueueError::ClientNotFound)?;

        let queue_message = client_record.ratchet_key.encrypt(queue_message)?;
        let queue_message_proto: airprotos::queue_service::v1::QueueMessage = queue_message.into();
        trace!("Enqueueing message in storage provider");

        let has_listener = queues
            .enqueue(&mut *txn, client_id, &queue_message_proto)
            .await?;

        client_record.update_queue_ratchet(txn.as_mut()).await?;

        Ok((client_record, has_listener))
    }
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;

    use aircommon::messages::client_ds::QsQueueMessageType;
    use anyhow::Context;
    use tokio::task::JoinSet;
    use tokio_util::sync::CancellationToken;

    use crate::qs::{
        client_record::persistence::tests::store_random_client_record,
        queue::{Queue, Queues},
        user_record::persistence::tests::store_random_user_record,
    };

    use super::*;

    #[sqlx::test]
    async fn no_race_in_enqueue(pool: PgPool) -> anyhow::Result<()> {
        let user_record = store_random_user_record(&pool).await?;
        let client_record = store_random_client_record(&pool, user_record.user_id).await?;

        let queue_notifier = Queues::new(pool.clone(), CancellationToken::new()).await?;

        let mut join_set = JoinSet::new();

        const N: u64 = 42;

        for _ in 0..N {
            let queue_message_payload = QsQueueMessagePayload {
                timestamp: TimeStamp::now(),
                message_type: QsQueueMessageType::WelcomeBundle,
                payload: vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9],
            };
            let pool = pool.clone();
            let queue_notifier = queue_notifier.clone();
            join_set.spawn(async move {
                let mut txn = pool.begin().await?;
                QsClientRecord::do_enqueue(
                    &mut txn,
                    client_record.client_id,
                    &queue_notifier,
                    &queue_message_payload,
                )
                .await?;
                txn.commit().await?;
                Ok::<(), anyhow::Error>(())
            });
        }

        let mut has_error = false;
        while let Some(result) = join_set.join_next().await {
            if let Err(error) = result? {
                error!(%error, "Error enqueuing message");
                has_error = true;
            }
        }

        let mut queue_messages = VecDeque::new();
        Queue::fetch_into(
            &pool,
            &client_record.client_id,
            0,
            N as usize,
            &mut queue_messages,
        )
        .await?;

        let client_record = QsClientRecord::load(&pool, &client_record.client_id)
            .await?
            .context("no client record")?;
        assert_eq!(client_record.ratchet_key.sequence_number(), N);

        let sequences_numbers = queue_messages
            .into_iter()
            .map(|m| m.sequence_number)
            .collect::<Vec<_>>();
        assert_eq!(sequences_numbers, (0..N).collect::<Vec<_>>());

        assert!(!has_error);

        Ok(())
    }

    #[sqlx::test]
    async fn load_client_ids(pool: PgPool) -> anyhow::Result<()> {
        use std::collections::HashSet;

        let user = store_random_user_record(&pool).await?;
        let other_user = store_random_user_record(&pool).await?;

        // Case 1: unknown anchor => Ok(None)
        let unknown = QsClientId::random(&mut rand::rng());
        assert_eq!(
            QsClientRecord::load_client_ids(&pool, &unknown).await?,
            None
        );

        let a = store_random_client_record(&pool, user.user_id).await?;
        let b = store_random_client_record(&pool, user.user_id).await?;
        // unrelated client: must NOT leak into results
        let _other = store_random_client_record(&pool, other_user.user_id).await?;

        // Case 3a: active anchor => all active client ids of the same user, including itself
        let result = QsClientRecord::load_client_ids(&pool, &a.client_id).await?;
        let got: HashSet<_> = result.expect("anchor missing").into_iter().collect();
        let want: HashSet<_> = [a.client_id, b.client_id].into_iter().collect();
        assert_eq!(got, want);

        // Case 3b: tombstone the queried client; anchor still resolves, returns only the live sibling
        QsClientRecord::soft_delete(&pool, &a.client_id).await?;
        assert_eq!(
            QsClientRecord::load_client_ids(&pool, &a.client_id).await?,
            Some(vec![b.client_id]),
        );

        // Case 2: tombstone the remaining sibling → anchor present, zero actives
        QsClientRecord::soft_delete(&pool, &b.client_id).await?;
        assert_eq!(
            QsClientRecord::load_client_ids(&pool, &a.client_id).await?,
            Some(vec![]),
        );

        Ok(())
    }
}
