// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use airapiclient::{ApiClient, as_api::ConnectionOfferResponder};
use aircommon::{
    codec::PersistenceCodec,
    credentials::keys::ClientSigningKey,
    crypto::{
        ear::keys::{FriendshipPackageEarKey, GroupStateEarKey},
        hash::Hashable as _,
        hpke::HpkeEncryptable,
        indexed_aead::keys::UserProfileKey,
    },
    identifiers::{QsReference, UserHandle, UserId},
    messages::{
        client_as::{ConnectionOfferMessage, EncryptedConnectionOffer},
        client_ds_out::{CreateGroupParamsOut, TargetedMessageParamsOut},
        connection_package::ConnectionPackage,
    },
};
use anyhow::Context;
use openmls::group::GroupId;
use sqlx::SqliteTransaction;
use tracing::info;

use crate::{
    Chat, ChatAttributes, ChatId, UserProfile,
    clients::{
        connection_offer::{FriendshipPackage, payload::ConnectionInfo},
        targeted_message::TargetedMessageContent,
    },
    contacts::{HandleContact, TargetedMessageContact},
    groups::{Group, PartialCreateGroupParams, openmls_provider::AirOpenMlsProvider},
    key_stores::{MemoryUserKeyStore, indexed_keys::StorableIndexedKey},
    store::StoreNotifier,
    utils::connection_ext::StoreExt,
};

use super::{CoreUser, connection_offer::payload::ConnectionOfferPayload};

impl CoreUser {
    /// Create a connection via a user handle.
    pub(crate) async fn add_contact_via_handle(
        &self,
        handle: UserHandle,
    ) -> anyhow::Result<Option<ChatId>> {
        let client = self.api_client()?;

        // Phase 1: Fetch a connection package from the AS
        let (connection_package, connection_offer_responder) =
            match client.as_connect_handle(handle.clone()).await {
                Ok(res) => res,
                Err(error) if error.is_not_found() => {
                    return Ok(None);
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
        let group_id = client.ds_request_group_id().await?;
        let connection_package = VerifiedConnectionPackagesWithGroupId {
            payload: verified_connection_package,
            group_id,
        };

        let client_reference = self.create_own_client_reference();

        self.with_transaction_and_notifier(async |txn, notifier| {
            // Phase 4: Create a connection group
            let local_group = connection_package
                .create_local_connection_group(
                    txn,
                    notifier,
                    &self.inner.key_store.signing_key,
                    handle.clone(),
                )
                .await?;

            let local_partial_contact = local_group
                .create_handle_contact(
                    txn,
                    notifier,
                    &self.inner.key_store,
                    client_reference,
                    self.user_id(),
                    handle,
                )
                .await?;

            // Phase 5: Create the connection group on the DS and send off the connection offer
            let chat_id = local_partial_contact
                .create_connection_group_via_handle(
                    &client,
                    self.signing_key(),
                    connection_offer_responder,
                )
                .await?;

            Ok(Some(chat_id))
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

        // Phase 1: Prepare the connection locally
        let group_id = client.ds_request_group_id().await?;
        let connection_package = VerifiedConnectionPackagesWithGroupId {
            payload: user_id,
            group_id,
        };

        let client_reference = self.create_own_client_reference();

        self.with_transaction_and_notifier(async |txn, notifier| {
            // Phase 4: Create a connection group and prepare the targeted message
            let local_group = connection_package
                .create_local_connection_group(txn, notifier, &self.inner.key_store.signing_key)
                .await?;

            let local_partial_contact = local_group
                .create_targeted_message_contact(
                    txn,
                    notifier,
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
        txn: &mut sqlx::SqliteTransaction<'_>,
        signing_key: &ClientSigningKey,
        attributes: &ChatAttributes,
    ) -> anyhow::Result<(Group, PartialCreateGroupParams)> {
        let group_data = PersistenceCodec::to_vec(attributes)?.into();

        let provider = AirOpenMlsProvider::new(txn);
        let (group, group_membership, partial_params) =
            Group::create_group(&provider, signing_key, self.group_id.clone(), group_data)?;

        group.store(txn.as_mut()).await?;
        group_membership.store(txn.as_mut()).await?;

        Ok((group, partial_params))
    }
}

impl VerifiedConnectionPackagesWithGroupId<ConnectionPackage> {
    async fn create_local_connection_group(
        self,
        txn: &mut sqlx::SqliteTransaction<'_>,
        notifier: &mut StoreNotifier,
        signing_key: &ClientSigningKey,
        handle: UserHandle,
    ) -> anyhow::Result<LocalGroup<ConnectionPackage>> {
        info!("Creating local connection group");
        let title = format!("Connection group: {}", handle.plaintext());
        let attributes = ChatAttributes::new(title, None);

        let (group, partial_params) = self
            .create_connection_group_internal(txn, signing_key, &attributes)
            .await?;

        let Self {
            payload: method_payload,
            group_id,
        } = self;

        // Create the connection chat
        let chat = Chat::new_handle_chat(group_id.clone(), attributes, handle.clone());
        chat.store(txn.as_mut(), notifier).await?;

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
        txn: &mut sqlx::SqliteTransaction<'_>,
        notifier: &mut StoreNotifier,
        signing_key: &ClientSigningKey,
    ) -> anyhow::Result<LocalGroup<UserId>> {
        info!("Creating local connection group");
        let user_profile = UserProfile::load(txn.as_mut(), &self.payload)
            .await?
            .context("Can't find user profile for target user")?;
        let title = format!("Connection group: {}", user_profile.display_name);
        let attributes = ChatAttributes::new(title, None);

        let (group, partial_params) = self
            .create_connection_group_internal(txn, signing_key, &attributes)
            .await?;

        let Self {
            payload: user_id,
            group_id,
        } = self;

        // Create the connection chat
        let chat = Chat::new_targeted_message_chat(group_id.clone(), attributes, user_id.clone());
        chat.store(txn.as_mut(), notifier).await?;

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
    async fn create_handle_contact(
        self,
        txn: &mut SqliteTransaction<'_>,
        notifier: &mut StoreNotifier,
        key_store: &MemoryUserKeyStore,
        own_client_reference: QsReference,
        own_user_id: &UserId,
        handle: UserHandle,
    ) -> anyhow::Result<LocalHandleContact<HandlePayload>> {
        let Self {
            group,
            partial_params,
            chat_id,
            payload: verified_connection_package,
        } = self;

        let own_user_profile_key = UserProfileKey::load_own(txn.as_mut()).await?;

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
                handle.clone(),
                verified_connection_package.hash(),
            )?
            .encrypt(verified_connection_package.encryption_key(), &[], &[]);

        let connection_offer_hash = connection_offer.hash();

        group.store_connection_offer_psk(txn.as_mut(), connection_offer_hash)?;

        // Create and persist a new partial contact
        HandleContact::new(
            handle,
            chat_id,
            friendship_package_ear_key,
            connection_offer_hash,
        )
        .upsert(txn.as_mut(), notifier)
        .await?;

        let encrypted_user_profile_key =
            own_user_profile_key.encrypt(group.identity_link_wrapper_key(), own_user_id)?;
        let params = partial_params.into_params(own_client_reference, encrypted_user_profile_key);

        Ok(LocalHandleContact::<HandlePayload> {
            group,
            params,
            chat_id,
            payload: HandlePayload {
                connection_offer,
                verified_connection_package,
            },
        })
    }
}

impl LocalGroup<UserId> {
    async fn create_targeted_message_contact(
        self,
        txn: &mut SqliteTransaction<'_>,
        notifier: &mut StoreNotifier,
        key_store: &MemoryUserKeyStore,
        own_client_reference: QsReference,
        own_user_id: &UserId,
        targeted_message_chat_id: ChatId,
    ) -> anyhow::Result<LocalHandleContact<TargetedMessagePayload>> {
        let Self {
            group,
            partial_params,
            chat_id,
            payload: user_id,
        } = self;

        let own_user_profile_key = UserProfileKey::load_own(txn.as_mut()).await?;

        let friendship_package = FriendshipPackage {
            friendship_token: key_store.friendship_token.clone(),
            wai_ear_key: key_store.wai_ear_key.clone(),
            user_profile_base_secret: own_user_profile_key.base_secret().clone(),
        };

        let friendship_package_ear_key = FriendshipPackageEarKey::random()?;

        // Create and persist a new partial contact
        let contact =
            TargetedMessageContact::new(user_id, chat_id, friendship_package_ear_key.clone());
        contact.upsert(txn.as_mut(), notifier).await?;

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
        let provider = AirOpenMlsProvider::new(txn);
        let targeted_message_params = targeted_message_group.create_targeted_application_message(
            &provider,
            &key_store.signing_key,
            contact.user_id,
            TargetedMessageContent::ConnectionRequest(connection_info),
        )?;

        Ok(LocalHandleContact::<TargetedMessagePayload> {
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

struct HandlePayload {
    connection_offer: EncryptedConnectionOffer,
    verified_connection_package: ConnectionPackage,
}

struct LocalHandleContact<Payload = HandlePayload> {
    group: Group,
    params: CreateGroupParamsOut,
    chat_id: ChatId,
    payload: Payload,
}

impl LocalHandleContact<HandlePayload> {
    async fn create_connection_group_via_handle(
        self,
        client: &ApiClient,
        signer: &ClientSigningKey,
        responder: ConnectionOfferResponder,
    ) -> anyhow::Result<ChatId> {
        let Self {
            group,
            params,
            chat_id,
            payload:
                HandlePayload {
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

impl LocalHandleContact<TargetedMessagePayload> {
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
