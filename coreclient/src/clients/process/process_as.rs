// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::{
    credentials::ClientCredential,
    crypto::{
        ear::keys::FriendshipPackageEarKey, hpke::HpkeDecryptable,
        indexed_aead::keys::UserProfileKey,
    },
    identifiers::{QualifiedGroupId, UserHandle, UserId},
    messages::{
        client_as::{ConnectionOfferHash, ConnectionOfferMessage},
        connection_package::{ConnectionPackage, ConnectionPackageHash},
    },
    time::TimeStamp,
};
use airprotos::auth_service::v1::{HandleQueueMessage, handle_queue_message};
use anyhow::{Context, Result, bail, ensure};
use openmls::group::GroupId;
use sqlx::SqliteConnection;
use tls_codec::DeserializeBytes;
use tracing::{error, warn};

use crate::{
    PartialContact, SystemMessage, TargetedMessageContact,
    chats::{PendingConnectionInfo, messages::TimestampedMessage},
    clients::{
        block_contact::{BlockedContact, BlockedContactError},
        connection_offer::{
            ConnectionOfferIn,
            payload::{ConnectionInfo, ConnectionOfferPayload},
        },
    },
    contacts::HandleContact,
    groups::ProfileInfo,
    user_handles::connection_packages::StorableConnectionPackage,
    utils::connection_ext::StoreExt,
};

use super::{AsCredentials, Chat, ChatAttributes, ChatId, CoreUser, FriendshipPackage, anyhow};

pub(crate) enum ConnectionInfoSource {
    ConnectionOffer(Box<ConnectionOfferSource>),
    TargetedMessage(Box<TargetedMessageSource>),
}

pub(crate) struct ConnectionOfferSource {
    pub(crate) connection_offer: ConnectionOfferMessage,
    pub(crate) user_handle: UserHandle,
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

struct HandleConnectionInfo {
    connection_offer_hash: ConnectionOfferHash,
    connection_package_hash: ConnectionPackageHash,
    handle: UserHandle,
}

impl ConnectionInfoSource {
    async fn into_parts(
        self,
        core_user: &CoreUser,
    ) -> Result<(
        ConnectionInfo,
        ClientCredential,
        Option<ChatId>,
        Option<HandleConnectionInfo>,
        Option<TimeStamp>,
    )> {
        match self {
            ConnectionInfoSource::ConnectionOffer(connection_offer_source) => {
                let ConnectionOfferSource {
                    connection_offer,
                    user_handle,
                    sent_at,
                } = *connection_offer_source;
                let connection_offer_hash = connection_offer.connection_offer_hash();
                let mut connection = core_user.pool().acquire().await?;
                let (cep_payload, hash) = core_user
                    .parse_and_verify_connection_offer(
                        &mut connection,
                        connection_offer,
                        user_handle.clone(),
                    )
                    .await?;
                let sender_client_credential = cep_payload.sender_client_credential;
                let handle_connection_info = HandleConnectionInfo {
                    connection_offer_hash,
                    connection_package_hash: hash,
                    handle: user_handle,
                };
                Ok((
                    cep_payload.connection_info,
                    sender_client_credential,
                    None,
                    Some(handle_connection_info),
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
    pub(crate) async fn process_handle_queue_message_event_loop(
        &self,
        user_handle: UserHandle,
        handle_queue_message: HandleQueueMessage,
    ) -> Result<ChatId> {
        let payload = handle_queue_message
            .payload
            .context("no payload in handle queue message")?;

        // Extract the server timestamp from the message
        let sent_at = handle_queue_message.created_at.map(TimeStamp::from);

        match payload {
            handle_queue_message::Payload::ConnectionOffer(eco) => {
                let connection_info_source =
                    ConnectionInfoSource::ConnectionOffer(Box::new(ConnectionOfferSource {
                        connection_offer: eco.try_into()?,
                        user_handle: user_handle.clone(),
                        sent_at,
                    }));
                self.process_connection_offer(connection_info_source).await
            }
        }
    }

    pub(crate) async fn process_connection_offer(
        &self,
        connection_info_source: ConnectionInfoSource,
    ) -> anyhow::Result<ChatId> {
        let (connection_info, sender_client_credential, origin_chat_id, handle_connection_info, sent_at) =
            connection_info_source.into_parts(self).await?;
        // Use the server's timestamp if available, otherwise fall back to current time
        let message_timestamp = sent_at.unwrap_or_else(TimeStamp::now);

        // Deny connection from blocked users
        if BlockedContact::check_blocked(self.pool(), sender_client_credential.identity()).await? {
            bail!(BlockedContactError);
        }

        // Load user profile => creates or updates a `User` record
        self.with_notifier(async |notifier| {
            let sender_profile_key = UserProfileKey::from_base_secret(
                connection_info
                    .friendship_package
                    .user_profile_base_secret
                    .clone(),
                sender_client_credential.identity(),
            )?;
            let profile_info = ProfileInfo {
                client_credential: sender_client_credential.clone(),
                user_profile_key: sender_profile_key,
            };
            let res = Self::fetch_and_store_user_profile(
                self.pool().acquire().await?.as_mut(),
                notifier,
                self.api_clients(),
                profile_info,
            )
            .await;
            if let Err(error) = res {
                warn!(%error, "Failed to fetch user profile; falling back to fetching group info");

                // Fetch external commit info
                let qgid = QualifiedGroupId::tls_deserialize_exact_bytes(
                    connection_info.connection_group_id.as_slice(),
                )?;
                let eci = self
                    .api_clients()
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
                    sender_client_credential.identity(),
                )?;

                // Fetch and store user profile (it also creates a new contact)
                let profile_info = ProfileInfo {
                    client_credential: sender_client_credential.clone(),
                    user_profile_key,
                };
                Self::fetch_and_store_user_profile(
                    self.pool().acquire().await?.as_mut(),
                    notifier,
                    self.api_clients(),
                    profile_info,
                )
                .await?;
            }
            Ok(())
        })
        .await?;

        self.with_transaction_and_notifier(async |txn, notifier| {
            let sender_user_id = sender_client_credential.identity();

            // Create pending unconfirmed chat
            let (chat, partial_contact) = self
                .create_pending_connection_chat(
                    txn.as_mut(),
                    &connection_info.connection_group_id,
                    sender_user_id.clone(),
                    connection_info.friendship_package.clone(),
                    handle_connection_info.as_ref(),
                )
                .await?;

            // Create pending connection info
            let (handle, connection_offer_hash, connection_package_hash) =
                if let Some(HandleConnectionInfo {
                    connection_offer_hash,
                    connection_package_hash,
                    handle,
                }) = handle_connection_info
                {
                    (
                        Some(handle),
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
                handle,
                connection_offer_hash,
                connection_package_hash,
            };

            // Create system messages for receipt and acceptance
            let received_system_message = match &partial_contact {
                PartialContact::Handle(contact) => {
                    // Connection via handle
                    SystemMessage::ReceivedHandleConnectionRequest {
                        sender: sender_user_id.clone(),
                        user_handle: contact.handle.clone(),
                    }
                }
                PartialContact::TargetedMessage(contact) => {
                    // Connection via targeted message
                    let origin_chat_id =
                        origin_chat_id.context("logic error: no origin chat id")?;
                    let origin_chat = Chat::load(txn.as_mut(), &origin_chat_id)
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
            chat.store(txn.as_mut(), notifier).await?;
            pending_chat.store(txn.as_mut(), notifier).await?;
            partial_contact.upsert(txn.as_mut(), notifier).await?;
            Self::store_new_messages(txn.as_mut(), notifier, chat.id(), chat_messages).await?;

            Ok(chat.id)
        })
        .await
    }

    /// Parse and verify the connection offer
    async fn parse_and_verify_connection_offer(
        &self,
        connection: &mut SqliteConnection,
        com: ConnectionOfferMessage,
        user_handle: UserHandle,
    ) -> Result<(ConnectionOfferPayload, ConnectionPackageHash)> {
        let (eco, hash) = com.into_parts();

        let decryption_key = ConnectionPackage::load_decryption_key(&mut *connection, &hash)
            .await?
            .context("No decryption key found for incoming connection offer")?;

        let cep_in = ConnectionOfferIn::decrypt(eco, &decryption_key, &[], &[])?;
        // Fetch authentication AS credentials of the sender if we don't have them already.
        let sender_domain = cep_in.sender_domain();

        // EncryptedConnectionOffer Phase 1: Load the AS credential of the sender.
        let as_intermediate_credential = AsCredentials::get(
            connection,
            &self.inner.api_clients,
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
        &self,
        connection: &mut SqliteConnection,
        group_id: &GroupId,
        sender_user_id: UserId,
        _friendship_package: FriendshipPackage,
        handle_connection_info: Option<&HandleConnectionInfo>,
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

        let partial_contact = if let Some(handle_connection_info) = handle_connection_info {
            PartialContact::Handle(HandleContact::new(
                handle_connection_info.handle.clone(),
                chat.id(),
                random_ear_key,
                handle_connection_info.connection_offer_hash,
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
