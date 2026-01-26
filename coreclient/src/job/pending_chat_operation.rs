// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::HashSet;

use aircommon::{
    identifiers::QualifiedGroupId,
    messages::client_ds_out::{DeleteGroupParamsOut, GroupOperationParamsOut, SelfRemoveParamsOut},
};
use chrono::{DateTime, Utc};
use mimi_room_policy::RoleIndex;
use openmls::group::GroupId;
use sqlx::SqliteConnection;

use crate::{
    Chat, ChatId, ChatMessage, SystemMessage,
    chats::messages::TimestampedMessage,
    clients::{CoreUser, update_key::update_chat_attributes},
    groups::Group,
    job::{Job, JobContext},
    utils::connection_ext::ConnectionExt,
};

#[derive(derive_more::From)]
pub(super) enum OperationType {
    Leave(SelfRemoveParamsOut),
    Delete(DeleteGroupParamsOut),
    Other(GroupOperationParamsOut),
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
    last_attempt: DateTime<Utc>,
}

impl Job<Vec<ChatMessage>> for PendingChatOperation {
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
            connection,
            notifier,
            key_store,
        } = context;
        let signer = &key_store.signing_key;
        let own_user_id = signer.credential().identity().clone();

        let qgid = QualifiedGroupId::try_from(self.group.group_id())?;

        let is_commit = self.operation.is_commit();
        let is_delete = self.operation.is_delete();

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
                    .ds_group_operation(params, signer, self.group.group_state_ear_key())
                    .await
            }
        };

        let ds_timestamp = match res {
            Ok(ds_timestamp) => ds_timestamp,
            Err(e) => {
                // TODO: Branch here depending on the group operation and the
                // error type. If sending a commit and we're getting a
                // `WrongEpochError`, we need to log the fact and fail this
                // operation indicating that we need to wait for the queue. To
                // the app, this should look like a network error.
                //
                // If we're sending a leave proposal, we can always just retry.
                //
                // If we're getting a network error, we should update the
                // `last_attempt`, store the job and return an error.
                //
                // If we're getting any other error, we schedule a retry. In the
                // future, we may want to take other actions depending on the
                // specific error.
                todo!("Handle error: {:?}", e);
            }
        };

        // If any of the following fails, something is very wrong.
        let messages = connection
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
}
