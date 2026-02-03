// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::{
    Chat, ChatAttributes, ChatId, ChatMessage, Contact, SystemMessage,
    chats::messages::TimestampedMessage,
    clients::{CoreUser, api_clients::ApiClients, update_key::update_chat_attributes},
    contacts::ContactAddInfos,
    groups::{Group, GroupData, client_auth_info::StorableClientCredential},
    job::{Job, JobContext},
    store::StoreNotifier,
    utils::connection_ext::ConnectionExt,
};
use aircommon::{
    codec::PersistenceCodec,
    credentials::{ClientCredential, keys::ClientSigningKey},
    identifiers::{QualifiedGroupId, UserId},
    messages::client_ds_out::{DeleteGroupParamsOut, GroupOperationParamsOut, SelfRemoveParamsOut},
    time::TimeStamp,
};
use anyhow::Context as _;
use chrono::{DateTime, Utc};
use mimi_room_policy::RoleIndex;
use openmls::group::GroupId;
use sqlx::{SqliteConnection, SqliteTransaction};
use std::collections::HashSet;
use tracing::error;

#[derive(derive_more::From)]
pub(super) enum OperationType {
    Leave(SelfRemoveParamsOut),
    Delete(DeleteGroupParamsOut),
    Other(Box<GroupOperationParamsOut>),
}

impl OperationType {
    fn is_commit(&self) -> bool {
        match self {
            OperationType::Leave(_) => false,
            OperationType::Delete(_) | OperationType::Other(_) => true,
        }
    }

    fn is_delete(&self) -> bool {
        matches!(self, OperationType::Delete(_))
    }
}

/// Represents a pending chat operation to be retried.
pub(super) struct PendingChatOperation {
    group: Group,
    operation: OperationType,
    #[allow(dead_code)]
    last_attempt: DateTime<Utc>,
}

impl Job for PendingChatOperation {
    type Output = Vec<ChatMessage>;

    async fn execute_logic(self, context: &mut JobContext<'_>) -> anyhow::Result<Vec<ChatMessage>> {
        self.execute_internal(context).await
    }

    async fn execute_dependencies(&mut self, _context: &mut JobContext<'_>) -> anyhow::Result<()> {
        Ok(())
    }
}

impl PendingChatOperation {
    pub(super) fn new(group: Group, message: impl Into<OperationType>) -> Self {
        Self {
            group,
            operation: message.into(),
            last_attempt: Utc::now(),
        }
    }

    pub(super) async fn store(&self, _connection: &mut SqliteConnection) -> sqlx::Result<()> {
        // Store the pending operation in the database.
        Ok(())
    }

    pub(super) async fn load(
        _connection: &mut SqliteConnection,
        _chat_id: &ChatId,
    ) -> sqlx::Result<Option<Self>> {
        // Load the pending operation from the database.
        Ok(None)
    }

    async fn delete(
        _connection: &mut SqliteConnection,
        _group_id: &GroupId,
    ) -> sqlx::Result<Option<Self>> {
        // Delete the pending operation from the database.
        Ok(None)
    }

    pub async fn execute_internal(
        mut self,
        context: &mut JobContext<'_>,
    ) -> anyhow::Result<Vec<ChatMessage>> {
        let JobContext {
            api_clients,
            pool,
            notifier,
            key_store,
        } = context;
        let signer = &key_store.signing_key;
        let own_user_id = signer.credential().identity().clone();

        let qgid = QualifiedGroupId::try_from(self.group.group_id())?;

        let is_commit = self.operation.is_commit();
        let is_delete = self.operation.is_delete();
        let is_leave = matches!(self.operation, OperationType::Leave(_));

        let api_client = api_clients.get(qgid.owning_domain())?;
        let res = match self.operation {
            OperationType::Leave(params) => {
                api_client
                    .ds_self_remove(params, signer, self.group.group_state_ear_key())
                    .await
            }
            OperationType::Delete(params) => {
                api_client
                    .ds_delete_group(params, signer, self.group.group_state_ear_key())
                    .await
            }
            OperationType::Other(params) => {
                api_client
                    .ds_group_operation(*params, signer, self.group.group_state_ear_key())
                    .await
            }
        };

        let ds_timestamp = match res {
            Ok(ds_timestamp) => ds_timestamp,
            Err(e) => {
                if e.is_wrong_epoch() && is_leave {
                    // Leaving should be successful even if we get a wrong epoch error
                    TimeStamp::now()
                } else {
                    error!(group_id=%qgid, error=?e, "Failed to execute pending chat operation");
                    // For now we just log the error and return. Later we'll
                    // want the following sematics:
                    // - If we can't tell whether the DS got the message, we
                    //   should retry later.
                    // - If we know the DS didn't get the message, we should
                    //   delete the pending operation and return an error.
                    // - If we're getting a WrongEpochError, we may want to mark
                    //   the chat as stalled until we get something from the
                    //   queue.
                    return Err(e.into());
                }
            }
        };

        // If any of the following fails, something is very wrong.
        let messages = pool
            .with_transaction(async |txn| {
                let Some(mut chat) = Chat::load_by_group_id(txn, self.group.group_id()).await?
                else {
                    anyhow::bail!("Chat not found for group: {:?}", self.group.group_id());
                };

                let past_members = if is_delete {
                    self.group.members(txn.as_mut()).await
                } else {
                    HashSet::new()
                };

                let group_messages = if is_commit {
                    let (mut group_messages, group_data) = self
                        .group
                        .merge_pending_commit(txn, None, ds_timestamp)
                        .await?;

                    if let Some(group_data) = group_data {
                        update_chat_attributes(
                            txn,
                            notifier,
                            &mut chat,
                            own_user_id,
                            group_data,
                            ds_timestamp,
                            &mut group_messages,
                        )
                        .await?;
                    }

                    group_messages
                } else {
                    // Post-process leave operation
                    self.group.room_state_change_role(
                        &own_user_id,
                        &own_user_id,
                        RoleIndex::Outsider,
                    )?;
                    vec![TimestampedMessage::system_message(
                        SystemMessage::Remove(own_user_id.clone(), own_user_id),
                        ds_timestamp,
                    )]
                };

                if is_delete {
                    chat.set_inactive(txn.as_mut(), notifier, past_members.into_iter().collect())
                        .await?;
                }

                self.group.store_update(txn.as_mut()).await?;
                let messages =
                    CoreUser::store_new_messages(&mut *txn, notifier, chat.id(), group_messages)
                        .await?;

                Self::delete(txn, self.group.group_id()).await?;
                Ok(messages)
            })
            .await?;

        Ok(messages)
    }

    /// Creates and stores a PendingChatOperation for removing users.
    pub(super) async fn create_remove(
        txn: &mut SqliteTransaction<'_>,
        signer: &ClientSigningKey,
        chat_id: ChatId,
        target_users: Vec<UserId>,
    ) -> anyhow::Result<Self> {
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

        let job = Self::new(group, Box::new(params));
        job.store(txn.as_mut()).await?;
        Ok(job)
    }
    pub(super) async fn create_leave(
        txn: &mut SqliteTransaction<'_>,
        signer: &ClientSigningKey,
        chat_id: ChatId,
    ) -> anyhow::Result<Self> {
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
    }

    pub(super) async fn create_update(
        txn: &mut SqliteTransaction<'_>,
        signer: &ClientSigningKey,
        chat_id: ChatId,
        new_chat_attributes: Option<&ChatAttributes>,
    ) -> anyhow::Result<Self> {
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

        let job = Self::new(group, Box::new(params));
        job.store(txn.as_mut()).await?;

        Ok(job)
    }

    /// Creates and stores a PendingChatOperation for deleting a chat.
    /// If the chat has only one member (the user themself), it is
    /// directly set to inactive instead.
    pub(super) async fn create_delete(
        txn: &mut SqliteTransaction<'_>,
        signer: &ClientSigningKey,
        notifier: &mut StoreNotifier,
        chat_id: ChatId,
    ) -> anyhow::Result<Option<Self>> {
        let mut chat = Chat::load(txn.as_mut(), &chat_id)
            .await?
            .with_context(|| format!("Can't find chat with id {chat_id}"))?;

        let group_id = chat.group_id();
        let mut group = Group::load_clean(txn, group_id)
            .await?
            .with_context(|| format!("Can't find group with id {group_id:?}"))?;

        let past_members = group.members(txn.as_mut()).await;

        if past_members.len() == 1 {
            chat.set_inactive(txn.as_mut(), notifier, past_members.into_iter().collect())
                .await?;
            Ok(None)
        } else {
            let message = group.stage_delete(txn, signer).await?;

            let job = Self::new(group, message);
            job.store(txn.as_mut()).await?;
            Ok(Some(job))
        }
    }

    pub(crate) async fn create_add(
        connection: &mut SqliteConnection,
        api_clients: &ApiClients,
        signer: &ClientSigningKey,
        chat_id: ChatId,
        new_members: Vec<UserId>,
    ) -> anyhow::Result<Self> {
        // Load local data to prepare add operation
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

        // Fetch add infos from the server
        let mut contact_add_infos: Vec<ContactAddInfos> = Vec::with_capacity(contacts.len());
        for contact in contacts {
            let add_info = contact.fetch_add_infos(connection, api_clients).await?;
            contact_add_infos.push(add_info);
        }

        let group_id = chat.group_id();
        let job = connection
            .with_transaction(async |txn| {
                let mut group = Group::load_clean(txn, group_id)
                    .await?
                    .with_context(|| format!("Can't find group with id {group_id:?}"))?;

                let own_id = signer.credential().identity();

                // Room policy check (doesn't apply changes to room state yet)
                for target in &new_members {
                    group.verify_role_change(own_id, target, RoleIndex::Regular)?;
                }

                // Adds new member and stages commit
                let params = group
                    .stage_invite(
                        txn,
                        signer,
                        contact_add_infos,
                        contact_wai_keys,
                        client_credentials,
                    )
                    .await?;

                // Create PendingChatOperation job
                let pending_chat_operation = PendingChatOperation::new(group, Box::new(params));
                pending_chat_operation.store(txn).await?;

                Ok(pending_chat_operation)
            })
            .await?;

        Ok(job)
    }
}
