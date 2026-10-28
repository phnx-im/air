// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::{
    crypto::hpke::{ClientIdDecryptionKey, HpkeDecryptable},
    identifiers::{ClientConfig, QsClientId},
    messages::AirProtocolVersion,
    virtual_client::KeyPackageBatchId,
};
use sqlx::PgTransaction;
use tls_codec::Serialize;
use tracing::{error, info, warn};

use crate::{
    messages::{
        intra_backend::{DsFanOutMessage, DsFanOutPayload, QsVirtualClientHint},
        qs_qs::{QsToQsMessage, QsToQsPayload},
    },
    qs::{errors::EnqueueError, staged_key_package::StagedKeyPackages},
};

use super::{
    PushNotificationProvider, Qs, client_id_decryption_key::StorableClientIdDecryptionKey,
    client_reconciliation::ReconcileOutcome, client_record::QsClientRecord, errors::QsEnqueueError,
    network_provider::NetworkProvider, qs_api::FederatedProcessingResult,
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
            let serialized_message = qs_to_qs_message.tls_serialize_detached()?;
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
                .await?
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
                QsClientRecord::load_user_client_ids(&self.db_pool, &client_config.client_id)
                    .await?
                    .ok_or(EnqueueError::ClientNotFound)?
            } else {
                vec![client_config.client_id]
            };

            // Clients that should be notified with a push notification.
            let mut clients_to_notify = Vec::new();

            match message.virtual_client_hint {
                Some(hint) => {
                    clients_to_notify = self
                        .enqueue_with_hint(
                            hint,
                            &client_config.client_id,
                            client_ids,
                            &message.payload,
                            &decryption_key,
                        )
                        .await?;
                }
                // The usual fan-out: enqueue message for each client in its own transaction.
                None => {
                    for qs_client_id in client_ids {
                        match self
                            .enqueue_in_own_transaction(qs_client_id, &message.payload)
                            .await
                        {
                            Ok(client_record) => clients_to_notify.extend(client_record),
                            Err(EnqueueError::ClientNotFound) => {
                                // Sibling was soft-deleted mid fan-out => drop silently
                            }
                            Err(error) => {
                                error!(
                                    %error, %qs_client_id,
                                    "Failed to enqueue message; message will be lost"
                                );
                            }
                        }
                    }
                }
            }

            // Send push notifications for clients that should be notified.
            for client_record in clients_to_notify {
                client_record
                    .send_push_notification(
                        &self.db_pool,
                        &self.pseudonymizer,
                        push_notification_provider,
                        push_token_ear_key.as_ref(),
                    )
                    .await;
            }
        }
        Ok(())
    }

    /// Applies `hint` and enqueues the message in one transaction. The hint
    /// takes effect only together with the enqueue.
    async fn enqueue_with_hint(
        &self,
        hint: QsVirtualClientHint,
        sender: &QsClientId,
        client_ids: Vec<QsClientId>,
        payload: &DsFanOutPayload,
        decryption_key: &ClientIdDecryptionKey,
    ) -> Result<Vec<QsClientRecord>, EnqueueError> {
        let mut txn = self.db_pool.begin().await?;

        // We apply the hint before the fan-out. The hint takes the user record lock. This lock
        // comes first in the lock order of the transaction.
        match hint {
            QsVirtualClientHint::PromoteStagedKeyPackages(batch_id) => {
                Self::promote_staged_key_packages(&mut txn, sender, &batch_id).await?;
            }
            QsVirtualClientHint::ReconcileClients(reconciliation) => {
                // We still deliver the echo of a rejected reconciliation. The DS has already
                // accepted the commit. A client that waits for the echo would be stuck without it.
                match self
                    .reconcile_clients(&mut txn, decryption_key, sender, reconciliation)
                    .await?
                {
                    ReconcileOutcome::Reconciled { cleared } => {
                        info!(cleared, "Reconciled client records");
                    }
                    ReconcileOutcome::Rejected(reason) => {
                        warn!(%reason, "Rejected client reconciliation");
                    }
                }
            }
        }

        let mut clients_to_notify = Vec::new();
        for qs_client_id in client_ids {
            match QsClientRecord::enqueue(
                &mut txn,
                qs_client_id,
                self.queues(),
                &self.pseudonymizer,
                payload,
            )
            .await
            {
                Ok(client_record) => clients_to_notify.extend(client_record),
                Err(EnqueueError::ClientNotFound) => {
                    // The sibling was soft-deleted during the fan-out.
                    warn!(%qs_client_id, "Cannot enqueue message: client not found");
                }
                // Any other error aborted the transaction. The hint rolls back with it.
                Err(error) => {
                    error!(
                        %error, %qs_client_id,
                        "Failed to enqueue message; aborting hint and fan-out"
                    );
                    return Err(error);
                }
            }
        }

        txn.commit().await?;
        Ok(clients_to_notify)
    }

    /// Enqueues a message for a single client in its own transaction.
    async fn enqueue_in_own_transaction(
        &self,
        client_id: QsClientId,
        payload: &DsFanOutPayload,
    ) -> Result<Option<QsClientRecord>, EnqueueError> {
        let mut txn = self.db_pool.begin().await?;
        let client_record = QsClientRecord::enqueue(
            &mut txn,
            client_id,
            self.queues(),
            &self.pseudonymizer,
            payload,
        )
        .await?;
        txn.commit().await?;
        Ok(client_record)
    }

    async fn promote_staged_key_packages(
        txn: &mut PgTransaction<'_>,
        client_id: &QsClientId,
        batch_id: &KeyPackageBatchId,
    ) -> Result<(), EnqueueError> {
        let Some(user_id) = QsClientRecord::load_user_id(txn.as_mut(), client_id).await? else {
            // Client was deleted between the DS fan-out and here => nothing to promote. The
            // fan-out below drops the message for the same reason.
            warn!(%client_id, "Cannot promote staged key packages: unknown client");
            return Ok(());
        };
        StagedKeyPackages::promote(txn, &user_id, batch_id).await?;
        Ok(())
    }
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
            PushNotificationError,
            client_reconciliation::tests::{init_qs, reference, signed_reconciliation, store_user},
            client_record::persistence::tests::store_random_client_record,
            queue::{Queue, tests::queue_len},
            user_record::{UserRecord, persistence::tests::store_random_user_record},
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

    fn queue_message_payload(payload: &[u8]) -> DsFanOutPayload {
        DsFanOutPayload::QueueMessage(QsQueueMessagePayload {
            timestamp: TimeStamp::now(),
            message_type: QsQueueMessageType::WelcomeBundle,
            payload: payload.to_vec(),
        })
    }

    async fn fan_out_message(
        pool: &PgPool,
        domain: Fqdn,
        client_id: QsClientId,
        payload: DsFanOutPayload,
        broadcast_to_all_client_queues: bool,
    ) -> anyhow::Result<DsFanOutMessage> {
        let decryption_key = StorableClientIdDecryptionKey::load(pool)
            .await?
            .expect("missing QS decryption key");
        let sealed_reference =
            decryption_key
                .encryption_key()
                .seal_client_config(ClientConfig {
                    client_id,
                    push_token_ear_key: None,
                })?;
        Ok(DsFanOutMessage {
            payload,
            client_reference: QsReference {
                client_homeserver_domain: domain,
                sealed_reference,
            },
            suppress_notifications: false.into(),
            broadcast_to_all_client_queues: broadcast_to_all_client_queues.into(),
            virtual_client_hint: None,
        })
    }

    #[sqlx::test]
    async fn enqueue_message_fans_out_to_all_active_clients(pool: PgPool) -> anyhow::Result<()> {
        let domain: Fqdn = "example.com".parse()?;
        let qs = Qs::initialize(
            pool.clone(),
            domain.clone(),
            Default::default(),
            0,
            CancellationToken::new(),
        )
        .await?;

        let user = store_random_user_record(&pool).await?;

        let client_a = store_random_client_record(&pool, user.user_id).await?;
        let client_b = store_random_client_record(&pool, user.user_id).await?;

        let expected_payload = b"fan-out test";
        let message = fan_out_message(
            &pool,
            domain,
            client_a.client_id,
            queue_message_payload(expected_payload),
            true,
        )
        .await?;

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

    fn commit_response_to(
        client_reference: QsReference,
        hint: QsVirtualClientHint,
    ) -> DsFanOutMessage {
        DsFanOutMessage {
            payload: DsFanOutPayload::QueueMessage(QsQueueMessagePayload {
                timestamp: TimeStamp::now(),
                message_type: QsQueueMessageType::WelcomeBundle,
                payload: b"echo".to_vec(),
            }),
            client_reference,
            suppress_notifications: true.into(),
            broadcast_to_all_client_queues: false.into(),
            virtual_client_hint: Some(hint),
        }
    }

    #[sqlx::test]
    async fn enqueue_message_reconciles_clients_with_the_echo(pool: PgPool) -> anyhow::Result<()> {
        let qs = init_qs(&pool).await?;
        let (user_id, signing_key) = store_user(&pool).await?;
        let sender = store_random_client_record(&pool, user_id).await?;
        let ghost = store_random_client_record(&pool, user_id).await?;

        let client_references = vec![reference(&qs, sender.client_id).await?];
        let reconciliation = signed_reconciliation(&qs, client_references, &signing_key).await?;
        let message = commit_response_to(
            reference(&qs, sender.client_id).await?,
            QsVirtualClientHint::ReconcileClients(reconciliation),
        );
        qs.enqueue_message(
            &NoopPushNotificationProvider,
            &UnreachableNetworkProvider,
            message,
        )
        .await?;

        assert_eq!(queue_len(&pool, sender.client_id).await?, 1);
        assert!(Qs::client_has_push_token(&pool, &sender.client_id).await?);
        assert!(!Qs::client_has_push_token(&pool, &ghost.client_id).await?);
        Ok(())
    }

    #[sqlx::test]
    async fn enqueue_message_delivers_echo_of_rejected_reconciliation(
        pool: PgPool,
    ) -> anyhow::Result<()> {
        let qs = init_qs(&pool).await?;
        let (user_id, _) = store_user(&pool).await?;
        let sender = store_random_client_record(&pool, user_id).await?;
        let ghost = store_random_client_record(&pool, user_id).await?;
        let (_, other_signing_key) = store_user(&pool).await?;

        let client_references = vec![reference(&qs, sender.client_id).await?];
        let reconciliation =
            signed_reconciliation(&qs, client_references, &other_signing_key).await?;
        let message = commit_response_to(
            reference(&qs, sender.client_id).await?,
            QsVirtualClientHint::ReconcileClients(reconciliation),
        );
        qs.enqueue_message(
            &NoopPushNotificationProvider,
            &UnreachableNetworkProvider,
            message,
        )
        .await?;

        assert_eq!(queue_len(&pool, sender.client_id).await?, 1);
        assert!(Qs::client_has_push_token(&pool, &ghost.client_id).await?);
        Ok(())
    }

    #[sqlx::test]
    async fn enqueue_message_skips_deleted_user(pool: PgPool) -> anyhow::Result<()> {
        let domain: Fqdn = "example.com".parse()?;
        let qs = Qs::initialize(
            pool.clone(),
            domain.clone(),
            Default::default(),
            0,
            CancellationToken::new(),
        )
        .await?;

        let user = store_random_user_record(&pool).await?;
        let client = store_random_client_record(&pool, user.user_id).await?;
        UserRecord::soft_delete(&pool, user.user_id).await?;

        let payload = queue_message_payload(b"direct fan-out test");
        let message =
            fan_out_message(&pool, domain, client.client_id, payload.clone(), false).await?;

        let result = qs
            .enqueue_in_own_transaction(client.client_id, &payload)
            .await;
        assert!(matches!(result, Err(EnqueueError::ClientNotFound)));
        qs.enqueue_message(
            &NoopPushNotificationProvider,
            &UnreachableNetworkProvider,
            message,
        )
        .await?;

        assert_eq!(queue_len(&pool, client.client_id).await?, 0);

        Ok(())
    }
}
