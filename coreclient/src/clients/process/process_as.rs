// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::{
    crypto::{hpke::HpkeDecryptable, indexed_aead::keys::UserProfileKey},
    identifiers::{QualifiedGroupId, UserHandle},
    messages::{
        client_as::{ConnectionOfferHash, ConnectionOfferMessage},
        client_ds::{AadMessage, AadPayload, JoinConnectionGroupParamsAad},
        client_ds_out::ExternalCommitInfoIn,
        connection_package::{ConnectionPackage, ConnectionPackageHash},
    },
};
use airprotos::auth_service::v1::{HandleQueueMessage, handle_queue_message};
use anyhow::{Context, Result, bail, ensure};
use mimi_room_policy::RoleIndex;
use openmls::prelude::MlsMessageOut;
use sqlx::SqliteConnection;
use sqlx::SqliteTransaction;
use tls_codec::DeserializeBytes;
use tracing::error;

use crate::{
    clients::{
        block_contact::{BlockedContact, BlockedContactError},
        connection_offer::{ConnectionOfferIn, payload::ConnectionOfferPayload},
    },
    groups::{Group, ProfileInfo},
    key_stores::indexed_keys::StorableIndexedKey,
    store::StoreNotifier,
    user_handles::connection_packages::StorableConnectionPackage,
    utils::connection_ext::{ConnectionExt, StoreExt},
};

use super::{
    AsCredentials, Chat, ChatAttributes, ChatId, Contact, CoreUser, EarEncryptable,
    FriendshipPackage, anyhow,
};

impl CoreUser {
    /// Process a queue message received from the AS handle queue.
    ///
    /// Returns the [`ChatId`] of any newly created chat.
    pub async fn process_handle_queue_message(
        &self,
        user_handle: &UserHandle,
        handle_queue_message: HandleQueueMessage,
    ) -> Result<ChatId> {
        let payload = handle_queue_message
            .payload
            .context("no payload in handle queue message")?;
        match payload {
            handle_queue_message::Payload::ConnectionOffer(eco) => {
                self.process_connection_offer(user_handle.clone(), eco.try_into()?)
                    .await
            }
        }
    }

    async fn process_connection_offer(
        &self,
        handle: UserHandle,
        ecep: ConnectionOfferMessage,
    ) -> Result<ChatId> {
        let mut connection = self.pool().acquire().await?;

        let connection_offer_hash = ecep.connection_offer_hash();

        // Parse & verify connection offer
        let (cep_payload, hash) = self
            .parse_and_verify_connection_offer(&mut connection, ecep, handle.clone())
            .await?;

        // Deny connection from blocked users
        let user_id = cep_payload.sender_client_credential.identity();
        if BlockedContact::check_blocked(&mut *connection, user_id).await? {
            bail!(BlockedContactError);
        }

        // Prepare group
        let own_user_profile_key = UserProfileKey::load_own(&mut *connection).await?;
        let (aad, qgid) = self.prepare_group(&cep_payload, &own_user_profile_key)?;

        // Fetch external commit info
        let eci = self.fetch_external_commit_info(&cep_payload, &qgid).await?;

        // Join group
        let (mut group, commit, group_info, mut member_profile_info) = self
            .join_group_externally(
                &mut connection,
                eci,
                &cep_payload,
                aad,
                connection_offer_hash,
            )
            .await?;

        // Verify that the group has only one other member and that it's
        // the sender of the CEP.
        let members = group.members(&mut *connection).await;

        ensure!(
            members.len() == 2,
            "Connection group has more than two members: {:?}",
            members
        );

        ensure!(
            members.contains(self.user_id())
                && members.contains(cep_payload.sender_client_credential.identity()),
            "Connection group has unexpected members: {:?}",
            members
        );

        // There should be only one user profile
        let contact_profile_info = member_profile_info
            .pop()
            .context("No user profile returned when joining connection group")?;

        debug_assert!(
            member_profile_info.is_empty(),
            "More than one user profile returned when joining connection group"
        );

        // Fetch and store user profile

        self.with_notifier(async |notifier| {
            self.fetch_and_store_user_profile(&mut connection, notifier, contact_profile_info)
                .await
        })
        .await?;

        // Create chat
        // Note: For now, the chat is immediately confirmed.
        let (mut chat, contact) = self
            .create_connection_chat(&mut connection, &group, &cep_payload)
            .await?;

        group.room_state_change_role(
            cep_payload.sender_client_credential.identity(),
            self.user_id(),
            RoleIndex::Regular,
        )?;

        let mut notifier = self.store_notifier();

        // Store group, chat & contact
        connection
            .with_transaction(async |txn| {
                self.store_group_chat_contact(txn, &mut notifier, &group, &mut chat, contact)
                    .await
            })
            .await?;

        // Send confirmation
        self.send_confirmation_to_ds(commit, group_info, &cep_payload, qgid)
            .await?;

        // Delete the connection package if it's not last resort
        connection
            .with_transaction(async |txn| {
                let is_last_resort =
                    <ConnectionPackage as StorableConnectionPackage>::is_last_resort(txn, &hash)
                        .await?
                        .unwrap_or(false);
                if !is_last_resort {
                    ConnectionPackage::delete(txn, &hash)
                        .await
                        .context("Failed to delete connection package")?;
                }
                Ok(())
            })
            .await?;

        notifier.notify();

        // Return the chat ID
        Ok(chat.id())
    }

    /// Parse and verify the connection offer
    async fn parse_and_verify_connection_offer(
        &self,
        connection: &mut SqliteConnection,
        com: ConnectionOfferMessage,
        user_handle: UserHandle,
    ) -> Result<(ConnectionOfferPayload, ConnectionPackageHash)> {
        let (eco, hash) = com.into_parts();

        let decryption_key = ConnectionPackage::load_decryption_key(connection, &hash)
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

    fn prepare_group(
        &self,
        cep_payload: &ConnectionOfferPayload,
        own_user_profile_key: &UserProfileKey,
    ) -> Result<(AadMessage, QualifiedGroupId)> {
        // We create a new group and signal that fact to the user,
        // so the user can decide if they want to accept the
        // connection.

        let encrypted_user_profile_key = own_user_profile_key.encrypt(
            &cep_payload.connection_group_identity_link_wrapper_key,
            self.user_id(),
        )?;

        let encrypted_friendship_package = FriendshipPackage {
            friendship_token: self.inner.key_store.friendship_token.clone(),
            wai_ear_key: self.inner.key_store.wai_ear_key.clone(),
            user_profile_base_secret: own_user_profile_key.base_secret().clone(),
        }
        .encrypt(&cep_payload.friendship_package_ear_key)?;

        let aad: AadMessage = AadPayload::JoinConnectionGroup(JoinConnectionGroupParamsAad {
            encrypted_friendship_package,
            encrypted_user_profile_key,
        })
        .into();
        let qgid = QualifiedGroupId::tls_deserialize_exact_bytes(
            cep_payload.connection_group_id.as_slice(),
        )?;

        Ok((aad, qgid))
    }

    async fn fetch_external_commit_info(
        &self,
        cep_payload: &ConnectionOfferPayload,
        qgid: &QualifiedGroupId,
    ) -> Result<ExternalCommitInfoIn> {
        Ok(self
            .inner
            .api_clients
            .get(qgid.owning_domain())?
            .ds_connection_group_info(
                cep_payload.connection_group_id.clone(),
                &cep_payload.connection_group_ear_key, //
            )
            .await?)
    }

    async fn join_group_externally(
        &self,
        connection: &mut SqliteConnection,
        eci: ExternalCommitInfoIn,
        cep_payload: &ConnectionOfferPayload,
        aad: AadMessage,
        connection_offer_hash: ConnectionOfferHash,
    ) -> Result<(Group, MlsMessageOut, MlsMessageOut, Vec<ProfileInfo>)> {
        let (group, commit, group_info, member_profile_info) = Group::join_group_externally(
            &mut *connection,
            &self.inner.api_clients,
            eci,
            self.signing_key(),
            cep_payload.connection_group_ear_key.clone(),
            cep_payload
                .connection_group_identity_link_wrapper_key
                .clone(),
            aad,
            Some(connection_offer_hash),
        )
        .await?;
        Ok((group, commit, group_info, member_profile_info))
    }

    async fn create_connection_chat(
        &self,
        connection: &mut SqliteConnection,
        group: &Group,
        cep_payload: &ConnectionOfferPayload,
    ) -> Result<(Chat, Contact)> {
        let sender_user_id = cep_payload.sender_client_credential.identity();

        let display_name = self
            .user_profile_internal(connection, sender_user_id)
            .await
            .display_name;

        let chat = Chat::new_connection_chat(
            group.group_id().clone(),
            sender_user_id.clone(),
            ChatAttributes::new(display_name.to_string(), None),
        )?;
        let contact = Contact::from_friendship_package(
            sender_user_id.clone(),
            chat.id(),
            cep_payload.friendship_package.clone(),
        )?;
        Ok((chat, contact))
    }

    async fn store_group_chat_contact(
        &self,
        txn: &mut SqliteTransaction<'_>,
        notifier: &mut StoreNotifier,
        group: &Group,
        chat: &mut Chat,
        contact: Contact,
    ) -> Result<()> {
        group.store(txn.as_mut()).await?;
        chat.store(txn.as_mut(), notifier).await?;
        contact.upsert(txn.as_mut(), notifier).await?;
        Ok(())
    }

    async fn send_confirmation_to_ds(
        &self,
        commit: MlsMessageOut,
        group_info: MlsMessageOut,
        cep_payload: &ConnectionOfferPayload,
        qgid: QualifiedGroupId,
    ) -> Result<()> {
        let qs_client_reference = self.create_own_client_reference();
        self.inner
            .api_clients
            .get(qgid.owning_domain())?
            .ds_join_connection_group(
                commit,
                group_info,
                qs_client_reference,
                &cep_payload.connection_group_ear_key,
            )
            .await?;
        Ok(())
    }
}
