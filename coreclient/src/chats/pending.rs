// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::{
    crypto::{ear::EarEncryptable, indexed_aead::keys::UserProfileKey},
    identifiers::{QualifiedGroupId, UserHandle},
    messages::{
        client_as::ConnectionOfferHash,
        client_ds::{AadMessage, AadPayload, JoinConnectionGroupParamsAad},
        connection_package::{ConnectionPackage, ConnectionPackageHash},
    },
    time::TimeStamp,
};
use anyhow::{Context, bail, ensure};
use mimi_room_policy::RoleIndex;
use tls_codec::DeserializeBytes;
use tracing::warn;

use crate::{
    Chat, ChatId, ChatType, PartialContact, SystemMessage,
    chats::messages::TimestampedMessage,
    clients::{
        CoreUser,
        connection_offer::{FriendshipPackage, payload::ConnectionInfo},
    },
    contacts::PartialContactType,
    groups::Group,
    key_stores::indexed_keys::StorableIndexedKey,
    user_handles::connection_packages::StorableConnectionPackage,
    utils::connection_ext::StoreExt,
};

pub(crate) struct PendingConnectionInfo {
    pub(crate) chat_id: ChatId,
    pub(crate) created_at: TimeStamp,
    pub(crate) connection_info: ConnectionInfo,
    pub(crate) handle: Option<UserHandle>,
    pub(crate) connection_offer_hash: Option<ConnectionOfferHash>,
    pub(crate) connection_package_hash: Option<ConnectionPackageHash>,
}

impl CoreUser {
    pub(crate) async fn accept_contact_request(&self, chat_id: ChatId) -> anyhow::Result<()> {
        // Load needed data
        let (chat, sender_user_id, pending_connection_info, partial_contact, own_user_profile_key) =
            self.with_transaction(async |txn| {
                let chat = Chat::load(txn.as_mut(), &chat_id)
                    .await?
                    .with_context(|| format!("Can't find chat with id {chat_id}"))?;
                let ChatType::PendingConnection(sender_user_id) = chat.chat_type() else {
                    bail!("Chat is not a pending connection");
                };
                let pending_connection_info = PendingConnectionInfo::load(txn.as_mut(), chat_id)
                    .await?
                    .with_context(|| {
                        format!("No pending connection info found for chat: {chat_id}")
                    })?;
                let own_user_profile_key = UserProfileKey::load_own(txn.as_mut()).await?;
                let sender_user_id = sender_user_id.clone();

                let partial_contact_type =
                    if let Some(handle) = pending_connection_info.handle.clone() {
                        PartialContactType::Handle(handle)
                    } else {
                        PartialContactType::TargetedMessage(sender_user_id.clone())
                    };
                let partial_contact = PartialContact::load(txn.as_mut(), &partial_contact_type)
                    .await?
                    .with_context(|| {
                        format!("No partial contact found for user: {sender_user_id:?}")
                    })?;

                Ok((
                    chat,
                    sender_user_id,
                    pending_connection_info,
                    partial_contact,
                    own_user_profile_key,
                ))
            })
            .await?;

        let PendingConnectionInfo {
            chat_id: _,
            created_at: _,
            connection_info,
            handle,
            connection_offer_hash,
            connection_package_hash,
        } = pending_connection_info;

        // Prepare group
        let (aad, qgid) = self.prepare_group(&connection_info, &own_user_profile_key)?;

        // Fetch external commit info
        let eci = self
            .api_clients()
            .get(qgid.owning_domain())?
            .ds_connection_group_info(
                connection_info.connection_group_id.clone(),
                &connection_info.connection_group_ear_key,
            )
            .await?;

        // Create a new group by joining it (if group already exists, it will be replaced)
        let (commit, group_info) = self
            .with_transaction_and_notifier(async |txn, notifier| {
                if Group::load_with_chat_id(txn.as_mut(), chat_id)
                    .await?
                    .is_some()
                {
                    warn!(%chat_id, "Group for pending chat already exists");
                    Group::delete_from_db(txn, chat.group_id()).await?;
                }

                // Join group
                let (mut group, commit, group_info, mut member_profile_info) =
                    Group::join_group_externally(
                        txn,
                        self.api_clients(),
                        eci,
                        self.signing_key(),
                        connection_info.connection_group_ear_key.clone(),
                        connection_info
                            .connection_group_identity_link_wrapper_key
                            .clone(),
                        aad,
                        connection_offer_hash,
                    )
                    .await?;

                // Verify that the group has only one other member and that it's
                // the sender of the CEP.
                let members = group.members(txn.as_mut()).await;

                ensure!(
                    members.len() == 2,
                    "Connection group has more than two members: {:?}",
                    members
                );

                ensure!(
                    members.contains(self.user_id()) && members.contains(&sender_user_id),
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
                self.fetch_and_store_user_profile(txn.as_mut(), notifier, contact_profile_info)
                    .await?;

                group.room_state_change_role(
                    &sender_user_id,
                    self.user_id(),
                    RoleIndex::Regular,
                )?;

                group.store_update(txn.as_mut()).await?;

                // Create system messages for acceptance
                let accepted_system_message = SystemMessage::AcceptedConnectionRequest {
                    contact: sender_user_id.clone(),
                    user_handle: handle.clone(),
                };
                let accepted_message =
                    TimestampedMessage::system_message(accepted_system_message, TimeStamp::now());
                let chat_messages = vec![accepted_message];
                Self::store_new_messages(txn.as_mut(), notifier, chat_id, chat_messages).await?;

                if let Some(hash) = connection_package_hash {
                    // Delete the connection package if it's not last resort
                    let is_last_resort =
                        <ConnectionPackage as StorableConnectionPackage>::is_last_resort(
                            txn, &hash,
                        )
                        .await?
                        .unwrap_or(false);
                    if !is_last_resort {
                        ConnectionPackage::delete(txn, &hash)
                            .await
                            .context("Failed to delete connection package")?;
                    }
                }

                Ok((commit, group_info))
            })
            .await?;

        // Send confirmation to DS
        let qs_client_reference = self.create_own_client_reference();
        self.api_clients()
            .get(qgid.owning_domain())?
            .ds_join_connection_group(
                commit,
                group_info,
                qs_client_reference,
                &connection_info.connection_group_ear_key,
            )
            .await?;

        // Mark the chat as an accepted connection and mark partial contact as complete, also
        // remove the pending connection info.
        self.with_transaction_and_notifier(async |txn, notifier| {
            chat.set_chat_type(
                txn.as_mut(),
                notifier,
                &ChatType::Connection(sender_user_id.clone()),
            )
            .await?;
            partial_contact
                .mark_as_complete(
                    txn,
                    notifier,
                    sender_user_id,
                    connection_info.friendship_package,
                )
                .await?;
            PendingConnectionInfo::delete(txn.as_mut(), chat_id).await?;
            Ok(())
        })
        .await?;

        Ok(())
    }

    fn prepare_group(
        &self,
        connection_info: &ConnectionInfo,
        own_user_profile_key: &UserProfileKey,
    ) -> anyhow::Result<(AadMessage, QualifiedGroupId)> {
        // We create a new group and signal that fact to the user,
        // so the user can decide if they want to accept the
        // connection.

        let encrypted_user_profile_key = own_user_profile_key.encrypt(
            &connection_info.connection_group_identity_link_wrapper_key,
            self.user_id(),
        )?;

        let encrypted_friendship_package = FriendshipPackage {
            friendship_token: self.key_store().friendship_token.clone(),
            wai_ear_key: self.key_store().wai_ear_key.clone(),
            user_profile_base_secret: own_user_profile_key.base_secret().clone(),
        }
        .encrypt(&connection_info.friendship_package_ear_key)?;

        let aad: AadMessage = AadPayload::JoinConnectionGroup(JoinConnectionGroupParamsAad {
            encrypted_friendship_package,
            encrypted_user_profile_key,
        })
        .into();
        let qgid = QualifiedGroupId::tls_deserialize_exact_bytes(
            connection_info.connection_group_id.as_slice(),
        )?;

        Ok((aad, qgid))
    }
}

mod persistence {
    use sqlx::{SqliteExecutor, query, query_as};

    use crate::store::StoreNotifier;

    use super::*;

    impl PendingConnectionInfo {
        pub(super) async fn load(
            executor: impl SqliteExecutor<'_>,
            chat_id: ChatId,
        ) -> sqlx::Result<Option<PendingConnectionInfo>> {
            query_as!(
                PendingConnectionInfo,
                r#"SELECT
                    chat_id AS "chat_id: ChatId",
                    created_at AS "created_at: TimeStamp",
                    connection_info AS "connection_info: ConnectionInfo",
                    handle AS "handle: _",
                    connection_offer_hash AS "connection_offer_hash: _",
                    connection_package_hash AS "connection_package_hash: _"
                FROM pending_connection_info
                WHERE chat_id = ?"#,
                chat_id,
            )
            .fetch_optional(executor)
            .await
        }

        pub(crate) async fn store(
            &self,
            executor: impl SqliteExecutor<'_>,
            notifier: &mut StoreNotifier,
        ) -> sqlx::Result<()> {
            query!(
                "INSERT OR REPLACE INTO pending_connection_info (
                    chat_id,
                    created_at,
                    connection_info,
                    handle,
                    connection_offer_hash,
                    connection_package_hash
                )
                VALUES (?,  ?, ?, ?, ?, ?)",
                self.chat_id,
                self.created_at,
                self.connection_info,
                self.handle,
                self.connection_offer_hash,
                self.connection_package_hash,
            )
            .execute(executor)
            .await?;
            notifier.update(self.chat_id);
            Ok(())
        }

        pub(super) async fn delete(
            executor: impl SqliteExecutor<'_>,
            chat_id: ChatId,
        ) -> sqlx::Result<()> {
            query!(
                "DELETE FROM pending_connection_info WHERE chat_id = ?",
                chat_id
            )
            .execute(executor)
            .await?;
            Ok(())
        }
    }
}
