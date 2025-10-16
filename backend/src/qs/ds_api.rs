// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::{
    crypto::hpke::HpkeDecryptable, identifiers::ClientConfig, messages::AirProtocolVersion,
};
use tls_codec::Serialize;

use crate::messages::{
    intra_backend::DsFanOutMessage,
    qs_qs::{QsToQsMessage, QsToQsPayload},
};

use super::{
    PushNotificationProvider, Qs, client_id_decryption_key::StorableClientIdDecryptionKey,
    client_record::QsClientRecord, errors::QsEnqueueError, network_provider::NetworkProvider,
    qs_api::FederatedProcessingResult,
};

impl Qs {
    /// Enqueue the given message. This endpoint is called by the local DS
    /// during a fanout operation. This endpoint does not necessairly return
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

            QsClientRecord::enqueue(
                &self.db_pool,
                client_config.client_id,
                self.queues(),
                push_notification_provider,
                message.payload,
                push_token_ear_key,
            )
            .await?;
        }
        Ok(())
    }
}
