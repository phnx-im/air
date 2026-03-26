// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::utils::connection_ext::ConnectionExt as _;
use aircommon::{
    credentials::keys::ClientSigningKey,
    crypto::ear::keys::{GroupStateEarKey, IdentityLinkWrapperKey},
    identifiers::QualifiedGroupId,
    messages::{client_ds::AadPayload, client_ds_out::ExternalCommitInfoIn},
};
use anyhow::{Context, Result};
use openmls::{
    group::GroupId,
    prelude::{LeafNodeIndex, MlsMessageOut},
};
use sqlx::{Connection, SqliteConnection, SqliteTransaction};
use tokio_util::sync::CancellationToken;
use tracing::{error, info};
use uuid::Uuid;

use crate::{
    ChatId,
    clients::{CoreUser, api_clients::ApiClients},
    groups::{Group, ProfileInfo},
    job::{operation::OperationData, profile::FetchUserProfileOperation},
    outbound_service::{OutboundServiceContext, error::OutboundServiceError},
};

pub(crate) struct Resync {
    pub(crate) chat_id: ChatId,
    pub(crate) group_id: GroupId,
    pub(crate) group_state_ear_key: GroupStateEarKey,
    pub(crate) identity_link_wrapper_key: IdentityLinkWrapperKey,
    pub(crate) original_leaf_index: LeafNodeIndex,
}

impl CoreUser {
    pub async fn enqueue_group_resync(&self, chat_id: ChatId) -> anyhow::Result<()> {
        let mut connection = self.pool().acquire().await?;
        let group = Group::load_with_chat_id(connection.as_mut(), chat_id)
            .await?
            .context("group not found")?;

        let resync = Resync {
            chat_id,
            group_id: group.group_id().clone(),
            group_state_ear_key: group.group_state_ear_key().clone(),
            identity_link_wrapper_key: group.identity_link_wrapper_key().clone(),
            original_leaf_index: group.own_index(),
        };

        resync.enqueue(&mut *connection).await?;

        self.outbound_service().notify_work();

        Ok(())
    }
}

impl OutboundServiceContext {
    pub(super) async fn perform_queued_resyncs(
        &self,
        run_token: &CancellationToken,
    ) -> anyhow::Result<()> {
        // Used to identify locked receipts by this task
        let task_id = Uuid::new_v4();
        loop {
            if run_token.is_cancelled() {
                return Ok(()); // the task is being stopped
            }

            let Some(resync) = self
                .pool
                .with_transaction(async |txn| Resync::dequeue(txn, task_id).await)
                .await?
            else {
                return Ok(());
            };
            info!(?resync.chat_id, "Performing chat resync");

            let mut connection = self.pool.acquire().await?;

            let group_id = resync.group_id.clone();

            let profile_infos = match resync
                .create_and_send_commit(&mut connection, &self.api_clients, self.signing_key())
                .await
            {
                Ok(profile_infos) => {
                    info!("Got profiles infos");
                    Resync::remove(&mut *connection, &group_id).await?;
                    // TODO: Schedule a job here that deals with fetching profile
                    // infos in the background.
                    profile_infos
                }
                Err(OutboundServiceError::Fatal(error)) => {
                    error!(%error, "Failed to send resync; dropping");
                    Resync::remove(&mut *connection, &group_id).await?;
                    return Err(error);
                }
                Err(OutboundServiceError::Recoverable(error)) => {
                    error!(%error, "Failed to send resync; will retry later");
                    continue;
                }
            };

            for ProfileInfo {
                client_credential,
                user_profile_key,
            } in profile_infos
            {
                if let Err(error) =
                    FetchUserProfileOperation::new(client_credential, user_profile_key)
                        .into_operation()
                        .enqueue(connection.as_mut())
                        .await
                {
                    error!(%error, "Failed to enqueue fetch profile operation");
                }
            }
        }
    }
}

impl Resync {
    /// Resync using an external commit.
    async fn create_and_send_commit(
        self,
        connection: &mut SqliteConnection,
        api_clients: &ApiClients,
        signer: &ClientSigningKey,
    ) -> Result<Vec<ProfileInfo>, OutboundServiceError> {
        // TODO: We should somehow mark the chat as "resyncing" in the DB and
        // reflect that in the UI.

        let external_commit_info = self
            .fetch_group_info(api_clients)
            .await
            .map_err(OutboundServiceError::recoverable)?;

        let original_leaf_index = self.original_leaf_index;

        let mut txn = connection
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(OutboundServiceError::recoverable)?;
        let (group, commit, group_info, member_profile_infos) = self
            .create_commit(&mut txn, api_clients, signer, external_commit_info)
            .await
            .map_err(OutboundServiceError::fatal)?;
        txn.commit()
            .await
            .map_err(OutboundServiceError::recoverable)?;

        Self::send_commit(
            api_clients,
            signer,
            &group,
            commit,
            group_info,
            original_leaf_index,
        )
        .await
        .map_err(OutboundServiceError::recoverable)?;

        Ok(member_profile_infos)
    }

    async fn fetch_group_info(&self, api_clients: &ApiClients) -> Result<ExternalCommitInfoIn> {
        let qgid: QualifiedGroupId = self.group_id.clone().try_into()?;
        let api_client = api_clients.get(qgid.owning_domain())?;
        let external_commit_info = api_client
            .ds_external_commit_info(self.group_id.clone(), &self.group_state_ear_key)
            .await?;

        Ok(external_commit_info)
    }

    async fn create_commit(
        self,
        txn: &mut SqliteTransaction<'_>,
        // Needs api clients until we can schedule group member authentication
        api_clients: &ApiClients,
        signer: &ClientSigningKey,
        external_commit_info: ExternalCommitInfoIn,
    ) -> Result<(Group, MlsMessageOut, MlsMessageOut, Vec<ProfileInfo>)> {
        // TODO: We should somehow mark the chat as "resyncing" in the DB and
        // reflect that in the UI.

        // Delete any old group states if they exist
        Group::delete_from_db(txn, &self.group_id).await?;

        let aad = AadPayload::Resync.into();
        let (new_group, commit, group_info, member_profile_info) = Group::join_group_externally(
            txn,
            api_clients,
            external_commit_info,
            signer,
            self.group_state_ear_key,
            self.identity_link_wrapper_key,
            aad,
            None, // This is not in response to a connection offer.
        )
        .await??;

        Ok((new_group, commit, group_info, member_profile_info))
    }

    async fn send_commit(
        api_clients: &ApiClients,
        signer: &ClientSigningKey,
        group: &Group,
        commit: MlsMessageOut,
        group_info: MlsMessageOut,
        original_leaf_index: LeafNodeIndex,
    ) -> Result<()> {
        let qgid: QualifiedGroupId = group.group_id().try_into()?;
        let api_client = api_clients.get(qgid.owning_domain())?;

        api_client
            .ds_resync(
                commit,
                group_info,
                signer,
                group.group_state_ear_key(),
                original_leaf_index,
            )
            .await?;
        Ok(())
    }
}

mod persistence {

    use sqlx::{SqliteExecutor, SqliteTransaction, query, query_as, query_scalar};
    use tracing::debug;
    use uuid::Uuid;

    use crate::ChatId;

    use super::*;

    impl Resync {
        pub(crate) async fn enqueue(&self, executor: impl SqliteExecutor<'_>) -> sqlx::Result<()> {
            debug!(
                ?self.group_id,
                ?self.chat_id,
                "Enqueueing resync"
            );

            let group_id_bytes = self.group_id.as_slice();
            let original_leaf_index = self.original_leaf_index.u32() as i32;
            query!(
                "INSERT INTO resync_queue
                    (group_id, chat_id,  group_state_ear_key, identity_link_wrapper_key, original_leaf_index)
                VALUES (?1, ?2, ?3, ?4, ?5)
                ON CONFLICT DO NOTHING",
                group_id_bytes,
                self.chat_id,
                self.group_state_ear_key,
                self.identity_link_wrapper_key,
                original_leaf_index
            )
            .execute(executor)
            .await?;
            Ok(())
        }

        /// Dequeue a resync operation for processing that has not been locked
        /// by this task.
        pub(crate) async fn dequeue(
            txn: &mut SqliteTransaction<'_>,
            task_id: Uuid,
        ) -> anyhow::Result<Option<Resync>> {
            struct ResyncRecord {
                chat_id: ChatId,
                group_id: Vec<u8>,
                group_state_ear_key: GroupStateEarKey,
                identity_link_wrapper_key: IdentityLinkWrapperKey,
                original_leaf_index: i32,
            }

            let Some(group_id) = query_scalar!(
                r#"
                SELECT group_id
                FROM resync_queue
                WHERE locked_by IS NULL OR locked_by != ?1
                LIMIT 1
                "#,
                task_id,
            )
            .fetch_optional(txn.as_mut())
            .await?
            else {
                return Ok(None);
            };

            let resync = query_as!(
                ResyncRecord,
                r#"UPDATE resync_queue
                    SET locked_by = ?2
                    WHERE group_id = ?1
                RETURNING
                    chat_id AS "chat_id: _",
                    group_id AS "group_id: _",
                    group_state_ear_key AS "group_state_ear_key: _",
                    identity_link_wrapper_key AS "identity_link_wrapper_key: _",
                    original_leaf_index AS "original_leaf_index: _"
                "#,
                group_id,
                task_id,
            )
            .fetch_optional(txn.as_mut())
            .await?
            .map(|record| Resync {
                chat_id: record.chat_id,
                group_id: GroupId::from_slice(&record.group_id),
                group_state_ear_key: record.group_state_ear_key,
                identity_link_wrapper_key: record.identity_link_wrapper_key,
                original_leaf_index: LeafNodeIndex::new(record.original_leaf_index as u32),
            });

            Ok(resync)
        }

        pub(crate) async fn is_pending_for_chat(
            executor: impl SqliteExecutor<'_>,
            chat_id: &ChatId,
        ) -> sqlx::Result<bool> {
            let record = query!(
                "SELECT EXISTS(SELECT 1 FROM resync_queue WHERE chat_id = ? LIMIT 1) AS row_exists",
                chat_id,
            )
            .fetch_one(executor)
            .await?;
            Ok(record.row_exists == 1)
        }

        pub(crate) async fn remove(
            executor: impl SqliteExecutor<'_>,
            group_id: &GroupId,
        ) -> sqlx::Result<()> {
            let group_id_bytes = group_id.as_slice();
            query!(
                "DELETE FROM resync_queue WHERE group_id = ?",
                group_id_bytes
            )
            .execute(executor)
            .await?;
            Ok(())
        }
    }
}
