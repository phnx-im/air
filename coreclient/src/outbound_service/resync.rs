// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{collections::HashSet, fmt, str::FromStr};

use aircommon::{
    credentials::keys::LeafSigningKey,
    crypto::aead::keys::{GroupStateEarKey, IdentityLinkWrapperKey},
    identifiers::{QualifiedGroupId, UserId},
    messages::{
        client_as::EncryptedFriendshipPackage, client_ds::AadPayload,
        client_ds_out::ExternalCommitInfoIn,
    },
    time::TimeStamp,
};
use anyhow::{Context, Result, anyhow, bail, ensure};
use apqmls::commit_builder::ApqCommitMessageBundle;
use chrono::{DateTime, Utc};
use openmls::{
    group::GroupId,
    prelude::{GroupContext, GroupEpoch, LeafNodeIndex, MlsMessageOut},
};
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;
use tracing::{Instrument, debug, error, info, info_span, warn};
use uuid::Uuid;

use crate::{
    Chat, ChatId, ChatMessage, ChatStatus, Contact, SystemMessage,
    chats::outgoing_requests::{self, OutgoingRequest},
    clients::{
        CoreUser,
        api_clients::ApiClients,
        multi_device::{ConnectionContact, HigherLevelGroup},
        own_client_info::OwnClientInfo,
    },
    db::access::{WriteConnection, WriteDbTransaction},
    groups::{
        DecryptedProfileInfos, Group, GroupClosed, RoomStateMismatch, handle_group_not_found_on_ds,
        self_group::SelfGroup, tear_down_group,
    },
    job::recoverable::Recoverable,
    outbound_service::{
        OutboundServiceContext,
        account_deletion::AccountDeletion,
        error::{OutboundServiceError, is_ds_not_found_error},
        retry::{RetryDecision, RetryPolicy},
    },
};

/// Tears down a group whose group context carries a tombstone as if the DS
/// returns "group not found". Also announces the deletion in the chat. An
/// outgoing request that went to a contact ends with a record of that instead,
/// see [`outgoing_requests::close_if_already_connected`].
///
/// A closed self group means that the account is deleted, see
/// [`AccountDeletion::self_group_closed`].
async fn handle_group_closed(
    txn: &mut WriteDbTransaction<'_>,
    group_id: &GroupId,
    closed: &GroupClosed,
) -> Result<()> {
    if OwnClientInfo::is_own_self_group(&mut *txn, group_id).await? {
        AccountDeletion::self_group_closed(&mut *txn).await?;
    }
    if let Some(mut chat) = Chat::load_by_group_id(&mut *txn, group_id).await?
        && !chat.status().is_inactive()
    {
        let at = closed.tombstone.timestamp.unwrap_or_else(TimeStamp::now);
        let already_connected = outgoing_requests::close_if_already_connected(
            txn,
            &mut chat,
            Some(&closed.tombstone),
            &closed.closer,
            at,
        )
        .await?;
        if !already_connected {
            ChatMessage::new_system_message(
                chat.id(),
                at,
                SystemMessage::DeleteGroup(closed.closer.clone()),
            )
            .store(&mut *txn)
            .await?;
        }
    }
    tear_down_group(txn, group_id).await
}

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
    /// Joining a connection group a sibling device already joined on accept.
    SiblingAccepted,
}

impl ResyncReason {
    fn as_str(&self) -> &'static str {
        match self {
            Self::FutureEpoch => "future_epoch",
            Self::MissingPqGroupState => "missing_pq_group_state",
            Self::VirtualClientDesync => "virtual_client_desync",
            Self::Manual => "manual",
            Self::Onboarding => "onboarding",
            Self::SiblingAccepted => "sibling_accepted",
        }
    }

    /// How the epoch of the served group state has to relate to the last local
    /// state the DS accepted, or `None` if this resync joins a group without
    /// such a state.
    fn epoch_requirement(self) -> Option<EpochRequirement> {
        match self {
            // A message from a later epoch, or a commit the DS accepted and we could not follow,
            // means that the group moved past our epoch.
            Self::FutureEpoch | Self::VirtualClientDesync => Some(EpochRequirement::Newer),
            Self::Manual | Self::MissingPqGroupState => Some(EpochRequirement::NotOlder),
            Self::Onboarding | Self::SiblingAccepted => None,
        }
    }
}

/// How the T epoch of the served group state has to relate to a bound.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EpochRequirement {
    Newer,
    NotOlder,
}

/// The T epoch of a group state and, for an APQ group, its PQ epoch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct GroupEpochs {
    t: GroupEpoch,
    pq: Option<GroupEpoch>,
}

impl GroupEpochs {
    fn of_group(group: &Group) -> Self {
        Self {
            t: group.mls_group().epoch(),
            pq: group.pq().map(|pq| pq.mls_group.epoch()),
        }
    }

    /// The epochs of the served group info. They are not verified yet, but a
    /// join verifies the signature over them.
    fn of_external_commit_info(info: &ExternalCommitInfoIn) -> Self {
        Self {
            t: info.verifiable_group_info.epoch(),
            pq: info.pq.as_ref().map(|pq| pq.group_info.epoch()),
        }
    }

    /// Whether these served epochs meet `requirement` against the `bound`.
    ///
    /// The PQ epoch must never go back, and an APQ group must stay one.
    fn meet(self, requirement: EpochRequirement, bound: GroupEpochs) -> bool {
        let t = match requirement {
            EpochRequirement::Newer => self.t > bound.t,
            EpochRequirement::NotOlder => self.t >= bound.t,
        };
        let pq = match (bound.pq, self.pq) {
            (None, _) => true,
            (Some(_), None) => false,
            (Some(bound), Some(served)) => served >= bound,
        };
        t && pq
    }
}

/// The group state a resync checks the served state against.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EpochBound {
    /// Nothing is checked. The resync's reason needs no check, and there is
    /// neither a stored commit nor a local state that replaced one.
    Unchecked,
    /// The last local state the DS accepted, with the requirement it has to
    /// meet.
    Accepted(EpochRequirement, GroupEpochs),
    /// The served state an earlier attempt of this resync built its stored
    /// commit on. That attempt checked it with the reason's requirement and
    /// verified it in the join.
    Attempt(GroupEpochs),
}

/// What the queue entry's stored commit says about the local group state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StoredCommit {
    /// There is no stored commit.
    None,
    /// The local group holds the state the stored commit produces, which the
    /// DS has not accepted yet. The commit was built on these served epochs.
    Current(GroupEpochs),
    /// The local group no longer holds the state the stored commit produces,
    /// because something else replaced it.
    Stale,
}

impl EpochBound {
    /// The bound for a resync for `reason`, given the queue entry's stored
    /// commit and the epochs of the local group.
    ///
    /// Fails if the reason needs a check but there is neither a stored commit
    /// nor a local group.
    fn resolve(
        reason: ResyncReason,
        stored: StoredCommit,
        local: Option<GroupEpochs>,
    ) -> Result<Self, StaleGroupSnapshot> {
        match (stored, local) {
            (StoredCommit::Current(built_on), _) => return Ok(Self::Attempt(built_on)),
            // The reason describes the state the local one replaced, so its rule
            // doesn't apply. Refusing anything older than the local state still
            // rules out a rollback.
            (StoredCommit::Stale, Some(local)) => {
                return Ok(Self::Accepted(EpochRequirement::NotOlder, local));
            }
            (StoredCommit::None | StoredCommit::Stale, _) => {}
        }
        let Some(requirement) = reason.epoch_requirement() else {
            return Ok(Self::Unchecked);
        };
        match local {
            Some(local) => Ok(Self::Accepted(requirement, local)),
            None => Err(StaleGroupSnapshot::NoLocalState { reason }),
        }
    }

    fn epochs(self) -> Option<GroupEpochs> {
        match self {
            Self::Unchecked => None,
            Self::Accepted(_, epochs) | Self::Attempt(epochs) => Some(epochs),
        }
    }

    /// Fails if a resync for `reason` must not join the `served` state.
    fn check(self, reason: ResyncReason, served: GroupEpochs) -> Result<(), StaleGroupSnapshot> {
        let (requirement, bound) = match self {
            Self::Unchecked => return Ok(()),
            Self::Accepted(requirement, bound) => (requirement, bound),
            Self::Attempt(bound) => (EpochRequirement::NotOlder, bound),
        };
        if served.meet(requirement, bound) {
            Ok(())
        } else {
            Err(StaleGroupSnapshot::Older {
                reason,
                bound,
                served,
            })
        }
    }

    /// Fails unless a verified `served` state may tear down the group, because
    /// it has no leaf of ours or closes the group.
    ///
    /// The group was open and had our leaf in the bound, so the served state
    /// has to be strictly newer.
    fn check_teardown(
        self,
        reason: ResyncReason,
        served: GroupEpochs,
    ) -> Result<(), StaleGroupSnapshot> {
        let Some(bound) = self.epochs() else {
            return Ok(());
        };
        if served.t > bound.t {
            Ok(())
        } else {
            Err(StaleGroupSnapshot::Older {
                reason,
                bound,
                served,
            })
        }
    }
}

/// A resync refused to join the served group state.
#[derive(Debug, thiserror::Error)]
pub(crate) enum StaleGroupSnapshot {
    /// The served state would take this client back behind the state it holds.
    #[error(
        "refusing {reason} resync into a stale group state: bound epochs {bound:?}, served epochs {served:?}"
    )]
    Older {
        reason: ResyncReason,
        bound: GroupEpochs,
        served: GroupEpochs,
    },
    /// There is no local state to check the served one against.
    #[error("refusing {reason} resync without a local group state to check against")]
    NoLocalState { reason: ResyncReason },
}

/// One leg of a group state, identified by its epoch and confirmed transcript
/// hash.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct LegFingerprint {
    epoch: GroupEpoch,
    #[serde(with = "serde_bytes")]
    confirmed_transcript_hash: Vec<u8>,
}

impl LegFingerprint {
    fn of_context(context: &GroupContext) -> Self {
        Self {
            epoch: context.epoch(),
            confirmed_transcript_hash: context.confirmed_transcript_hash().to_vec(),
        }
    }
}

/// A group state, identified by the epoch and confirmed transcript hash of each
/// leg.
///
/// The confirmed transcript hash chains over every commit, so with an honest DS
/// two states with the same fingerprint are the same.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct StateFingerprint {
    t: LegFingerprint,
    pq: Option<LegFingerprint>,
}

impl StateFingerprint {
    fn of_group(group: &Group) -> Self {
        Self {
            t: LegFingerprint::of_context(group.mls_group().public_group().group_context()),
            pq: group
                .pq()
                .map(|pq| LegFingerprint::of_context(pq.mls_group.public_group().group_context())),
        }
    }

    /// The fingerprint of the served group info. It is not verified.
    fn of_external_commit_info(info: &ExternalCommitInfoIn) -> Self {
        Self {
            t: LegFingerprint::of_context(info.verifiable_group_info.group_context()),
            pq: info
                .pq
                .as_ref()
                .map(|pq| LegFingerprint::of_context(pq.group_info.group_context())),
        }
    }
}

/// A resync commit the DS has not confirmed yet, stored with the queue entry.
///
/// The local group holds the state the commit produces from the moment the
/// commit is built, since an external commit merges when it is built. The
/// stored commit marks that state as not accepted by the DS, and a later
/// attempt resends it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct PendingResyncCommit {
    commit: ResyncCommit,
    /// The leaf the commit evicts.
    original_leaf_index: LeafNodeIndex,
    /// The epochs of the served state the commit was built on.
    built_on: GroupEpochs,
    /// The state the commit produces.
    produces: StateFingerprint,
    /// The members of the last local state the DS accepted, or `None` if there
    /// was no local group. Carried over when the commit is rebuilt.
    members_before: Option<HashSet<UserId>>,
    /// Whether this resync created the chat. Carried over when the commit is
    /// rebuilt.
    chat_created: bool,
}

impl PendingResyncCommit {
    /// Whether the local group still holds the state this commit produces.
    /// Otherwise something else replaced the group state, and the commit is
    /// stale.
    fn matches(&self, local: Option<&Group>) -> bool {
        local.is_some_and(|group| StateFingerprint::of_group(group) == self.produces)
    }
}

/// What a new resync commit builds on.
#[derive(Debug)]
struct JoinBase {
    bound: EpochBound,
    /// See [`PendingResyncCommit::members_before`].
    members_before: Option<HashSet<UserId>>,
    /// Whether an earlier attempt created the chat.
    chat_created: bool,
}

/// How a resync ended.
#[derive(Debug)]
pub(crate) enum ResyncOutcome {
    /// The DS accepted the commit.
    Synced(ChatId),
    /// The DS has no leaf of ours in the group, so we were removed. The chat is
    /// marked inactive.
    Removed,
    /// The queue entry was removed while resyncing, for example by a processed
    /// commit, which shows that the group is in sync, or by a teardown.
    Superseded,
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
            "sibling_accepted" => Self::SiblingAccepted,
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
    /// The request to recreate when this resync onboards into the connection
    /// group of an unanswered outgoing request.
    pub(crate) outgoing_request: Option<OutgoingRequest>,
    pub(crate) reason: ResyncReason,
    /// How many server errors this entry has collected so far.
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
            outgoing_request: None,
            reason,
            attempts: 0,
        }
    }

    pub(crate) fn for_sibling_accept(
        chat_id: ChatId,
        group_id: GroupId,
        pq_group_id: Option<GroupId>,
        group_state_ear_key: GroupStateEarKey,
        identity_link_wrapper_key: IdentityLinkWrapperKey,
        original_leaf_index: LeafNodeIndex,
    ) -> Self {
        Self {
            chat_id: Some(chat_id),
            group_id,
            pq_group_id,
            group_state_ear_key,
            identity_link_wrapper_key,
            original_leaf_index,
            shares_vc_leaf: true,
            connection_contact: None,
            outgoing_request: None,
            reason: ResyncReason::SiblingAccepted,
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
                outgoing_request,
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
                outgoing_request,
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
    /// Server errors count towards [`RetryPolicy::RESYNC`] with backoff, then the entry is marked
    /// `failed` until a manual resync or a processed commit clears it. Other errors retry on the
    /// next run.
    pub(super) async fn perform_queued_resyncs(
        &self,
        run_token: &CancellationToken,
    ) -> Result<(), OutboundServiceError> {
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
                .await
                .map_err(OutboundServiceError::fatal)?
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
    async fn perform_resync(
        &self,
        resync: Resync,
        now: DateTime<Utc>,
    ) -> Result<(), OutboundServiceError> {
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
                .resync(&mut connection, &self.api_clients, &signer, self.user_id())
                .await;
            if let Ok(ResyncOutcome::Synced(chat_id)) = &result {
                connection.notifier().update(*chat_id);
            }
            result
        };

        match result {
            Ok(ResyncOutcome::Synced(_)) => info!("Resync succeeded"),
            // We are not a member anymore, which was already handled inside.
            Ok(ResyncOutcome::Removed) => {}
            Ok(ResyncOutcome::Superseded) => {
                info!("Resync superseded, since its queue entry is gone");
            }
            // Leave the entry untouched and retry in a later run
            Err(
                error @ (OutboundServiceError::NetworkError
                | OutboundServiceError::RateLimited { .. }),
            ) => {
                warn!(%error, "Resync failed; retrying later");
                return Err(error);
            }
            Err(OutboundServiceError::Fatal(error)) => {
                if let Some(closed) = error.downcast_ref::<GroupClosed>() {
                    info!("Group was closed while out of sync; tearing down its MLS state");
                    self.db
                        .with_write_transaction(async |txn| {
                            handle_group_closed(txn, &group_id, closed).await
                        })
                        .await
                        .map_err(OutboundServiceError::fatal)?;
                    return Ok(());
                }

                if is_ds_not_found_error(&error) {
                    error!(%error, "Group not found on DS during resync; tearing down group");
                    self.db
                        .with_write_transaction(async |txn| {
                            handle_group_not_found_on_ds(txn, &group_id).await
                        })
                        .await
                        .map_err(OutboundServiceError::fatal)?;
                    return Ok(());
                }

                error!(%error, "Resync failed permanently; giving up");
                Resync::mark_failed(self.db.write().await?, &group_id, &error.to_string()).await?;
                return Ok(());
            }
            Err(OutboundServiceError::Recoverable(error)) => {
                match RetryPolicy::RESYNC.decide(error.cause, attempts) {
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
            }
        }

        Ok(())
    }
}

impl Resync {
    /// Resync using an external commit.
    ///
    /// Resends a stored commit first. If the DS refuses it, fetches the group
    /// state, finishes if the DS already holds the state the stored commit
    /// produces, and joins the served state with a new commit otherwise.
    async fn resync(
        self,
        mut connection: impl WriteConnection,
        api_clients: &ApiClients,
        signer: &LeafSigningKey,
        own_user_id: &UserId,
    ) -> Result<ResyncOutcome, OutboundServiceError> {
        if self.shares_vc_leaf && SelfGroup::load(&mut connection).await?.is_none() {
            return Err(OutboundServiceError::Recoverable(Recoverable::deferred(
                anyhow!(
                    "self group not joined yet; deferring onboarding of group {:?}",
                    self.group_id
                ),
            )));
        }

        let local_group = Group::load(&mut connection, &self.group_id).await?;
        let pending = Self::load_pending_commit(&mut connection, &self.group_id)
            .await?
            .flatten()
            .filter(|pending| pending.matches(local_group.as_ref()));
        drop(local_group);

        if let Some(pending) = &pending {
            match Self::send_commit(api_clients, signer, &self, pending).await {
                Ok(()) => {
                    info!("DS accepted the resent resync commit");
                    return self.finish(connection, pending).await;
                }
                Err(
                    error @ (OutboundServiceError::NetworkError
                    | OutboundServiceError::RateLimited { .. }),
                ) => return Err(error),
                // The DS may have applied the commit already, or the group moved on. The fetched
                // state tells which.
                Err(error) => {
                    info!(%error, "DS refused the resent resync commit; fetching the group state");
                }
            }
        }

        let external_commit_info = self.fetch_group_info(api_clients).await?;
        if let Some(pending) = &pending
            && StateFingerprint::of_external_commit_info(&external_commit_info) == pending.produces
        {
            info!("DS holds the state the stored resync commit produces");
            return self.finish(connection, pending).await;
        }

        Box::pin(self.create_and_send_commit_with_info(
            connection,
            api_clients,
            signer,
            own_user_id,
            external_commit_info,
        ))
        .await
    }

    /// Joins the served group state with a new commit and sends it.
    ///
    /// Refuses a served state older than the bound of this resync (see
    /// [`EpochBound`]). For a queued resync, stores the commit with the queue
    /// entry before sending it.
    pub(crate) async fn create_and_send_commit_with_info(
        mut self,
        mut connection: impl WriteConnection,
        api_clients: &ApiClients,
        signer: &LeafSigningKey,
        own_user_id: &UserId,
        external_commit_info: ExternalCommitInfoIn,
    ) -> Result<ResyncOutcome, OutboundServiceError> {
        let group_id = self.group_id.clone();
        let reason = self.reason;
        let original_leaf_index = self.resolve_original_leaf_index(&external_commit_info, signer);
        let served = GroupEpochs::of_external_commit_info(&external_commit_info);
        let connection_contact = self.connection_contact.take();
        let outgoing_request = self.outgoing_request.take();
        let ds_timestamp = TimeStamp::now();

        let mut txn = connection.begin().await?;

        let Some(base) = self
            .load_join_base(&mut txn)
            .await
            .map_err(OutboundServiceError::fatal)?
        else {
            return Ok(ResyncOutcome::Superseded);
        };
        let JoinBase {
            bound,
            members_before,
            chat_created: chat_created_before,
        } = base;
        bound
            .check(reason, served)
            .map_err(OutboundServiceError::fatal)?;

        let joined = Box::pin(self.create_commit(
            &mut txn,
            api_clients,
            signer,
            own_user_id,
            external_commit_info,
        ))
        .await;

        // A closed group fails the join after the verification. It tears the group down, so like
        // a state without our leaf it has to be newer than the bound.
        let joined = match joined {
            Err(error) if error.is::<GroupClosed>() => {
                bound
                    .check_teardown(reason, served)
                    .map_err(OutboundServiceError::fatal)?;
                return Err(OutboundServiceError::fatal(error));
            }
            joined => joined,
        };

        let Some(original_leaf_index) = original_leaf_index else {
            // The served state has no leaf of ours. That only shows that we were removed if a
            // member signed it, i.e. it verified, and if it is newer than the bound, which has our
            // leaf. A room state without us fails the join after the verification.
            match joined {
                Ok(_) => {}
                Err(error) if error.is::<RoomStateMismatch>() => {}
                Err(error) => return Err(OutboundServiceError::fatal(error)),
            }
            bound
                .check_teardown(reason, served)
                .map_err(OutboundServiceError::fatal)?;
            warn!(
                ?group_id,
                "No leaf carries our signature key: not a member of the group (according to the server)"
            );
            // Roll back the join, drop the resync, mark the chat as inactive and delete the
            // group state.
            drop(txn);
            connection
                .with_transaction(async |txn| -> anyhow::Result<()> {
                    handle_group_not_found_on_ds(txn, &group_id).await
                })
                .await
                .map_err(OutboundServiceError::fatal)?;
            return Ok(ResyncOutcome::Removed);
        };
        let (group, commit, member_profile_infos) = joined.map_err(OutboundServiceError::fatal)?;

        // Only the operations are scheduled here. The members come from a verified group info, so
        // fetching their profiles is fine even if the commit is rebuilt later.
        for profile_info in member_profile_infos.members {
            CoreUser::schedule_fetch_user_profile(&mut txn, profile_info).await?;
        }

        let existing_chat_id = match self.chat_id {
            Some(chat_id) => Some(chat_id),
            None => Chat::load_by_group_id(&mut txn, &group_id)
                .await?
                .map(|chat| chat.id()),
        };
        let chat_created = match existing_chat_id {
            Some(_) => chat_created_before,
            None => {
                if let Some(connection_contact) = connection_contact {
                    Self::create_connection_chat(&mut txn, &group, connection_contact)
                        .await
                        .map_err(OutboundServiceError::fatal)?;
                } else if let Some(outgoing_request) = outgoing_request {
                    outgoing_request
                        .store(&mut txn, &group, ds_timestamp)
                        .await
                        .map_err(OutboundServiceError::fatal)?;
                } else {
                    Self::create_group_chat(&mut txn, &group, own_user_id, ds_timestamp)
                        .await
                        .map_err(OutboundServiceError::fatal)?;
                }
                true
            }
        };

        let pending = PendingResyncCommit {
            commit,
            original_leaf_index,
            built_on: served,
            produces: StateFingerprint::of_group(&group),
            members_before,
            chat_created,
        };
        if self.is_queued() {
            Self::store_pending_commit(&mut txn, &group_id, &pending)
                .await
                .map_err(OutboundServiceError::fatal)?;
        }
        txn.commit().await?;

        Self::send_commit(api_clients, signer, &self, &pending).await?;

        self.finish(connection, &pending).await
    }

    /// Reads the queue entry and the local group to resolve what a new commit
    /// builds on.
    ///
    /// Returns `None` if the queue entry is gone. A commit processed since the
    /// attempt started removes it, and proves that the DS accepted the local
    /// state.
    async fn load_join_base(
        &self,
        txn: &mut WriteDbTransaction<'_>,
    ) -> anyhow::Result<Option<JoinBase>> {
        let local_group = Group::load(&mut *txn, &self.group_id).await?;
        let stored = if self.is_queued() {
            let Some(stored) = Self::load_pending_commit(&mut *txn, &self.group_id).await? else {
                return Ok(None);
            };
            stored
        } else {
            None
        };
        let stored_commit = match &stored {
            None => StoredCommit::None,
            Some(stored) if stored.matches(local_group.as_ref()) => {
                StoredCommit::Current(stored.built_on)
            }
            Some(_) => StoredCommit::Stale,
        };
        let bound = EpochBound::resolve(
            self.reason,
            stored_commit,
            local_group.as_ref().map(GroupEpochs::of_group),
        )?;
        let base = match stored {
            // Also from a stale commit, since the resync hasn't finished since.
            Some(stored) => JoinBase {
                bound,
                members_before: stored.members_before,
                chat_created: stored.chat_created,
            },
            None => JoinBase {
                bound,
                members_before: local_group.map(|group| group.members().collect()),
                chat_created: false,
            },
        };
        Ok(Some(base))
    }

    /// Whether this resync has a queue entry. A sibling accept runs directly
    /// instead.
    fn is_queued(&self) -> bool {
        self.reason != ResyncReason::SiblingAccepted
    }

    /// Completes the resync after the DS accepted `pending`: removes the queue
    /// entry, confirms an outgoing request whose acceptance arrived meanwhile,
    /// writes the system messages and marks the chat active.
    ///
    /// Everything comes from the stored commit and the queue entry, since the
    /// commit may have been built in an earlier attempt. Changes nothing and
    /// returns [`ResyncOutcome::Superseded`] if the queue entry of a queued
    /// resync is gone.
    async fn finish(
        &self,
        mut connection: impl WriteConnection,
        pending: &PendingResyncCommit,
    ) -> Result<ResyncOutcome, OutboundServiceError> {
        let chat_id = match self.chat_id {
            Some(chat_id) => chat_id,
            None => ChatId::try_from(&self.group_id).map_err(OutboundServiceError::fatal)?,
        };
        let ds_timestamp = TimeStamp::now();

        let result = connection
            .with_transaction(async |txn| -> anyhow::Result<bool> {
                let accepted_friendship_package =
                    Self::load_accepted_friendship_package(&mut *txn, &self.group_id).await?;
                let removed = Self::remove(&mut *txn, &self.group_id).await?;
                if self.is_queued() && !removed {
                    return Ok(false);
                }

                let group = Group::load(&mut *txn, &self.group_id)
                    .await?
                    .context("resynced group not found")?;

                if let Some(friendship_package) = accepted_friendship_package {
                    // A join that fails to confirm leaves the request open, but
                    // the rest of the resync still finishes.
                    let mut savepoint = txn.begin().await?;
                    match Self::confirm_parked_join(
                        &mut savepoint,
                        chat_id,
                        &group,
                        &friendship_package,
                        ds_timestamp,
                    )
                    .await
                    {
                        Ok(()) => savepoint.commit().await?,
                        Err(error) => {
                            warn!(%error, ?chat_id, "Failed to confirm the outgoing request");
                        }
                    }
                }

                if self.shares_vc_leaf && pending.chat_created {
                    ChatMessage::new_system_message(
                        chat_id,
                        ds_timestamp,
                        SystemMessage::Onboarded,
                    )
                    .store(&mut *txn)
                    .await?;
                }
                if let Some(before) = &pending.members_before {
                    let diff = MembersDiff::compute(before.clone(), group.members().collect());
                    for user_id in diff.added {
                        ChatMessage::new_system_message(
                            chat_id,
                            ds_timestamp,
                            SystemMessage::Add(None, user_id),
                        )
                        .store(&mut *txn)
                        .await?;
                    }
                    for user_id in diff.removed {
                        ChatMessage::new_system_message(
                            chat_id,
                            ds_timestamp,
                            SystemMessage::Remove(None, user_id),
                        )
                        .store(&mut *txn)
                        .await?;
                    }
                }
                Chat::update_status(txn, chat_id, &ChatStatus::Active).await?;
                Ok(true)
            })
            .await;
        match result {
            Ok(true) => Ok(ResyncOutcome::Synced(chat_id)),
            Ok(false) => Ok(ResyncOutcome::Superseded),
            Err(error) => {
                error!(%error, ?chat_id, "Failed to update chat after accepted resync commit");
                Self::remove(&mut connection, &self.group_id).await?;
                Ok(ResyncOutcome::Synced(chat_id))
            }
        }
    }

    /// Confirms the outgoing request of the chat with the friendship package
    /// of the recipient's join, which arrived while the resync was not
    /// accepted yet.
    async fn confirm_parked_join(
        txn: &mut WriteDbTransaction<'_>,
        chat_id: ChatId,
        group: &Group,
        friendship_package: &EncryptedFriendshipPackage,
        ds_timestamp: TimeStamp,
    ) -> anyhow::Result<()> {
        let mut chat = Chat::load(&mut *txn, &chat_id)
            .await?
            .context("chat of the outgoing request not found")?;
        let system_message =
            outgoing_requests::confirm_missed_join(txn, &mut chat, group, friendship_package)
                .await?;
        ChatMessage::new_system_message(chat_id, ds_timestamp, system_message)
            .store(&mut *txn)
            .await?;
        Ok(())
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
        let attributes =
            CoreUser::chat_attributes_from_group_data(&mut *txn, group, own_user_id, ds_timestamp)
                .await?;

        let chat = Chat::new_pending_group_chat(group.group_id().clone(), attributes);
        chat.store(&mut *txn).await?;
        Ok(chat.id())
    }

    pub(crate) fn find_own_leaf(
        external_commit_info: &ExternalCommitInfoIn,
        signer: &LeafSigningKey,
    ) -> Option<LeafNodeIndex> {
        let signature_key_bytes = signer.verifying_key().as_slice();
        external_commit_info
            .ratchet_tree_in
            .full_leaves()
            .find(|(_, leaf)| leaf.signature_key().as_slice() == signature_key_bytes)
            .map(|(index, _)| index)
    }

    fn resolve_original_leaf_index(
        &self,
        external_commit_info: &ExternalCommitInfoIn,
        signer: &LeafSigningKey,
    ) -> Option<LeafNodeIndex> {
        Self::find_own_leaf(external_commit_info, signer).inspect(|&index| {
            if index != self.original_leaf_index {
                info!(
                    group_id = ?self.group_id,
                    queued = %self.original_leaf_index,
                    resolved = %index,
                    "The leaf to resync moved since the resync was queued"
                );
            }
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
            .map_err(OutboundServiceError::from)
    }

    async fn create_commit(
        &self,
        txn: &mut WriteDbTransaction<'_>,
        // Needs api clients until we can schedule group member authentication
        api_clients: &ApiClients,
        signer: &LeafSigningKey,
        own_user_id: &UserId,
        external_commit_info: ExternalCommitInfoIn,
    ) -> Result<(Group, ResyncCommit, DecryptedProfileInfos)> {
        // TODO: We should somehow mark the chat as "resyncing" in the DB and
        // reflect that in the UI.

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
        if self.pq_group_id.is_some() {
            // APQ group
            let (group, bundle, member_profile_infos) = Box::pin(Group::join_apq_group_externally(
                txn,
                api_clients,
                external_commit_info,
                signer,
                own_user_id,
                self.group_state_ear_key.clone(),
                self.identity_link_wrapper_key.clone(),
                aad,
                vc_group_id,
                None,
            ))
            .await??;
            Ok((
                group,
                ResyncCommit::PQ(Box::new(bundle)),
                member_profile_infos,
            ))
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
                self.group_state_ear_key.clone(),
                self.identity_link_wrapper_key.clone(),
                aad,
                None,
                vc_group_id,
            )
            .await??;
            Ok((
                group,
                ResyncCommit::T(Box::new(ResyncTCommit { commit, group_info })),
                member_profile_infos,
            ))
        }
    }

    /// Sends the stored commit `pending` of this resync.
    async fn send_commit(
        api_clients: &ApiClients,
        signer: &LeafSigningKey,
        resync: &Resync,
        pending: &PendingResyncCommit,
    ) -> Result<(), OutboundServiceError> {
        let qgid: QualifiedGroupId = resync
            .group_id
            .clone()
            .try_into()
            .map_err(OutboundServiceError::fatal)?;
        let api_client = api_clients
            .get(qgid.owning_domain())
            .map_err(OutboundServiceError::fatal)?;

        match pending.commit.clone() {
            ResyncCommit::T(commit) => {
                api_client
                    .ds_resync(
                        commit.commit,
                        commit.group_info,
                        signer,
                        &resync.group_state_ear_key,
                        pending.original_leaf_index,
                    )
                    .await?
            }
            ResyncCommit::PQ(bundle) => {
                api_client
                    .ds_apq_resync(
                        *bundle,
                        signer,
                        &resync.group_state_ear_key,
                        pending.original_leaf_index,
                    )
                    .await?
            }
        };

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
            let outgoing_request = self.outgoing_request.as_ref().map(BlobEncoded);
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
                    outgoing_request,
                    status,
                    reason,
                    attempts
                )
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, 0)
                ON CONFLICT DO NOTHING",
                group_id,
                pq_group_id,
                self.chat_id,
                self.group_state_ear_key,
                self.identity_link_wrapper_key,
                original_leaf_index,
                self.shares_vc_leaf,
                connection_contact,
                outgoing_request,
                ResyncStatus::Pending as _,
                self.reason as _,
            )
            .execute(connection.as_mut())
            .await?;
            Ok(())
        }

        /// Like [`Resync::enqueue`], but resets an existing entry.
        ///
        /// Keeps the entry's stored commit and the friendship package of a
        /// recipient's join.
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
            let outgoing_request = self.outgoing_request.as_ref().map(BlobEncoded);
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
                    outgoing_request,
                    status,
                    reason,
                    attempts
                )
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, 0)
                ON CONFLICT (group_id) DO UPDATE SET
                    pq_group_id = excluded.pq_group_id,
                    chat_id = excluded.chat_id,
                    group_state_ear_key = excluded.group_state_ear_key,
                    identity_link_wrapper_key = excluded.identity_link_wrapper_key,
                    original_leaf_index = excluded.original_leaf_index,
                    shares_vc_leaf = excluded.shares_vc_leaf,
                    connection_contact = excluded.connection_contact,
                    outgoing_request = excluded.outgoing_request,
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
                outgoing_request,
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
                outgoing_request: Option<BlobDecoded<OutgoingRequest>>,
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
                    outgoing_request AS "outgoing_request: _",
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
                outgoing_request: record.outgoing_request.map(BlobDecoded::into_inner),
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

        /// Whether the group has a queue entry that onboards into the connection
        /// group of an outgoing request.
        pub(crate) async fn onboards_outgoing_request(
            mut connection: impl ReadConnection,
            group_id: &GroupId,
        ) -> sqlx::Result<bool> {
            let group_id = group_id.as_slice();
            query_scalar!(
                r#"SELECT EXISTS (
                    SELECT 1 FROM resync_queue
                    WHERE group_id = ? AND outgoing_request IS NOT NULL
                ) AS "onboards: bool""#,
                group_id,
            )
            .fetch_one(connection.as_mut())
            .await
        }

        /// Records that the recipient accepted the outgoing request the group's
        /// queue entry onboards into.
        pub(crate) async fn record_acceptance(
            mut connection: impl WriteConnection,
            group_id: &GroupId,
            friendship_package: &EncryptedFriendshipPackage,
        ) -> sqlx::Result<()> {
            let group_id = group_id.as_slice();
            let friendship_package = BlobEncoded(friendship_package);
            query!(
                "UPDATE resync_queue
                SET accepted_friendship_package = ?2
                WHERE group_id = ?1",
                group_id,
                friendship_package,
            )
            .execute(connection.as_mut())
            .await?;
            Ok(())
        }

        /// The stored commit of the group's queue entry. `None` if the group
        /// has no queue entry.
        pub(crate) async fn load_pending_commit(
            mut connection: impl ReadConnection,
            group_id: &GroupId,
        ) -> sqlx::Result<Option<Option<PendingResyncCommit>>> {
            let group_id = group_id.as_slice();
            let pending_commit = query_scalar!(
                r#"SELECT pending_commit AS "pending_commit: BlobDecoded<PendingResyncCommit>"
                FROM resync_queue
                WHERE group_id = ?"#,
                group_id,
            )
            .fetch_optional(connection.as_mut())
            .await?;
            Ok(pending_commit.map(|pending_commit| pending_commit.map(BlobDecoded::into_inner)))
        }

        /// Stores the commit of the group's queue entry, replacing an earlier
        /// one.
        ///
        /// Fails if the group has no queue entry, so that the commit is never
        /// dropped silently.
        pub(crate) async fn store_pending_commit(
            mut connection: impl WriteConnection,
            group_id: &GroupId,
            pending_commit: &PendingResyncCommit,
        ) -> anyhow::Result<()> {
            let group_id = group_id.as_slice();
            let pending_commit = BlobEncoded(pending_commit);
            let result = query!(
                "UPDATE resync_queue SET pending_commit = ?2 WHERE group_id = ?1",
                group_id,
                pending_commit,
            )
            .execute(connection.as_mut())
            .await?;
            ensure!(
                result.rows_affected() == 1,
                "no resync queue entry to store the commit with"
            );
            Ok(())
        }

        /// The friendship package of the recipient's join that the group's
        /// queue entry keeps.
        pub(crate) async fn load_accepted_friendship_package(
            mut connection: impl ReadConnection,
            group_id: &GroupId,
        ) -> sqlx::Result<Option<EncryptedFriendshipPackage>> {
            let group_id = group_id.as_slice();
            let package = query_scalar!(
                r#"SELECT accepted_friendship_package AS "accepted_friendship_package: BlobDecoded<EncryptedFriendshipPackage>"
                FROM resync_queue
                WHERE group_id = ?"#,
                group_id,
            )
            .fetch_optional(connection.as_mut())
            .await?;
            Ok(package.flatten().map(BlobDecoded::into_inner))
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

        /// Removes the group's queue entry. Returns whether there was one.
        pub(crate) async fn remove(
            mut connection: impl WriteConnection,
            group_id: &GroupId,
        ) -> sqlx::Result<bool> {
            let group_id_bytes = group_id.as_slice();
            let result = query!(
                "DELETE FROM resync_queue WHERE group_id = ?",
                group_id_bytes
            )
            .execute(connection.as_mut())
            .await?;
            Ok(result.rows_affected() > 0)
        }

        /// Whether the group's queue entry holds a commit the DS has not
        /// confirmed yet.
        pub(crate) async fn has_pending_commit(
            mut connection: impl ReadConnection,
            group_id: &GroupId,
        ) -> sqlx::Result<bool> {
            let group_id = group_id.as_slice();
            query_scalar!(
                r#"SELECT EXISTS (
                    SELECT 1 FROM resync_queue
                    WHERE group_id = ? AND pending_commit IS NOT NULL
                ) AS "pending: bool""#,
                group_id,
            )
            .fetch_one(connection.as_mut())
            .await
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
enum ResyncCommit {
    T(Box<ResyncTCommit>),
    PQ(Box<ApqCommitMessageBundle>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ResyncTCommit {
    commit: MlsMessageOut,
    group_info: MlsMessageOut,
}

#[cfg(test)]
mod tests {
    use std::assert_matches;

    use aircommon::crypto::aead::keys::FriendshipPackageEarKey;
    use airprotos::client::component::{GroupTombstone, TombstoneContext};
    use chrono::{TimeDelta, TimeZone};
    use openmls::prelude::OpenMlsProvider;
    use tls_codec::Serialize as _;

    use crate::{
        ChatAttributes, EventMessage, Message,
        db::access::DbAccess,
        groups::openmls_provider::AirOpenMlsProvider,
        job::pending_chat_operation::{
            PendingChatOperation,
            tests::{setup_group_and_chat, setup_group_with_pending_operation},
        },
        utils::persistence::open_db_in_memory,
    };

    use super::*;

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
            outgoing_request: None,
            reason,
            attempts: 0,
        };

        Ok((pool, resync))
    }

    /// A group with a chat, a pending chat operation and a queued resync, as a
    /// resync that finds the group deleted sees it. Returns the group's owner.
    async fn setup_deleted_group() -> anyhow::Result<(DbAccess, GroupId, ChatId, UserId)> {
        let (pool, chat_id) = Box::pin(setup_group_with_pending_operation()).await?;
        let group = Group::load_with_chat_id(pool.read().await?, chat_id)
            .await?
            .context("group not found")?;
        Resync::for_group(chat_id, &group, ResyncReason::Manual)
            .enqueue(pool.write().await?)
            .await?;
        assert!(
            PendingChatOperation::load(pool.read().await?, &chat_id)
                .await?
                .is_some()
        );
        Ok((
            pool,
            group.group_id().clone(),
            chat_id,
            group.own_user_id().clone(),
        ))
    }

    async fn load_chat(pool: &DbAccess, chat_id: ChatId) -> anyhow::Result<Chat> {
        pool.with_read_transaction(async |txn| Chat::load(txn, &chat_id).await)
            .await?
            .context("chat not found")
    }

    fn group_deleted() -> GroupClosed {
        GroupClosed {
            closer: UserId::random("example.com".parse().unwrap()),
            tombstone: GroupTombstone::deleted(
                Utc.with_ymd_and_hms(2026, 10, 13, 12, 0, 0).unwrap().into(),
            ),
        }
    }

    async fn deletion_message_times(
        pool: &DbAccess,
        chat_id: ChatId,
        deleter: &UserId,
    ) -> anyhow::Result<Vec<DateTime<Utc>>> {
        let messages = ChatMessage::load_multiple(pool.read().await?, chat_id, 100).await?;
        let deletion = Message::Event(EventMessage::System(SystemMessage::DeleteGroup(
            deleter.clone(),
        )));
        Ok(messages
            .iter()
            .filter(|message| message.message() == &deletion)
            .map(|message| message.timestamp())
            .collect())
    }

    async fn assert_group_torn_down(
        pool: &DbAccess,
        group_id: &GroupId,
        chat_id: ChatId,
    ) -> anyhow::Result<()> {
        assert!(Group::load(pool.read().await?, group_id).await?.is_none());
        assert!(
            PendingChatOperation::load(pool.read().await?, &chat_id)
                .await?
                .is_none()
        );
        assert!(
            Resync::status(pool.read().await?, group_id)
                .await?
                .is_none()
        );
        Ok(())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_deleted_group_ends_the_chat_with_its_members() -> anyhow::Result<()> {
        let (pool, group_id, chat_id, owner) = setup_deleted_group().await?;
        let deleted = group_deleted();

        pool.write()
            .await?
            .with_transaction(async |txn| handle_group_closed(txn, &group_id, &deleted).await)
            .await?;

        let chat = load_chat(&pool, chat_id).await?;
        assert_eq!(chat.status(), &ChatStatus::inactive(vec![owner]));
        assert_eq!(
            deletion_message_times(&pool, chat_id, &deleted.closer).await?,
            [*deleted.tombstone.timestamp.unwrap().as_ref()]
        );
        assert_group_torn_down(&pool, &group_id, chat_id).await?;

        Ok(())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn an_unknown_tombstone_without_timestamp_announces_the_deletion_now()
    -> anyhow::Result<()> {
        let (pool, group_id, chat_id, owner) = setup_deleted_group().await?;
        let closed = GroupClosed {
            closer: UserId::random("example.com".parse().unwrap()),
            tombstone: GroupTombstone {
                timestamp: None,
                context: TombstoneContext::Unknown,
            },
        };

        let before = Utc::now();
        pool.write()
            .await?
            .with_transaction(async |txn| handle_group_closed(txn, &group_id, &closed).await)
            .await?;
        let after = Utc::now();

        let chat = load_chat(&pool, chat_id).await?;
        assert_eq!(chat.status(), &ChatStatus::inactive(vec![owner]));
        let [time] = deletion_message_times(&pool, chat_id, &closed.closer).await?[..] else {
            panic!("expected one deletion message");
        };
        assert!(before <= time && time <= after);
        assert_group_torn_down(&pool, &group_id, chat_id).await?;

        Ok(())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_deleted_group_adds_no_message_to_an_inactive_chat() -> anyhow::Result<()> {
        let (pool, group_id, chat_id, _owner) = setup_deleted_group().await?;
        let deleted = group_deleted();
        let inactive = ChatStatus::inactive(Vec::new());
        pool.write()
            .await?
            .with_transaction(async |txn| Chat::update_status(txn, chat_id, &inactive).await)
            .await?;

        pool.write()
            .await?
            .with_transaction(async |txn| handle_group_closed(txn, &group_id, &deleted).await)
            .await?;

        let chat = load_chat(&pool, chat_id).await?;
        assert_eq!(chat.status(), &inactive);
        assert!(
            deletion_message_times(&pool, chat_id, &deleted.closer)
                .await?
                .is_empty()
        );
        assert_group_torn_down(&pool, &group_id, chat_id).await?;

        Ok(())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_deleted_group_without_a_chat_creates_none() -> anyhow::Result<()> {
        let (pool, group_id, chat_id, _owner) = setup_deleted_group().await?;
        let deleted = group_deleted();
        Chat::delete(pool.write().await?, chat_id).await?;
        let group = Group::load(pool.read().await?, &group_id)
            .await?
            .context("group deleted with the chat")?;
        let mut resync = Resync::for_group(chat_id, &group, ResyncReason::Onboarding);
        resync.chat_id = None;
        resync.enqueue(pool.write().await?).await?;
        assert!(
            Resync::status(pool.read().await?, &group_id)
                .await?
                .is_some()
        );
        let chats_before = Chat::load_ordered_ids(pool.read().await?).await?;

        pool.write()
            .await?
            .with_transaction(async |txn| handle_group_closed(txn, &group_id, &deleted).await)
            .await?;

        assert_eq!(
            Chat::load_ordered_ids(pool.read().await?).await?,
            chats_before
        );
        assert!(Group::load(pool.read().await?, &group_id).await?.is_none());
        assert!(
            Resync::status(pool.read().await?, &group_id)
                .await?
                .is_none()
        );

        Ok(())
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
    async fn onboarding_keeps_the_outgoing_request_and_its_acceptance() -> anyhow::Result<()> {
        let (pool, mut resync) = setup(ResyncReason::Onboarding).await?;
        let group_id = resync.group_id.clone();
        let request = OutgoingRequest::Targeted {
            user_id: UserId::random("example.com".parse()?),
            friendship_package_ear_key: FriendshipPackageEarKey::random()?,
            origin_group_id: Some(GroupId::from(QualifiedGroupId::new(
                Uuid::new_v4(),
                "example.com".parse()?,
            ))),
            sent_at: Some(TimeStamp::now()),
        };
        resync.chat_id = None;
        resync.outgoing_request = Some(request.clone());
        let mut connection = pool.write().await?;
        resync.enqueue(&mut connection).await?;
        assert!(Resync::onboards_outgoing_request(&mut connection, &group_id).await?);

        let friendship_package = EncryptedFriendshipPackage::random();
        Resync::record_acceptance(&mut connection, &group_id, &friendship_package).await?;

        let dequeued = connection
            .with_transaction(async |txn| Resync::dequeue(txn, Uuid::new_v4(), Utc::now()).await)
            .await?
            .expect("entry should be due");
        assert_eq!(dequeued.outgoing_request, Some(request));
        let accepted = Resync::load_accepted_friendship_package(&mut connection, &group_id)
            .await?
            .expect("the acceptance should be kept");
        assert_eq!(
            accepted.aead_ciphertext(),
            friendship_package.aead_ciphertext()
        );

        Ok(())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn enqueue_or_reset_keeps_the_acceptance() -> anyhow::Result<()> {
        let (pool, resync) = setup(ResyncReason::Onboarding).await?;
        let group_id = resync.group_id.clone();
        let mut connection = pool.write().await?;
        resync.enqueue(&mut connection).await?;
        let friendship_package = EncryptedFriendshipPackage::random();
        Resync::record_acceptance(&mut connection, &group_id, &friendship_package).await?;

        let mut manual = resync;
        manual.reason = ResyncReason::Manual;
        manual.enqueue_or_reset(&mut connection).await?;

        assert!(
            Resync::load_accepted_friendship_package(&mut connection, &group_id)
                .await?
                .is_some()
        );

        Ok(())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn only_onboarding_into_an_outgoing_request_takes_a_join() -> anyhow::Result<()> {
        let (pool, resync) = setup(ResyncReason::Onboarding).await?;
        let group_id = resync.group_id.clone();
        let mut connection = pool.write().await?;
        resync.enqueue(&mut connection).await?;
        assert!(!Resync::onboards_outgoing_request(&mut connection, &group_id).await?);

        let unknown = GroupId::from(QualifiedGroupId::new(
            Uuid::new_v4(),
            "example.com".parse()?,
        ));
        assert!(!Resync::onboards_outgoing_request(&mut connection, &unknown).await?);

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

    fn epochs(t: u64, pq: Option<u64>) -> GroupEpochs {
        GroupEpochs {
            t: GroupEpoch::from(t),
            pq: pq.map(GroupEpoch::from),
        }
    }

    fn check(
        reason: ResyncReason,
        built_on: Option<GroupEpochs>,
        local: Option<GroupEpochs>,
        served: GroupEpochs,
    ) -> Result<(), StaleGroupSnapshot> {
        let stored = built_on.map_or(StoredCommit::None, StoredCommit::Current);
        EpochBound::resolve(reason, stored, local)?.check(reason, served)
    }

    const ALL_REASONS: [ResyncReason; 6] = [
        ResyncReason::FutureEpoch,
        ResyncReason::MissingPqGroupState,
        ResyncReason::VirtualClientDesync,
        ResyncReason::Manual,
        ResyncReason::Onboarding,
        ResyncReason::SiblingAccepted,
    ];

    #[test]
    fn out_of_sync_resyncs_need_a_newer_epoch_than_the_local_one() {
        for reason in [ResyncReason::FutureEpoch, ResyncReason::VirtualClientDesync] {
            let local = Some(epochs(3, None));
            assert!(check(reason, None, local, epochs(4, None)).is_ok());
            assert!(check(reason, None, local, epochs(3, None)).is_err());
            assert!(check(reason, None, local, epochs(2, None)).is_err());
        }
    }

    #[test]
    fn other_resyncs_accept_the_local_epoch_but_not_an_older_one() {
        for reason in [ResyncReason::Manual, ResyncReason::MissingPqGroupState] {
            let local = Some(epochs(3, None));
            assert!(check(reason, None, local, epochs(4, None)).is_ok());
            assert!(check(reason, None, local, epochs(3, None)).is_ok());
            assert!(check(reason, None, local, epochs(2, None)).is_err());
        }
    }

    #[test]
    fn joins_without_accepted_state_are_not_checked() {
        for reason in [ResyncReason::Onboarding, ResyncReason::SiblingAccepted] {
            assert!(check(reason, None, Some(epochs(5, Some(5))), epochs(1, None)).is_ok());
            assert!(check(reason, None, None, epochs(1, None)).is_ok());
        }
    }

    #[test]
    fn checked_resyncs_without_local_group_are_refused() {
        for reason in [
            ResyncReason::FutureEpoch,
            ResyncReason::VirtualClientDesync,
            ResyncReason::Manual,
            ResyncReason::MissingPqGroupState,
        ] {
            assert_matches!(
                check(reason, None, None, epochs(1, None)),
                Err(StaleGroupSnapshot::NoLocalState { .. })
            );
        }
    }

    #[test]
    fn a_stored_commit_bounds_every_reason_by_the_state_it_was_built_on() {
        let built_on = Some(epochs(3, Some(2)));
        // The local group holds the unaccepted state the commit produced.
        let local = Some(epochs(4, Some(3)));
        for reason in ALL_REASONS {
            assert!(check(reason, built_on, local, epochs(3, Some(2))).is_ok());
            assert!(check(reason, built_on, local, epochs(5, Some(2))).is_ok());
            assert!(check(reason, built_on, local, epochs(2, Some(2))).is_err());
            assert!(check(reason, built_on, local, epochs(3, Some(1))).is_err());
            assert!(check(reason, built_on, None, epochs(3, Some(2))).is_ok());
        }
    }

    #[test]
    fn a_stale_stored_commit_bounds_every_reason_by_the_local_state() {
        let local = Some(epochs(3, Some(2)));
        for reason in ALL_REASONS {
            let bound = EpochBound::resolve(reason, StoredCommit::Stale, local).unwrap();
            assert!(bound.check(reason, epochs(3, Some(2))).is_ok());
            assert!(bound.check(reason, epochs(4, Some(2))).is_ok());
            assert!(bound.check(reason, epochs(2, Some(2))).is_err());
            assert!(bound.check(reason, epochs(3, Some(1))).is_err());
        }
        assert_eq!(
            EpochBound::resolve(ResyncReason::Onboarding, StoredCommit::Stale, None).unwrap(),
            EpochBound::Unchecked
        );
    }

    #[test]
    fn pq_epoch_must_not_go_back() {
        let reason = ResyncReason::Manual;
        let local = Some(epochs(3, Some(2)));
        assert!(check(reason, None, local, epochs(3, Some(2))).is_ok());
        assert!(check(reason, None, local, epochs(4, Some(3))).is_ok());
        assert!(check(reason, None, local, epochs(4, Some(1))).is_err());
        assert!(check(reason, None, local, epochs(4, None)).is_err());
        // A group whose local PQ leg is gone may get it back at any PQ epoch.
        assert!(check(reason, None, Some(epochs(3, None)), epochs(3, Some(0))).is_ok());
    }

    #[test]
    fn teardown_needs_a_strictly_newer_state_than_the_bound() {
        let reason = ResyncReason::Manual;
        let accepted =
            EpochBound::resolve(reason, StoredCommit::None, Some(epochs(3, None))).unwrap();
        let attempt =
            EpochBound::resolve(reason, StoredCommit::Current(epochs(3, None)), None).unwrap();
        for bound in [accepted, attempt] {
            assert!(bound.check_teardown(reason, epochs(4, None)).is_ok());
            assert!(bound.check_teardown(reason, epochs(3, None)).is_err());
        }
        let unchecked = EpochBound::resolve(
            ResyncReason::Onboarding,
            StoredCommit::None,
            Some(epochs(3, None)),
        )
        .unwrap();
        assert!(
            unchecked
                .check_teardown(ResyncReason::Onboarding, epochs(1, None))
                .is_ok()
        );
    }

    #[test]
    fn stale_snapshot_error_names_both_epochs() {
        let error = check(
            ResyncReason::FutureEpoch,
            None,
            Some(epochs(3, None)),
            epochs(2, None),
        )
        .expect_err("an older epoch is stale");
        let message = error.to_string();
        assert!(message.contains("future_epoch"), "{message}");
        assert!(message.contains("GroupEpoch(3)"), "{message}");
        assert!(message.contains("GroupEpoch(2)"), "{message}");
    }

    /// A group with a chat and a queued resync, and a commit as if the resync
    /// had joined the group's current state.
    async fn setup_pending_commit(
        reason: ResyncReason,
    ) -> anyhow::Result<(DbAccess, Resync, PendingResyncCommit)> {
        let (pool, group, chat_id, signing_key) = Box::pin(setup_group_and_chat()).await?;
        let resync = Resync::for_group(chat_id, &group, reason);
        let mut connection = pool.write().await?;
        resync.enqueue(&mut connection).await?;

        let provider = AirOpenMlsProvider::new(connection.as_mut());
        let group_info =
            group
                .mls_group()
                .export_group_info(provider.crypto(), &signing_key, false)?;
        let pending = PendingResyncCommit {
            commit: ResyncCommit::T(Box::new(ResyncTCommit {
                commit: group_info.clone(),
                group_info,
            })),
            original_leaf_index: LeafNodeIndex::new(1),
            built_on: epochs(0, None),
            produces: StateFingerprint::of_group(&group),
            members_before: Some(group.members().collect()),
            chat_created: true,
        };
        drop(connection);
        Ok((pool, resync, pending))
    }

    fn serialized_commit(pending: &PendingResyncCommit) -> anyhow::Result<Vec<u8>> {
        let ResyncCommit::T(commit) = &pending.commit else {
            bail!("not a T commit");
        };
        Ok([
            commit.commit.tls_serialize_detached()?,
            commit.group_info.tls_serialize_detached()?,
        ]
        .concat())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn pending_commit_round_trips_through_the_queue() -> anyhow::Result<()> {
        let (pool, resync, pending) = setup_pending_commit(ResyncReason::FutureEpoch).await?;
        let mut connection = pool.write().await?;
        assert!(
            Resync::load_pending_commit(&mut connection, &resync.group_id)
                .await?
                .context("entry should exist")?
                .is_none()
        );

        Resync::store_pending_commit(&mut connection, &resync.group_id, &pending).await?;

        let loaded = Resync::load_pending_commit(&mut connection, &resync.group_id)
            .await?
            .context("entry should exist")?
            .context("commit should be stored")?;
        assert_eq!(serialized_commit(&loaded)?, serialized_commit(&pending)?);
        assert_eq!(loaded.original_leaf_index, pending.original_leaf_index);
        assert_eq!(loaded.built_on, pending.built_on);
        assert_eq!(loaded.produces, pending.produces);
        assert_eq!(loaded.members_before, pending.members_before);
        assert_eq!(loaded.chat_created, pending.chat_created);

        Ok(())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn storing_a_commit_without_queue_entry_fails() -> anyhow::Result<()> {
        let (pool, resync, pending) = setup_pending_commit(ResyncReason::FutureEpoch).await?;
        let mut connection = pool.write().await?;
        Resync::remove(&mut connection, &resync.group_id).await?;

        assert!(
            Resync::store_pending_commit(&mut connection, &resync.group_id, &pending)
                .await
                .is_err()
        );
        assert!(
            Resync::load_pending_commit(&mut connection, &resync.group_id)
                .await?
                .is_none()
        );

        Ok(())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn enqueue_or_reset_keeps_the_stored_commit() -> anyhow::Result<()> {
        let (pool, resync, pending) = setup_pending_commit(ResyncReason::FutureEpoch).await?;
        let mut connection = pool.write().await?;
        Resync::store_pending_commit(&mut connection, &resync.group_id, &pending).await?;
        Resync::mark_failed(&mut connection, &resync.group_id, "boom").await?;

        let mut manual = resync;
        manual.reason = ResyncReason::Manual;
        manual.enqueue_or_reset(&mut connection).await?;

        assert!(
            Resync::load_pending_commit(&mut connection, &manual.group_id)
                .await?
                .context("entry should exist")?
                .is_some()
        );

        Ok(())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_stored_commit_counts_only_while_the_local_group_holds_its_state()
    -> anyhow::Result<()> {
        let (pool, resync, mut pending) = setup_pending_commit(ResyncReason::FutureEpoch).await?;
        let group = Group::load(pool.read().await?, &resync.group_id)
            .await?
            .context("group not found")?;
        assert!(pending.matches(Some(&group)));
        assert!(!pending.matches(None));

        pending.produces.t.confirmed_transcript_hash = vec![0; 32];
        assert!(!pending.matches(Some(&group)));
        pending.produces = StateFingerprint::of_group(&group);
        pending.produces.t.epoch = GroupEpoch::from(1);
        assert!(!pending.matches(Some(&group)));

        Ok(())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn join_base_uses_a_matching_stored_commit() -> anyhow::Result<()> {
        let (pool, resync, mut pending) = setup_pending_commit(ResyncReason::FutureEpoch).await?;
        pending.members_before = None;
        let mut connection = pool.write().await?;
        Resync::store_pending_commit(&mut connection, &resync.group_id, &pending).await?;

        let mut txn = connection.begin().await?;
        let base = resync
            .load_join_base(&mut txn)
            .await?
            .context("entry should exist")?;
        assert_eq!(base.bound, EpochBound::Attempt(pending.built_on));
        assert_eq!(base.members_before, None);
        assert!(base.chat_created);

        Ok(())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn join_base_bounds_a_stale_stored_commit_by_the_local_state() -> anyhow::Result<()> {
        for reason in [ResyncReason::FutureEpoch, ResyncReason::Onboarding] {
            let (pool, resync, mut pending) = setup_pending_commit(reason).await?;
            pending.produces.t.confirmed_transcript_hash = vec![0; 32];
            pending.members_before = None;
            let mut connection = pool.write().await?;
            Resync::store_pending_commit(&mut connection, &resync.group_id, &pending).await?;

            let mut txn = connection.begin().await?;
            let base = resync
                .load_join_base(&mut txn)
                .await?
                .context("entry should exist")?;
            let group = Group::load(&mut txn, &resync.group_id)
                .await?
                .context("group not found")?;
            assert_eq!(
                base.bound,
                EpochBound::Accepted(EpochRequirement::NotOlder, GroupEpochs::of_group(&group))
            );
            assert_eq!(base.members_before, None);
            assert!(base.chat_created);
        }

        Ok(())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn join_base_without_stored_commit_uses_the_local_state() -> anyhow::Result<()> {
        let (pool, resync, _) = setup_pending_commit(ResyncReason::FutureEpoch).await?;
        let mut connection = pool.write().await?;

        let mut txn = connection.begin().await?;
        let base = resync
            .load_join_base(&mut txn)
            .await?
            .context("entry should exist")?;
        let group = Group::load(&mut txn, &resync.group_id)
            .await?
            .context("group not found")?;
        assert_eq!(
            base.bound,
            EpochBound::Accepted(EpochRequirement::Newer, GroupEpochs::of_group(&group))
        );
        assert_eq!(base.members_before, Some(group.members().collect()));
        assert!(!base.chat_created);

        Ok(())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn join_base_is_missing_once_the_entry_is_removed() -> anyhow::Result<()> {
        let (pool, resync, pending) = setup_pending_commit(ResyncReason::FutureEpoch).await?;
        let mut connection = pool.write().await?;
        Resync::store_pending_commit(&mut connection, &resync.group_id, &pending).await?;
        Resync::remove(&mut connection, &resync.group_id).await?;

        let mut txn = connection.begin().await?;
        assert!(resync.load_join_base(&mut txn).await?.is_none());

        Ok(())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn finish_changes_nothing_once_the_entry_is_removed() -> anyhow::Result<()> {
        let (pool, resync, pending) = setup_pending_commit(ResyncReason::FutureEpoch).await?;
        let chat_id = resync.chat_id.context("chat id")?;
        let mut connection = pool.write().await?;
        connection
            .with_transaction(async |txn| {
                Chat::update_status(txn, chat_id, &ChatStatus::Pending).await
            })
            .await?;
        Resync::remove(&mut connection, &resync.group_id).await?;

        let outcome = resync.finish(&mut connection, &pending).await?;
        assert_matches!(outcome, ResyncOutcome::Superseded);
        drop(connection);
        assert_eq!(
            load_chat(&pool, chat_id).await?.status(),
            &ChatStatus::Pending
        );

        Ok(())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn finish_completes_when_the_parked_join_fails_to_confirm() -> anyhow::Result<()> {
        let (pool, resync, mut pending) = setup_pending_commit(ResyncReason::Onboarding).await?;
        let chat_id = resync.chat_id.context("chat id")?;
        pending.members_before = Some(HashSet::new());
        let mut connection = pool.write().await?;
        connection
            .with_transaction(async |txn| {
                Chat::update_status(txn, chat_id, &ChatStatus::Pending).await
            })
            .await?;
        // The group is no connection group, so the join can't confirm anything.
        Resync::record_acceptance(
            &mut connection,
            &resync.group_id,
            &EncryptedFriendshipPackage::random(),
        )
        .await?;

        let outcome = resync.finish(&mut connection, &pending).await?;
        assert_matches!(outcome, ResyncOutcome::Synced(id) if id == chat_id);
        assert_eq!(
            Resync::status(&mut connection, &resync.group_id).await?,
            None
        );
        let added = ChatMessage::load_multiple(&mut connection, chat_id, 100)
            .await?
            .into_iter()
            .filter(|message| {
                matches!(
                    message.message(),
                    Message::Event(EventMessage::System(SystemMessage::Add(None, _)))
                )
            })
            .count();
        assert!(added > 0);
        drop(connection);
        assert_eq!(
            load_chat(&pool, chat_id).await?.status(),
            &ChatStatus::Active
        );

        Ok(())
    }
}
