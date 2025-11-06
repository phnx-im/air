// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::{
    credentials::keys::ClientSigningKey,
    crypto::ear::keys::{GroupStateEarKey, IdentityLinkWrapperKey},
    identifiers::QualifiedGroupId,
    messages::{client_ds::AadPayload, client_ds_out::ExternalCommitInfoIn},
};
use anyhow::Result;
use openmls::{
    group::GroupId,
    prelude::{LeafNodeIndex, MlsMessageOut},
};
use sqlx::{Connection, SqliteConnection, SqliteTransaction};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error};
use uuid::Uuid;

use crate::{
    ChatId, UserProfile,
    clients::api_clients::ApiClients,
    groups::{Group, ProfileInfo},
    outbound_service::{OutboundService, OutboundServiceContext},
    utils::connection_ext::StoreExt,
};

pub(crate) struct Resync {
    pub(crate) chat_id: ChatId,
    pub(crate) group_id: GroupId,
    pub(crate) group_state_ear_key: GroupStateEarKey,
    pub(crate) identity_link_wrapper_key: IdentityLinkWrapperKey,
    pub(crate) original_leaf_index: LeafNodeIndex,
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

            let Some(resync) = Resync::dequeue(&self.pool, task_id).await? else {
                return Ok(());
            };
            debug!(?resync.chat_id, "dequeued resync");

            let mut connection = self.pool.acquire().await?;

            let group_id = resync.group_id.clone();

            let profile_infos = match resync
                .create_and_send_commit(&mut connection, &self.api_clients, &self.signing_key)
                .await
            {
                Ok(profile_infos) => {
                    Resync::remove(&mut *connection, &group_id).await?;
                    // TODO: Schedule a job here that deals with fetching profile
                    // infos in the background.
                    profile_infos
                }
                Err(SendResyncError::Fatal(error)) => {
                    error!(%error, "Failed to send resync; dropping");
                    Resync::remove(&mut *connection, &group_id).await?;
                    return Err(error);
                }
                Err(SendResyncError::Recoverable(error)) => {
                    error!(%error, "Failed to send resync; will retry later");
                    continue;
                }
            };

            for profile_info in profile_infos {
                if let Err(e) = UserProfile::fetch_and_store(
                    &mut connection,
                    &mut self.notifier(),
                    &self.api_clients,
                    profile_info,
                )
                .await
                {
                    error!(%e, "Failed to fetch and store user profile info during resync");
                };
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
    ) -> Result<Vec<ProfileInfo>, SendResyncError> {
        // TODO: We should somehow mark the chat as "resyncing" in the DB and
        // reflect that in the UI.

        let external_commit_info = self
            .fetch_group_info(api_clients)
            .await
            .map_err(SendResyncError::recoverable)?;

        let original_leaf_index = self.original_leaf_index;

        let mut txn = connection
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(SendResyncError::recoverable)?;
        let (group, commit, group_info, member_profile_infos) = self
            .create_commit(&mut txn, api_clients, signer, external_commit_info)
            .await
            .map_err(SendResyncError::fatal)?;
        txn.commit().await.map_err(SendResyncError::recoverable)?;

        Self::send_commit(
            api_clients,
            signer,
            &group,
            commit,
            group_info,
            original_leaf_index,
        )
        .await
        .map_err(SendResyncError::recoverable)?;

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
        .await?;

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

enum SendResyncError {
    Fatal(anyhow::Error),
    Recoverable(anyhow::Error),
}

impl SendResyncError {
    pub fn fatal<E: Into<anyhow::Error>>(error: E) -> Self {
        SendResyncError::Fatal(error.into())
    }

    pub fn recoverable<E: Into<anyhow::Error>>(error: E) -> Self {
        SendResyncError::Recoverable(error.into())
    }
}

impl OutboundService {
    #[allow(dead_code)]
    pub(crate) async fn enqueue_resync(&self, resync: Resync) -> anyhow::Result<()> {
        let mut connection = self.context.pool.acquire().await?;

        resync.enqueue(&mut *connection).await?;

        self.notify_work();

        Ok(())
    }
}

mod persistence {

    use sqlx::{SqliteExecutor, query, query_as};
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
            connection: impl SqliteExecutor<'_>,
            task_id: Uuid,
        ) -> anyhow::Result<Option<Resync>> {
            struct ResyncRecord {
                chat_id: ChatId,
                group_id: Vec<u8>,
                group_state_ear_key: GroupStateEarKey,
                identity_link_wrapper_key: IdentityLinkWrapperKey,
                original_leaf_index: i32,
            }

            let resync = query_as!(
                ResyncRecord,
                r#"UPDATE resync_queue
                    SET locked_by = ?1
                    WHERE group_id = (
                      SELECT group_id 
                      FROM resync_queue
                      WHERE locked_by IS NULL OR locked_by != ?1
                      LIMIT 1
                    )
                RETURNING
                    chat_id AS "chat_id: _",
                    group_id AS "group_id: _",
                    group_state_ear_key AS "group_state_ear_key: _",
                    identity_link_wrapper_key AS "identity_link_wrapper_key: _",
                    original_leaf_index AS "original_leaf_index: _"
                "#,
                task_id,
            )
            .fetch_optional(connection)
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
