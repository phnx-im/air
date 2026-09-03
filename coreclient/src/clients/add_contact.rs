// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use airapiclient::{ApiClient, as_api::AsConnectionOfferResponder};
use aircommon::{
    credentials::keys::UserSigningKey,
    crypto::{
        aead::keys::{FriendshipPackageEarKey, GroupStateEarKey, IdentityLinkWrapperKey},
        hash::Hashable as _,
        hpke::HpkeEncryptable,
        indexed_aead::keys::UserProfileKey,
    },
    identifiers::{QsReference, UserId, Username, UsernameHash},
    messages::{
        client_as::{ConnectionOfferHash, ConnectionOfferMessage, EncryptedConnectionOffer},
        client_ds_out::{CreateGroupParamsOut, TargetedMessageParamsOut},
        connection_package::ConnectionPackage,
    },
    time::TimeStamp,
};
use airprotos::client::group::GroupData;
use airprotos::client::group_bootstrap::{
    ConnectionContext, GroupBootstrapCarrier, HandleInitiatorContext, TargetedInitiatorContext,
};
use anyhow::{Context, bail};
use openmls::group::GroupId;
use tracing::{error, info};

use crate::{
    Chat, ChatId, ChatMessage, SystemMessage,
    chats::GroupDataExt,
    clients::{
        connection_offer::{FriendshipPackage, payload::ConnectionInfo},
        targeted_message::TargetedMessageContent,
    },
    contacts::{PartialContact, PartialContactType, TargetedMessageContact, UsernameContact},
    db::access::WriteDbTransaction,
    groups::{
        Group, PartialCreateGroupParams, group_bootstrap::secret_bytes,
        openmls_provider::AirOpenMlsProvider, self_group::SelfGroup,
    },
    key_stores::{MemoryUserKeyStore, indexed_keys::StorableIndexedKey},
};

use super::{CoreUser, connection_offer::payload::ConnectionOfferPayload};

#[derive(Debug)]
pub enum AddUsernameContactError {
    /// The contact could not be added because the username does not exist
    UsernameNotFound,
    /// There is already a pending contact request for this username
    DuplicateRequest,
    /// The given username is our own
    OwnUsername,
}

impl CoreUser {
    /// Create a connection with a new user via their username.
    ///
    /// Returns the [`ChatId`] of the newly created connection chat, or `None` if the username does
    /// not exist.
    ///
    /// The hash must be pre-computed before calling this function.
    pub async fn add_contact(
        &self,
        username: Username,
        hash: UsernameHash,
    ) -> anyhow::Result<Result<ChatId, AddUsernameContactError>> {
        let client = self.api_client()?;

        // Phase 0: Perform sanity checks
        // Check if a connection request is already pending
        if UsernameContact::load(self.db().read().await?, &username)
            .await?
            .is_some()
        {
            return Ok(Err(AddUsernameContactError::DuplicateRequest));
        }
        // Check if the target username is one of our own usernames
        if self.usernames().await?.contains(&username) {
            return Ok(Err(AddUsernameContactError::OwnUsername));
        }

        // Phase 1: Fetch a connection package from the AS
        let (connection_package, connection_offer_responder) =
            match client.as_connect_username(hash).await {
                Ok(res) => res,
                Err(error) if error.is_not_found() => {
                    return Ok(Err(AddUsernameContactError::UsernameNotFound));
                }
                Err(error) => return Err(error.into()),
            };

        // Phase 2: Verify the connection package
        let verified_connection_package = connection_package.verify()?;
        // We don't need to know if the connection package is last resort here,
        // so we can just turn it into a v2.
        let verified_connection_package: ConnectionPackage =
            verified_connection_package.into_current();

        // Phase 3: Prepare the connection locally
        // No need to provision a group profile here, because we only have the group title and no
        // any additional data to upload.
        let provision_group_profile = None;
        let request_pq_group_id = false;
        let (group_id, _, _) = client
            .ds_request_group_id(provision_group_profile, request_pq_group_id)
            .await?;
        let connection_package = VerifiedConnectionPackagesWithGroupId {
            payload: verified_connection_package,
            group_id,
        };

        let client_reference = self.create_own_client_reference();

        // Phase 4: Create the connection group locally and commit it.
        let local_partial_contact = Box::pin(self.db().with_write_transaction(async |txn| {
            let local_group = Box::pin(connection_package.create_local_connection_group(
                &mut *txn,
                &self.inner.key_store.signing_key,
                username.clone(),
            ))
            .await?;

            Box::pin(local_group.create_username_contact(
                txn,
                &self.inner.key_store,
                client_reference,
                self.user_id(),
                username,
            ))
            .await
        }))
        .await?;

        // Phase 5: Create the connection group on the DS and send off the connection offer
        let cleanup = local_partial_contact.cleanup();
        let result = Box::pin(local_partial_contact.create_connection_group_via_username(
            &client,
            self.signing_key(),
            connection_offer_responder,
        ))
        .await;
        match result {
            Ok(chat_id) => Ok(Ok(chat_id)),
            Err(error) => Err(self.handle_connection_group_error(cleanup, error).await),
        }
    }

    /// Create a connection with a new user via an existing group chat.
    ///
    /// The group chat must contain the user to connect to. Returns the [`ChatId`] of the newly
    /// created connection chat.
    pub async fn add_contact_from_group(
        &self,
        chat_id: ChatId,
        user_id: UserId,
    ) -> anyhow::Result<ChatId> {
        let client = self.api_client()?;

        // Phase 0: Sanity checks
        // Check whether we already have this user as a contact
        if self.contact(&user_id).await.is_some() {
            bail!("User is already a contact");
        }

        // Check whether we already have a pending connection request to this user
        if TargetedMessageContact::load(self.db().read().await?, &user_id)
            .await?
            .is_some()
        {
            bail!("Connection request is already pending");
        }

        // Phase 1: Prepare the connection locally
        // No need to provision a group profile here, because we only have the group title and no
        // any additional data to upload.
        let provision_group_profile = None;
        let request_pq_group_id = false;
        let (group_id, _, _) = client
            .ds_request_group_id(provision_group_profile, request_pq_group_id)
            .await?;
        let connection_package = VerifiedConnectionPackagesWithGroupId {
            payload: user_id,
            group_id,
        };

        let client_reference = self.create_own_client_reference();

        // Phase 4: Create the connection group and the targeted message
        // locally.
        let local_partial_contact = Box::pin(self.db().with_write_transaction(async |txn| {
            let local_group = connection_package
                .create_local_connection_group(&mut *txn, &self.inner.key_store.signing_key)
                .await?;

            Box::pin(local_group.create_targeted_message_contact(
                txn,
                &self.inner.key_store,
                client_reference,
                self.user_id(),
                chat_id,
            ))
            .await
        }))
        .await?;

        // Phase 5: Create the connection group on the DS and send off the connection offer
        let cleanup = local_partial_contact.cleanup();
        let result = Box::pin(
            local_partial_contact
                .create_connection_group_via_targeted_message(&client, self.signing_key()),
        )
        .await;
        match result {
            Ok(chat_id) => Ok(chat_id),
            Err(error) => Err(self.handle_connection_group_error(cleanup, error).await),
        }
    }

    /// Cleans up after a failed connection group setup and returns the error
    /// to propagate.
    async fn handle_connection_group_error(
        &self,
        group: DiscardedConnectionGroup,
        error: ConnectionGroupError,
    ) -> anyhow::Error {
        match error {
            ConnectionGroupError::NotCreated(error) => {
                self.discard_local_connection_group(group).await;
                error
            }
            ConnectionGroupError::CreatedThenFailed(error) => {
                error!(
                    %error,
                    chat_id = %group.chat_id,
                    "Connection group exists on the DS and on the sibling devices, keeping it locally"
                );
                error
            }
        }
    }

    /// Removes the local state of a connection group the DS did not accept.
    async fn discard_local_connection_group(&self, group: DiscardedConnectionGroup) {
        let DiscardedConnectionGroup {
            group_id,
            chat_id,
            contact,
            connection_offer_hash,
        } = group;
        let result = self
            .db()
            .with_write_transaction(async |txn| -> anyhow::Result<()> {
                Group::delete_from_db(&mut *txn, &group_id).await?;
                Chat::delete(&mut *txn, chat_id).await?;
                if let Some(contact) = PartialContact::load(&mut *txn, &contact).await? {
                    contact.delete(&mut *txn).await?;
                }
                if let Some(hash) = connection_offer_hash {
                    Group::delete_connection_offer_psk(txn, hash)?;
                }
                Ok(())
            })
            .await;
        if let Err(error) = result {
            error!(%error, "Failed to clean up the rejected connection group");
        }
    }
}

/// The local rows of a connection group the DS did not accept.
struct DiscardedConnectionGroup {
    group_id: GroupId,
    chat_id: ChatId,
    contact: PartialContactType,
    connection_offer_hash: Option<ConnectionOfferHash>,
}

/// A failure while establishing a connection group on the DS.
enum ConnectionGroupError {
    /// The DS did not accept the create-group request, so nothing outside this
    /// device knows about the group and the local rows can go.
    NotCreated(anyhow::Error),
    /// The DS accepted the group and a later send failed. The DS echoed the
    /// creation to the sibling queues when it accepted it, so the siblings
    /// install the group.
    CreatedThenFailed(anyhow::Error),
}

struct VerifiedConnectionPackagesWithGroupId<Payload = ConnectionPackage> {
    payload: Payload,
    group_id: GroupId,
}

impl<Payload> VerifiedConnectionPackagesWithGroupId<Payload> {
    async fn create_connection_group_internal(
        &self,
        txn: &mut WriteDbTransaction<'_>,
        signing_key: &UserSigningKey,
    ) -> anyhow::Result<(Group, PartialCreateGroupParams, Option<SelfGroup>)> {
        let identity_link_wrapper_key = IdentityLinkWrapperKey::random()?;
        let group_data_bytes = GroupData {
            encrypted_title: None,
            external_group_profile: None,
            legacy_title: Some(String::new()), // Old clients still expect a title
            legacy_picture: None,
        }
        .encode()?;

        let self_group = SelfGroup::load(&mut *txn).await?;

        let (group, partial_params) = Group::create_group(
            &mut *txn,
            signing_key,
            identity_link_wrapper_key,
            self.group_id.clone(),
            group_data_bytes,
            self_group.as_ref().map(|group| group.group_id()),
        )?;

        group.store(txn).await?;

        Ok((group, partial_params, self_group))
    }
}

impl VerifiedConnectionPackagesWithGroupId<ConnectionPackage> {
    async fn create_local_connection_group(
        self,
        txn: &mut WriteDbTransaction<'_>,
        signing_key: &UserSigningKey,
        username: Username,
    ) -> anyhow::Result<LocalGroup<ConnectionPackage>> {
        info!("Creating local connection group");

        let (group, partial_params, self_group) = self
            .create_connection_group_internal(&mut *txn, signing_key)
            .await?;

        let Self {
            payload: method_payload,
            group_id,
        } = self;

        // Create the connection chat
        let chat = Chat::new_handle_chat(group_id.clone(), username.clone());
        chat.store(&mut *txn).await?;

        // Create the initial system message for the chat
        let system_message = SystemMessage::NewHandleConnectionChat(username);
        let chat_message =
            ChatMessage::new_system_message(chat.id(), TimeStamp::now(), system_message);
        chat_message.store(&mut *txn).await?;

        Ok(LocalGroup {
            group,
            partial_params,
            self_group,
            chat_id: chat.id(),
            payload: method_payload,
        })
    }
}

impl VerifiedConnectionPackagesWithGroupId<UserId> {
    async fn create_local_connection_group(
        self,
        txn: &mut WriteDbTransaction<'_>,
        signing_key: &UserSigningKey,
    ) -> anyhow::Result<LocalGroup<UserId>> {
        info!("Creating local connection group");
        let (group, partial_params, self_group) = self
            .create_connection_group_internal(&mut *txn, signing_key)
            .await?;

        let Self {
            payload: user_id,
            group_id,
        } = self;

        // Create the connection chat
        let chat = Chat::new_targeted_message_chat(group_id.clone(), user_id.clone());
        chat.store(&mut *txn).await?;

        // Create the initial system message for the chat
        let system_message = SystemMessage::NewDirectConnectionChat(user_id.clone());
        let chat_message =
            ChatMessage::new_system_message(chat.id(), TimeStamp::now(), system_message);
        chat_message.store(txn).await?;

        Ok(LocalGroup {
            group,
            partial_params,
            self_group,
            chat_id: chat.id(),
            payload: user_id,
        })
    }
}

struct LocalGroup<Payload = ConnectionPackage> {
    group: Group,
    partial_params: PartialCreateGroupParams,
    self_group: Option<SelfGroup>,
    chat_id: ChatId,
    payload: Payload,
}

impl LocalGroup<ConnectionPackage> {
    async fn create_username_contact(
        self,
        txn: &mut WriteDbTransaction<'_>,
        key_store: &MemoryUserKeyStore,
        own_client_reference: QsReference,
        own_user_id: &UserId,
        username: Username,
    ) -> anyhow::Result<LocalUsernameContact<UsernamePayload>> {
        let Self {
            group,
            partial_params,
            self_group,
            chat_id,
            payload: verified_connection_package,
        } = self;

        let own_user_profile_key = UserProfileKey::load_own(&mut *txn).await?;

        let friendship_package = FriendshipPackage {
            friendship_token: key_store.friendship_token.clone(),
            wai_ear_key: key_store.wai_ear_key.clone(),
            user_profile_base_secret: own_user_profile_key.base_secret().clone(),
        };

        let friendship_package_ear_key = FriendshipPackageEarKey::random()?;

        // Create a connection offer
        let connection_package_hash = verified_connection_package.hash();
        let connection_offer_payload = ConnectionOfferPayload {
            sender_user_credential: key_store.signing_key.credential().clone(),
            connection_info: ConnectionInfo::new(
                &group,
                friendship_package,
                friendship_package_ear_key.clone(),
            ),
            connection_package_hash,
        };
        let connection_offer = connection_offer_payload
            .sign(
                &key_store.signing_key,
                username.clone(),
                verified_connection_package.hash(),
            )?
            .encrypt(verified_connection_package.encryption_key(), &[], &[]);

        let connection_offer_hash = connection_offer.hash();

        group.store_connection_offer_psk(&mut *txn, connection_offer_hash)?;

        // Create and persist a new partial contact
        UsernameContact::new(
            username.clone(),
            chat_id,
            friendship_package_ear_key.clone(),
            connection_offer_hash,
        )
        .upsert(&mut *txn)
        .await?;

        let encrypted_user_profile_key =
            own_user_profile_key.encrypt(group.identity_link_wrapper_key(), own_user_id)?;
        let mut params =
            partial_params.into_params(own_client_reference, encrypted_user_profile_key);

        if let Some(self_group) = &self_group {
            let connection = ConnectionContext::HandleInitiator(HandleInitiatorContext {
                username: Some(username.plaintext().to_owned()),
                friendship_package_ear_key: Some(secret_bytes(&friendship_package_ear_key)),
                connection_offer_hash: Some(connection_offer_hash),
            });
            params.group_bootstrap = Some(self_group.seal_group_bootstrap_param(
                txn,
                &group,
                GroupBootstrapCarrier::CreationEcho,
                Some(connection),
            )?);
        }

        Ok(LocalUsernameContact::<UsernamePayload> {
            group,
            params,
            chat_id,
            contact: PartialContactType::Handle(username),
            connection_offer_hash: Some(connection_offer_hash),
            payload: UsernamePayload {
                connection_offer,
                verified_connection_package,
            },
        })
    }
}

impl LocalGroup<UserId> {
    async fn create_targeted_message_contact(
        self,
        txn: &mut WriteDbTransaction<'_>,
        key_store: &MemoryUserKeyStore,
        own_client_reference: QsReference,
        own_user_id: &UserId,
        targeted_message_chat_id: ChatId,
    ) -> anyhow::Result<LocalUsernameContact<TargetedMessagePayload>> {
        let Self {
            group,
            partial_params,
            self_group,
            chat_id,
            payload: user_id,
        } = self;

        let own_user_profile_key = UserProfileKey::load_own(&mut *txn).await?;

        let friendship_package = FriendshipPackage {
            friendship_token: key_store.friendship_token.clone(),
            wai_ear_key: key_store.wai_ear_key.clone(),
            user_profile_base_secret: own_user_profile_key.base_secret().clone(),
        };

        let friendship_package_ear_key = FriendshipPackageEarKey::random()?;

        // Create and persist a new partial contact
        let contact = TargetedMessageContact::new(
            user_id.clone(),
            chat_id,
            friendship_package_ear_key.clone(),
        );
        contact.upsert(&mut *txn).await?;

        let encrypted_user_profile_key =
            own_user_profile_key.encrypt(group.identity_link_wrapper_key(), own_user_id)?;
        let mut params =
            partial_params.into_params(own_client_reference, encrypted_user_profile_key);

        if let Some(self_group) = &self_group {
            let connection = ConnectionContext::TargetedInitiator(TargetedInitiatorContext {
                user_id: Some(contact.user_id.clone().into()),
                friendship_package_ear_key: Some(secret_bytes(&friendship_package_ear_key)),
            });
            params.group_bootstrap = Some(self_group.seal_group_bootstrap_param(
                txn,
                &group,
                GroupBootstrapCarrier::CreationEcho,
                Some(connection),
            )?);
        }

        // Prepare targeted message
        let connection_info =
            ConnectionInfo::new(&group, friendship_package, friendship_package_ear_key);
        let mut targeted_message_group =
            Group::load_with_chat_id(&mut *txn, targeted_message_chat_id)
                .await?
                .context("Can't find group to send targeted message in")?;
        let provider = AirOpenMlsProvider::new(txn.as_mut());
        let targeted_message_params = targeted_message_group.create_targeted_application_message(
            &provider,
            &key_store.signing_key,
            contact.user_id,
            TargetedMessageContent::ConnectionRequest(connection_info),
        )?;

        Ok(LocalUsernameContact::<TargetedMessagePayload> {
            group,
            params,
            chat_id,
            contact: PartialContactType::TargetedMessage(user_id),
            connection_offer_hash: None,
            payload: TargetedMessagePayload {
                targeted_message_params,
                targeted_group_state_ear_key: targeted_message_group.group_state_ear_key().clone(),
            },
        })
    }
}

struct TargetedMessagePayload {
    targeted_message_params: TargetedMessageParamsOut,
    targeted_group_state_ear_key: GroupStateEarKey,
}

struct UsernamePayload {
    connection_offer: EncryptedConnectionOffer,
    verified_connection_package: ConnectionPackage,
}

struct LocalUsernameContact<Payload = UsernamePayload> {
    group: Group,
    params: CreateGroupParamsOut,
    chat_id: ChatId,
    contact: PartialContactType,
    connection_offer_hash: Option<ConnectionOfferHash>,
    payload: Payload,
}

impl<Payload> LocalUsernameContact<Payload> {
    /// The local rows to remove if the DS does not accept the group.
    fn cleanup(&self) -> DiscardedConnectionGroup {
        DiscardedConnectionGroup {
            group_id: self.group.group_id().clone(),
            chat_id: self.chat_id,
            contact: self.contact.clone(),
            connection_offer_hash: self.connection_offer_hash,
        }
    }
}

impl LocalUsernameContact<UsernamePayload> {
    /// Creates the group on the DS and sends off the connection offer.
    ///
    /// The error tells the caller whether the DS accepted the group before the
    /// failure, which decides whether the local state may be discarded.
    async fn create_connection_group_via_username(
        self,
        client: &ApiClient,
        signer: &UserSigningKey,
        responder: AsConnectionOfferResponder,
    ) -> Result<ChatId, ConnectionGroupError> {
        let Self {
            group,
            params,
            chat_id,
            payload:
                UsernamePayload {
                    connection_offer,
                    verified_connection_package,
                },
            ..
        } = self;

        info!("Creating connection group on DS");
        client
            .ds_create_group(params, signer, group.group_state_ear_key())
            .await
            .map_err(|error| ConnectionGroupError::NotCreated(error.into()))?;

        // Send off the connection offer. The group exists on the DS from here
        // on, so a failure must not take the local state with it.
        let hash = verified_connection_package.hash();
        let message = ConnectionOfferMessage::new(hash, connection_offer);
        responder
            .send(message)
            .await
            .map_err(|error| ConnectionGroupError::CreatedThenFailed(error.into()))?;

        Ok(chat_id)
    }
}

impl LocalUsernameContact<TargetedMessagePayload> {
    /// Creates the group on the DS and sends off the targeted message carrying
    /// the connection offer.
    ///
    /// The error tells the caller whether the DS accepted the group before the
    /// failure, which decides whether the local state may be discarded.
    async fn create_connection_group_via_targeted_message(
        self,
        client: &ApiClient,
        signer: &UserSigningKey,
    ) -> Result<ChatId, ConnectionGroupError> {
        let Self {
            group,
            params,
            chat_id,
            payload:
                TargetedMessagePayload {
                    targeted_message_params,
                    targeted_group_state_ear_key,
                },
            ..
        } = self;

        info!("Creating connection group on DS");
        client
            .ds_create_group(params, signer, group.group_state_ear_key())
            .await
            .map_err(|error| ConnectionGroupError::NotCreated(error.into()))?;

        // Send off the targeted message. The group exists on the DS from here
        // on, so a failure must not take the local state with it.
        // TODO: This should be scheduled in the outbound service
        client
            .ds_targeted_message(
                targeted_message_params,
                signer,
                &targeted_group_state_ear_key,
            )
            .await
            .map_err(|error| ConnectionGroupError::CreatedThenFailed(error.into()))?;

        Ok(chat_id)
    }
}
