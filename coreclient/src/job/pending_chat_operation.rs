// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use airapiclient::ds_api::DsRequestError;
use aircommon::{
    credentials::{ClientCredential, keys::ClientSigningKey},
    crypto::indexed_aead::keys::UserProfileKey,
    identifiers::{QualifiedGroupId, UserId},
    messages::client_ds_out::{
        ApqGroupOperationParamsOut, DeleteGroupParamsOut, GroupOperationParamsOut,
        SelfRemoveParamsOut,
    },
    time::TimeStamp,
};
use airprotos::client::{group::GroupData, self_group::SettingsUpdate};
use anyhow::{Context as _, anyhow, bail};
use apqmls::commit_builder::ApqCommitMessageBundle;
use chrono::{DateTime, Duration, Utc};
use mimi_room_policy::RoleIndex;
use openmls::group::GroupId;
use serde::{Deserialize, Serialize};
use sqlx::{query, query_as, query_scalar};
use tracing::{debug, error, info};

use crate::{
    Chat, ChatAttributes, ChatId, ChatMessage, ChatStatus, Contact, SystemMessage,
    chats::{GroupDataExt, messages::TimestampedMessage},
    clients::{
        CoreUser,
        api_clients::ApiClients,
        own_client_info::OwnClientInfo,
        update_key::update_chat_attributes,
        user_settings::{reconcile_pending_update, roll_back_settings},
    },
    db::access::{WriteConnection, WriteDbTransaction},
    groups::{
        Group, GroupDataBytes, PreparedInvitee, VerifiedGroup,
        client_auth_info::StorableClientCredential, handle_group_not_found_on_ds,
    },
    job::{
        Job, JobContext, JobContextReadConnection, JobError, chat_operation::ChatOperationError,
    },
    key_stores::indexed_keys::StorableIndexedKey,
};

// Having separate retry intervals for test and non-test is a hack until we can
// pass "now" directly into OutboundService runs.

#[cfg(not(any(test, feature = "test_utils")))]
const RETRY_INTERVAL: Duration = Duration::seconds(5);
#[cfg(any(test, feature = "test_utils"))]
const RETRY_INTERVAL: Duration = Duration::seconds(1);

#[derive(Clone, Serialize, Deserialize)]
pub(super) enum OperationType {
    Leave(Box<SelfRemoveParamsOut>),
    Delete(Box<DeleteGroupParamsOut>),
    ApqDelete {
        commit: Box<ApqCommitMessageBundle>,
    },
    Other {
        params: Box<GroupOperationParamsOut>,
        /// New chat picture (if any)
        ///
        /// It was already uploaded as part of the external group profile but is not yet set as the
        /// chat picture.
        #[serde(with = "serde_bytes")]
        new_chat_picture: Option<Vec<u8>>,
    },
    ApqOther {
        params: Box<ApqGroupOperationParamsOut>,
        /// New chat picture (if any)
        ///
        /// It was already uploaded as part of the external group profile but is not yet set as the
        /// chat picture.
        #[serde(with = "serde_bytes")]
        new_chat_picture: Option<Vec<u8>>,
    },
    SettingsUpdate {
        params: Box<ApqGroupOperationParamsOut>,
        /// The decoded intent; kept so the commit can be rebuilt after an epoch
        /// race.
        update: SettingsUpdate,
        /// Values of the touched settings before the update; used to roll back
        /// on terminal failure.
        previous: SettingsUpdate,
    },
}

impl std::fmt::Display for OperationType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let label = match self {
            OperationType::Leave(_) => "leave",
            OperationType::Delete(_) => "delete",
            OperationType::ApqDelete { .. } => "apq_delete",
            OperationType::Other { .. } => "other",
            OperationType::ApqOther { .. } => "apq_other",
            OperationType::SettingsUpdate { .. } => "settings_update",
        };
        f.write_str(label)
    }
}

impl OperationType {
    fn other(params: GroupOperationParamsOut) -> Self {
        Self::other_with_picture(params, None)
    }

    fn other_with_picture(
        params: GroupOperationParamsOut,
        new_chat_picture: Option<Vec<u8>>,
    ) -> Self {
        Self::Other {
            params: Box::new(params),
            new_chat_picture,
        }
    }

    fn apq_other(params: ApqGroupOperationParamsOut) -> Self {
        Self::apq_other_with_picture(params, None)
    }

    fn apq_other_with_picture(
        params: ApqGroupOperationParamsOut,
        new_chat_picture: Option<Vec<u8>>,
    ) -> Self {
        Self::ApqOther {
            params: Box::new(params),
            new_chat_picture,
        }
    }

    fn is_commit(&self) -> bool {
        match self {
            OperationType::Leave(_) => false,
            OperationType::Delete(_)
            | OperationType::ApqDelete { .. }
            | OperationType::Other { .. }
            | OperationType::ApqOther { .. }
            | OperationType::SettingsUpdate { .. } => true,
        }
    }

    fn is_delete(&self) -> bool {
        matches!(
            self,
            OperationType::Delete(_) | OperationType::ApqDelete { .. }
        )
    }

    /// Whether a DS rejection of this operation marks the group's commit as
    /// failed, which raises the desync banner on the chat. Settings updates
    /// skip this: a self-group settings race is reconciled silently through
    /// the queue and must not raise a banner on the Notes-to-self chat.
    fn marks_commit_failed(&self) -> bool {
        !matches!(self, OperationType::SettingsUpdate { .. })
    }
}

#[derive(Debug)]
enum PendingChatOperationStatus {
    ReadyToRetry,
    WaitingForQueueResponse,
}

/// Represents a pending chat operation to be retried.
pub(crate) struct PendingChatOperation {
    group: VerifiedGroup,
    operation: OperationType,
    // The time at which the operation should be retried. If None, it can be
    // retried immediately.
    retry_due_at: Option<DateTime<Utc>>,
    status: PendingChatOperationStatus,
    number_of_attempts: u32,
}

impl Job for PendingChatOperation {
    type Output = Vec<ChatMessage>;

    type DomainError = ChatOperationError;

    async fn execute_logic(
        mut self,
        context: &mut JobContext<'_, '_>,
    ) -> Result<Vec<ChatMessage>, JobError<ChatOperationError>> {
        match self.execute_internal(context).await {
            // Update retry_due at on network errors
            Err(JobError::NetworkError) => {
                #[cfg(not(any(test, feature = "test_utils")))]
                let retry_due = context.now + RETRY_INTERVAL;
                #[cfg(any(test, feature = "test_utils"))]
                let retry_due = context.now + RETRY_INTERVAL;
                self.update_retry_due_at(context.db.write().await?, retry_due)
                    .await?;
                let group_id = self.group.group_id();
                info!(
                    ?group_id,
                    next_retry = ?retry_due,
                    "Failed to execute PendingChatOperation, will retry later"
                );
                Err(JobError::NetworkError)
            }
            Err(JobError::NotFound) => {
                let group_id = self.group.group_id().clone();
                error!(?group_id, "Group not found on DS; cleaning up local state");
                context
                    .db
                    .write()
                    .await?
                    .with_transaction(async |txn| {
                        self.roll_back_settings_if_any(txn).await?;
                        handle_group_not_found_on_ds(txn, &group_id).await
                    })
                    .await?;
                Err(JobError::NotFound)
            }
            fatal_error @ Err(JobError::Fatal(_)) => {
                // Clean up job after fatal error
                context
                    .db
                    .write()
                    .await?
                    .with_transaction(async |txn| -> anyhow::Result<()> {
                        self.roll_back_settings_if_any(txn).await?;
                        let group = self.group.group_mut();
                        group.discard_pending_commit(&mut *txn).await?;
                        Self::delete(txn, self.group.group_id()).await?;
                        Ok(())
                    })
                    .await
                    .inspect_err(|error| {
                        error!(%error, "Failed to delete pending chat operation");
                    })
                    .ok();
                fatal_error
            }
            res => res,
        }
    }
}

impl PendingChatOperation {
    pub(super) fn new(group: VerifiedGroup, operation: OperationType) -> Self {
        Self {
            group,
            operation,
            retry_due_at: Utc::now().into(),
            status: PendingChatOperationStatus::ReadyToRetry,
            number_of_attempts: 0,
        }
    }

    pub fn group_id(&self) -> &GroupId {
        self.group.group_id()
    }

    pub(crate) fn is_leave(&self) -> bool {
        matches!(self.operation, OperationType::Leave(_))
    }

    pub(crate) fn is_settings_update(&self) -> bool {
        matches!(self.operation, OperationType::SettingsUpdate { .. })
    }

    /// Rolls back an optimistically applied settings update on terminal
    /// failure of the operation. No-op for other operation kinds.
    async fn roll_back_settings_if_any(
        &self,
        txn: &mut WriteDbTransaction<'_>,
    ) -> anyhow::Result<()> {
        if let OperationType::SettingsUpdate {
            update, previous, ..
        } = &self.operation
        {
            roll_back_settings(txn, update, previous).await?;
        }
        Ok(())
    }

    pub async fn execute_internal(
        &mut self,
        context: &mut JobContext<'_, '_>,
    ) -> Result<Vec<ChatMessage>, JobError<ChatOperationError>> {
        if let PendingChatOperationStatus::WaitingForQueueResponse = self.status {
            info!(
                group_id = ?self.group.group_id(),
                "Failed to execute PendingChatOperation for group because
                it is still waiting for a queue response",
            );
            // Re-assert the flag derived from the persisted job state, in case
            // the original write was lost.
            if self.operation.marks_commit_failed() {
                self.group
                    .group_mut()
                    .mark_commit_failed(context.db.write().await?)
                    .await?;
            }
            return Err(JobError::Blocked);
        }

        let JobContext {
            api_clients,
            db,
            key_store,
            now,
            qs_client_id,
            ..
        } = context;
        let signer = &key_store.signing_key;
        let own_user_id = signer.credential().user_id().clone();

        let qgid = QualifiedGroupId::try_from(self.group.group_id())?;

        let is_commit = self.operation.is_commit();
        let is_delete = self.operation.is_delete();
        let is_leave = matches!(self.operation, OperationType::Leave(_));

        let api_client = api_clients.get(qgid.owning_domain())?;

        // If this is a leave operation that has been tried before, we have to
        // check whether the group is still at the same epoch. If not, we have
        // to re-create the proposal.
        if let OperationType::Leave(leave_params) = &mut self.operation
            // This is always Some, because we know the MlsMessage is a
            // PublicMessage
            && let Some(message_epoch) = leave_params.t_remove_proposal.epoch()
            && message_epoch != self.group.mls_group().epoch()
            && self.number_of_attempts > 0
        {
            // No need to check the PQ epoch (if any) because a different PQ epoch implies a
            // different T epoch.
            let restaged = self.group.group_mut().restage_leave_group(
                db.write().await?,
                signer,
                leave_params,
            )?;
            **leave_params = restaged;
        }

        // Restage a settings update whose original commit is gone. While the
        // op's original commit is alive the group holds a matching pending
        // commit. Processing any incoming commit always discards our pending
        // commit, so an op with no pending commit means an unrelated commit
        // advanced the epoch and the stored params are stale. Rebuild the
        // commit against the current epoch from the stored update snapshot.
        let needs_settings_restage = matches!(self.operation, OperationType::SettingsUpdate { .. })
            && self.group.mls_group().pending_commit().is_none();
        if needs_settings_restage {
            self.restage_settings_update(db.write().await?).await?;
        }

        let encrypt_user_profile_key =
            async |connection: JobContextReadConnection| -> Result<_, JobError<ChatOperationError>> {
                let own_user_id = key_store.signing_key.credential().user_id();
                let own_user_profile_key = UserProfileKey::load_own(connection).await?;
                let own_encrypted_user_profile_key = own_user_profile_key
                    .encrypt(self.group.identity_link_wrapper_key(), own_user_id)
                    .map_err(|e| {
                        JobError::domain(ChatOperationError::UserProfileKeyEncryptionError(e))
                    })?;
                Ok(own_encrypted_user_profile_key)
            };

        let mut new_chat_picture = None;
        // TODO: Can we avoid cloning here?
        let res = match self.operation.clone() {
            OperationType::Leave(params) => {
                api_client
                    .ds_self_remove(*params, signer, self.group.group_state_ear_key())
                    .await
            }
            OperationType::Delete(params) => {
                api_client
                    .ds_delete_group(*params, signer, self.group.group_state_ear_key())
                    .await
            }
            OperationType::ApqDelete { commit } => {
                api_client
                    .ds_apq_delete_group(*commit, signer, self.group.group_state_ear_key())
                    .await
            }
            OperationType::Other {
                params,
                new_chat_picture: chat_picture,
            } => {
                new_chat_picture = chat_picture;
                let own_qs_client_reference = key_store.create_own_client_reference(qs_client_id);
                let own_encrypted_user_profile_key =
                    encrypt_user_profile_key(db.read().await?).await?;

                api_client
                    .ds_group_operation(
                        *params,
                        signer,
                        self.group.group_state_ear_key(),
                        own_qs_client_reference,
                        own_encrypted_user_profile_key,
                    )
                    .await
            }
            OperationType::ApqOther {
                params,
                new_chat_picture: chat_picture,
            } => {
                new_chat_picture = chat_picture;

                let own_qs_client_reference = key_store.create_own_client_reference(qs_client_id);
                let own_encrypted_user_profile_key =
                    encrypt_user_profile_key(db.read().await?).await?;

                api_client
                    .ds_apq_group_operation(
                        *params,
                        signer,
                        self.group.group_state_ear_key(),
                        own_qs_client_reference,
                        own_encrypted_user_profile_key,
                    )
                    .await
            }
            OperationType::SettingsUpdate { params, .. } => {
                // Sent exactly like `ApqOther`: same DS call, no chat picture,
                // no chat side effects.
                let own_qs_client_reference = key_store.create_own_client_reference(qs_client_id);
                let own_encrypted_user_profile_key =
                    encrypt_user_profile_key(db.read().await?).await?;

                api_client
                    .ds_apq_group_operation(
                        *params,
                        signer,
                        self.group.group_state_ear_key(),
                        own_qs_client_reference,
                        own_encrypted_user_profile_key,
                    )
                    .await
            }
        };

        let mut ds_has_confirmed_leave = true;
        let ds_timestamp = match res {
            Ok(ds_timestamp) => ds_timestamp,
            Err(error) => {
                self.number_of_attempts += 1;
                if !is_leave {
                    let job_error = self.handle_error(context.db.write().await?, error).await?;
                    return Err(job_error);
                }

                // The leave action is special in that we want to consider
                // it successful regardless of any DS errors and
                // post-process anyway. If the DS returned an error, we'll
                // try again later, but that's just for the benefit of the
                // server and the other chat members.
                info!(
                    group_id = ?self.group.group_id(),
                    "Leave operation failed due to DS error,
                    proceeding with local post-processing"
                );
                ds_has_confirmed_leave = false;
                TimeStamp::now()
            }
        };

        // If any of the following fails, something is very wrong.
        let messages = db
            .write()
            .await?
            .with_transaction(async |txn| {
                let Some(mut chat) =
                    Chat::load_by_group_id(&mut *txn, self.group.group_id()).await?
                else {
                    bail!("Chat not found for group: {:?}", self.group.group_id());
                };

                // Check if this chat operation is still pending for the chat. It might be, that it
                // was already processed and merged by the QS path. The queue handler is running
                // concurrently and might have acquired a transaction *before* this handler.
                if is_commit
                    && !PendingChatOperation::is_pending_for_chat(&mut *txn, chat.id()).await?
                {
                    return Ok(vec![]);
                }

                // Get the past members before merging the commit
                let past_members: Vec<_> = if is_delete {
                    self.group.members().collect()
                } else {
                    Vec::new()
                };

                let group_messages = if is_commit {
                    let (mut group_messages, group_data_bytes) = self
                        .group
                        .merge_pending_commit(&mut *txn, None, ds_timestamp)
                        .await?;

                    if let Some(bytes) = group_data_bytes {
                        let group_data = GroupData::decode(&bytes)?;
                        let (chat_title, _external_group_profile) =
                            group_data.into_parts(self.group.identity_link_wrapper_key());
                        if let Some(chat_title) = chat_title {
                            let attributes = ChatAttributes::new(chat_title, new_chat_picture);
                            // No need to fetch the group profile: this is our own pending commit, so
                            // the profile data is already available locally.
                            update_chat_attributes(
                                txn,
                                &mut chat,
                                &own_user_id,
                                attributes,
                                ds_timestamp,
                                &mut group_messages,
                            )
                            .await?;
                        }
                    }

                    group_messages
                } else if is_leave && !matches!(chat.status(), ChatStatus::Inactive(_)) {
                    // Post-process leave operation. No need to repeat this if
                    // it has already happened once (indicated by chat being
                    // inactive).

                    self.group.group_mut().room_state_change_role(
                        &own_user_id,
                        &own_user_id,
                        RoleIndex::Outsider,
                    )?;

                    vec![TimestampedMessage::system_message(
                        SystemMessage::Remove(own_user_id.clone(), own_user_id),
                        ds_timestamp,
                    )]
                } else {
                    // A leave operation that has already been attempted once so
                    // post-processing has already happened and there is nothing
                    // more to do.
                    vec![]
                };

                // If the chat isn't already inactive (which can be the case for
                // leave operations that have already been processed), and this
                // is either a delete operation or a leave operation, set the
                // chat to inactive.
                if !matches!(chat.status(), ChatStatus::Inactive(_)) && (is_delete || is_leave) {
                    chat.set_inactive(&mut *txn, past_members).await?;
                }

                let t_self_update_at = Some(ds_timestamp);
                let pq_self_update_at = self.group.is_apq().then_some(ds_timestamp);
                self.group
                    .group_mut()
                    .store_update(&mut *txn, t_self_update_at, pq_self_update_at)
                    .await?;
                let messages =
                    CoreUser::store_new_messages(&mut *txn, chat.id(), group_messages).await?;

                // Unless this is a leave operation that hasn't been confirmed
                // by the DS, we can delete the pending operation now.
                if !is_leave || ds_has_confirmed_leave {
                    Self::delete(txn, self.group.group_id()).await?;
                } else {
                    // If it's a leave operation that hasn't been confirmed by
                    // the DS, we want to set a due date for retrying
                    let retry_due = *now + RETRY_INTERVAL;
                    self.update_retry_due_at(txn, retry_due).await?;
                }

                Ok(messages)
            })
            .await?;

        Ok(messages)
    }

    /// Rebuilds a settings-update commit against the group's current epoch from
    /// the operation's stored update snapshot, replacing the stored params.
    ///
    /// The MLS commit must be signed with the self-group leaf key, which lives
    /// in [`OwnClientInfo`], not with the DS request signing key. Only the MLS
    /// commit is rebuilt here. The DS request envelope keeps using the key
    /// store signing key on the send path.
    async fn restage_settings_update(
        &mut self,
        mut connection: impl WriteConnection,
    ) -> anyhow::Result<()> {
        let self_group_signer = OwnClientInfo::load(&mut connection)
            .await?
            .self_group_signing_key
            .context("self-group signer was not initialized")?;

        let mut txn = connection.begin().await?;

        {
            // Destructure so the borrow of `self.operation` (params/update)
            // stays disjoint from the borrow of `self.group`.
            let OperationType::SettingsUpdate { params, update, .. } = &mut self.operation else {
                bail!("restage_settings_update called on a non-settings operation");
            };
            let new_params = self
                .group
                .group_mut()
                .stage_settings_update(&mut txn, &self_group_signer, update)
                .await?;
            **params = new_params;
        }

        // Persist the restaged params. Without this the stored blob keeps the
        // stale-epoch params, so a reload after a network retry would resend
        // them against an epoch the group has already moved past.
        self.update_operation_data(&mut txn).await?;

        txn.commit().await?;
        Ok(())
    }

    /// Reconciles a pending settings update against an `incoming` snapshot that
    /// a sibling's accepted commit already carried, and persists the result.
    ///
    /// Returns `true` when, after reconciliation, nothing is left that we still
    /// intend to change (the update equals the previous state), so the caller
    /// should delete the operation. Returns `false` when some fields remain
    /// pending, in which case the reconciled operation is persisted so the
    /// later restage rebuilds from it.
    pub(crate) async fn reconcile_settings_update(
        &mut self,
        mut connection: impl WriteConnection,
        incoming: &SettingsUpdate,
    ) -> anyhow::Result<bool> {
        let nothing_left = {
            let OperationType::SettingsUpdate {
                update, previous, ..
            } = &mut self.operation
            else {
                bail!("reconcile_settings_update called on a non-settings operation");
            };
            reconcile_pending_update(update, previous, incoming).await?;
            update == previous
        };
        if !nothing_left {
            self.update_operation_data(&mut connection).await?;
        }
        Ok(nothing_left)
    }

    async fn handle_error(
        &mut self,
        mut connection: impl WriteConnection,
        error: DsRequestError,
    ) -> Result<JobError<ChatOperationError>, JobError<ChatOperationError>> {
        debug!(?error, "DS request failed");
        const MAX_RETRIES: u32 = 5;
        if error.is_not_found() {
            // The group no longer exists on the DS. There is no point
            // in retrying, the group needs to be torn down instead.
            Ok(JobError::NotFound)
        } else if error.is_wrong_epoch() {
            // If we get a WrongEpochError, we know the commit was
            // either accepted on a previous try, or the DS rejected
            // it because another one got there first.
            self.mark_as_waiting_for_queue_response(&mut connection)
                .await?;
            if self.operation.marks_commit_failed() {
                self.group
                    .group_mut()
                    .mark_commit_failed(&mut connection)
                    .await?;
            }

            Err(JobError::Blocked)
        } else if error.is_network_error() && self.number_of_attempts < MAX_RETRIES {
            // If we get a network error (which means we don't know whether the request has been
            // processed by the DS), we want to try again until we've either succeeded or reached a
            // max number of retries.
            Ok(JobError::NetworkError)
        } else {
            let error = if self.number_of_attempts >= MAX_RETRIES {
                anyhow!(
                    "Job failed after {} attempts due to DS errors: {:?}",
                    MAX_RETRIES,
                    error
                )
            } else {
                anyhow!("Job failed due to DS error: {:?}", error)
            };
            Ok(JobError::Fatal(error))
        }
    }

    /// Creates and stores a PendingChatOperation for removing users.
    pub(super) async fn create_remove(
        txn: &mut WriteDbTransaction<'_>,
        signer: &ClientSigningKey,
        chat_id: ChatId,
        target_users: Vec<UserId>,
    ) -> anyhow::Result<Self> {
        let chat = Chat::load(&mut *txn, &chat_id)
            .await?
            .with_context(|| format!("Can't find chat with id {chat_id}"))?;
        let group_id = chat.group_id();
        let mut group = Group::load_clean_verified(&mut *txn, group_id)
            .await?
            .with_context(|| format!("No group found for group ID {group_id:?}"))?;

        let own_id = signer.credential().user_id();

        // Room policy checks
        for target in &target_users {
            group.verify_role_change(own_id, target, RoleIndex::Outsider)?;
        }

        let operation_type = if group.is_apq() {
            let params = group
                .group_mut()
                .stage_apq_remove(&mut *txn, signer, target_users)?;
            OperationType::apq_other(params)
        } else {
            let params = group
                .group_mut()
                .stage_remove(&mut *txn, signer, target_users)?;
            OperationType::other(params)
        };

        let job = Self::new(group, operation_type);
        job.store(txn).await?;
        Ok(job)
    }

    pub(super) async fn create_leave(
        txn: &mut WriteDbTransaction<'_>,
        signer: &ClientSigningKey,
        chat_id: ChatId,
    ) -> anyhow::Result<Self> {
        let chat = Chat::load(&mut *txn, &chat_id)
            .await?
            .with_context(|| format!("Can't find chat with id {chat_id}",))?;
        let group_id = chat.group_id();
        let mut group = Group::load_clean_verified(&mut *txn, group_id)
            .await?
            .with_context(|| format!("Can't find group with id {group_id:?}"))?;
        let own_id = signer.credential().user_id();
        group.verify_role_change(own_id, own_id, RoleIndex::Outsider)?;

        let params = group.group_mut().stage_leave_group(&mut *txn, signer)?;

        let job = Self::new(group, OperationType::Leave(Box::new(params)));
        job.store(txn).await?;
        Ok(job)
    }

    pub(super) async fn create_update(
        txn: &mut WriteDbTransaction<'_>,
        signer: &ClientSigningKey,
        chat_id: ChatId,
        new_group_data: Option<GroupData>,
        new_chat_picture: Option<Vec<u8>>,
    ) -> anyhow::Result<Self> {
        let group_data_bytes = new_group_data.map(|data| data.encode()).transpose()?;
        Self::create_update_with_raw_group_data(
            txn,
            signer,
            chat_id,
            group_data_bytes,
            new_chat_picture,
        )
        .await
    }

    pub(crate) async fn create_apq_self_update(
        txn: &mut WriteDbTransaction<'_>,
        signer: &ClientSigningKey,
        chat_id: ChatId,
    ) -> anyhow::Result<Self> {
        let mut group = Group::load_with_chat_id_clean_verified(&mut *txn, chat_id)
            .await?
            .with_context(|| format!("Can't find group with chat id {chat_id}"))?;
        let params = group.group_mut().apq_update(txn, signer)?;
        let job = Self::new(group, OperationType::apq_other(params));
        job.store(txn).await?;
        Ok(job)
    }

    /// Stages a self-group commit carrying the settings update and stores it as
    /// a pending chat operation.
    pub(crate) async fn create_settings_update(
        txn: &mut WriteDbTransaction<'_>,
        signer: &ClientSigningKey,
        self_group_id: &GroupId,
        update: SettingsUpdate,
        previous: SettingsUpdate,
    ) -> anyhow::Result<Self> {
        let mut group = Group::load_clean_verified(&mut *txn, self_group_id)
            .await?
            .with_context(|| format!("Can't find self group with id {self_group_id:?}"))?;

        let params = group
            .group_mut()
            .stage_settings_update(txn, signer, &update)
            .await?;

        let job = Self::new(
            group,
            OperationType::SettingsUpdate {
                params: Box::new(params),
                update,
                previous,
            },
        );
        job.store(txn).await?;
        Ok(job)
    }

    pub(crate) async fn create_update_with_raw_group_data(
        txn: &mut WriteDbTransaction<'_>,
        signer: &ClientSigningKey,
        chat_id: ChatId,
        group_data_bytes: Option<GroupDataBytes>,
        new_chat_picture: Option<Vec<u8>>,
    ) -> anyhow::Result<Self> {
        let mut group = Group::load_with_chat_id_clean_verified(&mut *txn, chat_id)
            .await?
            .with_context(|| format!("Can't find group with chat id {chat_id}"))?;

        let params = group
            .group_mut()
            .update(&mut *txn, signer, group_data_bytes)
            .await?;

        let job = Self::new(
            group,
            OperationType::other_with_picture(params, new_chat_picture),
        );
        job.store(txn).await?;

        Ok(job)
    }

    /// Creates and stores a PendingChatOperation for deleting a chat.
    /// If the chat has only one member (the user themself), it is
    /// directly set to inactive instead.
    pub(super) async fn create_delete(
        txn: &mut WriteDbTransaction<'_>,
        signer: &ClientSigningKey,
        chat_id: ChatId,
    ) -> anyhow::Result<Option<Self>> {
        let mut chat = Chat::load(&mut *txn, &chat_id)
            .await?
            .with_context(|| format!("Can't find chat with id {chat_id}"))?;

        let group_id = chat.group_id();
        let mut group = Group::load_clean_verified(&mut *txn, group_id)
            .await?
            .with_context(|| format!("Can't find group with id {group_id:?}"))?;

        let past_members: Vec<_> = group.members().collect();

        if past_members.len() == 1 {
            chat.set_inactive(txn, past_members).await?;
            Ok(None)
        } else {
            let operation_type = if group.is_apq() {
                let bundle = group.group_mut().stage_apq_delete(&mut *txn, signer)?;
                OperationType::ApqDelete {
                    commit: Box::new(bundle),
                }
            } else {
                let message = group.group_mut().stage_delete(&mut *txn, signer)?;
                OperationType::Delete(Box::new(message))
            };
            let job = Self::new(group, operation_type);
            job.store(txn).await?;
            Ok(Some(job))
        }
    }

    pub(crate) async fn create_add(
        mut connection: impl WriteConnection,
        api_clients: &ApiClients,
        signer: &ClientSigningKey,
        chat_id: ChatId,
        new_members: Vec<UserId>,
    ) -> Result<Self, JobError<ChatOperationError>> {
        // Load local data to prepare add operation
        let mut group = Group::load_verified_with_chat_id(&mut connection, chat_id)
            .await?
            .context("Can't find group for chat with id {chat_id:?}")?;

        // Bundle the per-invitee data (contact, client credential) before we
        // fetch the server-side add info, so that the parallel pieces stay
        // associated with the same user end-to-end.
        struct InviteeBuildup {
            contact: Contact,
            client_credential: ClientCredential,
        }
        let mut buildups = Vec::with_capacity(new_members.len());
        for new_member in &new_members {
            let contact = Contact::load(&mut connection, new_member)
                .await?
                .with_context(|| format!("Can't find contact {new_member:?}"))?;
            let client_credential =
                StorableClientCredential::load_by_user_id(&mut connection, new_member)
                    .await?
                    .map(ClientCredential::from)
                    .with_context(|| {
                        format!("Can't find client credential for contact {new_member:?}")
                    })?;
            buildups.push(InviteeBuildup {
                contact,
                client_credential,
            });
        }

        // Fetch add infos from the server and produce one PreparedInvitee per
        // entry so the staging API doesn't need parallel vectors.
        let mut invitees = Vec::with_capacity(buildups.len());
        for InviteeBuildup {
            contact,
            client_credential,
        } in buildups
        {
            let wai_key = contact.wai_ear_key().clone();
            let add_info = contact
                .fetch_add_infos(&mut connection, api_clients, group.is_apq())
                .await?;
            invitees.push(PreparedInvitee {
                add_info,
                wai_key,
                client_credential,
            });
        }

        connection
            .with_transaction(async |txn| {
                let own_id = signer.credential().user_id();

                // Room policy check (doesn't apply changes to room state yet)
                for target in &new_members {
                    group.verify_role_change(own_id, target, RoleIndex::Regular)?;
                }

                // Adds new member and stages commit
                let operation_type = if !group.is_apq() {
                    let params = group
                        .group_mut()
                        .stage_invite(&mut *txn, signer, invitees)?
                        // Check if we got a leaf node validation error which is domain specific and should
                        // be propagated to the user.
                        .map_err(|validation| {
                            JobError::domain(ChatOperationError::from(validation))
                        })?;
                    OperationType::other(params)
                } else {
                    let params = group
                        .group_mut()
                        .stage_apq_invite(&mut *txn, signer, signer, invitees)?
                        // Check if we got a leaf node validation error which is domain specific and should
                        // be propagated to the user.
                        .map_err(|validation| {
                            JobError::domain(ChatOperationError::from(validation))
                        })?;
                    OperationType::apq_other(params)
                };

                // Create PendingChatOperation job
                let pending_chat_operation = PendingChatOperation::new(group, operation_type);
                pending_chat_operation.store(txn).await?;

                Ok(pending_chat_operation)
            })
            .await
    }
}

mod persistence {
    use aircommon::codec::{BlobDecoded, BlobEncoded};
    use thiserror::Error;
    use uuid::Uuid;

    use crate::db::access::{ReadConnection, WriteConnection, WriteDbTransaction};

    use super::*;

    #[derive(Debug, Error)]
    #[error("Invalid PendingChatOperationStatus: {actual}")]
    pub struct PendingChatOperationStatusError {
        pub actual: String,
    }

    const READY_TO_RETRY: &str = "ready_to_retry";
    const WAITING_FOR_QUEUE_RESPONSE: &str = "waiting_for_queue_response";

    impl sqlx::Encode<'_, sqlx::Sqlite> for OperationType {
        fn encode_by_ref(
            &self,
            buf: &mut <sqlx::Sqlite as sqlx::Database>::ArgumentBuffer,
        ) -> Result<sqlx::encode::IsNull, sqlx::error::BoxDynError> {
            let s = self.to_string();
            <String as sqlx::Encode<sqlx::Sqlite>>::encode_by_ref(&s, buf)
        }
    }

    impl std::fmt::Display for PendingChatOperationStatus {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            match self {
                PendingChatOperationStatus::ReadyToRetry => write!(f, "{}", READY_TO_RETRY),
                PendingChatOperationStatus::WaitingForQueueResponse => {
                    write!(f, "{}", WAITING_FOR_QUEUE_RESPONSE)
                }
            }
        }
    }

    impl sqlx::Type<sqlx::Sqlite> for PendingChatOperationStatus {
        fn type_info() -> sqlx::sqlite::SqliteTypeInfo {
            <String as sqlx::Type<sqlx::Sqlite>>::type_info()
        }
    }

    impl sqlx::Decode<'_, sqlx::Sqlite> for PendingChatOperationStatus {
        fn decode(
            value: <sqlx::Sqlite as sqlx::Database>::ValueRef<'_>,
        ) -> Result<Self, sqlx::error::BoxDynError> {
            let s = <String as sqlx::Decode<sqlx::Sqlite>>::decode(value)?;
            match s.as_str() {
                READY_TO_RETRY => Ok(PendingChatOperationStatus::ReadyToRetry),
                WAITING_FOR_QUEUE_RESPONSE => {
                    Ok(PendingChatOperationStatus::WaitingForQueueResponse)
                }
                s => {
                    let e = PendingChatOperationStatusError {
                        actual: s.to_string(),
                    };
                    Err(Box::new(e) as _)
                }
            }
        }
    }

    impl sqlx::Encode<'_, sqlx::Sqlite> for PendingChatOperationStatus {
        fn encode_by_ref(
            &self,
            buf: &mut <sqlx::Sqlite as sqlx::Database>::ArgumentBuffer,
        ) -> Result<sqlx::encode::IsNull, sqlx::error::BoxDynError> {
            let s = self.to_string();
            <String as sqlx::Encode<sqlx::Sqlite>>::encode_by_ref(&s, buf)
        }
    }

    struct SqlPendingChatOperation {
        group_id: Vec<u8>,
        operation_data: BlobDecoded<OperationType>,
        retry_due_at: Option<DateTime<Utc>>,
        request_status: PendingChatOperationStatus,
        number_of_attempts: i64,
    }

    impl SqlPendingChatOperation {
        async fn into_pending_chat_operation(
            self,
            connection: impl ReadConnection,
        ) -> sqlx::Result<PendingChatOperation> {
            let group_id = GroupId::from_slice(&self.group_id);
            let group = Group::load_verified(connection, &group_id)
                .await?
                // This shouldn't happen, as the pending operation references an
                // existing group inside the database.
                .ok_or(sqlx::Error::RowNotFound)?;
            Ok(PendingChatOperation {
                group,
                operation: self.operation_data.0,
                retry_due_at: self.retry_due_at,
                status: self.request_status,
                number_of_attempts: self.number_of_attempts as u32,
            })
        }
    }

    impl PendingChatOperation {
        pub(super) async fn store(&self, mut connection: impl WriteConnection) -> sqlx::Result<()> {
            let operation_data = BlobEncoded(&self.operation);
            let group_id = self.group.group_id().as_slice();
            let operation_string = self.operation.to_string();
            // Store the pending operation in the database.
            query!(
                "INSERT INTO pending_chat_operation
                (group_id, operation_type, operation_data, retry_due_at, request_status)
                VALUES (?, ?, ?, ?, ?)",
                group_id,
                operation_string,
                operation_data as _,
                self.retry_due_at,
                PendingChatOperationStatus::ReadyToRetry as _
            )
            .execute(connection.as_mut())
            .await?;

            Ok(())
        }

        /// Re-serializes the in-memory operation into the stored blob.
        ///
        /// Used after the operation is rebuilt or reconciled in memory so a
        /// later reload from the database sees the current params rather than
        /// the ones stored at creation time.
        pub(super) async fn update_operation_data(
            &self,
            mut connection: impl WriteConnection,
        ) -> sqlx::Result<()> {
            let operation_data = BlobEncoded(&self.operation);
            let group_id = self.group.group_id().as_slice();
            query!(
                "UPDATE pending_chat_operation SET operation_data = ? WHERE group_id = ?",
                operation_data as _,
                group_id
            )
            .execute(connection.as_mut())
            .await?;

            Ok(())
        }

        pub(super) async fn update_retry_due_at(
            &mut self,
            mut connection: impl WriteConnection,
            retry_due: DateTime<Utc>,
        ) -> sqlx::Result<()> {
            let group_id = self.group.group_id().as_slice();
            let number_of_attempts_i64 = self.number_of_attempts as i64;
            // Update the retry due timestamp in the database and increase number_of_attempts.
            query!(
                "UPDATE pending_chat_operation
                SET retry_due_at = ?, number_of_attempts = ?
                WHERE group_id = ?",
                retry_due,
                number_of_attempts_i64,
                group_id
            )
            .execute(connection.as_mut())
            .await?;

            self.retry_due_at = Some(retry_due);

            Ok(())
        }

        pub(super) async fn mark_as_waiting_for_queue_response(
            &self,
            mut connection: impl WriteConnection,
        ) -> sqlx::Result<()> {
            let group_id = self.group.group_id().as_slice();
            query!(
                "UPDATE pending_chat_operation SET request_status = ? WHERE group_id = ?",
                PendingChatOperationStatus::WaitingForQueueResponse as _,
                group_id
            )
            .execute(connection.as_mut())
            .await?;

            Ok(())
        }

        /// Re-arms a parked operation so `dequeue` picks it up immediately.
        ///
        /// Sets the status back to ready-to-retry and the retry-due time to
        /// now. `dequeue` requires both `retry_due_at <= now` and the
        /// ready-to-retry status, and a parked op sits in
        /// `waiting_for_queue_response`, so both fields must be reset.
        pub(crate) async fn mark_as_ready_to_retry(
            &self,
            mut connection: impl WriteConnection,
        ) -> sqlx::Result<()> {
            let group_id = self.group.group_id().as_slice();
            let now = Utc::now();
            query!(
                "UPDATE pending_chat_operation
                SET request_status = ?, retry_due_at = ?
                WHERE group_id = ?",
                PendingChatOperationStatus::ReadyToRetry as _,
                now,
                group_id
            )
            .execute(connection.as_mut())
            .await?;

            Ok(())
        }

        pub(crate) async fn load_by_group_id(
            mut connection: impl ReadConnection,
            group_id: &GroupId,
        ) -> sqlx::Result<Option<Self>> {
            let group_id = group_id.as_slice();
            let sql_pending_operation = query_as!(
                SqlPendingChatOperation,
                r#"SELECT
                    group_id,
                    operation_data AS "operation_data: _",
                    retry_due_at AS "retry_due_at: _",
                    request_status AS "request_status: _",
                    number_of_attempts
                FROM pending_chat_operation
                WHERE group_id = ?"#,
                group_id
            )
            .fetch_optional(connection.as_mut())
            .await?;

            let Some(sql_pending_operation) = sql_pending_operation else {
                return Ok(None);
            };

            sql_pending_operation
                .into_pending_chat_operation(connection)
                .await
                .map(Some)
        }

        pub(crate) async fn load(
            mut connection: impl ReadConnection,
            chat_id: &ChatId,
        ) -> sqlx::Result<Option<Self>> {
            // Get the group id from the chat table and then load the pending operation.
            let sql_pending_operation = query_as!(
                SqlPendingChatOperation,
                r#"SELECT
                    pco.group_id,
                    pco.operation_data AS "operation_data: _",
                    pco.retry_due_at AS "retry_due_at: _",
                    pco.request_status AS "request_status: _",
                    pco.number_of_attempts
                FROM pending_chat_operation pco
                JOIN chat c ON pco.group_id = c.group_id
                WHERE c.chat_id = ?"#,
                chat_id
            )
            .fetch_optional(connection.as_mut())
            .await?;

            let Some(sql_pending_operation) = sql_pending_operation else {
                return Ok(None);
            };

            sql_pending_operation
                .into_pending_chat_operation(connection)
                .await
                .map(Some)
        }

        pub(crate) async fn is_pending_for_chat(
            mut connection: impl ReadConnection,
            chat_id: ChatId,
        ) -> sqlx::Result<bool> {
            let record = query!(
                "SELECT EXISTS(SELECT 1
            FROM pending_chat_operation pco
            JOIN chat c ON pco.group_id = c.group_id
            WHERE c.chat_id = ? LIMIT 1) AS row_exists",
                chat_id,
            )
            .fetch_one(connection.as_mut())
            .await?;
            Ok(record.row_exists == 1)
        }

        /// Dequeue a PendingChatOperation for retry by the OutboundService.
        pub(crate) async fn dequeue(
            txn: &mut WriteDbTransaction<'_>,
            task_id: Uuid,
            now: DateTime<Utc>,
        ) -> anyhow::Result<Option<Self>> {
            let Some(group_id) = query_scalar!(
                r#"
                SELECT group_id
                FROM pending_chat_operation
                WHERE (locked_by IS NULL OR locked_by != ?1)
                    AND request_status = ?2
                    AND retry_due_at <= ?3
                LIMIT 1
                "#,
                task_id,
                PendingChatOperationStatus::ReadyToRetry as _,
                now
            )
            .fetch_optional(txn.as_mut())
            .await?
            else {
                return Ok(None);
            };

            let Some(sql_pending_operation) = query_as!(
                SqlPendingChatOperation,
                r#"UPDATE pending_chat_operation
                    SET locked_by = ?2
                    WHERE group_id = ?1
                RETURNING
                    group_id,
                    operation_data AS "operation_data: _",
                    retry_due_at AS "retry_due_at: _",
                    request_status AS "request_status: _",
                    number_of_attempts
                "#,
                group_id,
                task_id,
            )
            .fetch_optional(txn.as_mut())
            .await?
            else {
                return Ok(None);
            };

            let pending_operation = sql_pending_operation
                .into_pending_chat_operation(txn)
                .await?;

            Ok(Some(pending_operation))
        }

        pub(crate) async fn delete(
            mut connection: impl WriteConnection,
            group_id: &GroupId,
        ) -> sqlx::Result<()> {
            let group_id = group_id.as_slice();
            // Delete the pending operation from the database.
            query!(
                "DELETE FROM pending_chat_operation WHERE group_id = ?",
                group_id
            )
            .execute(connection.as_mut())
            .await?;
            Ok(())
        }
    }
}

#[cfg(any(test, feature = "test_utils"))]
pub mod test_utils {

    use airprotos::client::component::AirComponent;

    use crate::db::access::ReadConnection;

    use super::*;

    pub struct PendingChatOperationInfo {
        pub operation_type: String,
        pub request_status: String,
        pub number_of_attempts: u32,
    }

    impl PendingChatOperationInfo {
        pub(crate) async fn load(
            connection: impl ReadConnection,
            chat_id: &ChatId,
        ) -> anyhow::Result<Option<Self>> {
            let pco = PendingChatOperation::load(connection, chat_id)
                .await?
                .map(|pco| PendingChatOperationInfo {
                    operation_type: pco.operation.to_string(),
                    request_status: pco.status.to_string(),
                    number_of_attempts: pco.number_of_attempts,
                });

            Ok(pco)
        }
    }

    impl PendingChatOperation {
        /// Creates a self-update commit that forces the given [`AirComponent`] into the own leaf
        /// node.
        ///
        /// Use this in tests to simulate an old client that advertises a different set of feature
        /// flags.
        pub(crate) async fn create_update_with_air_component(
            txn: &mut WriteDbTransaction<'_>,
            signer: &ClientSigningKey,
            chat_id: ChatId,
            air_component: AirComponent,
        ) -> anyhow::Result<Self> {
            let chat = Chat::load(&mut *txn, &chat_id)
                .await?
                .with_context(|| format!("Can't find chat with id {chat_id}"))?;
            let group_id = chat.group_id();
            let mut group = Group::load_clean_verified(&mut *txn, group_id)
                .await?
                .with_context(|| format!("Can't find group with id {group_id:?}"))?;

            let params = group
                .group_mut()
                .update_with_air_component(&mut *txn, signer, air_component)
                .await?;

            let job = Self::new(group, OperationType::other(params));
            job.store(txn).await?;
            Ok(job)
        }

        /// Serialized bytes of the staged commit's MLS message, i.e. the
        /// message the DS echoes back to the committer via fanout. Feed this
        /// back through the QS processing path to exercise the
        /// `OwnPendingCommit` merge path.
        pub(crate) fn staged_commit_message_bytes(&self) -> anyhow::Result<Vec<u8>> {
            use openmls::prelude::tls_codec::Serialize as _;
            let OperationType::Other { params, .. } = &self.operation else {
                bail!("not a group operation carrying a commit");
            };
            Ok(params.commit.mls_message().tls_serialize_detached()?)
        }
    }
}

#[cfg(test)]
mod tests {
    use aircommon::{
        assert_matches,
        credentials::{keys::ClientSigningKey, test_utils::create_test_credentials},
        crypto::aead::keys::IdentityLinkWrapperKey,
        identifiers::{QsClientId, QsUserId, QualifiedGroupId, UserId},
        mls_group_config::AppComponent,
    };
    use airprotos::{
        client::component::AirComponent,
        common::v1::{StatusDetails, StatusDetailsCode, WrongEpochDetail, status_details::Detail},
    };
    use chrono::{Duration, Utc};
    use openmls_traits::OpenMlsProvider;
    use uuid::Uuid;

    use crate::{
        ChatAttributes,
        db::access::{DbAccess, WriteDbTransaction},
        groups::{GroupDataBytes, openmls_provider::AirOpenMlsProvider},
        utils::persistence::open_db_in_memory,
    };

    use super::*;

    /// A DS error that reports a wrong-epoch rejection.
    fn wrong_epoch_error() -> DsRequestError {
        let details = StatusDetails {
            code: StatusDetailsCode::WrongEpoch.into(),
            detail: Some(Detail::WrongEpoch(WrongEpochDetail {})),
        };
        DsRequestError::Tonic(details.to_status(tonic::Code::InvalidArgument, "wrong epoch"))
    }

    /// Builds a single-member APQ self-group with a pending settings-update
    /// operation, stored in the database.
    async fn setup_self_group_settings_op()
    -> anyhow::Result<(DbAccess, PendingChatOperation, ClientSigningKey)> {
        let pool = DbAccess::for_tests(open_db_in_memory().await?);
        let (_aic_sk, signing_key) =
            create_test_credentials(UserId::random("example.com".parse()?));

        let mut connection = pool.write().await?;
        let job = connection
            .with_transaction(async |txn| -> anyhow::Result<_> {
                let t_group_id = GroupId::from(QualifiedGroupId::new(
                    Uuid::new_v4(),
                    "example.com".parse()?,
                ));
                let pq_group_id = GroupId::from(QualifiedGroupId::new(
                    Uuid::new_v4(),
                    "example.com".parse()?,
                ));
                let (group, _params) = Group::create_apq_group(
                    &mut *txn,
                    &signing_key,
                    IdentityLinkWrapperKey::random()?,
                    t_group_id,
                    pq_group_id,
                    GroupDataBytes::from(b"test-group-data".to_vec()),
                    None,
                    AirComponent::default_for_self_group(),
                )?;
                group.store(&mut *txn).await?;
                let mut group = VerifiedGroup::new_for_test(group);

                let update = SettingsUpdate {
                    send_read_receipts: Some(true),
                };
                let params = group
                    .group_mut()
                    .stage_settings_update(txn, &signing_key, &update)
                    .await?;

                let job = PendingChatOperation::new(
                    group,
                    OperationType::SettingsUpdate {
                        params: Box::new(params),
                        update,
                        previous: SettingsUpdate::default(),
                    },
                );
                job.store(txn).await?;
                Ok(job)
            })
            .await?;

        Ok((pool, job, signing_key))
    }

    /// A wrong-epoch rejection of a settings update parks the operation as
    /// `WaitingForQueueResponse` but does not mark the self-group commit as
    /// failed. A settings race must not raise a "desynced" banner.
    #[tokio::test(flavor = "multi_thread")]
    async fn wrong_epoch_parks_settings_without_marking_failed() -> anyhow::Result<()> {
        let (pool, mut pending, _signing_key) = setup_self_group_settings_op().await?;

        let result = pending
            .handle_error(pool.write().await?, wrong_epoch_error())
            .await;

        // Parked, not failed.
        assert_matches!(result, Err(JobError::Blocked));
        assert!(
            !pending.group.commit_failed(),
            "settings race must not mark the commit failed"
        );

        // The status is persisted as waiting for the queue response.
        let group_id = pending.group.group_id().clone();
        let reloaded = PendingChatOperation::load_by_group_id(pool.read().await?, &group_id)
            .await?
            .expect("operation should still exist");
        assert!(matches!(
            reloaded.status,
            PendingChatOperationStatus::WaitingForQueueResponse
        ));

        Ok(())
    }

    /// Clears any pending commit and advances the group's epoch by merging a
    /// forced self-update, simulating an unrelated commit landing.
    fn advance_epoch(
        group: &mut Group,
        txn: &mut WriteDbTransaction<'_>,
        signer: &ClientSigningKey,
    ) -> anyhow::Result<()> {
        let provider = AirOpenMlsProvider::new(txn.as_mut());
        let (t_mls_group, pq_mls_group) = group.apq_mls_groups_mut()?;
        t_mls_group.clear_pending_commit(provider.storage())?;
        pq_mls_group.clear_pending_commit(provider.storage())?;
        apqmls::commit_builder::CommitBuilder::from_groups(&mut *t_mls_group, &mut *pq_mls_group)
            .force_self_update(true)
            .finalize(&provider, signer, |_| true, |_| true)?;
        t_mls_group.merge_pending_commit(&provider)?;
        pq_mls_group.merge_pending_commit(&provider)?;
        Ok(())
    }

    /// An incoming settings update on the same commit wins: the pending
    /// settings op is deleted, and our local pending commit is discarded.
    #[tokio::test(flavor = "multi_thread")]
    async fn discard_deletes_settings_op_when_commit_carries_update() -> anyhow::Result<()> {
        let (pool, mut pending, _signer) = setup_self_group_settings_op().await?;
        let group_id = pending.group.group_id().clone();

        // A throwaway staged commit. The settings-update branch ignores its
        // contents, so any staged commit works.
        let (_src_pool, src, _src_signer) = setup_self_group_settings_op().await?;
        let staged = src
            .group
            .mls_group()
            .pending_commit()
            .expect("staged commit");

        let mut connection = pool.write().await?;
        connection
            .with_transaction(async |txn| -> anyhow::Result<()> {
                let incoming = SettingsUpdate {
                    send_read_receipts: Some(true),
                };
                pending
                    .group
                    .group_mut()
                    .discard_pending_commit_and_operations(txn, &group_id, staged, Some(&incoming))
                    .await?;

                assert!(
                    PendingChatOperation::load_by_group_id(&mut *txn, &group_id)
                        .await?
                        .is_none(),
                    "op should be deleted when the incoming snapshot covers the pending change"
                );
                assert!(pending.group.mls_group().pending_commit().is_none());
                Ok(())
            })
            .await?;

        Ok(())
    }

    /// A commit that carries no settings update keeps the op and re-arms it, so
    /// `dequeue` picks it up immediately, while our local pending commit is
    /// still discarded.
    #[tokio::test(flavor = "multi_thread")]
    async fn discard_keeps_and_rearms_settings_op_without_incoming_update() -> anyhow::Result<()> {
        let (pool, mut pending, _signer) = setup_self_group_settings_op().await?;
        let group_id = pending.group.group_id().clone();

        let (_src_pool, src, _src_signer) = setup_self_group_settings_op().await?;
        let staged = src
            .group
            .mls_group()
            .pending_commit()
            .expect("staged commit");

        let mut connection = pool.write().await?;
        connection
            .with_transaction(async |txn| -> anyhow::Result<()> {
                // Park the op as a wrong-epoch rejection would.
                pending
                    .mark_as_waiting_for_queue_response(&mut *txn)
                    .await?;

                pending
                    .group
                    .group_mut()
                    .discard_pending_commit_and_operations(txn, &group_id, staged, None)
                    .await?;

                // Kept and re-armed: dequeue returns it now.
                let dequeued =
                    PendingChatOperation::dequeue(&mut *txn, Uuid::new_v4(), Utc::now()).await?;
                assert!(dequeued.is_some(), "re-armed op should dequeue");
                assert!(pending.group.mls_group().pending_commit().is_none());
                Ok(())
            })
            .await?;

        Ok(())
    }

    /// An empty incoming snapshot, as a newer sibling's unknown-only update
    /// decodes to on our side, covers none of our pending fields. The op is
    /// kept, re-armed, and its `update`/`previous` are left unchanged.
    #[tokio::test(flavor = "multi_thread")]
    async fn discard_keeps_settings_op_when_incoming_snapshot_empty() -> anyhow::Result<()> {
        let (pool, mut pending, _signer) = setup_self_group_settings_op().await?;
        let group_id = pending.group.group_id().clone();

        let (_src_pool, src, _src_signer) = setup_self_group_settings_op().await?;
        let staged = src
            .group
            .mls_group()
            .pending_commit()
            .expect("staged commit");

        let mut connection = pool.write().await?;
        connection
            .with_transaction(async |txn| -> anyhow::Result<()> {
                pending
                    .mark_as_waiting_for_queue_response(&mut *txn)
                    .await?;

                // A commit that carried a settings update, but one that decoded
                // to an empty snapshot on our side.
                pending
                    .group
                    .group_mut()
                    .discard_pending_commit_and_operations(
                        txn,
                        &group_id,
                        staged,
                        Some(&SettingsUpdate::default()),
                    )
                    .await?;

                // Kept: still present, and its snapshots are unchanged.
                let reloaded = PendingChatOperation::load_by_group_id(&mut *txn, &group_id)
                    .await?
                    .expect("op should be kept when the incoming snapshot is empty");
                let OperationType::SettingsUpdate {
                    update, previous, ..
                } = &reloaded.operation
                else {
                    bail!("expected a settings-update operation");
                };
                assert_eq!(
                    update,
                    &SettingsUpdate {
                        send_read_receipts: Some(true),
                    }
                );
                assert_eq!(previous, &SettingsUpdate::default());

                // Re-armed: dequeue returns it now.
                let dequeued =
                    PendingChatOperation::dequeue(&mut *txn, Uuid::new_v4(), Utc::now()).await?;
                assert!(dequeued.is_some(), "re-armed op should dequeue");
                Ok(())
            })
            .await?;

        Ok(())
    }

    /// After an unrelated commit advanced the epoch, restaging rebuilds the
    /// settings commit against the new epoch: a new pending commit exists and
    /// its AppEphemeral payload decrypts under the new epoch key back to the
    /// stored update snapshot.
    #[tokio::test(flavor = "multi_thread")]
    async fn restage_settings_update_rebuilds_against_new_epoch() -> anyhow::Result<()> {
        let (pool, mut pending, signing_key) = setup_self_group_settings_op().await?;
        let group_id = pending.group.group_id().clone();

        {
            let mut connection = pool.write().await?;
            let mut txn = connection.begin().await?;

            // The restage path signs the MLS commit with the self-group leaf
            // key from OwnClientInfo, so it must be stored.
            OwnClientInfo {
                qs_user_id: QsUserId::random(),
                qs_client_id: QsClientId::random(&mut rand::rng()),
                user_id: signing_key.credential().user_id().clone(),
                self_group_id: Some(group_id.clone()),
                self_group_signing_key: Some(signing_key.clone()),
            }
            .store(&mut txn)
            .await?;

            advance_epoch(pending.group.group_mut(), &mut txn, &signing_key)?;
            txn.commit().await?;
        }

        assert!(
            pending.group.mls_group().pending_commit().is_none(),
            "precondition: no pending commit after the unrelated commit"
        );

        pending.restage_settings_update(pool.write().await?).await?;

        assert!(
            pending.group.mls_group().pending_commit().is_some(),
            "restage should stage a fresh commit"
        );

        // The restaged commit decrypts under the new epoch key back to the
        // stored update snapshot.
        let mut connection = pool.write().await?;
        let mut txn = connection.begin().await?;
        let mut receiver = Group::load(&mut txn, &group_id)
            .await?
            .expect("group stored in setup");
        let staged = pending
            .group
            .mls_group()
            .pending_commit()
            .expect("restaged commit");
        let extracted = receiver.extract_settings_updates(&mut txn, staged).await;
        assert_eq!(
            extracted,
            vec![SettingsUpdate {
                send_read_receipts: Some(true),
            }]
        );
        txn.commit().await?;

        Ok(())
    }

    /// Restaging persists the rebuilt params: after a simulated network retry
    /// (reload from the database) the stored operation carries the restaged
    /// params, not the stale-epoch ones from creation time.
    #[tokio::test(flavor = "multi_thread")]
    async fn restage_settings_update_persists_params_for_reload() -> anyhow::Result<()> {
        use aircommon::codec::PersistenceCodec;

        let (pool, mut pending, signing_key) = setup_self_group_settings_op().await?;
        let group_id = pending.group.group_id().clone();

        // Snapshot of the params stored at creation time, before the epoch
        // moves and the op is restaged.
        let params_before = {
            let reloaded = PendingChatOperation::load_by_group_id(pool.read().await?, &group_id)
                .await?
                .expect("op stored in setup");
            PersistenceCodec::to_vec(&reloaded.operation)?
        };

        {
            let mut connection = pool.write().await?;
            let mut txn = connection.begin().await?;

            // The restage path signs the MLS commit with the self-group leaf
            // key from OwnClientInfo, so it must be stored.
            OwnClientInfo {
                qs_user_id: QsUserId::random(),
                qs_client_id: QsClientId::random(&mut rand::rng()),
                user_id: signing_key.credential().user_id().clone(),
                self_group_id: Some(group_id.clone()),
                self_group_signing_key: Some(signing_key.clone()),
            }
            .store(&mut txn)
            .await?;

            advance_epoch(pending.group.group_mut(), &mut txn, &signing_key)?;
            txn.commit().await?;
        }

        pending.restage_settings_update(pool.write().await?).await?;

        // The reloaded op matches the in-memory restaged op, and differs from
        // the params stored at creation time.
        let reloaded = PendingChatOperation::load_by_group_id(pool.read().await?, &group_id)
            .await?
            .expect("op still present after restage");
        let reloaded_bytes = PersistenceCodec::to_vec(&reloaded.operation)?;
        let in_memory_bytes = PersistenceCodec::to_vec(&pending.operation)?;
        assert_eq!(
            reloaded_bytes, in_memory_bytes,
            "reloaded params must match the in-memory restaged params"
        );
        assert_ne!(
            reloaded_bytes, params_before,
            "restage must have replaced the stale-epoch params in the database"
        );

        Ok(())
    }

    async fn setup_group_and_chat()
    -> anyhow::Result<(DbAccess, VerifiedGroup, ChatId, ClientSigningKey)> {
        let pool = DbAccess::for_tests(open_db_in_memory().await?);
        let mut connection = pool.write().await?;

        let user_id = UserId::random("example.com".parse().unwrap());
        let (_aic_sk, signing_key) = create_test_credentials(user_id.clone());

        let qgid = QualifiedGroupId::new(Uuid::new_v4(), user_id.domain().clone());
        let group_id = GroupId::from(qgid);
        let group_data_bytes = GroupDataBytes::from(b"test-group-data".to_vec());

        let identity_link_wrapper_key = IdentityLinkWrapperKey::random()?;

        let (group, _) = Group::create_group(
            &mut connection,
            &signing_key,
            identity_link_wrapper_key,
            group_id.clone(),
            group_data_bytes,
        )?;
        group.store(&mut connection).await?;
        let group = VerifiedGroup::new_for_test(group);

        let chat = Chat::new_group_chat(
            group_id.clone(),
            ChatAttributes::new("Test chat".into(), None),
        );
        let chat_id = chat.id();
        chat.store(&mut connection).await?;

        Ok((pool, group, chat_id, signing_key))
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn store_and_load_roundtrip() -> anyhow::Result<()> {
        let (pool, mut group, chat_id, signing_key) = setup_group_and_chat().await?;

        let leave_params = group
            .group_mut()
            .stage_leave_group(pool.write().await?, &signing_key)?;
        let pending =
            PendingChatOperation::new(group, OperationType::Leave(Box::new(leave_params)));

        pending.store(pool.write().await?).await?;

        let loaded = PendingChatOperation::load(pool.read().await?, &chat_id)
            .await?
            .expect("Loading stored operation failed");

        assert!(matches!(loaded.operation, OperationType::Leave(_)));
        assert_eq!(loaded.group.group_id(), pending.group.group_id());
        assert_eq!(loaded.retry_due_at, pending.retry_due_at);

        Ok(())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn update_retry_due_at_persists() -> anyhow::Result<()> {
        let (pool, mut group, chat_id, signing_key) = setup_group_and_chat().await?;
        let mut connection = pool.write().await?;

        let leave_params = group
            .group_mut()
            .stage_leave_group(&mut connection, &signing_key)?;
        let mut pending =
            PendingChatOperation::new(group, OperationType::Leave(Box::new(leave_params)));
        pending.store(&mut connection).await?;

        let new_timestamp = Utc::now() + Duration::seconds(30);
        pending
            .update_retry_due_at(&mut connection, new_timestamp)
            .await?;

        let reloaded = PendingChatOperation::load(&mut connection, &chat_id)
            .await?
            .expect("should load");
        assert_eq!(reloaded.retry_due_at, Some(new_timestamp));

        Ok(())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn mark_as_waiting_for_queue_response_updates_status() -> anyhow::Result<()> {
        let (pool, mut group, _chat_id, signing_key) = setup_group_and_chat().await?;
        let mut connection = pool.write().await?;

        let leave_params = group
            .group_mut()
            .stage_leave_group(&mut connection, &signing_key)?;
        let pending =
            PendingChatOperation::new(group, OperationType::Leave(Box::new(leave_params)));
        pending.store(&mut connection).await?;

        // Initially the job is ready to retry.
        let uuid = Uuid::new_v4();
        let now = Utc::now();
        connection
            .with_transaction(async |txn| {
                let ready = PendingChatOperation::dequeue(&mut *txn, uuid, now).await?;
                assert!(ready.is_some());

                pending
                    .mark_as_waiting_for_queue_response(&mut *txn)
                    .await?;

                // After marking, it should no longer be returned for retries.
                let uuid = Uuid::new_v4();
                let ready = PendingChatOperation::dequeue(txn, uuid, now).await?;
                assert!(ready.is_none());

                Ok(())
            })
            .await
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn not_found_ds_error_is_routed_to_not_found() -> anyhow::Result<()> {
        let (pool, mut group, _chat_id, signing_key) = setup_group_and_chat().await?;

        let leave_params = group
            .group_mut()
            .stage_leave_group(pool.write().await?, &signing_key)?;
        let mut pending =
            PendingChatOperation::new(group, OperationType::Leave(Box::new(leave_params)));

        // A "group not found" response from the DS must be classified as
        // NotFound so the group is torn down, not retried as a generic fatal.
        let error = DsRequestError::Tonic(tonic::Status::not_found("group not found"));
        let result = pending.handle_error(pool.write().await?, error).await;

        assert_matches!(result, Ok(JobError::NotFound));

        Ok(())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn delete_removes_pending_operation() -> anyhow::Result<()> {
        let (pool, mut group, chat_id, signing_key) = setup_group_and_chat().await?;
        let mut connection = pool.write().await?;

        let leave_params = group
            .group_mut()
            .stage_leave_group(&mut connection, &signing_key)?;
        let pending =
            PendingChatOperation::new(group, OperationType::Leave(Box::new(leave_params)));
        pending.store(&mut connection).await?;

        // Delete and ensure the row is gone.
        connection
            .with_transaction(async |txn| {
                PendingChatOperation::delete(&mut *txn, pending.group.group_id()).await?;

                let loaded = PendingChatOperation::load(&mut *txn, &chat_id).await?;
                assert!(loaded.is_none());

                let uuid = Uuid::new_v4();
                let now = Utc::now();
                let ready = PendingChatOperation::dequeue(txn, uuid, now).await?;
                assert!(ready.is_none());

                Ok(())
            })
            .await
    }
}
