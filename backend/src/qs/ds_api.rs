// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::{
    crypto::hpke::HpkeDecryptable,
    identifiers::{ClientConfig, QsClientId},
    messages::AirProtocolVersion,
    virtual_client::KeyPackageBatchId,
};
use thiserror::Error;
use tls_codec::Serialize;
use tracing::error;

use crate::{
    messages::{
        intra_backend::{DsFanOutMessage, QsVirtualClientHint},
        qs_qs::{QsToQsMessage, QsToQsPayload},
    },
    qs::{errors::EnqueueError, staged_key_package::StagedKeyPackages},
};

use super::{
    PushNotificationProvider, Qs, client_id_decryption_key::StorableClientIdDecryptionKey,
    client_record::QsClientRecord, errors::QsEnqueueError, network_provider::NetworkProvider,
    qs_api::FederatedProcessingResult,
};

impl Qs {
    /// Enqueue the given message. This endpoint is called by the local DS
    /// during a fanout operation. This endpoint does not necessarily return
    /// quickly. It can attempt to do the full fanout and return potential
    /// failed transmissions to the DS.
    ///
    /// This endpoint is used for enqueining messages in both local and remote
    /// queues, depending on the FQDN of the client.
    #[tracing::instrument(skip_all, err)]
    pub async fn enqueue_message<N: NetworkProvider + Send, P: PushNotificationProvider + Send>(
        &self,
        push_notification_provider: &P,
        network_provider: &N,
        message: DsFanOutMessage,
    ) -> Result<(), QsEnqueueError<N>> {
        let own_domain = self.domain.clone();
        if message.client_reference.client_homeserver_domain != own_domain {
            // Federated message
            let qs_to_qs_message = QsToQsMessage {
                protocol_version: AirProtocolVersion::Alpha,
                sender: own_domain.clone(),
                recipient: message.client_reference.client_homeserver_domain.clone(),
                payload: QsToQsPayload::FanOutMessageRequest(message.clone()),
            };
            let serialized_message = qs_to_qs_message
                .tls_serialize_detached()
                .map_err(|_| QsEnqueueError::LibraryError)?;
            network_provider
                .deliver(
                    serialized_message,
                    message.client_reference.client_homeserver_domain,
                )
                .await
                .map_err(QsEnqueueError::NetworkError)
                .and_then(|result| {
                    if matches!(result, FederatedProcessingResult::Ok) {
                        Ok(())
                    } else {
                        Err(QsEnqueueError::InvalidResponse)
                    }
                })?
        } else {
            // Local message
            let decryption_key = StorableClientIdDecryptionKey::load(&self.db_pool)
                .await
                .map_err(|_| QsEnqueueError::StorageError)?
                // There should always be a decryption key in the database.
                .ok_or(QsEnqueueError::LibraryError)?;
            let client_config = ClientConfig::decrypt(
                message.client_reference.sealed_reference,
                &decryption_key,
                &[],
                &[],
            )?;

            // Since we only care about suppression of background push
            // notifications, we can just opt to not to send the push token ear
            // key.
            let push_token_ear_key = if message.suppress_notifications.into() {
                None
            } else {
                client_config.push_token_ear_key
            };

            // When broadcasting, fan out to all of the user's emulator clients.
            // Otherwise, deliver only to the requested clients.
            let client_ids = if message.broadcast_to_all_client_queues.into() {
                QsClientRecord::load_client_ids(&self.db_pool, &client_config.client_id)
                    .await
                    .map_err(|_| QsEnqueueError::StorageError)?
                    .ok_or(EnqueueError::ClientNotFound)?
            } else {
                vec![client_config.client_id]
            };
            for qs_client_id in client_ids {
                match QsClientRecord::enqueue(
                    &self.db_pool,
                    qs_client_id,
                    self.queues(),
                    push_notification_provider,
                    &message.payload,
                    push_token_ear_key.as_ref(),
                )
                .await
                {
                    Ok(()) => (),
                    Err(EnqueueError::ClientNotFound) => {
                        // Sibling was soft-deleted mid fan-out => drop silently
                    }
                    Err(error) => {
                        error!(
                            %error,
                            %qs_client_id, "Failed to enqueue message; message will be lost"
                        );
                    }
                }
            }

            if let Some(QsVirtualClientHint::PromoteStagedKeyPackages {
                epoch_id,
                leaf_index,
                generation,
            }) = message.virtual_client_hint
            {
                self.promote_staged_key_packages(
                    &client_config.client_id,
                    &KeyPackageBatchId {
                        epoch_id,
                        leaf_index,
                        generation,
                    },
                )
                .await
            }
        }
        Ok(())
    }

    async fn promote_staged_key_packages(
        &self,
        client_id: &QsClientId,
        batch_id: &KeyPackageBatchId,
    ) {
        if let Err(error) = self
            .try_promote_staged_key_packages(client_id, batch_id)
            .await
        {
            error!(%error, "Failed to promote staged key packages");
        }
    }

    async fn try_promote_staged_key_packages(
        &self,
        client_id: &QsClientId,
        batch_id: &KeyPackageBatchId,
    ) -> Result<(), PromoteStagedKeyPackagesError> {
        let mut txn = self.db_pool.begin().await?;
        let user_id = QsClientRecord::load_user_id(&mut *txn, client_id)
            .await?
            .ok_or(PromoteStagedKeyPackagesError::ClientNotFound)?;
        StagedKeyPackages::promote(&mut txn, &user_id, batch_id).await?;
        txn.commit().await?;
        Ok(())
    }
}

#[derive(Debug, Error)]
enum PromoteStagedKeyPackagesError {
    #[error(transparent)]
    Sqlx(#[from] sqlx::Error),
    #[error("Client not found")]
    ClientNotFound,
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;

    use aircommon::{
        identifiers::{Fqdn, QsReference},
        messages::{
            QueueMessage,
            client_ds::{QsQueueMessagePayload, QsQueueMessageType},
            push_token::PushToken,
        },
        time::TimeStamp,
    };
    use sqlx::PgPool;
    use tokio_util::sync::CancellationToken;

    use crate::{
        air_service::BackendService,
        messages::intra_backend::DsFanOutPayload,
        qs::{
            PushNotificationError, client_record::persistence::tests::store_random_client_record,
            queue::Queue, user_record::persistence::tests::store_random_user_record,
        },
    };

    use super::*;

    #[derive(Debug)]
    struct NoopPushNotificationProvider;

    impl PushNotificationProvider for NoopPushNotificationProvider {
        async fn push(&self, _push_token: PushToken) -> Result<(), PushNotificationError> {
            Ok(())
        }
    }

    #[derive(Debug)]
    struct UnreachableNetworkProvider;

    impl NetworkProvider for UnreachableNetworkProvider {
        type NetworkError = std::io::Error;

        async fn deliver(
            &self,
            _bytes: Vec<u8>,
            _destination: Fqdn,
        ) -> Result<FederatedProcessingResult, Self::NetworkError> {
            unreachable!()
        }
    }

    #[sqlx::test]
    async fn enqueue_message_fans_out_to_all_active_clients(pool: PgPool) -> anyhow::Result<()> {
        let domain: Fqdn = "example.com".parse()?;
        let qs =
            Qs::initialize(pool.clone(), domain.clone(), None, CancellationToken::new()).await?;

        let user = store_random_user_record(&pool).await?;

        let client_a = store_random_client_record(&pool, user.user_id).await?;
        let client_b = store_random_client_record(&pool, user.user_id).await?;

        let decryption_key = StorableClientIdDecryptionKey::load(&pool)
            .await?
            .expect("missing QS decryption key");
        let sealed_reference =
            decryption_key
                .encryption_key()
                .seal_client_config(ClientConfig {
                    client_id: client_a.client_id,
                    push_token_ear_key: None,
                })?;

        let expected_payload = b"fan-out test";
        let message = DsFanOutMessage {
            payload: DsFanOutPayload::QueueMessage(QsQueueMessagePayload {
                timestamp: TimeStamp::now(),
                message_type: QsQueueMessageType::WelcomeBundle,
                payload: expected_payload.to_vec(),
            }),
            client_reference: QsReference {
                client_homeserver_domain: domain.clone(),
                sealed_reference,
            },
            suppress_notifications: false.into(),
            broadcast_to_all_client_queues: true.into(),
            virtual_client_hint: None,
        };

        qs.enqueue_message(
            &NoopPushNotificationProvider,
            &UnreachableNetworkProvider,
            message,
        )
        .await?;

        for client in [client_a, client_b] {
            let mut buf = VecDeque::new();
            let client_id = client.client_id;
            Queue::fetch_into(&pool, &client_id, 0, 10, &mut buf).await?;
            assert_eq!(buf.len(), 1, "client {client_id} did not receive message");

            let ciphertext: QueueMessage = buf.pop_front().unwrap().try_into().unwrap();
            let payload = client.ratchet_key.clone().decrypt(ciphertext).unwrap();
            assert_eq!(payload.payload, expected_payload);
            assert_eq!(payload.message_type, QsQueueMessageType::WelcomeBundle);
        }

        Ok(())
    }
}
