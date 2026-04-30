// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::{
    credentials::ClientCredential,
    crypto::{
        aead::keys::FriendshipPackageEarKey, hpke::HpkeDecryptable,
        indexed_aead::keys::UserProfileKey,
    },
    identifiers::{QualifiedGroupId, UserId, Username},
    messages::{
        client_as::{ConnectionOfferHash, ConnectionOfferMessage},
        connection_package::{ConnectionPackage, ConnectionPackageHash},
    },
    time::TimeStamp,
};
use airprotos::auth_service::v1::{UsernameQueueMessage, username_queue_message};
use anyhow::{Context, Result, anyhow, bail, ensure};
use chrono::Utc;
use openmls::group::GroupId;
use tls_codec::DeserializeBytes;
use tracing::{error, warn};

use crate::{
    PartialContact, SystemMessage, TargetedMessageContact,
    chats::{PendingConnectionInfo, messages::TimestampedMessage},
    clients::{
        api_clients::ApiClients,
        block_contact::{BlockedContact, BlockedContactError},
        connection_offer::{
            ConnectionOfferIn,
            payload::{ConnectionInfo, ConnectionOfferPayload},
        },
    },
    contacts::UsernameContact,
    db_access::{ReadConnection, WriteConnection},
    groups::ProfileInfo,
    job::{Job, JobContext, JobContextDb},
    usernames::connection_packages::StorableConnectionPackage,
};

use super::{AsCredentials, Chat, ChatAttributes, ChatId, CoreUser, FriendshipPackage};

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
    pub(crate) sender_client_credential: ClientCredential,
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
        ClientCredential,
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
                let sender_client_credential = cep_payload.sender_client_credential;
                let username_connection_info = UsernameConnectionInfo {
                    connection_offer_hash,
                    connection_package_hash: hash,
                    username,
                };
                Ok((
                    cep_payload.connection_info,
                    sender_client_credential,
                    None,
                    Some(username_connection_info),
                    sent_at,
                ))
            }
            ConnectionInfoSource::TargetedMessage(targeted_message_source) => {
                let TargetedMessageSource {
                    connection_info,
                    sender_client_credential,
                    origin_chat_id,
                    sent_at,
                } = *targeted_message_source;
                Ok((
                    connection_info,
                    sender_client_credential,
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
    ) -> Result<ChatId> {
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
                };
                let chat_id =
                    Self::process_connection_offer(&mut context, connection_info_source).await?;

                Ok(chat_id)
            }
        }
    }

    pub(crate) async fn process_connection_offer(
        context: &mut JobContext<'_, '_>,
        connection_info_source: ConnectionInfoSource,
    ) -> anyhow::Result<ChatId> {
        let api_clients = context.api_clients.clone();
        let (
            connection_info,
            sender_client_credential,
            origin_chat_id,
            username_connection_info,
            sent_at,
        ) = connection_info_source
            .into_parts(context.db.write().await?, &api_clients)
            .await?;

        // Use the server's timestamp if available, otherwise fall back to current time
        let message_timestamp = sent_at.unwrap_or_else(TimeStamp::now);

        // Deny connection from blocked users
        if BlockedContact::check_blocked(
            context.db.read().await?,
            sender_client_credential.user_id(),
        )
        .await?
        {
            bail!(BlockedContactError);
        }

        // Idempotency: skip if the chat for this connection offer already exists.
        // ChatId is deterministic from the group_id, so a duplicate offer will
        // produce the same chat_id and we can safely return early.
        let chat_id = ChatId::try_from(&connection_info.connection_group_id)?;
        let chat = {
            let mut connection = context.db.read().await?;
            let txn = connection.begin().await?;
            Chat::load(txn, &chat_id).await?
        };

        if chat.is_some() {
            return Ok(chat_id);
        }

        // Immediately fetch the user profile. This might fail if the user updated their
        // profile in the meantime => fallback to fetching group info.
        let sender_profile_key = UserProfileKey::from_base_secret(
            connection_info
                .friendship_package
                .user_profile_base_secret
                .clone(),
            sender_client_credential.user_id(),
        )?;

        let fetch_profile_job = CoreUser::fetch_user_profile_job((
            sender_client_credential.clone(),
            sender_profile_key,
        ));

        if let Err(error) = fetch_profile_job.execute(context).await {
            warn!(%error, "Failed to fetch user profile; falling back to fetching group info");

            // Fetch external commit info
            let qgid = QualifiedGroupId::tls_deserialize_exact_bytes(
                connection_info.connection_group_id.as_slice(),
            )?;
            let eci = context
                .api_clients
                .get(qgid.owning_domain())?
                .ds_connection_group_info(
                    connection_info.connection_group_id.clone(),
                    &connection_info.connection_group_ear_key,
                )
                .await?;
            ensure!(
                eci.encrypted_user_profile_keys.len() == 1,
                "Unjoined connection group must have exactly one user profile key"
            );

            // Decrypt user profile key
            let encrypted_user_profile_key = &eci.encrypted_user_profile_keys[0];
            let user_profile_key = UserProfileKey::decrypt(
                &connection_info.connection_group_identity_link_wrapper_key,
                encrypted_user_profile_key,
                sender_client_credential.user_id(),
            )?;

            // Fetch and store user profile (it also creates a new contact)
            let profile_info = ProfileInfo {
                client_credential: sender_client_credential.clone(),
                user_profile_key,
            };

            CoreUser::fetch_user_profile_job(profile_info)
                .execute(context)
                .await?;
        }

        context
            .db
            .write()
            .await?
            .with_transaction(async |txn| {
                let sender_user_id = sender_client_credential.user_id();

                // Create pending unconfirmed chat
                let (chat, partial_contact) = Self::create_pending_connection_chat(
                    &mut *txn,
                    &connection_info.connection_group_id,
                    sender_user_id.clone(),
                    connection_info.friendship_package.clone(),
                    username_connection_info.as_ref(),
                )
                .await?;

                // Create pending connection info
                let (username, connection_offer_hash, connection_package_hash) =
                    if let Some(UsernameConnectionInfo {
                        connection_offer_hash,
                        connection_package_hash,
                        username,
                    }) = username_connection_info
                    {
                        (
                            Some(username),
                            Some(connection_offer_hash),
                            Some(connection_package_hash),
                        )
                    } else {
                        (None, None, None)
                    };
                let pending_chat = PendingConnectionInfo {
                    chat_id: chat.id(),
                    created_at: TimeStamp::now(),
                    connection_info,
                    handle: username,
                    connection_offer_hash,
                    connection_package_hash,
                };

                // Create system messages for receipt and acceptance
                let received_system_message = match &partial_contact {
                    PartialContact::Username(contact) => {
                        // Connection via username
                        SystemMessage::ReceivedHandleConnectionRequest {
                            sender: sender_user_id.clone(),
                            user_handle: contact.username.clone(),
                        }
                    }
                    PartialContact::TargetedMessage(contact) => {
                        // Connection via targeted message
                        let origin_chat_id =
                            origin_chat_id.context("logic error: no origin chat id")?;
                        let origin_chat = Chat::load(&mut *txn, &origin_chat_id)
                            .await?
                            .context("no origin chat")?;
                        SystemMessage::ReceivedDirectConnectionRequest {
                            sender: contact.user_id.clone(),
                            chat_name: origin_chat.attributes.title.clone(),
                        }
                    }
                };
                let received_message =
                    TimestampedMessage::system_message(received_system_message, message_timestamp);
                let chat_messages = vec![received_message];

                // Store chat, pending connection info, partial contact and system message
                // Note: Group is not created here!
                chat.store(&mut *txn).await?;
                pending_chat.store(&mut *txn).await?;
                partial_contact.upsert(&mut *txn).await?;
                Self::store_new_messages(txn, chat.id(), chat_messages).await?;

                Ok(chat.id)
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

        let decryption_key = ConnectionPackage::load_decryption_key(&mut connection, &hash)
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

    async fn create_pending_connection_chat(
        connection: impl ReadConnection,
        group_id: &GroupId,
        sender_user_id: UserId,
        _friendship_package: FriendshipPackage,
        username_connection_info: Option<&UsernameConnectionInfo>,
    ) -> anyhow::Result<(Chat, PartialContact)> {
        let display_name = Self::user_profile_internal(connection, &sender_user_id)
            .await
            .display_name;
        let chat = Chat::new_pending_connection_chat(
            group_id.clone(),
            sender_user_id.clone(),
            ChatAttributes::new(display_name.to_string(), None),
        );

        // FIXME(901): For incoming contacts, there is no EAR key but it is required.
        let random_ear_key = FriendshipPackageEarKey::random()?;

        let partial_contact = if let Some(username_connection_info) = username_connection_info {
            PartialContact::Username(UsernameContact::new(
                username_connection_info.username.clone(),
                chat.id(),
                random_ear_key,
                username_connection_info.connection_offer_hash,
            ))
        } else {
            PartialContact::TargetedMessage(TargetedMessageContact::new(
                sender_user_id.clone(),
                chat.id(),
                random_ear_key,
            ))
        };

        Ok((chat, partial_contact))
    }
}
