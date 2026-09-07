// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{borrow::Cow, collections::HashSet};

use airapiclient::ds_api::DsAttachmentTarget;
use aircommon::{crypto::errors::EncryptionError, identifiers::UserId};
use airprotos::{
    client::{
        group::{EncryptedGroupTitle, GroupData, GroupProfile},
        self_group::LinkedDevice,
    },
    delivery_service::v1::StorageObjectType,
};
use anyhow::{Context, anyhow, bail};
use apqmls::messages::ApqKeyPackage;
use openmls::treesync::errors::LeafNodeValidationError;
use thiserror::Error;
use uuid::Uuid;

use crate::{
    Chat, ChatAttributes, ChatId, ChatMessage, ChatStatus,
    db::access::WriteConnection,
    groups::Group,
    job::{Job, JobContext, JobContextDb, JobError, pending_chat_operation::PendingChatOperation},
};

#[derive(Debug, Clone)]
enum ChatOperationType {
    RemoveMembers(Vec<UserId>),
    /// Removes individual self-group leaves, identified by client id.
    RemoveClients(Vec<Uuid>),
    /// Adds a newly linked device's leaf to the self group.
    AddClient {
        key_package: Box<ApqKeyPackage>,
        device: LinkedDevice,
    },
    Leave,
    Delete,
    Update(Option<ChatAttributes>, DerivationEpoch),
    ApqUpdate(DerivationEpoch),
}

/// Whether a self-update commit also opens a new virtual-client derivation
/// epoch.
///
/// Only the emulation group, i.e. the self group, has derivation epochs, and
/// only its periodic self-update rotates them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DerivationEpoch {
    Rotate,
    Keep,
}

impl DerivationEpoch {
    pub(crate) fn rotates(self) -> bool {
        self == Self::Rotate
    }
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
    #[error("failed to encrypt user profile key")]
    UserProfileKeyEncryptionError(EncryptionError),
}

/// Executes any pending operation for the chat, so that a job starts from a
/// clean group state.
pub(super) async fn execute_pending_operation(
    chat_id: ChatId,
    context: &mut JobContext<'_, '_>,
) -> Result<(), JobError<ChatOperationError>> {
    let pending_operation = context
        .db
        .write()
        .await?
        .with_transaction(async |txn| PendingChatOperation::load(txn, &chat_id).await)
        .await?;

    if let Some(pending_operation) = pending_operation {
        // We can just propagate any error here, as the this job isn't
        // persisted and doesn't need to be cleaned up.
        pending_operation.execute(context).await?;
    }

    Ok(())
}

/// Loads the group of the chat with the given id.
///
/// Errors if the chat is gone or inactive, since no operation may run on it
/// then.
pub(super) async fn load_active_group(
    db: &mut JobContextDb<'_, '_>,
    chat_id: ChatId,
) -> anyhow::Result<Group> {
    let chat = {
        let mut connection = db.read().await?;
        let txn = connection.begin().await?;
        Chat::load(txn, &chat_id)
            .await?
            .ok_or(anyhow!("No chat found for ID {chat_id}"))?
    };

    if let ChatStatus::Inactive(_) = chat.status() {
        bail!("Cannot execute operation on inactive chat");
    }

    Group::load_clean(db.read().await?, chat.group_id())
        .await?
        .ok_or_else(|| anyhow!("No group found for chat {chat_id}"))
}

impl Job for ChatOperation {
    type Output = Vec<ChatMessage>;

    type DomainError = ChatOperationError;

    async fn execute_logic(
        self,
        context: &mut JobContext<'_, '_>,
    ) -> Result<Vec<ChatMessage>, JobError<Self::DomainError>> {
        Box::pin(self.execute_internal(context)).await
    }

    async fn execute_dependencies(
        &mut self,
        context: &mut JobContext<'_, '_>,
    ) -> Result<(), JobError<Self::DomainError>> {
        execute_pending_operation(self.chat_id, context).await
    }
}

impl ChatOperation {
    pub(crate) fn remove_members(chat_id: ChatId, users: Vec<UserId>) -> Self {
        ChatOperation {
            chat_id,
            operation: ChatOperationType::RemoveMembers(users),
        }
    }

    pub(crate) fn remove_clients(chat_id: ChatId, client_ids: Vec<Uuid>) -> Self {
        ChatOperation {
            chat_id,
            operation: ChatOperationType::RemoveClients(client_ids),
        }
    }

    pub(crate) fn add_client(
        chat_id: ChatId,
        key_package: ApqKeyPackage,
        device: LinkedDevice,
    ) -> Self {
        ChatOperation {
            chat_id,
            operation: ChatOperationType::AddClient {
                key_package: Box::new(key_package),
                device,
            },
        }
    }

    pub(crate) fn leave_chat(chat_id: ChatId) -> Self {
        ChatOperation {
            chat_id,
            operation: ChatOperationType::Leave,
        }
    }

    pub(crate) fn update(
        chat_id: ChatId,
        chat_attributes: Option<ChatAttributes>,
        derivation_epoch: DerivationEpoch,
    ) -> Self {
        ChatOperation {
            chat_id,
            operation: ChatOperationType::Update(chat_attributes, derivation_epoch),
        }
    }

    pub(crate) fn apq_update(chat_id: ChatId, derivation_epoch: DerivationEpoch) -> Self {
        ChatOperation {
            chat_id,
            operation: ChatOperationType::ApqUpdate(derivation_epoch),
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
        db: &mut JobContextDb<'_, '_>,
    ) -> anyhow::Result<()> {
        let group = load_active_group(db, self.chat_id).await?;

        match &mut self.operation {
            ChatOperationType::RemoveMembers(user_ids) => {
                let members: HashSet<_> = group.members().collect();
                user_ids.retain(|user_id| members.contains(user_id));
            }
            ChatOperationType::RemoveClients(client_ids) => {
                let self_group_members: HashSet<_> =
                    group.self_group_client_ids().into_iter().collect();
                client_ids.retain(|client_id| self_group_members.contains(client_id));
            }
            // The following operations are always valid as long as the
            // group is active.
            ChatOperationType::AddClient { .. }
            | ChatOperationType::Leave
            | ChatOperationType::Delete
            | ChatOperationType::Update(..)
            | ChatOperationType::ApqUpdate(_) => {}
        }
        Ok(())
    }

    async fn execute_internal(
        mut self,
        context: &mut JobContext<'_, '_>,
    ) -> Result<Vec<ChatMessage>, JobError<ChatOperationError>> {
        // Check whether our operation is still. It may be refined in case the
        // group state has changed, either due to a PendingChatOperation
        // executed as a dependency, or one or more commits arriving from the
        // QS.
        self.check_validity_and_refine(&mut context.db).await?;

        match self.operation.clone() {
            ChatOperationType::RemoveMembers(user_ids) => {
                if user_ids.is_empty() {
                    return Ok(Vec::new());
                }
                self.execute_remove_members(context, user_ids).await
            }
            ChatOperationType::RemoveClients(client_ids) => {
                if client_ids.is_empty() {
                    return Ok(Vec::new());
                }
                self.execute_remove_clients(context, client_ids).await
            }
            ChatOperationType::AddClient {
                key_package,
                device,
            } => self.execute_add_client(context, *key_package, device).await,
            ChatOperationType::Leave => self.execute_leave_chat(context).await,
            ChatOperationType::Delete => self.execute_delete(context).await,
            ChatOperationType::Update(chat_attributes, derivation_epoch) => {
                self.execute_update(context, chat_attributes, derivation_epoch)
                    .await
            }
            ChatOperationType::ApqUpdate(derivation_epoch) => {
                self.execute_apq_self_update(context, derivation_epoch)
                    .await
            }
        }
    }

    /// Remove users from the chat
    async fn execute_remove_members(
        &mut self,
        context: &mut JobContext<'_, '_>,
        users: Vec<UserId>,
    ) -> Result<Vec<ChatMessage>, JobError<ChatOperationError>> {
        let JobContext { db, key_store, .. } = context;
        let job = db
            .write()
            .await?
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

    /// Remove individual self-group leaves, identified by client id.
    async fn execute_remove_clients(
        &mut self,
        context: &mut JobContext<'_, '_>,
        client_ids: Vec<Uuid>,
    ) -> Result<Vec<ChatMessage>, JobError<ChatOperationError>> {
        let JobContext { db, .. } = context;
        let job = db
            .write()
            .await?
            .with_transaction(async |txn| {
                PendingChatOperation::create_remove_clients(txn, client_ids).await
            })
            .await?;

        job.execute(context).await
    }

    /// Add a newly linked device's leaf to the self group.
    async fn execute_add_client(
        &mut self,
        context: &mut JobContext<'_, '_>,
        key_package: ApqKeyPackage,
        device: LinkedDevice,
    ) -> Result<Vec<ChatMessage>, JobError<ChatOperationError>> {
        let JobContext { db, key_store, .. } = context;
        let job = db
            .write()
            .await?
            .with_transaction(async |txn| {
                PendingChatOperation::create_add_client(
                    txn,
                    &key_store.signing_key,
                    &key_store.wai_ear_key,
                    key_package,
                    device,
                )
                .await
            })
            .await?;

        job.execute(context).await
    }

    /// Leave the chat
    async fn execute_leave_chat(
        &mut self,
        context: &mut JobContext<'_, '_>,
    ) -> Result<Vec<ChatMessage>, JobError<ChatOperationError>> {
        let JobContext { db, key_store, .. } = context;
        let Some(job) = db
            .write()
            .await?
            .with_transaction(async |txn| {
                let mut chat = Chat::load(&mut *txn, &self.chat_id)
                    .await?
                    .with_context(|| format!("No chat with id {}", self.chat_id))?;
                let group = Group::load_clean_verified(&mut *txn, chat.group_id())
                    .await?
                    .with_context(|| format!("No group with id {:?}", chat.group_id()))?;

                // Short-circuit if the group has already a pending self-remove proposal from our
                // own leaf from other virtual client.
                if group.group().has_pending_own_self_remove() {
                    if !matches!(chat.status(), ChatStatus::Inactive(_)) {
                        // A leave system message is already recorded. Just deactivate the chat.
                        chat.set_status(&mut *txn, ChatStatus::inactive(Vec::new()))
                            .await?;
                    }
                    return Ok(None);
                }

                PendingChatOperation::create_leave(txn, &key_store.signing_key, self.chat_id)
                    .await
                    .map(Some)
            })
            .await?
        else {
            return Ok(Vec::new());
        };
        job.execute(context).await
    }

    /// Update the chat
    async fn execute_update(
        self,
        context: &mut JobContext<'_, '_>,
        chat_attributes: Option<ChatAttributes>,
        derivation_epoch: DerivationEpoch,
    ) -> Result<Vec<ChatMessage>, JobError<ChatOperationError>> {
        let JobContext {
            api_clients,
            http_client,
            db,
            key_store,
            ..
        } = context;

        let (group_data, new_chat_picture) = if let Some(attributes) = chat_attributes.as_ref()
            && attributes.is_empty()
        {
            // Empty chat attributes => erase group data
            (Some(GroupData::empty()), None)
        } else if let Some(attributes) = chat_attributes {
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
                    DsAttachmentTarget::Group {
                        group_state_ear_key: group.group_state_ear_key(),
                        group_id: group.group_id(),
                        sender_index: group.own_index(),
                    },
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
                legacy_title: Some(group_profile.title),
                legacy_picture: None,
            };
            (Some(group_data), attributes.picture)
        } else {
            (None, None)
        };

        let job = db
            .write()
            .await?
            .with_transaction(async |txn| {
                PendingChatOperation::create_update(
                    txn,
                    &key_store.signing_key,
                    self.chat_id,
                    group_data,
                    new_chat_picture,
                    derivation_epoch,
                )
                .await
            })
            .await?;

        job.execute(context).await
    }

    async fn execute_apq_self_update(
        self,
        context: &mut JobContext<'_, '_>,
        derivation_epoch: DerivationEpoch,
    ) -> Result<Vec<ChatMessage>, JobError<ChatOperationError>> {
        let JobContext { db, key_store, .. } = context;
        let job = db
            .write()
            .await?
            .with_transaction(async |txn| {
                PendingChatOperation::create_apq_self_update(
                    txn,
                    &key_store.signing_key,
                    self.chat_id,
                    derivation_epoch,
                )
                .await
            })
            .await?;
        job.execute(context).await
    }

    async fn execute_delete(
        self,
        context: &mut JobContext<'_, '_>,
    ) -> Result<Vec<ChatMessage>, JobError<ChatOperationError>> {
        let JobContext { db, key_store, .. } = context;
        let job = db
            .write()
            .await?
            .with_transaction(async |txn| {
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
