// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::HashSet;

use aircommon::identifiers::UserId;
use airprotos::{
    client::group::{GroupData, GroupProfile},
    delivery_service::v1::StorageObjectType,
};
use anyhow::{Context, anyhow, bail};
use sqlx::SqliteConnection;

use crate::{
    Chat, ChatAttributes, ChatId, ChatMessage, ChatStatus,
    groups::Group,
    job::{Job, JobContext, JobError, pending_chat_operation::PendingChatOperation},
    utils::connection_ext::ConnectionExt,
};

#[derive(Debug, Clone)]
enum ChatOperationType {
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

impl Job for ChatOperation {
    type Output = Vec<ChatMessage>;

    async fn execute_logic(
        self,
        context: &mut JobContext<'_>,
    ) -> Result<Vec<ChatMessage>, JobError> {
        self.execute_internal(context).await
    }

    async fn execute_dependencies(&mut self, context: &mut JobContext<'_>) -> Result<(), JobError> {
        // Execute any pending operation for this chat first.
        let pending_operation = context
            .pool
            .with_transaction(async |txn| Ok(PendingChatOperation::load(txn, &self.chat_id).await?))
            .await?;

        if let Some(pending_operation) = pending_operation {
            // We can just propagate any error here, as the this job isn't
            // persisted and doesn't need to be cleaned up.
            pending_operation.execute(context).await?;
        }

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
        connection: &mut SqliteConnection,
    ) -> anyhow::Result<()> {
        let chat = Chat::load(connection, &self.chat_id)
            .await?
            .ok_or(anyhow!("No chat found for ID {}", self.chat_id))?;

        if let ChatStatus::Inactive(_) = chat.status() {
            bail!("Cannot execute operation on inactive chat");
        }

        let group = Group::load_clean(connection, chat.group_id())
            .await?
            .ok_or_else(|| anyhow::anyhow!("No group found for chat {}", self.chat_id))?;

        match &mut self.operation {
            ChatOperationType::AddMembers(user_ids) => {
                let members: HashSet<_> = group.members().collect();
                user_ids.retain(|user_id| !members.contains(user_id));

                if user_ids.is_empty() {
                    bail!("All users to add are already members of the group");
                }
            }
            ChatOperationType::RemoveMembers(user_ids) => {
                let members: HashSet<_> = group.members().collect();
                user_ids.retain(|user_id| members.contains(user_id));

                if user_ids.is_empty() {
                    bail!("None of the users to remove are members of the group");
                }
            }
            // The following operations are always valid as long as the
            // group is active.
            ChatOperationType::Leave | ChatOperationType::Delete | ChatOperationType::Update(_) => {
            }
        }
        Ok(())
    }

    async fn execute_internal(
        mut self,
        context: &mut JobContext<'_>,
    ) -> Result<Vec<ChatMessage>, JobError> {
        // Check whether our operation is still. It may be refined in case the
        // group state has changed, either due to a PendingChatOperation
        // executed as a dependency, or one or more commits arriving from the
        // QS.
        context
            .pool
            .with_connection(async |connection| self.check_validity_and_refine(connection).await)
            .await?;

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
                self.execute_update(context, chat_attributes).await
            }
        }
    }

    async fn execute_add_members(
        &mut self,
        context: &mut JobContext<'_>,
        users: Vec<UserId>,
    ) -> Result<Vec<ChatMessage>, JobError> {
        let JobContext {
            api_clients,
            pool,
            key_store,
            ..
        } = context;
        let job = pool
            .with_connection(async |connection| {
                PendingChatOperation::create_add(
                    connection,
                    api_clients,
                    &key_store.signing_key,
                    self.chat_id,
                    users,
                )
                .await
            })
            .await?;

        job.execute(context).await
    }

    /// Remove users from the chat
    async fn execute_remove_members(
        &mut self,
        context: &mut JobContext<'_>,
        users: Vec<UserId>,
    ) -> Result<Vec<ChatMessage>, JobError> {
        let JobContext {
            pool, key_store, ..
        } = context;
        let job = pool
            .with_transaction(async |txn| {
                PendingChatOperation::create_remove(
                    txn,
                    &key_store.signing_key,
                    self.chat_id,
                    users,
                )
                .await
            })
            .await?;

        job.execute(context).await
    }

    /// Leave the chat
    async fn execute_leave_chat(
        &mut self,
        context: &mut JobContext<'_>,
    ) -> Result<Vec<ChatMessage>, JobError> {
        let JobContext {
            pool, key_store, ..
        } = context;
        let job = pool
            .with_transaction(async |txn| {
                PendingChatOperation::create_leave(txn, &key_store.signing_key, self.chat_id).await
            })
            .await?;

        job.execute(context).await
    }

    /// Update the chat
    async fn execute_update(
        self,
        context: &mut JobContext<'_>,
        chat_attributes: Option<ChatAttributes>,
    ) -> Result<Vec<ChatMessage>, JobError> {
        let JobContext {
            api_clients,
            http_client,
            pool,
            key_store,
            ..
        } = context;

        let group_data = if let Some(attributes) = chat_attributes {
            let chat_id = self.chat_id;
            let group = Group::load_with_chat_id_clean(pool.acquire().await?.as_mut(), chat_id)
                .await?
                .with_context(|| format!("No group with chat id {chat_id}"))?;

            // Encrypt
            let group_profile = GroupProfile::from(attributes);
            let (ciphertext, external) = group_profile
                .encrypt(group.identity_link_wrapper_key())
                .context("Failed to encrypt group profile")?;

            // Provision
            let api_client = api_clients.default_client()?;
            let content_length = ciphertext.len().try_into().context("usize overflow")?;
            let provision_response = api_client
                .ds_provision_attachment(
                    &key_store.signing_key,
                    group.group_state_ear_key(),
                    group.group_id(),
                    group.own_index(),
                    content_length,
                    StorageObjectType::GroupProfile,
                )
                .await?;
            let object_id = provision_response.object_id.context("no object id")?;
            let external = external.build(object_id.into());

            // Upload
            if provision_response.post_policy.is_some() {
                return Err(anyhow!("Post policy is not supported yet").into());
            } else {
                // upload encrypted content via signed PUT url
                let mut request = http_client.put(provision_response.upload_url);
                for header in provision_response.upload_headers {
                    request = request.header(header.key, header.value);
                }
                request
                    .body(ciphertext)
                    .send()
                    .await
                    .context("Failed to upload group profile")?
                    .error_for_status()
                    .context("Failed to upload group profile")?;
            }

            Some(GroupData {
                title: group_profile.title,
                picture: group_profile.picture,
                external_group_profile: Some(external),
            })
        } else {
            None
        };

        let job = pool
            .with_transaction(async |txn| {
                PendingChatOperation::create_update(
                    txn,
                    &key_store.signing_key,
                    self.chat_id,
                    group_data,
                )
                .await
            })
            .await?;

        job.execute(context).await
    }

    async fn execute_delete(
        self,
        context: &mut JobContext<'_>,
    ) -> Result<Vec<ChatMessage>, JobError> {
        let JobContext {
            pool,
            notifier,
            key_store,
            ..
        } = context;
        let job = pool
            .with_transaction(async |txn| {
                PendingChatOperation::create_delete(
                    txn,
                    &key_store.signing_key,
                    notifier,
                    self.chat_id,
                )
                .await
            })
            .await?;

        if let Some(job) = job {
            job.execute(context).await
        } else {
            Ok(Vec::new())
        }
    }
}
