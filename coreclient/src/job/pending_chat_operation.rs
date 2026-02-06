// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::{
    Chat, ChatAttributes, ChatId, ChatMessage, ChatStatus, Contact, SystemMessage,
    chats::messages::TimestampedMessage,
    clients::{CoreUser, api_clients::ApiClients, update_key::update_chat_attributes},
    contacts::ContactAddInfos,
    groups::{Group, GroupData, client_auth_info::StorableClientCredential},
    job::{Job, JobContext, JobError},
    store::StoreNotifier,
    utils::connection_ext::ConnectionExt,
};
use airapiclient::ds_api::DsRequestError;
use aircommon::{
    codec::PersistenceCodec,
    credentials::{ClientCredential, keys::ClientSigningKey},
    identifiers::{QualifiedGroupId, UserId},
    messages::client_ds_out::{DeleteGroupParamsOut, GroupOperationParamsOut, SelfRemoveParamsOut},
    time::TimeStamp,
};
use anyhow::{Context as _, anyhow, bail};
use chrono::{DateTime, Duration, Utc};
use mimi_room_policy::RoleIndex;
use openmls::group::GroupId;
use serde::{Deserialize, Serialize};
use sqlx::{SqliteConnection, SqlitePool, SqliteTransaction, query, query_as};
use std::collections::HashSet;

#[derive(Clone, derive_more::From, Serialize, Deserialize)]
pub(super) enum OperationType {
    Leave(SelfRemoveParamsOut),
    Delete(DeleteGroupParamsOut),
    Other(Box<GroupOperationParamsOut>),
}

impl ToString for OperationType {
    fn to_string(&self) -> String {
        match self {
            OperationType::Leave(_) => "leave".to_string(),
            OperationType::Delete(_) => "delete".to_string(),
            OperationType::Other(_) => "other".to_string(),
        }
    }
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

#[derive(Debug)]
enum PendingChatOperationStatus {
    ReadyToRetry,
    WaitingForQueueResponse,
}

/// Represents a pending chat operation to be retried.
pub(crate) struct PendingChatOperation {
    group: Group,
    operation: OperationType,
    // The time at which the operation should be retried. If None, it can be
    // retried immediately.
    retry_due_at: Option<DateTime<Utc>>,
    status: PendingChatOperationStatus,
    number_of_attempts: u32,
}

impl Job for PendingChatOperation {
    type Output = Vec<ChatMessage>;

    async fn execute_logic(
        self,
        context: &mut JobContext<'_>,
    ) -> Result<Vec<ChatMessage>, JobError> {
        self.execute_internal(context).await
    }
}

impl PendingChatOperation {
    pub(super) fn new(group: Group, message: impl Into<OperationType>) -> Self {
        Self {
            group,
            operation: message.into(),
            retry_due_at: Utc::now().into(),
            status: PendingChatOperationStatus::ReadyToRetry,
            number_of_attempts: 0,
        }
    }

    pub fn group_id(&self) -> &GroupId {
        self.group.group_id()
    }

    pub async fn execute_internal(
        mut self,
        context: &mut JobContext<'_>,
    ) -> Result<Vec<ChatMessage>, JobError> {
        if matches!(
            self.status,
            PendingChatOperationStatus::WaitingForQueueResponse
        ) {
            tracing::info!(
                group_id = ?self.group.group_id(), "Failed to execute PendingChatOperation for group because it is still waiting for a queue response",
            );
            return Err(JobError::NetworkError);
        }

        let JobContext {
            api_clients,
            pool,
            notifier,
            key_store,
            now,
        } = context;
        let signer = &key_store.signing_key;
        let own_user_id = signer.credential().identity().clone();

        let qgid = QualifiedGroupId::try_from(self.group.group_id())?;

        let is_commit = self.operation.is_commit();
        let is_delete = self.operation.is_delete();
        let is_leave = matches!(self.operation, OperationType::Leave(_));

        let api_client = api_clients.get(qgid.owning_domain())?;

        let retry_due = *now + Duration::seconds(5);
        pool.with_connection(async |connection| {
            self.update_retry_due_at(connection, retry_due).await
        })
        .await?;

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
            OperationType::Other(params) => {
                api_client
                    .ds_group_operation(*params, signer, self.group.group_state_ear_key())
                    .await
            }
        };

        let mut have_left_successfully = true;
        let ds_timestamp = match res {
            Ok(ds_timestamp) => ds_timestamp,
            Err(error) => {
                self.handle_error(pool, is_leave, error).await?;

                // The only case where we reach here is for leave operations
                // with a network error, in which case we want to continue
                // processing as if the operation were successful.
                have_left_successfully = false;
                TimeStamp::now()
            }
        };

        // If any of the following fails, something is very wrong.
        let messages = pool
            .with_transaction(async |txn| {
                let Some(mut chat) = Chat::load_by_group_id(txn, self.group.group_id()).await?
                else {
                    bail!("Chat not found for group: {:?}", self.group.group_id());
                };

                // Get the past members before merging the commit
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
                } else if is_leave && !matches!(chat.status(), ChatStatus::Inactive(_)) {
                    // Post-process leave operation. No need to repeat this if
                    // it has already happened once (indicated by chat being
                    // inactive).

                    self.group.room_state_change_role(
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
                    // post-processing has already happened.
                    // If we were successful this time, delete the job.
                    if have_left_successfully {
                        Self::delete(txn, self.group.group_id()).await?;
                    }

                    // In either case, just return an empty vec of messages.
                    return Ok(vec![]);
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

    async fn handle_error(
        &mut self,
        pool: &mut SqlitePool,
        is_leave: bool,
        error: DsRequestError,
    ) -> Result<(), JobError> {
        if error.is_wrong_epoch() && is_leave {
            // The leave action is special in that we want to consider
            // it successful regardless of any DS errors and
            // post-process anyway. If the DS returned an error, we'll
            // try again later, but that's just for the benefit of the
            // server and the other chat members.
            tracing::info!(
                group_id = ?self.group.group_id(), "Leave operation failed due to WrongEpochError, proceeding with local post-processing"
            );
            Ok(())
        } else if error.is_wrong_epoch() {
            // If we get a WrongEpochError, we know the commit was
            // either accepted on a previous try, or the DS rejected
            // it because another one got there first.
            pool.with_connection(async |connection| {
                self.mark_as_waiting_for_queue_response(connection).await
            })
            .await?;
            // We return a FatalError here to indicate that the job should be
            // considered failed.
            return Err(JobError::FatalError(anyhow!("WrongEpochError")));
        } else if error.is_network_error() || self.number_of_attempts > 0 {
            // A network error is an error where we don't know whether the
            // server has received and processed the request. If we either get a
            // network error, or if we've gotten a network error in the past, we
            // want to try again until we've either succeeded or reached a max
            // number of retries.

            const MAX_RETRIES: u32 = 5;
            if self.number_of_attempts >= MAX_RETRIES {
                pool.with_transaction(async |txn| {
                    self.group.discard_pending_commit(txn).await?;
                    Self::delete(txn, self.group.group_id()).await?;
                    Ok(())
                })
                .await?;
                return Err(JobError::FatalError(anyhow!(
                    "Job failed after {} attempts due to network errors",
                    MAX_RETRIES
                )));
            }

            // If we haven't reached the max number of retries leave the job
            // as-is, so it can be retried later.
            return Err(JobError::NetworkError);
        } else {
            // For other errors, we consider the operation failed and delete the
            // job.
            pool.with_transaction(async |txn| {
                self.group.discard_pending_commit(txn).await?;
                Self::delete(txn, self.group.group_id()).await?;
                Ok(())
            })
            .await?;
            return Err(JobError::FatalError(anyhow!(
                "Job failed due to an unexpected error: {:?}",
                error
            )));
        }
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

mod persistence {
    use std::str::FromStr;

    use sqlx::SqliteExecutor;
    use thiserror::Error;
    use uuid::Uuid;

    use super::*;

    #[derive(Debug, Error)]
    #[error("Invalid PendingChatOperationStatus: {actual}")]
    pub struct PendingChatOperationStatusError {
        pub actual: String,
    }

    impl FromStr for PendingChatOperationStatus {
        type Err = PendingChatOperationStatusError;

        fn from_str(s: &str) -> Result<Self, Self::Err> {
            match s {
                "ready_to_retry" => Ok(PendingChatOperationStatus::ReadyToRetry),
                "waiting_for_queue_response" => {
                    Ok(PendingChatOperationStatus::WaitingForQueueResponse)
                }
                s => Err(PendingChatOperationStatusError {
                    actual: s.to_string(),
                }),
            }
        }
    }

    struct SqlPendingChatOperation {
        group_id: Vec<u8>,
        operation_data: Vec<u8>,
        retry_due_at: Option<DateTime<Utc>>,
        request_status: String,
        number_of_attempts: i64,
    }

    impl SqlPendingChatOperation {
        async fn into_pending_chat_operation(
            self,
            connection: &mut SqliteConnection,
        ) -> sqlx::Result<PendingChatOperation> {
            let group_id = GroupId::from_slice(&self.group_id);
            let group = Group::load(connection, &group_id)
                .await?
                // This shouldn't happen, as the pending operation references an
                // existing group inside the database.
                .ok_or(sqlx::Error::RowNotFound)?;
            let operation: OperationType = PersistenceCodec::from_slice(&self.operation_data)
                .map_err(|e| sqlx::Error::Decode(Box::new(e)))?;
            let status = PendingChatOperationStatus::from_str(&self.request_status)
                .map_err(|e| sqlx::Error::Decode(Box::new(e)))?;

            Ok(PendingChatOperation {
                group,
                operation,
                retry_due_at: self.retry_due_at,
                status,
                number_of_attempts: self.number_of_attempts as u32,
            })
        }
    }

    impl PendingChatOperation {
        pub(super) async fn store(&self, connection: &mut SqliteConnection) -> sqlx::Result<()> {
            let operation_data = PersistenceCodec::to_vec(&self.operation)
                .map_err(|e| sqlx::Error::Encode(Box::new(e)))?;
            let group_id = self.group.group_id().as_slice();
            let operation_string = self.operation.to_string();
            // Store the pending operation in the database.
            query!("INSERT INTO pending_chat_operation (group_id, operation_type, operation_data, retry_due_at, request_status) VALUES (?, ?, ?, ?, ?)",
            group_id,
            operation_string,
            operation_data,
            self.retry_due_at,
            "ready_to_retry"
        )
        .execute(connection)
        .await?;

            Ok(())
        }

        pub(super) async fn update_retry_due_at(
            &mut self,
            connection: impl SqliteExecutor<'_>,
            retry_due: DateTime<Utc>,
        ) -> sqlx::Result<()> {
            let group_id = self.group.group_id().as_slice();
            // Update the retry due timestamp in the database and increase number_of_attempts.
            query!(
                "UPDATE pending_chat_operation SET retry_due_at = ?, number_of_attempts = number_of_attempts + 1 WHERE group_id = ?",
                retry_due,
                group_id
            )
            .execute(connection)
            .await?;

            self.retry_due_at = Some(retry_due);
            self.number_of_attempts += 1;

            Ok(())
        }

        pub(super) async fn mark_as_waiting_for_queue_response(
            &self,
            connection: &mut SqliteConnection,
        ) -> sqlx::Result<()> {
            let group_id = self.group.group_id().as_slice();
            query!(
                "UPDATE pending_chat_operation SET request_status = ? WHERE group_id = ?",
                "waiting_for_queue_response",
                group_id
            )
            .execute(connection)
            .await?;

            Ok(())
        }

        pub(crate) async fn load(
            connection: &mut SqliteConnection,
            chat_id: &ChatId,
        ) -> sqlx::Result<Option<Self>> {
            // Get the group id from the chat table and then load the pending operation.
            let sql_pending_operation = query_as!(
                SqlPendingChatOperation,
                r#"SELECT pco.group_id, pco.operation_data, pco.retry_due_at AS "retry_due_at: _", pco.request_status, pco.number_of_attempts
            FROM pending_chat_operation pco
            JOIN chat c ON pco.group_id = c.group_id
            WHERE c.chat_id = ?"#,
                chat_id
            )
            .fetch_optional(&mut *connection)
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
            executor: impl SqliteExecutor<'_>,
            chat_id: &ChatId,
        ) -> sqlx::Result<bool> {
            let record = query!(
                "SELECT EXISTS(SELECT 1
            FROM pending_chat_operation pco
            JOIN chat c ON pco.group_id = c.group_id
            WHERE c.chat_id = ? LIMIT 1) AS row_exists",
                chat_id,
            )
            .fetch_one(executor)
            .await?;
            Ok(record.row_exists == 1)
        }

        /// Dequeue a PendingChatOperation for retry by the OutboundService.
        pub(crate) async fn dequeue(
            connection: &mut SqliteConnection,
            task_id: Uuid,
            now: DateTime<Utc>,
        ) -> anyhow::Result<Option<Self>> {
            let sql_pending_operation = query_as!(
                SqlPendingChatOperation,
                r#"UPDATE pending_chat_operation
                    SET locked_by = ?1
                    WHERE group_id = (
                      SELECT group_id
                      FROM pending_chat_operation
                      WHERE (locked_by IS NULL OR locked_by != ?1)
                      AND request_status = "ready_to_retry"
                      AND retry_due_at <= ?2
                      LIMIT 1
                    )
                RETURNING
                    group_id,
                    operation_data,
                    retry_due_at AS "retry_due_at: _",
                    request_status,
                    number_of_attempts
                "#,
                task_id,
                now
            )
            .fetch_optional(&mut *connection)
            .await?;

            let Some(sql_pending_operation) = sql_pending_operation else {
                return Ok(None);
            };

            let pending_operation = sql_pending_operation
                .into_pending_chat_operation(connection)
                .await?;

            Ok(Some(pending_operation))
        }

        pub(crate) async fn delete(
            connection: &mut SqliteConnection,
            group_id: &GroupId,
        ) -> sqlx::Result<()> {
            let group_id = group_id.as_slice();
            // Delete the pending operation from the database.
            query!(
                "DELETE FROM pending_chat_operation WHERE group_id = ?",
                group_id
            )
            .execute(connection)
            .await?;
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aircommon::{
        credentials::{keys::ClientSigningKey, test_utils::create_test_credentials},
        identifiers::{QualifiedGroupId, UserId},
    };
    use chrono::{Duration, Utc};
    use sqlx::SqlitePool;
    use uuid::Uuid;

    use crate::{store::StoreNotifier, utils::persistence::open_db_in_memory};

    async fn setup_group_and_chat() -> anyhow::Result<(SqlitePool, Group, ChatId, ClientSigningKey)>
    {
        let pool = open_db_in_memory().await?;
        let mut connection = pool.acquire().await?;

        let user_id = UserId::random("example.com".parse().unwrap());
        let (_aic_sk, signing_key) = create_test_credentials(user_id.clone());

        let qgid = QualifiedGroupId::new(Uuid::new_v4(), user_id.domain().clone());
        let group_id = GroupId::from(qgid);
        let group_data = GroupData::from(b"test-group-data".to_vec());

        let (group, membership, _) =
            Group::create_group(&mut connection, &signing_key, group_id.clone(), group_data)?;
        group.store(&mut *connection).await?;
        membership.store(&mut *connection).await?;

        let mut notifier = StoreNotifier::noop();
        let chat = Chat::new_group_chat(
            group_id.clone(),
            ChatAttributes::new("Test chat".into(), None),
        );
        let chat_id = chat.id();
        chat.store(&mut connection, &mut notifier).await?;

        Ok((pool, group, chat_id, signing_key))
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn store_and_load_roundtrip() -> anyhow::Result<()> {
        let (pool, mut group, chat_id, signing_key) = setup_group_and_chat().await?;
        let mut connection = pool.acquire().await?;

        let leave_params = group.stage_leave_group(&mut connection, &signing_key)?;
        let pending = PendingChatOperation::new(group, leave_params);

        pending.store(&mut connection).await?;

        let loaded = PendingChatOperation::load(&mut connection, &chat_id)
            .await?
            .expect("should load");

        assert!(matches!(loaded.operation, OperationType::Leave(_)));
        assert_eq!(loaded.group.group_id(), pending.group.group_id());
        assert_eq!(loaded.retry_due_at, pending.retry_due_at);

        Ok(())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn update_retry_due_at_persists() -> anyhow::Result<()> {
        let (pool, mut group, chat_id, signing_key) = setup_group_and_chat().await?;
        let mut connection = pool.acquire().await?;

        let leave_params = group.stage_leave_group(&mut connection, &signing_key)?;
        let mut pending = PendingChatOperation::new(group, leave_params);
        pending.store(&mut connection).await?;

        let new_timestamp = Utc::now() + Duration::seconds(30);
        pending
            .update_retry_due_at(connection.as_mut(), new_timestamp)
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
        let mut connection = pool.acquire().await?;

        let leave_params = group.stage_leave_group(&mut connection, &signing_key)?;
        let pending = PendingChatOperation::new(group, leave_params);
        pending.store(&mut connection).await?;

        // Initially the job is ready to retry.
        let uuid = Uuid::new_v4();
        let now = Utc::now();
        let ready = PendingChatOperation::dequeue(&mut connection, uuid, now).await?;
        assert!(ready.is_some());

        pending
            .mark_as_waiting_for_queue_response(&mut connection)
            .await?;

        // After marking, it should no longer be returned for retries.
        let uuid = Uuid::new_v4();
        let ready = PendingChatOperation::dequeue(&mut connection, uuid, now).await?;
        assert!(ready.is_none());

        Ok(())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn delete_removes_pending_operation() -> anyhow::Result<()> {
        let (pool, mut group, chat_id, signing_key) = setup_group_and_chat().await?;
        let mut connection = pool.acquire().await?;

        let leave_params = group.stage_leave_group(&mut connection, &signing_key)?;
        let pending = PendingChatOperation::new(group, leave_params);
        pending.store(&mut connection).await?;

        // Delete and ensure the row is gone.
        PendingChatOperation::delete(&mut connection, pending.group.group_id()).await?;

        let loaded = PendingChatOperation::load(&mut connection, &chat_id).await?;
        assert!(loaded.is_none());

        let uuid = Uuid::new_v4();
        let now = Utc::now();
        let ready = PendingChatOperation::dequeue(&mut connection, uuid, now).await?;
        assert!(ready.is_none());

        Ok(())
    }
}
