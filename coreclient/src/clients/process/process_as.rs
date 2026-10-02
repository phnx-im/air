// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::{
    credentials::UserCredential,
    crypto::hpke::HpkeDecryptable,
    identifiers::Username,
    messages::{
        client_as::{ConnectionOfferHash, ConnectionOfferMessage},
        connection_package::ConnectionPackageHash,
    },
    time::TimeStamp,
};
use airprotos::auth_service::v1::{UsernameQueueMessage, username_queue_message};
use anyhow::{Context, Result, anyhow, bail};
use chrono::Utc;
use tracing::error;

use crate::{
    chats::connection_requests::{self, IncomingRequest, IncomingRequestSource, StoredRequest},
    clients::{
        api_clients::ApiClients,
        block_contact::{BlockedContact, BlockedContactError},
        connection_offer::{
            ConnectionOfferIn,
            payload::{ConnectionInfo, ConnectionOfferPayload},
        },
    },
    db::access::WriteConnection,
    groups::client_auth_info::StorableUserCredential,
    job::{JobContext, JobContextDb},
    usernames::connection_packages::ConnectionPackageRecord,
};

use super::{AsCredentials, Chat, ChatId, CoreUser};

pub(crate) enum ConnectionInfoSource {
    ConnectionOffer(Box<ConnectionOfferSource>),
    TargetedMessage(Box<TargetedMessageSource>),
}

pub(crate) struct ConnectionOfferSource {
    pub(crate) connection_offer: ConnectionOfferMessage,
    pub(crate) username: Username,
    /// Timestamp when the connection offer was enqueued on the server
    pub(crate) sent_at: Option<TimeStamp>,
}

pub(crate) struct TargetedMessageSource {
    pub(crate) connection_info: ConnectionInfo,
    pub(crate) sender_user_credential: UserCredential,
    pub(crate) origin_chat_id: ChatId,
    /// Timestamp when the targeted message was enqueued on the QS
    pub(crate) sent_at: TimeStamp,
}

struct UsernameConnectionInfo {
    connection_offer_hash: ConnectionOfferHash,
    connection_package_hash: ConnectionPackageHash,
    username: Username,
}

impl ConnectionInfoSource {
    async fn into_parts(
        self,
        connection: impl WriteConnection,
        api_clients: &ApiClients,
    ) -> Result<(
        ConnectionInfo,
        UserCredential,
        Option<ChatId>,
        Option<UsernameConnectionInfo>,
        Option<TimeStamp>,
    )> {
        match self {
            ConnectionInfoSource::ConnectionOffer(connection_offer_source) => {
                let ConnectionOfferSource {
                    connection_offer,
                    username,
                    sent_at,
                } = *connection_offer_source;
                let connection_offer_hash = connection_offer.connection_offer_hash();
                let (cep_payload, hash) = CoreUser::parse_and_verify_connection_offer(
                    connection,
                    api_clients,
                    connection_offer,
                    username.clone(),
                )
                .await?;
                let sender_user_credential = cep_payload.sender_user_credential;
                let username_connection_info = UsernameConnectionInfo {
                    connection_offer_hash,
                    connection_package_hash: hash,
                    username,
                };
                Ok((
                    cep_payload.connection_info,
                    sender_user_credential,
                    None,
                    Some(username_connection_info),
                    sent_at,
                ))
            }
            ConnectionInfoSource::TargetedMessage(targeted_message_source) => {
                let TargetedMessageSource {
                    connection_info,
                    sender_user_credential,
                    origin_chat_id,
                    sent_at,
                } = *targeted_message_source;
                Ok((
                    connection_info,
                    sender_user_credential,
                    Some(origin_chat_id),
                    None,
                    Some(sent_at),
                ))
            }
        }
    }
}

impl CoreUser {
    pub(crate) async fn process_username_queue_message_event_loop(
        &self,
        username: Username,
        queue_message: UsernameQueueMessage,
    ) -> Result<Option<StoredRequest>> {
        let payload = queue_message
            .payload
            .context("no payload in username queue message")?;

        // Extract the server timestamp from the message
        let sent_at = queue_message.created_at.map(TimeStamp::from);

        match payload {
            username_queue_message::Payload::ConnectionOffer(eco) => {
                let connection_info_source =
                    ConnectionInfoSource::ConnectionOffer(Box::new(ConnectionOfferSource {
                        connection_offer: eco.try_into()?,
                        username: username.clone(),
                        sent_at,
                    }));
                let mut context = JobContext {
                    api_clients: &self.inner.api_clients,
                    http_client: &self.inner.http_client,
                    db: JobContextDb::Db(self.inner.db.clone()),
                    key_store: &self.inner.key_store,
                    now: Utc::now(),
                    qs_client_id: &self.inner.qs_client_id,
                };
                let stored =
                    Self::process_connection_offer(&mut context, connection_info_source).await?;
                if stored.is_some() {
                    // Hands the request to the siblings.
                    self.outbound_service().notify_pending_chat_operations();
                }
                Ok(stored)
            }
        }
    }

    /// Stores an incoming connection request as pending. A request via a
    /// username is also parked for the siblings.
    ///
    /// Returns where it was stored, or `None` if the request is known already.
    pub(crate) async fn process_connection_offer(
        context: &mut JobContext<'_, '_>,
        connection_info_source: ConnectionInfoSource,
    ) -> anyhow::Result<Option<StoredRequest>> {
        let api_clients = context.api_clients.clone();
        let (
            connection_info,
            sender_user_credential,
            origin_chat_id,
            username_connection_info,
            sent_at,
        ) = connection_info_source
            .into_parts(context.db.write().await?, &api_clients)
            .await?;

        // Use the server's timestamp if available, otherwise fall back to current time
        let message_timestamp = sent_at.unwrap_or_else(TimeStamp::now);

        // Deny connection from blocked users
        if BlockedContact::check_blocked(context.db.read().await?, sender_user_credential.user_id())
            .await?
        {
            bail!(BlockedContactError);
        }

        // Idempotency: the request id is deterministic from the group id, so
        // a duplicate offer is recognized. It may be shown in the chat of a
        // newer request of the same sender.
        let request_id = ChatId::try_from(&connection_info.connection_group_id)?;
        let is_known = {
            let mut connection = context.db.read().await?;
            let txn = connection.begin().await?;
            connection_requests::is_known(txn, request_id).await?
        };
        if is_known {
            return Ok(None);
        }

        CoreUser::fetch_request_sender_profile(context, &connection_info, &sender_user_credential)
            .await?;

        let sender = sender_user_credential.user_id().clone();
        context
            .db
            .write()
            .await?
            .with_transaction(async |txn| {
                // Only this device got a username offer from the AS queue. A
                // targeted message reaches every sibling on its own.
                let forward = username_connection_info.is_some();
                let source = match username_connection_info {
                    Some(UsernameConnectionInfo {
                        connection_offer_hash,
                        connection_package_hash,
                        username,
                    }) => IncomingRequestSource::Username {
                        username,
                        connection_offer_hash,
                        connection_package_hash,
                    },
                    None => {
                        let origin_chat_id =
                            origin_chat_id.context("logic error: no origin chat id")?;
                        let origin_chat = Chat::load(&mut *txn, &origin_chat_id)
                            .await?
                            .context("no origin chat")?;
                        let crate::ChatType::Group(_) = origin_chat.chat_type else {
                            bail!("Non-group chat as targeted message origin");
                        };
                        IncomingRequestSource::Group {
                            origin_group_id: origin_chat.group_id,
                        }
                    }
                };
                // Forwarding and provisioning hand the verified credential of
                // the sender to the siblings.
                StorableUserCredential::new(sender_user_credential.clone())
                    .store(&mut *txn)
                    .await?;
                let request = IncomingRequest {
                    connection_info,
                    sender,
                    source,
                    received_at: message_timestamp,
                };
                let stored = request.store(txn).await?;
                if forward {
                    connection_requests::park_received(txn, request_id, &sender_user_credential)
                        .await?;
                }
                Ok(Some(stored))
            })
            .await
    }

    /// Parse and verify the connection offer
    async fn parse_and_verify_connection_offer(
        mut connection: impl WriteConnection,
        api_clients: &ApiClients,
        com: ConnectionOfferMessage,
        user_handle: Username,
    ) -> Result<(ConnectionOfferPayload, ConnectionPackageHash)> {
        let (eco, hash) = com.into_parts();

        let decryption_key = ConnectionPackageRecord::load_decryption_key(&mut connection, &hash)
            .await?
            .context("No decryption key found for incoming connection offer")?;

        let cep_in = ConnectionOfferIn::decrypt(eco, &decryption_key, &[], &[])?;
        // Fetch authentication AS credentials of the sender if we don't have them already.
        let sender_domain = cep_in.sender_domain();

        // EncryptedConnectionOffer Phase 1: Load the AS credential of the sender.
        let as_intermediate_credential = AsCredentials::get(
            connection,
            api_clients,
            sender_domain,
            cep_in.signer_fingerprint(),
        )
        .await?;
        let payload = cep_in
            .verify(
                as_intermediate_credential.verifying_key(),
                user_handle,
                hash,
            )
            .map_err(|error| {
                error!(%error, "Error verifying connection offer");
                anyhow!("Error verifying connection offer")
            })?;

        Ok((payload, hash))
    }
}
