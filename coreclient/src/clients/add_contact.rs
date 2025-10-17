// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use airapiclient::{ApiClient, as_api::ConnectionOfferResponder};
use aircommon::{
    codec::PersistenceCodec,
    credentials::keys::ClientSigningKey,
    crypto::{
        ear::keys::FriendshipPackageEarKey, hash::Hashable as _, hpke::HpkeEncryptable,
        indexed_aead::keys::UserProfileKey,
    },
    identifiers::{QsReference, UserHandle, UserId},
    messages::{
        client_as::{ConnectionOfferMessage, EncryptedConnectionOffer},
        client_ds_out::CreateGroupParamsOut,
        connection_package::ConnectionPackage,
    },
};
use openmls::group::GroupId;
use sqlx::SqliteTransaction;
use tracing::info;

use crate::{
    Chat, ChatAttributes, ChatId,
    clients::connection_offer::FriendshipPackage,
    contacts::HandleContact,
    groups::{Group, PartialCreateGroupParams, openmls_provider::AirOpenMlsProvider},
    key_stores::{MemoryUserKeyStore, indexed_keys::StorableIndexedKey},
    store::StoreNotifier,
    utils::connection_ext::StoreExt,
};

use super::{CoreUser, connection_offer::payload::ConnectionOfferPayload};

impl CoreUser {
    /// Create a connection with via a user handle.
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
            verified_connection_package,
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
}

struct VerifiedConnectionPackagesWithGroupId {
    verified_connection_package: ConnectionPackage,
    group_id: GroupId,
}

impl VerifiedConnectionPackagesWithGroupId {
    async fn create_local_connection_group(
        self,
        txn: &mut sqlx::SqliteTransaction<'_>,
        notifier: &mut StoreNotifier,
        signing_key: &ClientSigningKey,
        handle: UserHandle,
    ) -> anyhow::Result<LocalGroup> {
        let Self {
            verified_connection_package,
            group_id,
        } = self;

        info!("Creating local connection group");
        let title = format!("Connection group: {}", handle.plaintext());
        let attributes = ChatAttributes::new(title, None);
        let group_data = PersistenceCodec::to_vec(&attributes)?.into();

        let provider = AirOpenMlsProvider::new(txn);
        let (group, group_membership, partial_params) =
            Group::create_group(&provider, signing_key, group_id.clone(), group_data)?;

        group_membership.store(txn.as_mut()).await?;
        group.store(txn.as_mut()).await?;

        // TODO: Once we allow multi-client, invite all our other clients to the
        // connection group.

        // Create the connection chat
        let chat = Chat::new_handle_chat(group_id.clone(), attributes, handle.clone());
        chat.store(txn.as_mut(), notifier).await?;

        Ok(LocalGroup {
            group,
            partial_params,
            chat_id: chat.id(),
            verified_connection_package,
        })
    }
}

struct LocalGroup {
    group: Group,
    partial_params: PartialCreateGroupParams,
    chat_id: ChatId,
    verified_connection_package: ConnectionPackage,
}

impl LocalGroup {
    async fn create_handle_contact(
        self,
        txn: &mut SqliteTransaction<'_>,
        notifier: &mut StoreNotifier,
        key_store: &MemoryUserKeyStore,
        own_client_reference: QsReference,
        own_user_id: &UserId,
        handle: UserHandle,
    ) -> anyhow::Result<LocalHandleContact> {
        let Self {
            group,
            partial_params,
            chat_id,
            verified_connection_package,
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
            connection_group_id: group.group_id().clone(),
            connection_group_ear_key: group.group_state_ear_key().clone(),
            connection_group_identity_link_wrapper_key: group.identity_link_wrapper_key().clone(),
            friendship_package_ear_key: friendship_package_ear_key.clone(),
            friendship_package,
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

        Ok(LocalHandleContact {
            group,
            connection_offer,
            params,
            chat_id,
            verified_connection_package,
        })
    }
}

struct LocalHandleContact {
    group: Group,
    connection_offer: EncryptedConnectionOffer,
    params: CreateGroupParamsOut,
    chat_id: ChatId,
    verified_connection_package: ConnectionPackage,
}

impl LocalHandleContact {
    async fn create_connection_group_via_handle(
        self,
        client: &ApiClient,
        signer: &ClientSigningKey,
        responder: ConnectionOfferResponder,
    ) -> anyhow::Result<ChatId> {
        let Self {
            group,
            connection_offer,
            params,
            chat_id,
            verified_connection_package,
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
