// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::identifiers::UserId;
use invite_users_flow::InviteUsersData;

use crate::{
    ChatId, ChatMessage,
    utils::connection_ext::{ConnectionExt as _, StoreExt},
};

use super::CoreUser;

impl CoreUser {
    /// Invite users to an existing chat.
    ///
    /// Since this function causes the creation of an MLS commit, it can cause
    /// more than one effect on the group. As a result this function returns a
    /// vector of [`ChatMessage`]s that represents the changes to the
    /// group. Note that these returned message have already been persisted.
    pub(crate) async fn invite_users(
        &self,
        chat_id: ChatId,
        invited_users: &[UserId],
    ) -> anyhow::Result<Vec<ChatMessage>> {
        let mut connection = self.pool().acquire().await?;

        // Phase 1: Load all the relevant chat and all the contacts we
        // want to add.
        let invite_prepared = InviteUsersData::load(&mut connection, chat_id, invited_users)
            .await?
            // Phase 2: Load add infos for each contact
            // This needs the connection load (and potentially fetch and store).
            .load_add_infos(&mut connection, &self.inner.api_clients)
            .await?;

        // Phase 3: Load the group and create the commit to add the new members
        let invite = invite_prepared
            .stage_invite(self.user_id(), &mut connection, &self.inner.key_store)
            .await?;

        // Phase 4: Send the commit to the DS
        // The DS responds with the timestamp of the commit.
        let invited = invite
            .ds_group_operation(&self.inner.api_clients, self.signing_key())
            .await?;

        // Phase 5: Merge the commit into the group
        // Now that we know the commit went through, we can merge the commit
        let messages = connection
            .with_transaction(async |txn| {
                self.with_notifier(async |notifier| {
                    invited.merge_pending_commit(txn, notifier, chat_id).await
                })
                .await
            })
            .await?;

        Ok(messages)
    }
}

mod invite_users_flow {
    use aircommon::{
        credentials::{ClientCredential, keys::ClientSigningKey},
        crypto::ear::keys::WelcomeAttributionInfoEarKey,
        identifiers::{Fqdn, UserId},
        messages::client_ds_out::GroupOperationParamsOut,
        time::TimeStamp,
    };
    use anyhow::Context;
    use mimi_room_policy::RoleIndex;
    use openmls::group::GroupId;
    use sqlx::SqliteConnection;

    use crate::{
        Chat, ChatId, ChatMessage, Contact,
        clients::{CoreUser, api_clients::ApiClients},
        contacts::ContactAddInfos,
        groups::{Group, client_auth_info::StorableClientCredential},
        key_stores::MemoryUserKeyStore,
        store::StoreNotifier,
        utils::connection_ext::ConnectionExt,
    };

    pub(super) struct InviteUsersData<S> {
        group_id: GroupId,
        invited_users: Vec<UserId>,
        owner_domain: Fqdn,
        contact_wai_keys: Vec<WelcomeAttributionInfoEarKey>,
        client_credentials: Vec<ClientCredential>,
        state: S,
    }

    impl InviteUsersData<()> {
        pub(super) async fn load(
            connection: &mut SqliteConnection,
            chat_id: ChatId,
            invited_users: &[UserId],
        ) -> anyhow::Result<InviteUsersData<Vec<Contact>>> {
            let chat = Chat::load(&mut *connection, &chat_id)
                .await?
                .with_context(|| format!("Can't find chat with id {chat_id}"))?;

            let mut contact_wai_keys = Vec::with_capacity(invited_users.len());
            let mut contacts = Vec::with_capacity(invited_users.len());
            let mut client_credentials = Vec::with_capacity(invited_users.len());

            for invited_user in invited_users {
                // Get the WAI keys and client credentials for the invited users.
                let contact = Contact::load(&mut *connection, invited_user)
                    .await?
                    .with_context(|| format!("Can't find contact {invited_user:?}"))?;
                contact_wai_keys.push(contact.wai_ear_key().clone());

                if let Some(client_credential) =
                    StorableClientCredential::load_by_user_id(&mut *connection, invited_user)
                        .await?
                {
                    client_credentials.push(ClientCredential::from(client_credential));
                }

                contacts.push(contact);
            }

            Ok(InviteUsersData {
                group_id: chat.group_id().clone(),
                invited_users: invited_users.to_vec(),
                owner_domain: chat.owner_domain(),
                contact_wai_keys,
                client_credentials,
                state: contacts,
            })
        }
    }

    impl InviteUsersData<Vec<Contact>> {
        pub(super) async fn load_add_infos(
            self,
            connection: &mut SqliteConnection,
            api_clients: &ApiClients,
        ) -> anyhow::Result<InviteUsersData<Vec<ContactAddInfos>>> {
            let Self {
                group_id,
                invited_users,
                owner_domain,
                contact_wai_keys,
                client_credentials,
                state: contacts,
            } = self;

            let mut contact_add_infos: Vec<ContactAddInfos> = Vec::with_capacity(contacts.len());
            for contact in contacts {
                let add_info = contact.fetch_add_infos(connection, api_clients).await?;
                contact_add_infos.push(add_info);
            }

            Ok(InviteUsersData {
                group_id,
                invited_users,
                owner_domain,
                contact_wai_keys,
                client_credentials,
                state: contact_add_infos,
            })
        }
    }

    impl InviteUsersData<Vec<ContactAddInfos>> {
        pub(super) async fn stage_invite(
            self,
            sender_id: &UserId,
            connection: &mut SqliteConnection,
            key_store: &MemoryUserKeyStore,
        ) -> anyhow::Result<InviteUsersParams> {
            let Self {
                group_id,
                invited_users,
                owner_domain,
                contact_wai_keys,
                client_credentials,
                state: contact_add_infos,
            } = self;

            let (group, params) = connection
                .with_transaction(async |txn| {
                    let mut group = Group::load_clean(txn, &group_id)
                        .await?
                        .with_context(|| format!("Can't find group with id {group_id:?}"))?;

                    // Room policy check
                    for target in &invited_users {
                        group.room_state_change_role(sender_id, target, RoleIndex::Regular)?;
                    }

                    // Adds new member and stages commit
                    let params = group
                        .stage_invite(
                            txn,
                            &key_store.signing_key,
                            contact_add_infos,
                            contact_wai_keys,
                            client_credentials,
                        )
                        .await?;

                    Ok((group, params))
                })
                .await?;

            Ok(InviteUsersParams {
                group,
                params,
                owner_domain,
            })
        }
    }

    pub(super) struct InviteUsersParams {
        group: Group,
        params: GroupOperationParamsOut,
        owner_domain: Fqdn,
    }

    impl InviteUsersParams {
        pub(super) async fn ds_group_operation(
            self,
            api_clients: &ApiClients,
            signer: &ClientSigningKey,
        ) -> anyhow::Result<InvitedUsers> {
            let Self {
                group,
                params,
                owner_domain,
            } = self;

            let ds_timestamp = api_clients
                .get(&owner_domain)?
                .ds_group_operation(params, signer, group.group_state_ear_key())
                .await?;

            Ok(InvitedUsers {
                group,
                ds_timestamp,
            })
        }
    }

    pub(super) struct InvitedUsers {
        group: Group,
        ds_timestamp: TimeStamp,
    }

    impl InvitedUsers {
        pub(super) async fn merge_pending_commit(
            self,
            connection: &mut sqlx::SqliteConnection,
            notifier: &mut StoreNotifier,
            chat_id: ChatId,
        ) -> anyhow::Result<Vec<ChatMessage>> {
            let Self {
                mut group,
                ds_timestamp,
            } = self;

            let (group_messages, _) = group
                .merge_pending_commit(&mut *connection, None, ds_timestamp)
                .await?;
            group.store_update(&mut *connection).await?;
            CoreUser::store_new_messages(&mut *connection, notifier, chat_id, group_messages).await
        }
    }
}
