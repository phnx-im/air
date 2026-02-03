// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::identifiers::UserId;
use anyhow::bail;
use sqlx::SqliteConnection;

use crate::{
    ChatAttributes, ChatId, ChatMessage,
    groups::Group,
    job::{Job, JobContext, pending_chat_operation::PendingChatOperation},
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

    async fn execute_logic(self, context: &mut JobContext<'_>) -> anyhow::Result<Vec<ChatMessage>> {
        self.execute_internal(context).await
    }

    async fn execute_dependencies(&mut self, context: &mut JobContext<'_>) -> anyhow::Result<()> {
        // Execute any pending operation for this chat first.
        let pending_operation = context
            .pool
            .with_connection(async |connection| {
                let pending_operation =
                    PendingChatOperation::load(connection, &self.chat_id).await?;
                Ok(pending_operation)
            })
            .await?;

        if let Some(pending_operation) = pending_operation {
            pending_operation.execute(context).await?;
        }

        // Check whether our operation is still valid after the pending
        // operation was been executed.
        context
            .pool
            .with_connection(async |connection| self.check_validity_and_refine(connection).await)
            .await?;

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
        let group = Group::load_with_chat_id_clean(connection, self.chat_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("No group found for chat {}", self.chat_id))?;

        if !group.mls_group().is_active() {
            bail!("Cannot execute operation on inactive group");
        }

        match &mut self.operation {
            ChatOperationType::AddMembers(user_ids) => {
                let members = group.members(connection).await;
                user_ids.retain(|user_id| !members.contains(user_id));

                if user_ids.is_empty() {
                    bail!("All users to add are already members of the group");
                }
            }
            ChatOperationType::RemoveMembers(user_ids) => {
                let members = group.members(connection).await;
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

    /// Remove users from the chat with the given [`ChatId`].
    async fn execute_remove_members(
        &mut self,
        context: &mut JobContext<'_>,
        users: Vec<UserId>,
    ) -> anyhow::Result<Vec<ChatMessage>> {
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

    /// Leave the chat with the given [`ChatId`].
    async fn execute_leave_chat(
        &mut self,
        context: &mut JobContext<'_>,
    ) -> anyhow::Result<Vec<ChatMessage>> {
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

    /// Leave the chat with the given [`ChatId`].
    async fn execute_update(
        self,
        context: &mut JobContext<'_>,
        chat_attributes: Option<&ChatAttributes>,
    ) -> anyhow::Result<Vec<ChatMessage>> {
        let JobContext {
            pool, key_store, ..
        } = context;
        let job = pool
            .with_transaction(async |txn| {
                PendingChatOperation::create_update(
                    txn,
                    &key_store.signing_key,
                    self.chat_id,
                    chat_attributes,
                )
                .await
            })
            .await?;

        job.execute(context).await
    }

    async fn execute_delete(
        self,
        context: &mut JobContext<'_>,
    ) -> anyhow::Result<Vec<ChatMessage>> {
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
