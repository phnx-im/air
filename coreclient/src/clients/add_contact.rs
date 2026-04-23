// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use airapiclient::{ApiClient, as_api::AsConnectionOfferResponder};
use aircommon::{
    credentials::keys::ClientSigningKey,
    crypto::{
        aead::keys::{FriendshipPackageEarKey, GroupStateEarKey, IdentityLinkWrapperKey},
        hash::Hashable as _,
        hpke::HpkeEncryptable,
        indexed_aead::keys::UserProfileKey,
    },
    identifiers::{QsReference, UserId, Username, UsernameHash},
    messages::{
        client_as::{ConnectionOfferMessage, EncryptedConnectionOffer},
        client_ds_out::{CreateGroupParamsOut, TargetedMessageParamsOut},
        connection_package::ConnectionPackage,
    },
    time::TimeStamp,
};
use airprotos::client::group::{EncryptedGroupTitle, GroupData};
use anyhow::{Context, bail};
use openmls::group::GroupId;
use tracing::info;

use crate::{
    Chat, ChatAttributes, ChatId, ChatMessage, SystemMessage, UserProfile,
    chats::GroupDataExt,
    clients::{
        connection_offer::{FriendshipPackage, payload::ConnectionInfo},
        targeted_message::TargetedMessageContent,
    },
    contacts::{TargetedMessageContact, UsernameContact},
    db_access::WriteDbTransaction,
    groups::{Group, PartialCreateGroupParams, openmls_provider::AirOpenMlsProvider},
    key_stores::{MemoryUserKeyStore, indexed_keys::StorableIndexedKey},
    store::Store,
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
    /// Create a connection via a username.
    ///
    /// The hash of the username must be pre-computed before calling this function.
    pub(crate) async fn add_contact_via_username(
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
        let (group_id, _) = client.ds_request_group_id(provision_group_profile).await?;
        let connection_package = VerifiedConnectionPackagesWithGroupId {
            payload: verified_connection_package,
            group_id,
        };

        let client_reference = self.create_own_client_reference();

        self.db()
            .with_write_transaction(async |txn| {
                // Phase 4: Create a connection group
                let local_group = connection_package
                    .create_local_connection_group(
                        &mut *txn,
                        &self.inner.key_store.signing_key,
                        username.clone(),
                    )
                    .await?;

                let local_partial_contact = local_group
                    .create_username_contact(
                        txn,
                        &self.inner.key_store,
                        client_reference,
                        self.user_id(),
                        username,
                    )
                    .await?;

                // Phase 5: Create the connection group on the DS and send off the connection offer
                let chat_id = local_partial_contact
                    .create_connection_group_via_username(
                        &client,
                        self.signing_key(),
                        connection_offer_responder,
                    )
                    .await?;

                Ok(Ok(chat_id))
            })
            .await
    }

    /// Create a connection with a user through a targeted message in a shared
    /// chat.
    pub(crate) async fn add_contact_via_targeted_message(
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
        let (group_id, _) = client.ds_request_group_id(provision_group_profile).await?;
        let connection_package = VerifiedConnectionPackagesWithGroupId {
            payload: user_id,
            group_id,
        };

        let client_reference = self.create_own_client_reference();

        self.db()
            .with_write_transaction(async |txn| {
                // Phase 4: Create a connection group and prepare the targeted message
                let local_group = connection_package
                    .create_local_connection_group(&mut *txn, &self.inner.key_store.signing_key)
                    .await?;

                let local_partial_contact = local_group
                    .create_targeted_message_contact(
                        &mut *txn,
                        &self.inner.key_store,
                        client_reference,
                        self.user_id(),
                        chat_id,
                    )
                    .await?;

                // Phase 5: Create the connection group on the DS and send off the connection offer
                let chat_id = local_partial_contact
                    .create_connection_group_via_targeted_message(&client, self.signing_key())
                    .await?;

                Ok(chat_id)
            })
            .await
    }
}

struct VerifiedConnectionPackagesWithGroupId<Payload = ConnectionPackage> {
    payload: Payload,
    group_id: GroupId,
}

impl<Payload> VerifiedConnectionPackagesWithGroupId<Payload> {
    async fn create_connection_group_internal(
        &self,
        txn: &mut WriteDbTransaction<'_>,
        signing_key: &ClientSigningKey,
        title: String,
    ) -> anyhow::Result<(Group, PartialCreateGroupParams)> {
        let identity_link_wrapper_key = IdentityLinkWrapperKey::random()?;
        let encrypted_title = EncryptedGroupTitle::encrypt(&title, &identity_link_wrapper_key)?;
        let group_data_bytes = GroupData {
            title,
            encrypted_title: Some(encrypted_title),
            picture: None,
            // No group profile is uploaded, because there is no additational data except for the
            // title.
            external_group_profile: None,
        }
        .encode()?;

        let (group, partial_params) = Group::create_group(
            &mut *txn,
            signing_key,
            identity_link_wrapper_key,
            self.group_id.clone(),
            group_data_bytes,
        )?;

        group.store(txn).await?;

        Ok((group, partial_params))
    }
}

impl VerifiedConnectionPackagesWithGroupId<ConnectionPackage> {
    async fn create_local_connection_group(
        self,
        txn: &mut WriteDbTransaction<'_>,
        signing_key: &ClientSigningKey,
        username: Username,
    ) -> anyhow::Result<LocalGroup<ConnectionPackage>> {
        info!("Creating local connection group");
        let title = format!("Connection group: {}", username.plaintext());
        let attributes = ChatAttributes::new(title, None);

        let (group, partial_params) = self
            .create_connection_group_internal(&mut *txn, signing_key, attributes.title.clone())
            .await?;

        let Self {
            payload: method_payload,
            group_id,
        } = self;

        // Create the connection chat
        let chat = Chat::new_handle_chat(group_id.clone(), attributes, username.clone());
        chat.store(&mut *txn).await?;

        // Create the initial system message for the chat
        let system_message = SystemMessage::NewHandleConnectionChat(username);
        let chat_message =
            ChatMessage::new_system_message(chat.id(), TimeStamp::now(), system_message);
        chat_message.store(&mut *txn).await?;

        Ok(LocalGroup {
            group,
            partial_params,
            chat_id: chat.id(),
            payload: method_payload,
        })
    }
}

impl VerifiedConnectionPackagesWithGroupId<UserId> {
    async fn create_local_connection_group(
        self,
        txn: &mut WriteDbTransaction<'_>,
        signing_key: &ClientSigningKey,
    ) -> anyhow::Result<LocalGroup<UserId>> {
        info!("Creating local connection group");
        let user_profile = UserProfile::load(&mut *txn, &self.payload)
            .await?
            .context("Can't find user profile for target user")?;
        let title = format!("Connection group: {}", user_profile.display_name);
        let attributes = ChatAttributes::new(title, None);

        let (group, partial_params) = self
            .create_connection_group_internal(&mut *txn, signing_key, attributes.title.clone())
            .await?;

        let Self {
            payload: user_id,
            group_id,
        } = self;

        // Create the connection chat
        let chat = Chat::new_targeted_message_chat(group_id.clone(), attributes, user_id.clone());
        chat.store(&mut *txn).await?;

        // Create the initial system message for the chat
        let system_message = SystemMessage::NewDirectConnectionChat(user_id.clone());
        let chat_message =
            ChatMessage::new_system_message(chat.id(), TimeStamp::now(), system_message);
        chat_message.store(txn).await?;

        Ok(LocalGroup {
            group,
            partial_params,
            chat_id: chat.id(),
            payload: user_id,
        })
    }
}

struct LocalGroup<Payload = ConnectionPackage> {
    group: Group,
    partial_params: PartialCreateGroupParams,
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
            sender_client_credential: key_store.signing_key.credential().clone(),
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
            username,
            chat_id,
            friendship_package_ear_key,
            connection_offer_hash,
        )
        .upsert(&mut *txn)
        .await?;

        let encrypted_user_profile_key =
            own_user_profile_key.encrypt(group.identity_link_wrapper_key(), own_user_id)?;
        let params = partial_params.into_params(own_client_reference, encrypted_user_profile_key);

        Ok(LocalUsernameContact::<UsernamePayload> {
            group,
            params,
            chat_id,
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
        let contact =
            TargetedMessageContact::new(user_id, chat_id, friendship_package_ear_key.clone());
        contact.upsert(&mut *txn).await?;

        let encrypted_user_profile_key =
            own_user_profile_key.encrypt(group.identity_link_wrapper_key(), own_user_id)?;
        let params = partial_params.into_params(own_client_reference, encrypted_user_profile_key);

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
    payload: Payload,
}

impl LocalUsernameContact<UsernamePayload> {
    async fn create_connection_group_via_username(
        self,
        client: &ApiClient,
        signer: &ClientSigningKey,
        responder: AsConnectionOfferResponder,
    ) -> anyhow::Result<ChatId> {
        let Self {
            group,
            params,
            chat_id,
            payload:
                UsernamePayload {
                    connection_offer,
                    verified_connection_package,
                },
        } = self;

        info!("Creating connection group on DS");
        client
            .ds_create_group(params, signer, group.group_state_ear_key())
            .await?;

        // Send off the connection offer.
        let hash = verified_connection_package.hash();
        let message = ConnectionOfferMessage::new(hash, connection_offer);
        responder.send(message).await?;

        Ok(chat_id)
    }
}

impl LocalUsernameContact<TargetedMessagePayload> {
    async fn create_connection_group_via_targeted_message(
        self,
        client: &ApiClient,
        signer: &ClientSigningKey,
    ) -> anyhow::Result<ChatId> {
        let Self {
            group,
            params,
            chat_id,
            payload:
                TargetedMessagePayload {
                    targeted_message_params,
                    targeted_group_state_ear_key,
                },
        } = self;

        info!("Creating connection group on DS");
        client
            .ds_create_group(params, signer, group.group_state_ear_key())
            .await?;

        // Send off the targeted message.
        // TODO: This should be scheduled in the outbound service
        client
            .ds_targeted_message(
                targeted_message_params,
                signer,
                &targeted_group_state_ear_key,
            )
            .await?;

        Ok(chat_id)
    }
}
