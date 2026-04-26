// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use airapiclient::ds_api::DsRequestError;
use aircommon::{
    credentials::{ClientCredential, keys::ClientSigningKey},
    identifiers::{QualifiedGroupId, UserId},
    messages::client_ds_out::{DeleteGroupParamsOut, GroupOperationParamsOut, SelfRemoveParamsOut},
    time::TimeStamp,
};
use airprotos::client::group::GroupData;
use anyhow::{Context as _, anyhow, bail};
use chrono::{DateTime, Duration, Utc};
use mimi_room_policy::RoleIndex;
use openmls::group::GroupId;
use serde::{Deserialize, Serialize};
use sqlx::{query, query_as, query_scalar};
use tracing::{debug, error, info};

use crate::{
    Chat, ChatAttributes, ChatId, ChatMessage, ChatStatus, Contact, SystemMessage,
    chats::{GroupDataExt, messages::TimestampedMessage},
    clients::{CoreUser, api_clients::ApiClients, update_key::update_chat_attributes},
    contacts::ContactAddInfos,
    db_access::{WriteConnection, WriteDbConnection, WriteDbTransaction},
    groups::{
        Group, VerifiedGroup, client_auth_info::StorableClientCredential,
        handle_group_not_found_on_ds,
    },
    job::{Job, JobContext, JobError, chat_operation::ChatOperationError},
};

// Having separate retry intervals for test and non-test is a hack until we can
// pass "now" directly into OutboundService runs.

#[cfg(not(any(test, feature = "test_utils")))]
const RETRY_INTERVAL: Duration = Duration::seconds(5);
#[cfg(any(test, feature = "test_utils"))]
const RETRY_INTERVAL: Duration = Duration::seconds(1);

#[derive(Clone, Serialize, Deserialize)]
pub(super) enum OperationType {
    Leave(SelfRemoveParamsOut),
    Delete(DeleteGroupParamsOut),
    Other {
        params: Box<GroupOperationParamsOut>,
        /// New chat picture (if any)
        ///
        /// It was already uploaded as part of the external group profile but is not yet set as the
        /// chat picture.
        #[serde(with = "serde_bytes")]
        new_chat_picture: Option<Vec<u8>>,
    },
}

impl std::fmt::Display for OperationType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OperationType::Leave(_) => write!(f, "leave"),
            OperationType::Delete(_) => write!(f, "delete"),
            OperationType::Other { .. } => write!(f, "other"),
        }
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

    fn is_commit(&self) -> bool {
        match self {
            OperationType::Leave(_) => false,
            OperationType::Delete(_) | OperationType::Other { .. } => true,
        }
    }

    fn is_delete(&self) -> bool {
        matches!(self, OperationType::Delete(_))
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
        context: &mut JobContext<'_>,
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
                    .with_write_transaction(async |txn| {
                        handle_group_not_found_on_ds(txn, &group_id).await
                    })
                    .await?;
                Err(JobError::NotFound)
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

    pub async fn execute_internal(
        &mut self,
        context: &mut JobContext<'_>,
    ) -> Result<Vec<ChatMessage>, JobError<ChatOperationError>> {
        if let PendingChatOperationStatus::WaitingForQueueResponse = self.status {
            info!(
                group_id = ?self.group.group_id(),
                "Failed to execute PendingChatOperation for group because
                it is still waiting for a queue response",
            );
            return Err(JobError::Blocked);
        }

        let JobContext {
            api_clients,
            db,
            key_store,
            now,
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
            && let Some(message_epoch) = leave_params.remove_proposal.epoch()
            && message_epoch != self.group.mls_group().epoch()
            && self.number_of_attempts > 0
        {
            *leave_params = self
                .group
                .group_mut()
                .stage_leave_group(db.write().await?, signer)?;
        }

        let mut new_chat_picture = None;
        let res = match self.operation.clone() {
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
            OperationType::Other {
                params,
                new_chat_picture: chat_picture,
            } => {
                new_chat_picture = chat_picture;
                api_client
                    .ds_group_operation(*params, signer, self.group.group_state_ear_key())
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
            .with_write_transaction(async |txn| {
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

                self.group
                    .group_mut()
                    .store_update(&mut *txn, Some(ds_timestamp))
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

    async fn handle_error(
        &mut self,
        connection: impl WriteConnection,
        error: DsRequestError,
    ) -> Result<JobError<ChatOperationError>, JobError<ChatOperationError>> {
        debug!(?error, "DS request failed");
        const MAX_RETRIES: u32 = 5;
        if error.is_wrong_epoch() {
            // If we get a WrongEpochError, we know the commit was
            // either accepted on a previous try, or the DS rejected
            // it because another one got there first.
            self.mark_as_waiting_for_queue_response(connection).await?;

            Err(JobError::Blocked)
        } else if error.is_not_found() {
            Err(JobError::NotFound)
        } else if (error.is_network_error() || self.number_of_attempts > 0)
            && self.number_of_attempts < MAX_RETRIES
        {
            // If we either get a network error (which means we don't know
            // whether the request has been processed by the DS), or if we've
            // gotten a network error in the past, we want to try again until
            // we've either succeeded or reached a max number of retries.
            Ok(JobError::NetworkError)
        } else {
            // For other errors or if the max number of retries has been
            // reached, we consider the operation failed and delete the job.
            connection
                .with_transaction(async |txn| -> anyhow::Result<_> {
                    self.group
                        .group_mut()
                        .discard_pending_commit(&mut *txn)
                        .await?;
                    Self::delete(txn, self.group.group_id()).await?;
                    Ok(())
                })
                .await?;

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

        let params = group
            .group_mut()
            .stage_remove(&mut *txn, signer, target_users)
            .await?;

        let job = Self::new(group, OperationType::other(params));
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

        let job = Self::new(group, OperationType::Leave(params));
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
        let chat = Chat::load(&mut *txn, &chat_id)
            .await?
            .with_context(|| format!("Can't find chat with id {chat_id}"))?;
        let group_id = chat.group_id();
        let mut group = Group::load_clean_verified(&mut *txn, group_id)
            .await?
            .with_context(|| format!("Can't find group with id {group_id:?}"))?;
        let group_data_bytes = new_group_data.map(|data| data.encode()).transpose()?;

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
            let message = group.group_mut().stage_delete(&mut *txn, signer).await?;

            let job = Self::new(group, OperationType::Delete(message));
            job.store(txn).await?;
            Ok(Some(job))
        }
    }

    pub(crate) async fn create_add(
        connection: &mut WriteDbConnection,
        api_clients: &ApiClients,
        signer: &ClientSigningKey,
        chat_id: ChatId,
        new_members: Vec<UserId>,
    ) -> Result<Self, JobError<ChatOperationError>> {
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
            let add_info = contact
                .fetch_add_infos(&mut *connection, api_clients)
                .await?;
            contact_add_infos.push(add_info);
        }

        let group_id = chat.group_id();
        connection
            .with_transaction(async |txn| {
                let mut group = Group::load_clean_verified(&mut *txn, group_id)
                    .await
                    .map_err(JobError::fatal)?
                    .with_context(|| format!("Can't find group with id {group_id:?}"))
                    .map_err(JobError::fatal)?;

                let own_id = signer.credential().user_id();

                // Room policy check (doesn't apply changes to room state yet)
                for target in &new_members {
                    group.verify_role_change(own_id, target, RoleIndex::Regular)?;
                }

                // Adds new member and stages commit
                let params = group
                    .group_mut()
                    .stage_invite(
                        &mut *txn,
                        signer,
                        contact_add_infos,
                        contact_wai_keys,
                        client_credentials,
                    )
                    .await?
                    // Check if we got a leaf node validation error which is domain specific and should
                    // be propagated to the user.
                    .map_err(|validation| JobError::domain(ChatOperationError::from(validation)))?;

                // Create PendingChatOperation job
                let pending_chat_operation =
                    PendingChatOperation::new(group, OperationType::other(params));
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

    use crate::db_access::{ReadConnection, WriteConnection, WriteDbTransaction};

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
            buf: &mut <sqlx::Sqlite as sqlx::Database>::ArgumentBuffer<'_>,
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
            buf: &mut <sqlx::Sqlite as sqlx::Database>::ArgumentBuffer<'_>,
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

    use crate::db_access::ReadConnection;

    use super::*;

    pub struct PendingChatOperationInfo {
        pub operation_type: String,
        pub request_status: String,
        pub number_of_attempts: u32,
    }

    impl PendingChatOperationInfo {
        pub async fn load(
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
}

#[cfg(test)]
mod tests {
    use aircommon::{
        credentials::{keys::ClientSigningKey, test_utils::create_test_credentials},
        crypto::aead::keys::IdentityLinkWrapperKey,
        identifiers::{QualifiedGroupId, UserId},
    };
    use chrono::{Duration, Utc};
    use uuid::Uuid;

    use crate::{
        ChatAttributes, db_access::DbAccess, groups::GroupDataBytes,
        utils::persistence::open_db_in_memory,
    };

    use super::*;

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
        let mut connection = pool.read().await?;

        let leave_params = group
            .group_mut()
            .stage_leave_group(pool.write().await?, &signing_key)?;
        let pending = PendingChatOperation::new(group, OperationType::Leave(leave_params));

        pending.store(pool.write().await?).await?;

        let loaded = PendingChatOperation::load(&mut connection, &chat_id)
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
        let mut pending = PendingChatOperation::new(group, OperationType::Leave(leave_params));
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
        let pending = PendingChatOperation::new(group, OperationType::Leave(leave_params));
        pending.store(&mut connection).await?;

        // Initially the job is ready to retry.
        let uuid = Uuid::new_v4();
        let now = Utc::now();
        connection
            .with_transaction(async |txn| {
                let ready = PendingChatOperation::dequeue(txn, uuid, now).await?;
                assert!(ready.is_some());

                pending.mark_as_waiting_for_queue_response(txn).await?;

                // After marking, it should no longer be returned for retries.
                let uuid = Uuid::new_v4();
                let ready = PendingChatOperation::dequeue(txn, uuid, now).await?;
                assert!(ready.is_none());

                Ok(())
            })
            .await
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn delete_removes_pending_operation() -> anyhow::Result<()> {
        let (pool, mut group, chat_id, signing_key) = setup_group_and_chat().await?;
        let mut connection = pool.write().await?;

        let leave_params = group
            .group_mut()
            .stage_leave_group(&mut connection, &signing_key)?;
        let pending = PendingChatOperation::new(group, OperationType::Leave(leave_params));
        pending.store(&mut connection).await?;

        // Delete and ensure the row is gone.
        connection
            .with_transaction(async |txn| {
                PendingChatOperation::delete(txn, pending.group.group_id()).await?;

                let loaded = PendingChatOperation::load(txn, &chat_id).await?;
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
