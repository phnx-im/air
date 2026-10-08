// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Following a sibling emulator client into a group it created or externally
//! joined.
//!
//! The DS echoes the group bootstrap blob of an accepted creation or join to
//! all of the acting user's client queues. This module turns such an echo into
//! the same local state the acting client built.

use aircommon::{
    codec::PersistenceCodec,
    crypto::indexed_aead::keys::UserProfileKey,
    identifiers::{QualifiedGroupId, UserId},
    messages::{client_as::ConnectionOfferHash, client_ds::GroupBootstrapEcho},
    time::TimeStamp,
};
use airprotos::client::group_bootstrap::{GroupBootstrapBlob, GroupBootstrapCarrier};
use anyhow::{Context, Result, bail, ensure};
use mimi_room_policy::RoleIndex;
use tracing::debug;

use crate::{
    Chat, ChatId, ChatMessage, ChatStatus, Contact, SystemMessage, TargetedMessageContact,
    chats::{PendingConnectionRequest, connection_requests},
    clients::CoreUser,
    contacts::UsernameContact,
    db::access::WriteDbTransaction,
    groups::{
        Group,
        client_auth_info::StorableUserCredential,
        group_bootstrap::{BootstrapConnection, GroupBootstrapContents},
        self_group::SelfGroup,
    },
};

use super::process_qs::QsMessageOutcome;

impl CoreUser {
    pub(super) async fn handle_group_bootstrap_echo(
        &self,
        txn: &mut WriteDbTransaction<'_>,
        echo: GroupBootstrapEcho,
        carrier: GroupBootstrapCarrier,
        ds_timestamp: TimeStamp,
    ) -> Result<QsMessageOutcome> {
        // A duplicate echo must not reinstall the group.
        if Group::load(&mut *txn, &echo.group_id).await?.is_some() {
            debug!(group_id = ?echo.group_id, "Group bootstrap for a group we already have");
            return Ok(QsMessageOutcome::empty());
        }

        let self_group = SelfGroup::load(&mut *txn)
            .await?
            .context("no self group to open the group bootstrap with")?;

        let blob: GroupBootstrapBlob = PersistenceCodec::from_slice(&echo.group_bootstrap)
            .context("failed to decode the group bootstrap blob")?;
        let (bootstrap, epoch_id) =
            self_group
                .group()
                .open_group_bootstrap(txn, &blob, carrier, &echo.group_id)?;
        let contents = GroupBootstrapContents::try_from(bootstrap)?;
        ensure!(
            contents.group_id == echo.group_id && contents.pq_group_id == echo.pq_group_id,
            "group bootstrap and echo disagree on the group ids"
        );

        let connection_offer_hash = connection_offer_psk_for(carrier, &contents)?;
        let qgid = QualifiedGroupId::try_from(echo.group_id.clone())?;
        let snapshot = self
            .api_clients()
            .get(qgid.owning_domain())?
            .ds_epoch_snapshot(
                echo.group_id.clone(),
                echo.epoch,
                &contents.group_state_ear_key,
            )
            .await
            .context("failed to fetch the epoch snapshot")?;

        let mut group = match carrier {
            GroupBootstrapCarrier::CreationEcho => {
                Box::pin(Group::vc_join_at_creation(
                    txn,
                    self.api_clients(),
                    snapshot,
                    &contents,
                    epoch_id,
                    self.user_id(),
                ))
                .await?
            }
            GroupBootstrapCarrier::JoinEcho => {
                Box::pin(Group::vc_join_via_sibling_external_commit(
                    txn,
                    self.api_clients(),
                    snapshot,
                    &contents,
                    epoch_id,
                    self.user_id(),
                    connection_offer_hash,
                ))
                .await?
            }
        };

        let settled_chat = match &contents.connection {
            None => {
                self.install_bootstrapped_group_chat(txn, &group, ds_timestamp)
                    .await?;
                None
            }
            Some(connection) => {
                Box::pin(self.install_bootstrapped_connection_chat(
                    txn,
                    &mut group,
                    connection,
                    ds_timestamp,
                ))
                .await?
            }
        };

        Ok(QsMessageOutcome::stale_chats(settled_chat))
    }

    /// Creates the chat of a group chat a sibling created, taking the
    /// attributes from the group data extension the snapshot carried.
    async fn install_bootstrapped_group_chat(
        &self,
        txn: &mut WriteDbTransaction<'_>,
        group: &Group,
        ds_timestamp: TimeStamp,
    ) -> Result<()> {
        ensure_only_member(group, self.user_id())?;

        let attributes =
            Self::chat_attributes_from_group_data(txn, group, self.user_id(), ds_timestamp).await?;

        let chat = Chat::new_group_chat(group.group_id().clone(), attributes);
        chat.store(&mut *txn).await?;
        ChatMessage::new_system_message(
            chat.id(),
            ds_timestamp,
            SystemMessage::CreateGroup(self.user_id().clone()),
        )
        .store(&mut *txn)
        .await?;

        Ok(())
    }

    /// Creates the chat and contact rows of a connection chat a sibling created
    /// or accepted.
    ///
    /// Returns the chat that showed the pending requests that was accepted. Its
    /// notifications are stale.
    async fn install_bootstrapped_connection_chat(
        &self,
        txn: &mut WriteDbTransaction<'_>,
        group: &mut Group,
        connection: &BootstrapConnection,
        ds_timestamp: TimeStamp,
    ) -> Result<Option<ChatId>> {
        let settled_chat = match connection {
            BootstrapConnection::HandleInitiator {
                username,
                friendship_package_ear_key,
                connection_offer_hash,
            } => {
                ensure_only_member(group, self.user_id())?;

                let chat = Chat::new_handle_chat(group.group_id().clone(), username.clone());
                chat.store(&mut *txn).await?;
                ChatMessage::new_system_message(
                    chat.id(),
                    ds_timestamp,
                    SystemMessage::NewHandleConnectionChat(username.clone()),
                )
                .store(&mut *txn)
                .await?;

                UsernameContact::new(
                    username.clone(),
                    chat.id(),
                    friendship_package_ear_key.clone(),
                    *connection_offer_hash,
                )
                .upsert(&mut *txn)
                .await?;
                // The peer's external commit will reference this PSK.
                group.store_connection_offer_psk(&mut *txn, *connection_offer_hash)?;
                None
            }

            BootstrapConnection::TargetedInitiator {
                user_id,
                friendship_package_ear_key,
            } => {
                ensure_only_member(group, self.user_id())?;

                let chat =
                    Chat::new_targeted_message_chat(group.group_id().clone(), user_id.clone());
                chat.store(&mut *txn).await?;
                ChatMessage::new_system_message(
                    chat.id(),
                    ds_timestamp,
                    SystemMessage::NewDirectConnectionChat(user_id.clone()),
                )
                .store(&mut *txn)
                .await?;

                TargetedMessageContact::new(
                    user_id.clone(),
                    chat.id(),
                    friendship_package_ear_key.clone(),
                )
                .upsert(&mut *txn)
                .await?;
                None
            }

            BootstrapConnection::Accept {
                user_id,
                friendship_package,
                connection_offer_hash,
            } => {
                let members: Vec<_> = group.members().collect();
                ensure!(
                    members.len() == 2
                        && members.contains(self.user_id())
                        && members.contains(user_id),
                    "connection group has unexpected members: {members:?}"
                );

                let user_profile_key = UserProfileKey::from_base_secret(
                    friendship_package.user_profile_base_secret.clone(),
                    user_id,
                )?;
                let credential = StorableUserCredential::load_by_user_id(&mut *txn, user_id)
                    .await?
                    .with_context(|| format!("no verified credential for {user_id:?}"))?;
                Self::schedule_fetch_user_profile(&mut *txn, (credential.into(), user_profile_key))
                    .await?;

                // The DS admits the joiner to its room state before it takes
                // the snapshot, so a well-formed snapshot already lists us.
                // Patch only one that does not, it would fail the member check
                // on the next commit.
                if group.room_state_role(self.user_id())?.is_none() {
                    group.room_state_change_role(user_id, self.user_id(), RoleIndex::Regular)?;
                    let now = TimeStamp::now();
                    group.store_update(&mut *txn, Some(now), Some(now)).await?;
                }

                let chat =
                    Chat::new_onboarding_connection_chat(group.group_id().clone(), user_id.clone());

                // This client may already hold pending requests of the
                // sender, forwarded by a sibling or from a targeted message,
                // which reaches every sibling. They are shown in another chat
                // if the two devices disagree on the newest request. The
                // username an offer went to is only in that pending state.
                let request_id = chat.id();
                let accepted = PendingConnectionRequest::load(&mut *txn, request_id).await?;
                let user_handle = accepted.and_then(|request| request.username);
                let host = PendingConnectionRequest::chat_of_sender(&mut *txn, user_id).await?;
                if let Some(host) = host {
                    connection_requests::settle_accepted(txn, host).await?;
                }

                chat.store(&mut *txn).await?;
                Chat::update_status(&mut *txn, chat.id(), &ChatStatus::Active).await?;
                if let Some(host) = host
                    && host != chat.id()
                    && let Some(host_chat) = Chat::load(&mut *txn, &host).await?
                {
                    ChatMessage::move_to_chat(&mut *txn, host, chat.id()).await?;
                    Group::delete_from_db(txn, host_chat.group_id()).await?;
                    Chat::delete(&mut *txn, host).await?;
                }
                ChatMessage::new_system_message(
                    chat.id(),
                    ds_timestamp,
                    SystemMessage::AcceptedConnectionRequest {
                        contact: user_id.clone(),
                        user_handle,
                    },
                )
                .store(&mut *txn)
                .await?;

                Contact {
                    user_id: user_id.clone(),
                    wai_ear_key: friendship_package.wai_ear_key.clone(),
                    friendship_token: friendship_package.friendship_token.clone(),
                    chat_id: chat.id(),
                    supported_features: None,
                }
                .upsert(&mut *txn)
                .await?;

                if let Some(hash) = connection_offer_hash {
                    Group::delete_connection_offer_psk(txn, *hash)?;
                }
                host
            }
        };

        Ok(settled_chat)
    }
}

/// Checks that the blob's connection context matches the operation the echo
/// announces, and returns the connection-offer PSK a join has to install
/// before it applies the commit.
fn connection_offer_psk_for(
    carrier: GroupBootstrapCarrier,
    contents: &GroupBootstrapContents,
) -> Result<Option<ConnectionOfferHash>> {
    match (carrier, &contents.connection) {
        (
            GroupBootstrapCarrier::JoinEcho,
            Some(BootstrapConnection::Accept {
                connection_offer_hash,
                ..
            }),
        ) => Ok(*connection_offer_hash),
        (GroupBootstrapCarrier::JoinEcho, _) => {
            bail!("a join echo must carry an accept connection context")
        }
        (GroupBootstrapCarrier::CreationEcho, Some(BootstrapConnection::Accept { .. })) => {
            bail!("a creation echo must not carry an accept connection context")
        }
        (GroupBootstrapCarrier::CreationEcho, _) => Ok(None),
    }
}

/// A group a sibling just created holds only the virtual client's own leaf.
fn ensure_only_member(group: &Group, own_user_id: &UserId) -> Result<()> {
    let members: Vec<_> = group.members().collect();
    ensure!(
        members.len() == 1 && members.contains(own_user_id),
        "a freshly created group has members other than us: {members:?}"
    );
    Ok(())
}
