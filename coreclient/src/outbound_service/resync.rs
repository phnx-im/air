// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{collections::HashSet, fmt, str::FromStr};

use airapiclient::ds_api::ExternalCommitInfoIn;
use aircommon::{
    credentials::keys::LeafSigningKey,
    crypto::aead::keys::{GroupStateEarKey, IdentityLinkWrapperKey},
    identifiers::{QualifiedGroupId, UserId},
    messages::client_ds::AadPayload,
    time::TimeStamp,
};
use airprotos::client::group::GroupData;
use anyhow::{Context, Result, anyhow, bail};
use apqmls::commit_builder::ApqCommitMessageBundle;
use chrono::{DateTime, TimeDelta, Utc};
use openmls::{
    group::GroupId,
    prelude::{LeafNodeIndex, MlsMessageOut},
};
use tokio_util::sync::CancellationToken;
use tracing::{Instrument, debug, error, info, info_span, warn};
use uuid::Uuid;

use crate::{
    Chat, ChatId, ChatMessage, ChatStatus, Contact, SystemMessage,
    chats::{ChatAttributes, GroupDataExt},
    clients::{
        CoreUser,
        api_clients::ApiClients,
        multi_device::{ConnectionContact, HigherLevelGroup},
        own_client_info::OwnClientInfo,
    },
    db::access::{WriteConnection, WriteDbTransaction},
    groups::{
        DecryptedProfileInfos, Group, ProfileInfo, handle_group_not_found_on_ds,
        self_group::SelfGroup,
    },
    job::{operation::OperationData, profile::FetchUserProfileOperation},
    outbound_service::{
        OutboundServiceContext,
        error::{
            OutboundServiceError, classify_ds_error, is_ds_not_found_error, is_ds_rejection_error,
            is_ds_wrong_epoch_error,
        },
    },
};

/// DS rejections before a queued resync is given up on.
const MAX_RESYNC_ATTEMPTS: u32 = 5;

/// Why a group is being resynced.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ResyncReason {
    /// We missed a commit.
    FutureEpoch,
    /// The local PQ leg of an APQ group is gone.
    MissingPqGroupState,
    /// Commit built against a derivation epoch we do not hold.
    VirtualClientDesync,
    /// Requested by the user.
    Manual,
    /// Onboarding a freshly linked device into a higher-level group.
    Onboarding,
}

impl ResyncReason {
    fn as_str(&self) -> &'static str {
        match self {
            Self::FutureEpoch => "future_epoch",
            Self::MissingPqGroupState => "missing_pq_group_state",
            Self::VirtualClientDesync => "virtual_client_desync",
            Self::Manual => "manual",
            Self::Onboarding => "onboarding",
        }
    }
}

impl FromStr for ResyncReason {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        Ok(match s {
            "future_epoch" => Self::FutureEpoch,
            "missing_pq_group_state" => Self::MissingPqGroupState,
            "virtual_client_desync" => Self::VirtualClientDesync,
            "manual" => Self::Manual,
            "onboarding" => Self::Onboarding,
            _ => bail!("Invalid resync reason: {s}"),
        })
    }
}

impl fmt::Display for ResyncReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// State of a queue entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResyncStatus {
    Pending,
    /// Cleared only by a manual resync or a processed commit.
    Failed,
}

impl ResyncStatus {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Failed => "failed",
        }
    }
}

impl FromStr for ResyncStatus {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        Ok(match s {
            "pending" => Self::Pending,
            "failed" => Self::Failed,
            _ => bail!("Invalid resync status: {s}"),
        })
    }
}

impl fmt::Display for ResyncStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The queue entry of a group, rendered for the debug screen.
#[derive(Debug, Clone)]
pub struct ResyncDebugInfo {
    pub status: String,
    pub reason: String,
    pub attempts: u32,
    pub not_before: Option<String>,
    pub last_error: Option<String>,
}

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
    /// The contact to create alongside the chat when this resync onboards into a
    /// connection group.
    pub(crate) connection_contact: Option<ConnectionContact>,
    pub(crate) reason: ResyncReason,
    /// How many DS rejections this entry has collected so far.
    pub(crate) attempts: u32,
}

impl Resync {
    pub(crate) fn for_group(chat_id: ChatId, group: &Group, reason: ResyncReason) -> Self {
        Self {
            chat_id: Some(chat_id),
            group_id: group.group_id().clone(),
            pq_group_id: group.pq_group_id(),
            group_state_ear_key: group.group_state_ear_key().clone(),
            identity_link_wrapper_key: group.identity_link_wrapper_key().clone(),
            original_leaf_index: group.own_index(),
            shares_vc_leaf: group.own_leaf_is_virtual_client(),
            connection_contact: None,
            reason,
            attempts: 0,
        }
    }
}

impl CoreUser {
    pub async fn enqueue_group_resync(&self, chat_id: ChatId) -> anyhow::Result<()> {
        let group = Group::load_with_chat_id(self.db().read().await?, chat_id)
            .await?
            .context("group not found")?;

        let resync = Resync::for_group(chat_id, &group, ResyncReason::Manual);

        resync.enqueue_or_reset(self.db().write().await?).await?;

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
                connection,
            } = group;

            let resync = Resync {
                chat_id: None,
                group_id,
                pq_group_id,
                group_state_ear_key,
                identity_link_wrapper_key,
                original_leaf_index: LeafNodeIndex::new(vc_leaf_index),
                shares_vc_leaf: true,
                connection_contact: connection,
                reason: ResyncReason::Onboarding,
                attempts: 0,
            };

            resync.enqueue(&mut *txn).await?;
            queued += 1;
        }

        Ok(queued)
    }
}

impl OutboundServiceContext {
    /// Drains the resync queue.
    ///
    /// DS rejections count towards [`MAX_RESYNC_ATTEMPTS`] with backoff, then the entry is marked
    /// `failed` until a manual resync or a processed commit clears it. Other errors retry on the
    /// next run.
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

            let now = Utc::now();
            let Some(resync) = self
                .db
                .with_write_transaction(async |txn| Resync::dequeue(txn, task_id, now).await)
                .await?
            else {
                return Ok(());
            };

            let span = info_span!(
                "resync",
                chat_id = ?resync.chat_id,
                group_id = ?resync.group_id,
                reason = %resync.reason,
                attempt = resync.attempts + 1,
            );
            self.perform_resync(resync, now).instrument(span).await?;
        }
    }

    /// Performs a single dequeued resync and records its outcome in the queue.
    async fn perform_resync(&self, resync: Resync, now: DateTime<Utc>) -> anyhow::Result<()> {
        info!("Performing resync");

        let group_id = resync.group_id.clone();
        let attempts = resync.attempts;

        // The self group is rejoined with the per-device self-group key, all other groups with the
        // user key.
        let signer = match self.signer_for_group(&group_id).await {
            Ok(signer) => signer,
            Err(error) => {
                error!(%error, "Failed to get signer for group");
                return Ok(());
            }
        };

        let result = {
            let mut connection = self.db.write().await?;
            let result = resync
                .create_and_send_commit(&mut connection, &self.api_clients, &signer, self.user_id())
                .await;
            if let Ok(Some((chat_id, _))) = &result {
                Resync::remove(&mut connection, &group_id).await?;
                connection.notifier().update(*chat_id);
                // TODO: Schedule a job here that deals with fetching profile infos in the
                // background.
            }
            result
        };

        let profile_infos = match result {
            Ok(Some((_, profile_infos))) => {
                info!("Resync succeeded");
                profile_infos
            }
            // We are not a member anymore, which was already handled inside.
            Ok(None) => return Ok(()),
            Err(OutboundServiceError::Fatal(error)) => {
                if is_ds_not_found_error(&error) {
                    error!(%error, "Group not found on DS during resync; tearing down group");
                    self.db
                        .with_write_transaction(async |txn| {
                            handle_group_not_found_on_ds(txn, &group_id).await
                        })
                        .await?;
                    return Ok(());
                }

                error!(%error, "Resync failed permanently; giving up");
                Resync::mark_failed(self.db.write().await?, &group_id, &error.to_string()).await?;
                return Ok(());
            }
            Err(OutboundServiceError::Recoverable(error)) => {
                match retry_decision(&error, attempts) {
                    RetryDecision::Retry => {
                        warn!(%error, "Resync failed; retrying later");
                    }
                    RetryDecision::Backoff { attempts, retry_in } => {
                        warn!(%error, ?retry_in, "Resync failed; retrying later");
                        Resync::record_failed_attempt(
                            self.db.write().await?,
                            &group_id,
                            attempts,
                            now + retry_in,
                            &error.to_string(),
                        )
                        .await?;
                    }
                    RetryDecision::GiveUp => {
                        error!(%error, "Resync failed permanently; giving up");
                        Resync::mark_failed(self.db.write().await?, &group_id, &error.to_string())
                            .await?;
                    }
                }
                return Ok(());
            }
        };

        let mut connection = self.db.write().await?;
        for ProfileInfo {
            user_credential,
            user_profile_key,
        } in profile_infos.members
        {
            if let Err(error) = FetchUserProfileOperation::new(user_credential, user_profile_key)
                .into_operation()
                .enqueue(&mut connection)
                .await
            {
                error!(%error, "Failed to enqueue fetch profile operation");
            }
        }

        Ok(())
    }
}

/// What to do with a queue entry after a recoverable error.
#[derive(Debug, PartialEq, Eq)]
enum RetryDecision {
    /// Leave the entry untouched. The next run picks it up again.
    Retry,
    /// Spend an attempt and defer the next one.
    Backoff { attempts: u32, retry_in: TimeDelta },
    /// Spend the last attempt and give up.
    GiveUp,
}

fn retry_decision(error: &anyhow::Error, attempts: u32) -> RetryDecision {
    if !is_ds_rejection_error(error) || is_ds_wrong_epoch_error(error) {
        return RetryDecision::Retry;
    }
    let attempts = attempts + 1;
    if attempts >= MAX_RESYNC_ATTEMPTS {
        RetryDecision::GiveUp
    } else {
        RetryDecision::Backoff {
            attempts,
            retry_in: resync_backoff(attempts),
        }
    }
}

/// Backoff 1m -> 2m -> 4m -> 8m, doubling per spent attempt. With
/// [`MAX_RESYNC_ATTEMPTS`] the entry is given up on after the 8m wait. The
/// cap only matters if the attempt limit grows.
fn resync_backoff(attempts: u32) -> TimeDelta {
    const RESYNC_BACKOFF_BASE: TimeDelta = TimeDelta::seconds(30);
    const RESYNC_BACKOFF_MAX: TimeDelta = TimeDelta::seconds(60 * 60);

    let factor = 1i32 << attempts.min(16);
    (RESYNC_BACKOFF_BASE * factor).min(RESYNC_BACKOFF_MAX)
}

impl Resync {
    /// Resync using an external commit.
    ///
    /// Returns the chat the resync applies to, which for an onboarding resync is
    /// only created here, once the commit has been accepted. Returns `None` when
    /// the server has no leaf of ours in the group, i.e. we were removed: the
    /// chat is then marked inactive and the resync dropped.
    async fn create_and_send_commit(
        mut self,
        mut connection: impl WriteConnection,
        api_clients: &ApiClients,
        signer: &LeafSigningKey,
        own_user_id: &UserId,
    ) -> Result<Option<(ChatId, DecryptedProfileInfos)>, OutboundServiceError> {
        let shares_vc_leaf = self.shares_vc_leaf;
        if shares_vc_leaf
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
        let existing_chat_id = self.chat_id;
        let Some(original_leaf_index) =
            self.resolve_original_leaf_index(&external_commit_info, signer)
        else {
            warn!(
                group_id = ?self.group_id,
                "No leaf carries our signature key: not a member of the group (according to the server)"
            );
            // Drop the resync, mark the chat as inactive and delete the group state.
            connection
                .with_transaction(async |txn| -> anyhow::Result<()> {
                    handle_group_not_found_on_ds(txn, &self.group_id).await
                })
                .await
                .map_err(OutboundServiceError::recoverable)?;
            return Ok(None);
        };
        let connection_contact = self.connection_contact.take();
        let ds_timestamp = TimeStamp::now();

        let mut txn = connection
            .begin()
            .await
            .map_err(OutboundServiceError::recoverable)?;
        let (group, commit, member_profile_infos, members_diff) = Box::pin(self.create_commit(
            &mut txn,
            api_clients,
            signer,
            own_user_id,
            external_commit_info,
        ))
        .await
        .map_err(OutboundServiceError::fatal)?;

        let (chat_id, chat_created) = match existing_chat_id {
            Some(chat_id) => (chat_id, false),
            None => (
                if let Some(connection_contact) = connection_contact {
                    Self::create_connection_chat(&mut txn, &group, connection_contact)
                        .await
                        .map_err(OutboundServiceError::fatal)?
                } else {
                    Self::create_group_chat(&mut txn, &group, own_user_id, ds_timestamp)
                        .await
                        .map_err(OutboundServiceError::fatal)?
                },
                true,
            ),
        };

        txn.commit()
            .await
            .map_err(OutboundServiceError::recoverable)?;

        Self::send_commit(api_clients, signer, &group, commit, original_leaf_index).await?;

        let res = connection
            .with_transaction(async |txn| -> anyhow::Result<()> {
                if shares_vc_leaf && chat_created {
                    let system_message = ChatMessage::new_system_message(
                        chat_id,
                        ds_timestamp,
                        SystemMessage::Onboarded,
                    );
                    system_message.store(&mut *txn).await?;
                }
                for user_id in members_diff.added {
                    let system_message = ChatMessage::new_system_message(
                        chat_id,
                        ds_timestamp,
                        SystemMessage::Add(None, user_id),
                    );
                    system_message.store(&mut *txn).await?;
                }
                for user_id in members_diff.removed {
                    let system_message = ChatMessage::new_system_message(
                        chat_id,
                        ds_timestamp,
                        SystemMessage::Remove(None, user_id),
                    );
                    system_message.store(&mut *txn).await?;
                }
                Chat::update_status(txn, chat_id, &ChatStatus::Active).await?;
                Ok(())
            })
            .await;
        if let Err(error) = res {
            error!(%error, ?chat_id, "Failed to update chat after accepted resync commit");
        }

        Ok(Some((chat_id, member_profile_infos)))
    }

    /// Create the local chat (and contact) for a connection group we just onboarded into.
    async fn create_connection_chat(
        txn: &mut WriteDbTransaction<'_>,
        group: &Group,
        contact: ConnectionContact,
    ) -> Result<ChatId> {
        let chat =
            Chat::new_onboarding_connection_chat(group.group_id().clone(), contact.user_id.clone());
        chat.store(&mut *txn).await?;

        Contact {
            user_id: contact.user_id,
            wai_ear_key: contact.wai_ear_key,
            friendship_token: contact.friendship_token,
            chat_id: chat.id(),
            supported_features: None,
        }
        .upsert(&mut *txn)
        .await?;

        Ok(chat.id())
    }

    /// Create the local chat for a group we just onboarded into.
    async fn create_group_chat(
        txn: &mut WriteDbTransaction<'_>,
        group: &Group,
        own_user_id: &UserId,
        ds_timestamp: TimeStamp,
    ) -> Result<ChatId> {
        let group_data_bytes = group.group_data().context("No group data")?;
        let group_data = GroupData::decode(&group_data_bytes)?;
        let (title, group_profile_part) = group_data.into_parts(group.identity_link_wrapper_key());
        let title = title.context("No group title")?;

        let picture = CoreUser::resolve_group_profile_part(
            &mut *txn,
            group.group_id(),
            own_user_id,
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

        Ok(chat.id())
    }

    fn resolve_original_leaf_index(
        &self,
        external_commit_info: &ExternalCommitInfoIn,
        signer: &LeafSigningKey,
    ) -> Option<LeafNodeIndex> {
        let signature_key_bytes = signer.verifying_key().as_slice();
        external_commit_info
            .ratchet_tree_in
            .full_leaves()
            .find(|(_, leaf)| leaf.signature_key().as_slice() == signature_key_bytes)
            .map(|(index, _)| {
                if index != self.original_leaf_index {
                    info!(
                        group_id = ?self.group_id,
                        queued = %self.original_leaf_index,
                        resolved = %index,
                        "The leaf to resync moved since the resync was queued"
                    );
                }
                index
            })
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
        signer: &LeafSigningKey,
        own_user_id: &UserId,
        external_commit_info: ExternalCommitInfoIn,
    ) -> Result<(Group, ResyncCommit, DecryptedProfileInfos, MembersDiff)> {
        // TODO: We should somehow mark the chat as "resyncing" in the DB and
        // reflect that in the UI.

        // Collect members of the group before we delete it.
        let members_before: Option<HashSet<UserId>> = Group::load(&mut *txn, &self.group_id)
            .await?
            .map(|group| group.members().collect());

        // Delete any old group states if they exist
        Group::delete_from_db(txn, &self.group_id).await?;

        let vc_group_id = if self.shares_vc_leaf {
            Some(
                OwnClientInfo::load_self_group_id(&mut *txn)
                    .await?
                    .context("self group not found")?,
            )
        } else {
            None
        };

        let aad = AadPayload::Resync.into();
        let (group, commit, member_profile_infos) = if self.pq_group_id.is_some() {
            // APQ group
            let (group, bundle, member_profile_infos) = Group::join_apq_group_externally(
                txn,
                api_clients,
                external_commit_info,
                signer,
                own_user_id,
                self.group_state_ear_key,
                self.identity_link_wrapper_key,
                aad,
                vc_group_id,
            )
            .await??;
            (
                group,
                ResyncCommit::PQ(Box::new(bundle)),
                member_profile_infos,
            )
        } else {
            // The self group is always an APQ group, so a T-only resync can never
            // concern it.
            let LeafSigningKey::User(signer) = signer else {
                bail!("self-group signer in a non-APQ resync");
            };
            let (group, commit, group_info, member_profile_infos) = Group::join_group_externally(
                txn,
                api_clients,
                external_commit_info,
                signer,
                self.group_state_ear_key,
                self.identity_link_wrapper_key,
                aad,
                None, // This is not in response to a connection offer.
                None, // A resync joins a group we are already a member of.
                vc_group_id,
            )
            .await??;
            (
                group,
                ResyncCommit::T(Box::new(ResyncTCommit { commit, group_info })),
                member_profile_infos,
            )
        };

        let members_after: HashSet<UserId> = group.members().collect();
        let diff = members_before
            .map(|before| MembersDiff::compute(before, members_after))
            .unwrap_or_default();

        Ok((group, commit, member_profile_infos, diff))
    }

    async fn send_commit(
        api_clients: &ApiClients,
        signer: &LeafSigningKey,
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

#[derive(Debug, Default)]
struct MembersDiff {
    added: HashSet<UserId>,
    removed: HashSet<UserId>,
}

impl MembersDiff {
    fn compute(mut before: HashSet<UserId>, mut after: HashSet<UserId>) -> Self {
        let intersection: HashSet<_> = before.intersection(&after).cloned().collect();
        before.retain(|user_id| !intersection.contains(user_id));
        after.retain(|user_id| !intersection.contains(user_id));
        Self {
            added: after,
            removed: before,
        }
    }
}

mod persistence {
    use aircommon::codec::{BlobDecoded, BlobEncoded};
    use sqlx::{
        Database, Decode, Encode, Sqlite, Type, encode::IsNull, error::BoxDynError, query,
        query_as, query_scalar,
    };
    use uuid::Uuid;

    use crate::{
        ChatId,
        db::access::{ReadConnection, WriteConnection, WriteDbTransaction},
        utils::persistence::{GroupIdRefWrapper, GroupIdWrapper},
    };

    use super::*;

    macro_rules! sqlx_text_enum {
        ($ty:ty) => {
            impl Type<Sqlite> for $ty {
                fn type_info() -> <Sqlite as Database>::TypeInfo {
                    <String as Type<Sqlite>>::type_info()
                }
            }

            impl Encode<'_, Sqlite> for $ty {
                fn encode_by_ref(
                    &self,
                    buf: &mut <Sqlite as Database>::ArgumentBuffer,
                ) -> Result<IsNull, BoxDynError> {
                    Encode::<Sqlite>::encode(self.as_str(), buf)
                }
            }

            impl Decode<'_, Sqlite> for $ty {
                fn decode(value: <Sqlite as Database>::ValueRef<'_>) -> Result<Self, BoxDynError> {
                    let s: &str = Decode::<Sqlite>::decode(value)?;
                    Ok(Self::from_str(s)?)
                }
            }
        };
    }

    sqlx_text_enum!(ResyncReason);
    sqlx_text_enum!(ResyncStatus);

    impl Resync {
        pub(crate) async fn enqueue(
            &self,
            mut connection: impl WriteConnection,
        ) -> sqlx::Result<()> {
            debug!(
                ?self.group_id,
                ?self.chat_id,
                %self.reason,
                "Enqueueing resync"
            );

            let group_id = GroupIdRefWrapper::from(&self.group_id);
            let pq_group_id = self.pq_group_id.as_ref().map(GroupIdRefWrapper::from);
            let original_leaf_index = self.original_leaf_index.u32() as i32;
            let connection_contact = self.connection_contact.as_ref().map(BlobEncoded);
            query!(
                "INSERT INTO resync_queue (
                    group_id,
                    pq_group_id,
                    chat_id,
                    group_state_ear_key,
                    identity_link_wrapper_key,
                    original_leaf_index,
                    shares_vc_leaf,
                    connection_contact,
                    status,
                    reason,
                    attempts
                )
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 0)
                ON CONFLICT DO NOTHING",
                group_id,
                pq_group_id,
                self.chat_id,
                self.group_state_ear_key,
                self.identity_link_wrapper_key,
                original_leaf_index,
                self.shares_vc_leaf,
                connection_contact,
                ResyncStatus::Pending as _,
                self.reason as _,
            )
            .execute(connection.as_mut())
            .await?;
            Ok(())
        }

        /// Like [`Resync::enqueue`], but resets an existing entry.
        pub(crate) async fn enqueue_or_reset(
            &self,
            mut connection: impl WriteConnection,
        ) -> sqlx::Result<()> {
            debug!(
                ?self.group_id,
                ?self.chat_id,
                %self.reason,
                "Enqueueing resync, resetting any existing entry"
            );

            let group_id = GroupIdRefWrapper::from(&self.group_id);
            let pq_group_id = self.pq_group_id.as_ref().map(GroupIdRefWrapper::from);
            let original_leaf_index = self.original_leaf_index.u32() as i32;
            let connection_contact = self.connection_contact.as_ref().map(BlobEncoded);
            query!(
                "INSERT INTO resync_queue (
                    group_id,
                    pq_group_id,
                    chat_id,
                    group_state_ear_key,
                    identity_link_wrapper_key,
                    original_leaf_index,
                    shares_vc_leaf,
                    connection_contact,
                    status,
                    reason,
                    attempts
                )
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 0)
                ON CONFLICT (group_id) DO UPDATE SET
                    pq_group_id = excluded.pq_group_id,
                    chat_id = excluded.chat_id,
                    group_state_ear_key = excluded.group_state_ear_key,
                    identity_link_wrapper_key = excluded.identity_link_wrapper_key,
                    original_leaf_index = excluded.original_leaf_index,
                    shares_vc_leaf = excluded.shares_vc_leaf,
                    connection_contact = excluded.connection_contact,
                    status = excluded.status,
                    attempts = 0,
                    not_before = NULL,
                    last_error = NULL,
                    reason = excluded.reason,
                    locked_by = NULL",
                group_id,
                pq_group_id,
                self.chat_id,
                self.group_state_ear_key,
                self.identity_link_wrapper_key,
                original_leaf_index,
                self.shares_vc_leaf,
                connection_contact,
                ResyncStatus::Pending as _,
                self.reason as _,
            )
            .execute(connection.as_mut())
            .await?;
            Ok(())
        }

        /// The state of the queue entry of the given group, if there is one.
        pub(crate) async fn status(
            mut connection: impl ReadConnection,
            group_id: &GroupId,
        ) -> sqlx::Result<Option<ResyncStatus>> {
            let group_id = group_id.as_slice();
            query_scalar!(
                r#"SELECT status AS "status: ResyncStatus"
                FROM resync_queue
                WHERE group_id = ?"#,
                group_id
            )
            .fetch_optional(connection.as_mut())
            .await
        }

        /// Whether the group has a queue entry that was given up on.
        pub(crate) async fn is_failed(
            connection: impl ReadConnection,
            group_id: &GroupId,
        ) -> sqlx::Result<bool> {
            Ok(Self::status(connection, group_id).await? == Some(ResyncStatus::Failed))
        }

        /// The queue entry of the given group, rendered for the debug screen.
        pub(crate) async fn debug_info(
            mut connection: impl ReadConnection,
            group_id: &GroupId,
        ) -> sqlx::Result<Option<ResyncDebugInfo>> {
            struct DebugRecord {
                status: ResyncStatus,
                reason: ResyncReason,
                attempts: i64,
                not_before: Option<DateTime<Utc>>,
                last_error: Option<String>,
            }

            let group_id = group_id.as_slice();
            let record = query_as!(
                DebugRecord,
                r#"SELECT
                    status AS "status: _",
                    reason AS "reason: _",
                    attempts,
                    not_before AS "not_before: _",
                    last_error
                FROM resync_queue
                WHERE group_id = ?"#,
                group_id
            )
            .fetch_optional(connection.as_mut())
            .await?;

            Ok(record.map(|record| ResyncDebugInfo {
                status: record.status.to_string(),
                reason: record.reason.to_string(),
                attempts: record.attempts as u32,
                not_before: record.not_before.map(|dt| dt.to_rfc3339()),
                last_error: record.last_error,
            }))
        }

        /// Dequeue a due resync operation for processing that has not been
        /// locked by this task.
        pub(crate) async fn dequeue(
            txn: &mut WriteDbTransaction<'_>,
            task_id: Uuid,
            now: DateTime<Utc>,
        ) -> anyhow::Result<Option<Resync>> {
            struct ResyncRecord {
                chat_id: Option<ChatId>,
                group_id: GroupIdWrapper,
                pq_group_id: Option<GroupIdWrapper>,
                group_state_ear_key: GroupStateEarKey,
                identity_link_wrapper_key: IdentityLinkWrapperKey,
                original_leaf_index: i32,
                shares_vc_leaf: bool,
                connection_contact: Option<BlobDecoded<ConnectionContact>>,
                reason: ResyncReason,
                attempts: i64,
            }

            let Some(group_id) = query_scalar!(
                r#"
                SELECT group_id
                FROM resync_queue
                WHERE (locked_by IS NULL OR locked_by != ?1)
                    AND status = ?2
                    AND (not_before IS NULL OR not_before <= ?3)
                LIMIT 1
                "#,
                task_id,
                ResyncStatus::Pending as _,
                now,
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
                    shares_vc_leaf AS "shares_vc_leaf: _",
                    connection_contact AS "connection_contact: _",
                    reason AS "reason: _",
                    attempts
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
                connection_contact: record.connection_contact.map(BlobDecoded::into_inner),
                reason: record.reason,
                attempts: record.attempts as u32,
            });

            Ok(resync)
        }

        pub(crate) async fn status_for_chat(
            mut connection: impl ReadConnection,
            chat_id: &ChatId,
        ) -> sqlx::Result<Option<ResyncStatus>> {
            // An onboarding entry has no chat id yet, so also match on the chat's group.
            query_scalar!(
                r#"SELECT status AS "status: _"
                FROM resync_queue
                WHERE chat_id = ?1
                    OR group_id = (SELECT group_id FROM chat WHERE chat_id = ?1)"#,
                chat_id,
            )
            .fetch_optional(connection.as_mut())
            .await
        }

        /// Records a spent attempt and when the next one may run.
        pub(crate) async fn record_failed_attempt(
            mut connection: impl WriteConnection,
            group_id: &GroupId,
            attempts: u32,
            not_before: DateTime<Utc>,
            last_error: &str,
        ) -> sqlx::Result<()> {
            let group_id = group_id.as_slice();
            let attempts = attempts as i64;
            query!(
                "UPDATE resync_queue
                SET attempts = ?2, not_before = ?3, last_error = ?4
                WHERE group_id = ?1",
                group_id,
                attempts,
                not_before,
                last_error,
            )
            .execute(connection.as_mut())
            .await?;
            Ok(())
        }

        /// Gives up on the entry, keeping it for inspection.
        pub(crate) async fn mark_failed(
            mut connection: impl WriteConnection,
            group_id: &GroupId,
            last_error: &str,
        ) -> sqlx::Result<()> {
            let group_id_bytes = group_id.as_slice();
            query!(
                "UPDATE resync_queue
                SET status = ?2, not_before = NULL, last_error = ?3
                WHERE group_id = ?1",
                group_id_bytes,
                ResyncStatus::Failed as _,
                last_error,
            )
            .execute(connection.as_mut())
            .await?;
            Ok(())
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

#[cfg(test)]
mod tests {
    use std::{assert_matches, time::Duration};

    use airapiclient::ds_api::DsRequestError;
    use airprotos::common::v1::{
        StatusDetails, StatusDetailsCode, WrongEpochDetail, status_details::Detail,
    };

    use crate::{ChatAttributes, db::access::DbAccess, utils::persistence::open_db_in_memory};

    use super::*;

    fn ds_rejection() -> anyhow::Error {
        DsRequestError::Tonic(tonic::Status::invalid_argument("rejected")).into()
    }

    fn ds_wrong_epoch() -> anyhow::Error {
        let details = StatusDetails {
            code: StatusDetailsCode::WrongEpoch.into(),
            detail: Some(Detail::WrongEpoch(WrongEpochDetail {})),
        };
        DsRequestError::Tonic(details.to_status(tonic::Code::InvalidArgument, "wrong epoch")).into()
    }

    #[test]
    fn local_error_does_not_spend_an_attempt() {
        let error = anyhow!("self group not joined yet");
        assert_eq!(retry_decision(&error, 3), RetryDecision::Retry);
    }

    #[test]
    fn network_error_does_not_spend_an_attempt() {
        let error: anyhow::Error = DsRequestError::Timeout(Duration::from_secs(1)).into();
        assert_eq!(retry_decision(&error, 3), RetryDecision::Retry);
    }

    #[test]
    fn wrong_epoch_does_not_spend_an_attempt() {
        assert_eq!(retry_decision(&ds_wrong_epoch(), 3), RetryDecision::Retry);
    }

    #[test]
    fn ds_rejection_spends_an_attempt_with_backoff() {
        assert_eq!(
            retry_decision(&ds_rejection(), 0),
            RetryDecision::Backoff {
                attempts: 1,
                retry_in: TimeDelta::minutes(1),
            }
        );
        assert_eq!(
            retry_decision(&ds_rejection(), 3),
            RetryDecision::Backoff {
                attempts: 4,
                retry_in: TimeDelta::minutes(8),
            }
        );
    }

    #[test]
    fn last_ds_rejection_gives_up() {
        assert_eq!(
            retry_decision(&ds_rejection(), MAX_RESYNC_ATTEMPTS - 1),
            RetryDecision::GiveUp
        );
    }

    /// A chat and a matching queue entry. The queue only stores ids and keys,
    /// so no MLS group is needed.
    async fn setup(reason: ResyncReason) -> anyhow::Result<(DbAccess, Resync)> {
        let pool = DbAccess::for_tests(open_db_in_memory().await?);

        let qgid = QualifiedGroupId::new(Uuid::new_v4(), "example.com".parse()?);
        let group_id = GroupId::from(qgid);
        let chat = Chat::new_group_chat(
            group_id.clone(),
            ChatAttributes::new("Test chat".into(), None),
        );
        let chat_id = chat.id();
        chat.store(pool.write().await?).await?;

        let resync = Resync {
            chat_id: Some(chat_id),
            group_id,
            pq_group_id: None,
            group_state_ear_key: GroupStateEarKey::random()?,
            identity_link_wrapper_key: IdentityLinkWrapperKey::random()?,
            original_leaf_index: LeafNodeIndex::new(0),
            shares_vc_leaf: false,
            connection_contact: None,
            reason,
            attempts: 0,
        };

        Ok((pool, resync))
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn enqueue_is_idempotent() -> anyhow::Result<()> {
        let (pool, resync) = setup(ResyncReason::FutureEpoch).await?;
        let group_id = resync.group_id.clone();
        let mut connection = pool.write().await?;

        resync.enqueue(&mut connection).await?;
        Resync::record_failed_attempt(
            &mut connection,
            &group_id,
            3,
            Utc::now() - TimeDelta::seconds(1),
            "boom",
        )
        .await?;

        // A second enqueue must not reset the reason or the spent attempts.
        let mut again = resync;
        again.reason = ResyncReason::Manual;
        again.enqueue(&mut connection).await?;

        let dequeued = connection
            .with_transaction(async |txn| Resync::dequeue(txn, Uuid::new_v4(), Utc::now()).await)
            .await?
            .expect("entry should be due");
        assert_eq!(dequeued.reason, ResyncReason::FutureEpoch);
        assert_eq!(dequeued.attempts, 3);

        Ok(())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn failed_entry_is_inert() -> anyhow::Result<()> {
        let (pool, resync) = setup(ResyncReason::FutureEpoch).await?;
        let group_id = resync.group_id.clone();
        let chat_id = resync.chat_id.expect("chat id");
        let mut connection = pool.write().await?;

        resync.enqueue(&mut connection).await?;
        Resync::mark_failed(&mut connection, &group_id, "boom").await?;

        assert_eq!(
            Resync::status(&mut connection, &group_id).await?,
            Some(ResyncStatus::Failed)
        );

        // Re-scheduling the same group is a no-op while the entry is failed.
        let mut again = resync;
        again.reason = ResyncReason::Manual;
        again.enqueue(&mut connection).await?;
        assert_matches!(
            Resync::status(&mut connection, &group_id).await?,
            Some(ResyncStatus::Failed)
        );

        assert_matches!(
            Resync::status_for_chat(&mut connection, &chat_id).await?,
            Some(ResyncStatus::Failed)
        );
        assert!(
            connection
                .with_transaction(async |txn| Resync::dequeue(txn, Uuid::new_v4(), Utc::now()).await)
                .await?
                .is_none()
        );

        Ok(())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn enqueue_or_reset_revives_failed_entry() -> anyhow::Result<()> {
        let (pool, resync) = setup(ResyncReason::FutureEpoch).await?;
        let group_id = resync.group_id.clone();
        let mut connection = pool.write().await?;

        resync.enqueue(&mut connection).await?;
        Resync::record_failed_attempt(
            &mut connection,
            &group_id,
            4,
            Utc::now() + TimeDelta::hours(1),
            "boom",
        )
        .await?;
        Resync::mark_failed(&mut connection, &group_id, "boom").await?;

        let mut manual = resync;
        manual.reason = ResyncReason::Manual;
        manual.enqueue_or_reset(&mut connection).await?;

        assert_eq!(
            Resync::status(&mut connection, &group_id).await?,
            Some(ResyncStatus::Pending)
        );
        let dequeued = connection
            .with_transaction(async |txn| Resync::dequeue(txn, Uuid::new_v4(), Utc::now()).await)
            .await?
            .expect("entry should be due again");
        assert_eq!(dequeued.reason, ResyncReason::Manual);
        assert_eq!(dequeued.attempts, 0);

        Ok(())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn backoff_defers_the_next_attempt() -> anyhow::Result<()> {
        let (pool, resync) = setup(ResyncReason::FutureEpoch).await?;
        let group_id = resync.group_id.clone();
        let mut connection = pool.write().await?;

        resync.enqueue(&mut connection).await?;

        let now = Utc::now();
        let not_before = now + TimeDelta::minutes(5);
        Resync::record_failed_attempt(&mut connection, &group_id, 1, not_before, "boom").await?;

        assert!(
            connection
                .with_transaction(async |txn| Resync::dequeue(txn, Uuid::new_v4(), now).await)
                .await?
                .is_none()
        );

        let later = not_before + TimeDelta::seconds(1);
        let dequeued = connection
            .with_transaction(async |txn| Resync::dequeue(txn, Uuid::new_v4(), later).await)
            .await?
            .expect("entry should be due");
        assert_eq!(dequeued.attempts, 1);

        Ok(())
    }
}
