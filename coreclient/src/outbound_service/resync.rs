// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::{
    credentials::keys::ClientSigningKey,
    crypto::aead::keys::{GroupStateEarKey, IdentityLinkWrapperKey},
    identifiers::QualifiedGroupId,
    messages::{client_ds::AadPayload, client_ds_out::ExternalCommitInfoIn},
    time::TimeStamp,
};
use airprotos::client::group::GroupData;
use anyhow::{Context, Result, anyhow};
use apqmls::commit_builder::ApqCommitMessageBundle;
use openmls::{
    group::GroupId,
    prelude::{LeafNodeIndex, MlsMessageOut},
};
use tokio_util::sync::CancellationToken;
use tracing::{error, info};
use uuid::Uuid;

use crate::{
    Chat, ChatId, ChatMessage, ChatStatus, SystemMessage,
    chats::{ChatAttributes, GroupDataExt},
    clients::{CoreUser, api_clients::ApiClients, multi_device::HigherLevelGroup},
    db::access::{WriteConnection, WriteDbTransaction},
    groups::{
        DecryptedProfileInfos, Group, ProfileInfo, handle_group_not_found_on_ds,
        self_group::SelfGroup,
    },
    job::{operation::OperationData, profile::FetchUserProfileOperation},
    outbound_service::{
        OutboundServiceContext,
        error::{OutboundServiceError, classify_ds_error, is_ds_not_found_error},
    },
};

pub(crate) struct Resync {
    /// `None` while onboarding an emulator client into a higher-level group:
    /// there is no chat to point at yet, since it is created together with the
    /// group the external commit joins.
    pub(crate) chat_id: Option<ChatId>,
    pub(crate) group_id: GroupId,
    pub(crate) pq_group_id: Option<GroupId>,
    pub(crate) group_state_ear_key: GroupStateEarKey,
    pub(crate) identity_link_wrapper_key: IdentityLinkWrapperKey,
    /// The leaf the external commit evicts. When onboarding an emulator client
    /// this is the virtual client's prior membership, currently operated by a
    /// sibling emulator, rather than a leaf of our own.
    pub(crate) original_leaf_index: LeafNodeIndex,
    /// Whether the leaf this resync replaces is shared with sibling emulator
    /// clients, i.e. it onboards an emulator client into a higher-level group or
    /// re-syncs one that is already on a shared leaf. The new leaf then has to be
    /// derived from the virtual client's emulation epoch, which
    /// [`Resync::create_commit`] resolves when it builds the commit.
    pub(crate) shares_vc_leaf: bool,
}

impl CoreUser {
    pub async fn enqueue_group_resync(&self, chat_id: ChatId) -> anyhow::Result<()> {
        let group = Group::load_with_chat_id(self.db().read().await?, chat_id)
            .await?
            .context("group not found")?;

        let resync = Resync {
            chat_id: Some(chat_id),
            group_id: group.group_id().clone(),
            pq_group_id: group.pq_group_id(),
            group_state_ear_key: group.group_state_ear_key().clone(),
            identity_link_wrapper_key: group.identity_link_wrapper_key().clone(),
            original_leaf_index: group.own_index(),
            shares_vc_leaf: group.own_leaf_is_virtual_client(),
        };

        resync.enqueue(self.db().write().await?).await?;

        self.outbound_service().notify_work();

        Ok(())
    }

    /// Onboard this emulator client into every higher-level group the virtual
    /// client is already a member of, using variant B (external commit) of the
    /// mls-virtual-clients draft: queue a resync that evicts the virtual
    /// client's prior membership and re-joins on a leaf derived from the shared
    /// emulation epoch.
    ///
    /// Returns the number of groups queued. The external commits themselves run
    /// in the outbound service, which retries each one independently, so a group
    /// that is momentarily unreachable does not block linking.
    pub(crate) async fn enqueue_vc_onboarding(
        txn: &mut WriteDbTransaction<'_>,
        groups: Vec<HigherLevelGroup>,
    ) -> anyhow::Result<usize> {
        let mut queued = 0;
        for group in groups {
            let HigherLevelGroup {
                group_id,
                pq_group_id,
                group_state_ear_key,
                identity_link_wrapper_key,
                vc_leaf_index,
            } = group;

            let resync = Resync {
                chat_id: None,
                group_id,
                pq_group_id,
                group_state_ear_key,
                identity_link_wrapper_key,
                original_leaf_index: LeafNodeIndex::new(vc_leaf_index),
                shares_vc_leaf: true,
            };

            resync.enqueue(&mut *txn).await?;
            queued += 1;
        }

        Ok(queued)
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
                .db
                .with_write_transaction(async |txn| Resync::dequeue(txn, task_id).await)
                .await?
            else {
                return Ok(());
            };
            info!(?resync.chat_id, "Performing chat resync");

            let group_id = resync.group_id.clone();

            let result = {
                let mut connection = self.db.write().await?;

                let result = resync
                    .create_and_send_commit(&mut connection, &self.api_clients, self.signing_key())
                    .await;
                if let Ok((chat_id, _)) = &result {
                    info!("Got profiles infos");
                    Resync::remove(&mut connection, &group_id).await?;
                    connection.notifier().update(*chat_id);
                    // TODO: Schedule a job here that deals with fetching profile
                    // infos in the background.
                }
                result
            };

            let profile_infos = match result {
                Ok((_, profile_infos)) => profile_infos,
                Err(OutboundServiceError::Fatal(error)) => {
                    if is_ds_not_found_error(&error) {
                        error!(%error, "Group not found during resync; cleaning up local state");
                        self.db
                            .with_write_transaction(async |txn| {
                                handle_group_not_found_on_ds(txn, &group_id).await
                            })
                            .await?;
                        continue;
                    }

                    error!(%error, "Failed to send resync; dropping");
                    Resync::remove(self.db.write().await?, &group_id).await?;
                    return Err(error);
                }
                Err(OutboundServiceError::Recoverable(error)) => {
                    error!(%error, "Failed to send resync; will retry later");
                    continue;
                }
            };

            let mut connection = self.db.write().await?;
            for ProfileInfo {
                user_credential,
                user_profile_key,
            } in profile_infos.members
            {
                if let Err(error) =
                    FetchUserProfileOperation::new(user_credential, user_profile_key)
                        .into_operation()
                        .enqueue(&mut connection)
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
    ///
    /// Returns the chat the resync applies to, which for an onboarding resync is
    /// only created here, once the commit has been accepted.
    async fn create_and_send_commit(
        self,
        mut connection: impl WriteConnection,
        api_clients: &ApiClients,
        signer: &ClientSigningKey,
    ) -> Result<(ChatId, DecryptedProfileInfos), OutboundServiceError> {
        // TODO: We should somehow mark the chat as "resyncing" in the DB and
        // reflect that in the UI.

        if self.shares_vc_leaf
            && SelfGroup::load(&mut connection)
                .await
                .map_err(OutboundServiceError::recoverable)?
                .is_none()
        {
            return Err(OutboundServiceError::recoverable(anyhow!(
                "self group not joined yet; deferring onboarding of group {:?}",
                self.group_id
            )));
        }

        let external_commit_info = self.fetch_group_info(api_clients).await?;

        let original_leaf_index = self.original_leaf_index;
        let existing_chat_id = self.chat_id;

        let mut txn = connection
            .begin()
            .await
            .map_err(OutboundServiceError::recoverable)?;
        let (group, commit, member_profile_infos) =
            Box::pin(self.create_commit(&mut txn, api_clients, signer, external_commit_info))
                .await
                .map_err(OutboundServiceError::fatal)?;

        let chat_id = match existing_chat_id {
            Some(chat_id) => chat_id,
            None => Self::create_chat(&mut txn, &group, signer)
                .await
                .map_err(OutboundServiceError::fatal)?,
        };

        txn.commit()
            .await
            .map_err(OutboundServiceError::recoverable)?;

        Self::send_commit(api_clients, signer, &group, commit, original_leaf_index).await?;

        // Mark chat as active once the commit is accepted by the DS
        connection
            .with_transaction(async |txn| -> anyhow::Result<()> {
                Chat::update_status(txn, chat_id, &ChatStatus::Active).await?;
                Ok(())
            })
            .await
            .map_err(OutboundServiceError::recoverable)?;

        Ok((chat_id, member_profile_infos))
    }

    /// Create the local chat for a group we just onboarded into.
    async fn create_chat(
        txn: &mut WriteDbTransaction<'_>,
        group: &Group,
        signer: &ClientSigningKey,
    ) -> Result<ChatId> {
        let group_data_bytes = group.group_data().context("No group data")?;
        let group_data = GroupData::decode(&group_data_bytes)?;
        let (title, group_profile_part) = group_data.into_parts(group.identity_link_wrapper_key());
        let title = title.context("No group title")?;
        let sender_id = signer.credential().user_id().clone();
        let ds_timestamp = TimeStamp::now();

        let picture = CoreUser::resolve_group_profile_part(
            &mut *txn,
            group.group_id(),
            &sender_id,
            ds_timestamp,
            group_profile_part,
            true,
        )
        .await?;

        let chat = Chat::new_pending_group_chat(
            group.group_id().clone(),
            ChatAttributes { title, picture },
        );
        chat.store(&mut *txn).await?;

        let system_message =
            ChatMessage::new_system_message(chat.id(), ds_timestamp, SystemMessage::Onboarded);
        system_message.store(&mut *txn).await?;

        Ok(chat.id())
    }

    async fn fetch_group_info(
        &self,
        api_clients: &ApiClients,
    ) -> Result<ExternalCommitInfoIn, OutboundServiceError> {
        let qgid: QualifiedGroupId = self
            .group_id
            .clone()
            .try_into()
            .map_err(OutboundServiceError::fatal)?;
        let api_client = api_clients
            .get(qgid.owning_domain())
            .map_err(OutboundServiceError::fatal)?;
        api_client
            .ds_external_commit_info(
                self.group_id.clone(),
                self.pq_group_id.clone(),
                &self.group_state_ear_key,
            )
            .await
            .map_err(classify_ds_error)
    }

    async fn create_commit(
        self,
        txn: &mut WriteDbTransaction<'_>,
        // Needs api clients until we can schedule group member authentication
        api_clients: &ApiClients,
        signer: &ClientSigningKey,
        external_commit_info: ExternalCommitInfoIn,
    ) -> Result<(Group, ResyncCommit, DecryptedProfileInfos)> {
        // TODO: We should somehow mark the chat as "resyncing" in the DB and
        // reflect that in the UI.

        // Delete any old group states if they exist
        Group::delete_from_db(txn, &self.group_id).await?;

        let vc_epoch_id = if self.shares_vc_leaf {
            Some(CoreUser::register_self_group_vc_emulation_epoch(&mut *txn).await?)
        } else {
            None
        };

        let aad = AadPayload::Resync.into();
        if self.pq_group_id.is_some() {
            // APQ group
            let (group, bundle, member_profile_infos) = Group::join_apq_group_externally(
                txn,
                api_clients,
                external_commit_info,
                signer,
                self.group_state_ear_key,
                self.identity_link_wrapper_key,
                aad,
                vc_epoch_id,
            )
            .await??;
            Ok((
                group,
                ResyncCommit::PQ(Box::new(bundle)),
                member_profile_infos,
            ))
        } else {
            let (group, commit, group_info, member_profile_infos) = Group::join_group_externally(
                txn,
                api_clients,
                external_commit_info,
                signer,
                self.group_state_ear_key,
                self.identity_link_wrapper_key,
                aad,
                None, // This is not in response to a connection offer.
                vc_epoch_id,
            )
            .await??;
            Ok((
                group,
                ResyncCommit::T(Box::new(ResyncTCommit { commit, group_info })),
                member_profile_infos,
            ))
        }
    }

    async fn send_commit(
        api_clients: &ApiClients,
        signer: &ClientSigningKey,
        group: &Group,
        commit: ResyncCommit,
        original_leaf_index: LeafNodeIndex,
    ) -> Result<(), OutboundServiceError> {
        let qgid: QualifiedGroupId = group
            .group_id()
            .try_into()
            .map_err(OutboundServiceError::fatal)?;
        let api_client = api_clients
            .get(qgid.owning_domain())
            .map_err(OutboundServiceError::fatal)?;

        let response = match commit {
            ResyncCommit::T(commit) => {
                api_client
                    .ds_resync(
                        commit.commit,
                        commit.group_info,
                        signer,
                        group.group_state_ear_key(),
                        original_leaf_index,
                    )
                    .await
            }
            ResyncCommit::PQ(bundle) => {
                api_client
                    .ds_apq_resync(
                        *bundle,
                        signer,
                        group.group_state_ear_key(),
                        original_leaf_index,
                    )
                    .await
            }
        };

        response.map_err(classify_ds_error)?;
        Ok(())
    }
}

mod persistence {

    use sqlx::{query, query_as, query_scalar};
    use tracing::debug;
    use uuid::Uuid;

    use crate::{
        ChatId,
        db::access::{ReadConnection, WriteConnection, WriteDbTransaction},
        utils::persistence::{GroupIdRefWrapper, GroupIdWrapper},
    };

    use super::*;

    impl Resync {
        pub(crate) async fn enqueue(
            &self,
            mut connection: impl WriteConnection,
        ) -> sqlx::Result<()> {
            debug!(
                ?self.group_id,
                ?self.chat_id,
                "Enqueueing resync"
            );

            let group_id = GroupIdRefWrapper::from(&self.group_id);
            let pq_group_id = self.pq_group_id.as_ref().map(GroupIdRefWrapper::from);
            let original_leaf_index = self.original_leaf_index.u32() as i32;
            query!(
                "INSERT INTO resync_queue (
                    group_id,
                    pq_group_id,
                    chat_id,
                    group_state_ear_key,
                    identity_link_wrapper_key,
                    original_leaf_index,
                    shares_vc_leaf
                )
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                ON CONFLICT DO NOTHING",
                group_id,
                pq_group_id,
                self.chat_id,
                self.group_state_ear_key,
                self.identity_link_wrapper_key,
                original_leaf_index,
                self.shares_vc_leaf
            )
            .execute(connection.as_mut())
            .await?;
            Ok(())
        }

        /// Dequeue a resync operation for processing that has not been locked
        /// by this task.
        pub(crate) async fn dequeue(
            txn: &mut WriteDbTransaction<'_>,
            task_id: Uuid,
        ) -> anyhow::Result<Option<Resync>> {
            struct ResyncRecord {
                chat_id: Option<ChatId>,
                group_id: GroupIdWrapper,
                pq_group_id: Option<GroupIdWrapper>,
                group_state_ear_key: GroupStateEarKey,
                identity_link_wrapper_key: IdentityLinkWrapperKey,
                original_leaf_index: i32,
                shares_vc_leaf: bool,
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
                    pq_group_id AS "pq_group_id: _",
                    group_state_ear_key AS "group_state_ear_key: _",
                    identity_link_wrapper_key AS "identity_link_wrapper_key: _",
                    original_leaf_index AS "original_leaf_index: _",
                    shares_vc_leaf AS "shares_vc_leaf: _"
                "#,
                group_id,
                task_id,
            )
            .fetch_optional(txn.as_mut())
            .await?
            .map(|record| Resync {
                chat_id: record.chat_id,
                group_id: record.group_id.0,
                pq_group_id: record.pq_group_id.map(|id| id.0),
                group_state_ear_key: record.group_state_ear_key,
                identity_link_wrapper_key: record.identity_link_wrapper_key,
                original_leaf_index: LeafNodeIndex::new(record.original_leaf_index as u32),
                shares_vc_leaf: record.shares_vc_leaf,
            });

            Ok(resync)
        }

        pub(crate) async fn is_pending_for_chat(
            mut connection: impl ReadConnection,
            chat_id: &ChatId,
        ) -> sqlx::Result<bool> {
            // Matches either the stored chat_id or group_id: an onboarding resync
            // has no chat yet, but an existing group does.
            struct QueuedIds {
                chat_id: Option<ChatId>,
                group_id: GroupIdWrapper,
            }

            let queued = query_as!(
                QueuedIds,
                r#"SELECT chat_id AS "chat_id: _", group_id AS "group_id: _" FROM resync_queue"#
            )
            .fetch_all(connection.as_mut())
            .await?;

            Ok(queued.into_iter().any(|queued| {
                queued.chat_id.as_ref() == Some(chat_id)
                    || ChatId::try_from(&queued.group_id.0).is_ok_and(|derived| &derived == chat_id)
            }))
        }

        pub(crate) async fn remove(
            mut connection: impl WriteConnection,
            group_id: &GroupId,
        ) -> sqlx::Result<()> {
            let group_id_bytes = group_id.as_slice();
            query!(
                "DELETE FROM resync_queue WHERE group_id = ?",
                group_id_bytes
            )
            .execute(connection.as_mut())
            .await?;
            Ok(())
        }
    }
}

enum ResyncCommit {
    T(Box<ResyncTCommit>),
    PQ(Box<ApqCommitMessageBundle>),
}

struct ResyncTCommit {
    commit: MlsMessageOut,
    group_info: MlsMessageOut,
}
