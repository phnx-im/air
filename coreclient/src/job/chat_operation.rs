// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{borrow::Cow, collections::HashSet};

use aircommon::identifiers::UserId;
use airprotos::{
    client::group::{EncryptedGroupTitle, GroupData, GroupProfile},
    delivery_service::v1::StorageObjectType,
};
use anyhow::{Context, anyhow, bail};
use openmls::treesync::errors::LeafNodeValidationError;
use thiserror::Error;

use crate::{
    Chat, ChatAttributes, ChatId, ChatMessage, ChatStatus,
    db_access::DbAccess,
    groups::Group,
    job::{Job, JobContext, JobError, pending_chat_operation::PendingChatOperation},
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

/// Specific errors which can occur when executing a [`ChatOperation`].
#[derive(Debug, Error)]
pub(crate) enum ChatOperationError {
    #[error(transparent)]
    LeafNodeValidation(#[from] LeafNodeValidationError),
}

impl Job for ChatOperation {
    type Output = Vec<ChatMessage>;

    type DomainError = ChatOperationError;

    async fn execute_logic(
        self,
        context: &mut JobContext<'_>,
    ) -> Result<Vec<ChatMessage>, JobError<Self::DomainError>> {
        self.execute_internal(context).await
    }

    async fn execute_dependencies(
        &mut self,
        context: &mut JobContext<'_>,
    ) -> Result<(), JobError<Self::DomainError>> {
        // Execute any pending operation for this chat first.
        let pending_operation = context
            .db
            .with_write_transaction(async |txn| {
                PendingChatOperation::load(txn, &self.chat_id).await
            })
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
    async fn check_validity_and_refine(&mut self, db: &DbAccess) -> anyhow::Result<()> {
        let chat = Chat::load(db.read().await?, &self.chat_id)
            .await?
            .ok_or(anyhow!("No chat found for ID {}", self.chat_id))?;

        if let ChatStatus::Inactive(_) = chat.status() {
            bail!("Cannot execute operation on inactive chat");
        }

        let group = Group::load_clean(db.read().await?, chat.group_id())
            .await?
            .ok_or_else(|| anyhow::anyhow!("No group found for chat {}", self.chat_id))?;

        match &mut self.operation {
            ChatOperationType::AddMembers(user_ids) => {
                let members: HashSet<_> = group.members().collect();
                user_ids.retain(|user_id| !members.contains(user_id));
            }
            ChatOperationType::RemoveMembers(user_ids) => {
                let members: HashSet<_> = group.members().collect();
                user_ids.retain(|user_id| members.contains(user_id));
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
    ) -> Result<Vec<ChatMessage>, JobError<ChatOperationError>> {
        // Check whether our operation is still. It may be refined in case the
        // group state has changed, either due to a PendingChatOperation
        // executed as a dependency, or one or more commits arriving from the
        // QS.
        self.check_validity_and_refine(context.db).await?;

        match self.operation.clone() {
            ChatOperationType::AddMembers(user_ids) => {
                if user_ids.is_empty() {
                    return Ok(Vec::new());
                }
                self.execute_add_members(context, user_ids).await
            }
            ChatOperationType::RemoveMembers(user_ids) => {
                if user_ids.is_empty() {
                    return Ok(Vec::new());
                }
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
    ) -> Result<Vec<ChatMessage>, JobError<ChatOperationError>> {
        let JobContext {
            api_clients,
            db,
            key_store,
            ..
        } = context;
        let job = PendingChatOperation::create_add(
            &mut db.write().await?,
            api_clients,
            &key_store.signing_key,
            self.chat_id,
            users,
        )
        .await?;

        job.execute(context).await
    }

    /// Remove users from the chat
    async fn execute_remove_members(
        &mut self,
        context: &mut JobContext<'_>,
        users: Vec<UserId>,
    ) -> Result<Vec<ChatMessage>, JobError<ChatOperationError>> {
        let JobContext { db, key_store, .. } = context;
        let job = db
            .with_write_transaction(async |txn| {
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
    ) -> Result<Vec<ChatMessage>, JobError<ChatOperationError>> {
        let JobContext { db, key_store, .. } = context;
        let job = db
            .with_write_transaction(async |txn| {
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
    ) -> Result<Vec<ChatMessage>, JobError<ChatOperationError>> {
        let JobContext {
            api_clients,
            http_client,
            db,
            key_store,
            ..
        } = context;

        let (group_data, new_chat_picture) = if let Some(attributes) = chat_attributes {
            let chat_id = self.chat_id;
            let group = Group::load_with_chat_id_clean(db.read().await?, chat_id)
                .await?
                .with_context(|| format!("No group with chat id {chat_id}"))?;

            // Encrypt
            let picture = attributes.picture.as_deref().map(Cow::Borrowed);
            let group_profile = GroupProfile::new(attributes.title, None, picture);
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

            let encrypted_title = EncryptedGroupTitle::encrypt(
                &group_profile.title,
                group.identity_link_wrapper_key(),
            )
            .context("Failed to encrypt group title")?;

            let group_data = GroupData {
                encrypted_title: Some(encrypted_title),
                external_group_profile: Some(external),
                legacy_title: None,
            };
            (Some(group_data), attributes.picture)
        } else {
            (None, None)
        };

        let job = db
            .with_write_transaction(async |txn| {
                PendingChatOperation::create_update(
                    txn,
                    &key_store.signing_key,
                    self.chat_id,
                    group_data,
                    new_chat_picture,
                )
                .await
            })
            .await?;

        job.execute(context).await
    }

    async fn execute_delete(
        self,
        context: &mut JobContext<'_>,
    ) -> Result<Vec<ChatMessage>, JobError<ChatOperationError>> {
        let JobContext { db, key_store, .. } = context;
        let job = db
            .with_write_transaction(async |txn| {
                PendingChatOperation::create_delete(txn, &key_store.signing_key, self.chat_id).await
            })
            .await?;

        if let Some(job) = job {
            job.execute(context).await
        } else {
            Ok(Vec::new())
        }
    }
}
