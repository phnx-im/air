// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::identifiers::UserId;
use anyhow::bail;

use crate::{
    ChatAttributes, ChatId, ChatMessage,
    groups::Group,
    job::{
        Job, JobContext, chat_operation::add_users_flow::AddUsersData,
        pending_chat_operation::PendingChatOperation,
    },
};

#[derive(Debug, Clone)]
pub(crate) enum ChatOperationType {
    AddMembers(Vec<UserId>),
    RemoveMembers(Vec<UserId>),
    Leave,
    Delete,
    Update(Option<ChatAttributes>),
}

pub(crate) struct ChatOperation {
    chat_id: ChatId,
    operation: ChatOperationType,
}

impl Job<Vec<ChatMessage>> for ChatOperation {
    async fn execute_logic(self, context: &mut JobContext<'_>) -> anyhow::Result<Vec<ChatMessage>> {
        self.execute_internal(context).await
    }

    async fn execute_dependencies(&mut self, context: &mut JobContext<'_>) -> anyhow::Result<()> {
        // Execute any pending operation for this chat first.
        let pending_operation =
            PendingChatOperation::load(&mut context.connection, &self.chat_id).await?;

        if let Some(pending_operation) = pending_operation {
            pending_operation.execute(context).await?;
        }

        // Check whether our operation is still valid after the pending
        // operation was been executed.
        self.check_validity_and_refine(context).await?;

        Ok(())
    }
}

impl ChatOperation {
    pub(crate) fn add_members(chat_id: ChatId, users: Vec<UserId>) -> Self {
        ChatOperation {
            chat_id,
            operation: ChatOperationType::AddMembers(users),
        }
    }

    pub(crate) fn remove_members(chat_id: ChatId, users: Vec<UserId>) -> Self {
        ChatOperation {
            chat_id,
            operation: ChatOperationType::RemoveMembers(users),
        }
    }

    pub(crate) fn leave_chat(chat_id: ChatId) -> Self {
        ChatOperation {
            chat_id,
            operation: ChatOperationType::Leave,
        }
    }

    pub(crate) fn update(chat_id: ChatId, chat_attributes: Option<ChatAttributes>) -> Self {
        ChatOperation {
            chat_id,
            operation: ChatOperationType::Update(chat_attributes),
        }
    }

    pub(crate) fn delete_chat(chat_id: ChatId) -> Self {
        ChatOperation {
            chat_id,
            operation: ChatOperationType::Delete,
        }
    }

    /// Check whether the operation is still valid given the current state of
    /// the group. If the operation is partially valid (e.g. one of the users to
    /// add is already a member), refine the operation to only include the valid
    /// parts.
    ///
    /// Returns an error if the operation is no longer valid.
    async fn check_validity_and_refine(
        &mut self,
        context: &mut JobContext<'_>,
    ) -> anyhow::Result<()> {
        let group = Group::load_with_chat_id_clean(&mut context.connection, self.chat_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("No group found for chat {}", self.chat_id))?;

        if !group.mls_group().is_active() {
            return Err(anyhow::anyhow!(
                "Cannot execute operation on inactive group"
            ));
        }

        match &self.operation {
            ChatOperationType::AddMembers(user_ids) => {
                let members = group.members(context.connection.as_mut()).await;
                let refined_user_ids: Vec<UserId> = user_ids
                    .iter()
                    .filter(|&user_id| !members.contains(user_id))
                    .cloned()
                    .collect();

                if refined_user_ids.is_empty() {
                    bail!("All users to add are already members of the group");
                }

                self.operation = ChatOperationType::AddMembers(refined_user_ids);
            }
            ChatOperationType::RemoveMembers(user_ids) => {
                let members = group.members(context.connection.as_mut()).await;
                let refined_user_ids: Vec<UserId> = user_ids
                    .iter()
                    .filter(|&user_id| members.contains(user_id))
                    .cloned()
                    .collect();

                if refined_user_ids.is_empty() {
                    bail!("None of the users to remove are members of the group");
                }

                self.operation = ChatOperationType::RemoveMembers(refined_user_ids);
            }
            // The following operations are always valid as long as the
            // group is active.
            ChatOperationType::Leave | ChatOperationType::Delete | ChatOperationType::Update(_) => {
                ()
            }
        }
        Ok(())
    }

    async fn execute_internal(
        mut self,
        context: &mut JobContext<'_>,
    ) -> anyhow::Result<Vec<ChatMessage>> {
        match self.operation.clone() {
            ChatOperationType::AddMembers(user_ids) => {
                self.execute_add_members(context, user_ids).await
            }
            ChatOperationType::RemoveMembers(user_ids) => {
                self.execute_remove_members(context, user_ids).await
            }
            ChatOperationType::Leave => self.execute_leave_chat(context).await,
            ChatOperationType::Delete => self.execute_delete(context).await,
            ChatOperationType::Update(chat_attributes) => {
                self.execute_update(context, chat_attributes.as_ref()).await
            }
        }
    }

    async fn execute_add_members(
        &mut self,
        context: &mut JobContext<'_>,
        users: Vec<UserId>,
    ) -> anyhow::Result<Vec<ChatMessage>> {
        let JobContext {
            api_clients,
            connection,
            key_store,
            ..
        } = context;
        // Phase 1: Load all the relevant chat and all the contacts we
        // want to add.
        let invite_prepared = AddUsersData::load_chat(connection, self.chat_id, users)
            .await?
            // Phase 2: Load add infos for each contact
            // This needs the connection to load (and potentially fetch and store).
            .load_add_infos(connection, &api_clients)
            .await?;

        // Until here, everything is repeatable and can be retried

        // Phase 3: Load the group and create the commit to add the new members.
        // This creates and persists the PendingChatOperation in the database.
        let job = invite_prepared.create_job(connection, &key_store).await?;

        job.execute(context).await
    }

    /// Remove users from the chat with the given [`ChatId`].
    async fn execute_remove_members(
        &mut self,
        context: &mut JobContext<'_>,
        users: Vec<UserId>,
    ) -> anyhow::Result<Vec<ChatMessage>> {
        let JobContext {
            connection,
            key_store,
            ..
        } = context;
        let job = PendingChatOperation::create_remove(
            connection,
            &key_store.signing_key,
            self.chat_id,
            users,
        )
        .await?;

        job.execute(context).await
    }

    /// Leave the chat with the given [`ChatId`].
    async fn execute_leave_chat(
        &mut self,
        context: &mut JobContext<'_>,
    ) -> anyhow::Result<Vec<ChatMessage>> {
        let JobContext {
            connection,
            key_store,
            ..
        } = context;
        let job =
            PendingChatOperation::create_leave(connection, &key_store.signing_key, self.chat_id)
                .await?;

        job.execute(context).await
    }

    /// Leave the chat with the given [`ChatId`].
    async fn execute_update(
        self,
        context: &mut JobContext<'_>,
        chat_attributes: Option<&ChatAttributes>,
    ) -> anyhow::Result<Vec<ChatMessage>> {
        let JobContext {
            connection,
            key_store,
            ..
        } = context;
        let job = PendingChatOperation::create_update(
            connection,
            &key_store.signing_key,
            self.chat_id,
            chat_attributes,
        )
        .await?;

        job.execute(context).await
    }

    async fn execute_delete(
        self,
        context: &mut JobContext<'_>,
    ) -> anyhow::Result<Vec<ChatMessage>> {
        let JobContext {
            connection,
            notifier,
            key_store,
            ..
        } = context;
        let job = PendingChatOperation::create_delete(
            connection,
            &key_store.signing_key,
            notifier,
            self.chat_id,
        )
        .await?;

        if let Some(job) = job {
            job.execute(context).await
        } else {
            Ok(Vec::new())
        }
    }
}

pub(crate) mod add_users_flow {
    use aircommon::{
        credentials::ClientCredential, crypto::ear::keys::WelcomeAttributionInfoEarKey,
        identifiers::UserId,
    };
    use anyhow::Context;
    use mimi_room_policy::RoleIndex;
    use openmls::group::GroupId;
    use sqlx::SqliteConnection;

    use crate::{
        Chat, ChatId, Contact,
        clients::api_clients::ApiClients,
        contacts::ContactAddInfos,
        groups::{Group, client_auth_info::StorableClientCredential},
        job::pending_chat_operation::PendingChatOperation,
        key_stores::MemoryUserKeyStore,
        utils::connection_ext::ConnectionExt,
    };

    pub(crate) struct AddUsersData<S> {
        group_id: GroupId,
        new_members: Vec<UserId>,
        contact_wai_keys: Vec<WelcomeAttributionInfoEarKey>,
        client_credentials: Vec<ClientCredential>,
        state: S,
    }

    impl AddUsersData<()> {
        pub(crate) async fn load_chat(
            connection: &mut SqliteConnection,
            chat_id: ChatId,
            new_members: Vec<UserId>,
        ) -> anyhow::Result<AddUsersData<Vec<Contact>>> {
            let chat = Chat::load(&mut *connection, &chat_id)
                .await?
                .with_context(|| format!("Can't find chat with id {chat_id}"))?;

            let mut contact_wai_keys = Vec::with_capacity(new_members.len());
            let mut contacts = Vec::with_capacity(new_members.len());
            let mut client_credentials = Vec::with_capacity(new_members.len());

            for new_member in &new_members {
                // Get the WAI keys and client credentials for the invited users.
                let contact = Contact::load(&mut *connection, new_member)
                    .await?
                    .with_context(|| format!("Can't find contact {new_member:?}"))?;
                contact_wai_keys.push(contact.wai_ear_key().clone());

                if let Some(client_credential) =
                    StorableClientCredential::load_by_user_id(&mut *connection, new_member).await?
                {
                    client_credentials.push(ClientCredential::from(client_credential));
                }

                contacts.push(contact);
            }

            Ok(AddUsersData {
                group_id: chat.group_id().clone(),
                new_members,
                contact_wai_keys,
                client_credentials,
                state: contacts,
            })
        }
    }

    impl AddUsersData<Vec<Contact>> {
        pub(crate) async fn load_add_infos(
            self,
            connection: &mut SqliteConnection,
            api_clients: &ApiClients,
        ) -> anyhow::Result<AddUsersData<Vec<ContactAddInfos>>> {
            let Self {
                group_id,
                new_members,
                contact_wai_keys,
                client_credentials,
                state: contacts,
            } = self;

            let mut contact_add_infos: Vec<ContactAddInfos> = Vec::with_capacity(contacts.len());
            for contact in contacts {
                let add_info = contact.fetch_add_infos(connection, api_clients).await?;
                contact_add_infos.push(add_info);
            }

            Ok(AddUsersData {
                group_id,
                new_members,
                contact_wai_keys,
                client_credentials,
                state: contact_add_infos,
            })
        }
    }

    impl AddUsersData<Vec<ContactAddInfos>> {
        /// Stage the addition of new members to the group and create a
        /// PendingChatOperation for it. The PendingChatOperation is stored in
        /// the database within this function.
        pub(super) async fn create_job(
            self,
            connection: &mut SqliteConnection,
            key_store: &MemoryUserKeyStore,
        ) -> anyhow::Result<PendingChatOperation> {
            let Self {
                group_id,
                new_members,
                contact_wai_keys,
                client_credentials,
                state: contact_add_infos,
            } = self;

            let pending_chat_operation = connection
                .with_transaction(async |txn| {
                    let mut group = Group::load_clean(txn, &group_id)
                        .await?
                        .with_context(|| format!("Can't find group with id {group_id:?}"))?;

                    let own_id = key_store.signing_key.credential().identity();

                    // Room policy check (doesn't apply changes to room state yet)
                    for target in &new_members {
                        group.verify_role_change(own_id, target, RoleIndex::Regular)?;
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

                    // Create PendingChatOperation job
                    let pending_chat_operation = PendingChatOperation::new(group, params);
                    pending_chat_operation.store(txn).await?;

                    Ok(pending_chat_operation)
                })
                .await?;

            Ok(pending_chat_operation)
        }
    }
}

mod remove_users_flow {
    use aircommon::{credentials::keys::ClientSigningKey, identifiers::UserId};
    use anyhow::Context;
    use mimi_room_policy::RoleIndex;
    use sqlx::SqliteConnection;

    use crate::{
        Chat, ChatId, groups::Group, job::pending_chat_operation::PendingChatOperation,
        utils::connection_ext::ConnectionExt,
    };

    impl PendingChatOperation {
        /// Creates and stores a PendingChatOperation for removing users.
        pub(super) async fn create_remove(
            connection: &mut SqliteConnection,
            signer: &ClientSigningKey,
            chat_id: ChatId,
            target_users: Vec<UserId>,
        ) -> anyhow::Result<Self> {
            connection
                .with_transaction(async |txn| {
                    let chat = Chat::load(txn.as_mut(), &chat_id)
                        .await?
                        .with_context(|| format!("Can't find chat with id {chat_id}"))?;
                    let group_id = chat.group_id();
                    let mut group = Group::load_clean(txn, group_id)
                        .await?
                        .with_context(|| format!("No group found for group ID {group_id:?}"))?;

                    let own_id = signer.credential().identity();

                    // Room policy checks
                    for target in &target_users {
                        group.verify_role_change(own_id, target, RoleIndex::Outsider)?;
                    }

                    let params = group
                        .stage_remove(txn.as_mut(), signer, target_users)
                        .await?;

                    let job = Self::new(group, params);
                    job.store(txn.as_mut()).await?;
                    Ok(job)
                })
                .await
        }
    }
}

mod leave_chat_flow {
    use aircommon::credentials::keys::ClientSigningKey;
    use anyhow::Context;
    use mimi_room_policy::RoleIndex;
    use sqlx::SqliteConnection;

    use crate::{
        Chat, ChatId, groups::Group, job::pending_chat_operation::PendingChatOperation,
        utils::connection_ext::ConnectionExt,
    };

    impl PendingChatOperation {
        pub(super) async fn create_leave(
            connection: &mut SqliteConnection,
            signer: &ClientSigningKey,
            chat_id: ChatId,
        ) -> anyhow::Result<Self> {
            connection
                .with_transaction(async |txn| {
                    let chat = Chat::load(txn.as_mut(), &chat_id)
                        .await?
                        .with_context(|| format!("Can't find chat with id {chat_id}",))?;
                    let group_id = chat.group_id();
                    let mut group = Group::load_clean(txn, group_id)
                        .await?
                        .with_context(|| format!("Can't find group with id {group_id:?}"))?;
                    let own_id = signer.credential().identity();
                    group.verify_role_change(own_id, own_id, RoleIndex::Outsider)?;

                    let params = group.stage_leave_group(txn, signer)?;

                    let job = Self::new(group, params);
                    job.store(txn.as_mut()).await?;
                    Ok(job)
                })
                .await
        }
    }
}

mod update_chat_attributes_flow {
    use aircommon::{codec::PersistenceCodec, credentials::keys::ClientSigningKey};
    use anyhow::Context as _;
    use sqlx::SqliteConnection;

    use crate::{
        Chat, ChatAttributes, ChatId,
        groups::{Group, GroupData},
        job::pending_chat_operation::PendingChatOperation,
        utils::connection_ext::ConnectionExt,
    };

    impl PendingChatOperation {
        pub(super) async fn create_update(
            connection: &mut SqliteConnection,
            signer: &ClientSigningKey,
            chat_id: ChatId,
            new_chat_attributes: Option<&ChatAttributes>,
        ) -> anyhow::Result<Self> {
            connection
                .with_transaction(async |txn| {
                    let chat = Chat::load(txn.as_mut(), &chat_id)
                        .await?
                        .with_context(|| format!("Can't find chat with id {chat_id}"))?;
                    let group_id = chat.group_id();
                    let mut group = Group::load_clean(txn, group_id)
                        .await?
                        .with_context(|| format!("Can't find group with id {group_id:?}"))?;
                    let group_data = match new_chat_attributes {
                        Some(attrs) => Some(GroupData::from(PersistenceCodec::to_vec(attrs)?)),
                        None => None,
                    };

                    let params = group.update(txn, signer, group_data).await?;

                    let job = Self::new(group, params);
                    job.store(txn.as_mut()).await?;

                    Ok(job)
                })
                .await
        }
    }
}

mod delete_chat_flow {
    use aircommon::credentials::keys::ClientSigningKey;
    use anyhow::Context;
    use sqlx::SqliteConnection;

    use crate::{
        Chat, ChatId, groups::Group, job::pending_chat_operation::PendingChatOperation,
        store::StoreNotifier, utils::connection_ext::ConnectionExt as _,
    };

    impl PendingChatOperation {
        /// Creates and stores a PendingChatOperation for deleting a chat.
        /// If the chat has only one member (the user themself), it is
        /// directly set to inactive instead.
        pub(super) async fn create_delete(
            connection: &mut SqliteConnection,
            signer: &ClientSigningKey,
            notifier: &mut StoreNotifier,
            chat_id: ChatId,
        ) -> anyhow::Result<Option<Self>> {
            connection
                .with_transaction(async |txn| {
                    let mut chat = Chat::load(txn.as_mut(), &chat_id)
                        .await?
                        .with_context(|| format!("Can't find chat with id {chat_id}"))?;

                    let group_id = chat.group_id();
                    let mut group = Group::load_clean(txn, group_id)
                        .await?
                        .with_context(|| format!("Can't find group with id {group_id:?}"))?;

                    let past_members = group.members(txn.as_mut()).await;

                    if past_members.len() == 1 {
                        chat.set_inactive(
                            txn.as_mut(),
                            notifier,
                            past_members.into_iter().collect(),
                        )
                        .await?;
                        Ok(None)
                    } else {
                        let message = group.stage_delete(txn, signer).await?;

                        let job = Self::new(group, message);
                        job.store(txn.as_mut()).await?;
                        Ok(Some(job))
                    }
                })
                .await
        }
    }
}
