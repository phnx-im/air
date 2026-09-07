// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::identifiers::USERNAME_REFRESH_THRESHOLD;
use airprotos::{
    auth_service::v1::OperationType,
    client::{app_data::GroupAppData, group::GroupData},
};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use crate::{
    Chat, ChatAttributes, ChatId,
    chats::{GroupDataExt, GroupDataProfilePart},
    groups::Group,
    job::{
        JobError,
        chat_operation::{ChatOperation, DerivationEpoch},
        operation::{Operation, OperationData, OperationId, OperationKind},
        pending_chat_operation::PendingChatOperation,
    },
    usernames::UsernameRecord,
};

use super::{OutboundServiceContext, error::OutboundServiceError};

/// Number of key packages to upload (excluding the last resort key package)
#[cfg(not(feature = "test_utils"))]
pub const KEY_PACKAGES: usize = 100;

#[cfg(feature = "test_utils")]
pub const KEY_PACKAGES: usize = 10; // to go faster

/// Number of APQ key packages to upload (excluding the last resort key package)
///
/// Currently only a last resort key package is uploaded.
pub const APQ_KEY_PACKAGES: usize = 0;

/// Interval at which the self-update in a group is executed.
const SELF_UPDATE_INTERVAL: Duration = Duration::days(1);

/// Interval at which the joint APQ self-update is executed.
///
/// This is always greater than [`SELF_UPDATE_INTERVAL`].
const PQ_SELF_UPDATE_INTERVAL: Duration = Duration::days(7);

/// A task to be executed at some point in the future
#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct TimedTask {
    pub(crate) kind: TimedTaskKind,
}

impl TimedTask {
    pub(crate) fn new(kind: TimedTaskKind) -> Self {
        Self { kind }
    }
}

impl OperationData for TimedTask {
    fn kind() -> OperationKind {
        OperationKind::TimedTask
    }

    fn generate_id(&self) -> OperationId {
        let mut id = Vec::new();
        id.extend_from_slice(b"timed_task");
        match self.kind {
            TimedTaskKind::KeyPackageUpload => id.push(0),
            TimedTaskKind::UsernameRefresh => id.push(1),
            TimedTaskKind::SelfUpdate => id.push(2),
            TimedTaskKind::TokenReplenishment { operation_type } => {
                id.push(3);
                id.extend(i32::from(operation_type).to_le_bytes());
            }
        }
        OperationId(id)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum TimedTaskKind {
    KeyPackageUpload,
    // reserved 4 = removed ApqKeyPackageUpload
    #[serde(alias = "HandleRefresh")]
    UsernameRefresh,
    SelfUpdate,
    TokenReplenishment {
        #[serde(with = "operation_type_serde")]
        operation_type: OperationType,
    },
}

impl TimedTaskKind {
    pub(super) fn default_retry_interval(&self) -> Duration {
        match self {
            TimedTaskKind::KeyPackageUpload => Duration::minutes(5),
            TimedTaskKind::UsernameRefresh => Duration::minutes(5),
            TimedTaskKind::SelfUpdate => Duration::minutes(5),
            TimedTaskKind::TokenReplenishment { operation_type } => match operation_type {
                OperationType::Unspecified => Duration::MAX,
                OperationType::AddUsername => Duration::minutes(5),
                OperationType::GetInviteCode => Duration::minutes(5),
            },
        }
    }
}

mod operation_type_serde {
    use serde::{Deserialize, Deserializer, Serializer, de};

    use airprotos::auth_service::v1::OperationType;

    pub fn serialize<S>(operation_type: &OperationType, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_i32(i32::from(*operation_type))
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<OperationType, D::Error>
    where
        D: Deserializer<'de>,
    {
        let idx = i32::deserialize(deserializer)?;
        OperationType::try_from(idx)
            .map_err(|_| de::Error::custom(format!("invalid operation type: {idx}")))
    }
}

#[cfg(feature = "test_utils")]
mod test_utils {
    use chrono::DateTime;

    use crate::outbound_service::OutboundService;

    use super::*;

    impl OutboundService {
        pub async fn schedule_key_package_upload(&self, due_at: DateTime<Utc>) -> sqlx::Result<()> {
            TimedTask::new(TimedTaskKind::KeyPackageUpload)
                .into_operation()
                .schedule_at(due_at)
                .enqueue(self.context.db.write().await?)
                .await
        }

        pub async fn schedule_self_update(&self, due_at: DateTime<Utc>) -> sqlx::Result<()> {
            TimedTask::new(TimedTaskKind::SelfUpdate)
                .into_operation()
                .schedule_at(due_at)
                .enqueue(self.context.db.write().await?)
                .await
        }
    }
}

/// Context for timed tasks
///
/// Recreated for each loop iteration.
struct TimedTaskContext {
    loaded_credentials: bool,
}

impl OutboundServiceContext {
    pub(super) async fn execute_timed_tasks(
        &self,
        run_token: &CancellationToken,
    ) -> anyhow::Result<()> {
        self.ensure_timed_tasks_exist().await?;

        let mut timed_task_context = TimedTaskContext {
            loaded_credentials: false,
        };

        // Used to identify locked receipts by this task
        let task_id = Uuid::new_v4();
        loop {
            if run_token.is_cancelled() {
                return Ok(()); // the task is being stopped
            }

            let now = Utc::now();

            let Some(mut op) = self
                .db
                .with_write_transaction(async |txn| {
                    Operation::<TimedTask>::dequeue(txn, task_id, now).await
                })
                .await?
            else {
                return Ok(());
            };
            let task_kind = op.data.kind;
            debug!(?task_kind, "dequeued task");

            let res =
                Box::pin(self.handle_task(run_token, task_kind, &mut timed_task_context)).await;

            let interval = match res {
                Ok(interval) => interval,
                Err(error) => {
                    error!(%error, "Failed to execute timed task");
                    task_kind.default_retry_interval()
                }
            };

            // Schedule next run
            op.reschedule(self.db.write().await?, Utc::now() + interval)
                .await?;
        }
    }

    async fn ensure_timed_tasks_exist(&self) -> Result<(), anyhow::Error> {
        TimedTask::new(TimedTaskKind::KeyPackageUpload)
            .into_operation()
            .enqueue_if_not_exists(self.db.write().await?)
            .await?;
        // TODO delete old apq key packages upload task
        TimedTask::new(TimedTaskKind::UsernameRefresh)
            .into_operation()
            .enqueue_if_not_exists(self.db.write().await?)
            .await?;
        TimedTask::new(TimedTaskKind::SelfUpdate)
            .into_operation()
            .enqueue_if_not_exists(self.db.write().await?)
            .await?;
        for operation_type in OperationType::all() {
            TimedTask::new(TimedTaskKind::TokenReplenishment { operation_type })
                .into_operation()
                .enqueue_if_not_exists(self.db.write().await?)
                .await?;
        }
        Ok(())
    }

    /// On success, returns the next due time for the task.
    async fn handle_task(
        &self,
        run_token: &CancellationToken,
        task_kind: TimedTaskKind,
        context: &mut TimedTaskContext,
    ) -> anyhow::Result<Duration> {
        debug!(?task_kind, "handling task");

        match task_kind {
            TimedTaskKind::KeyPackageUpload => Box::pin(self.upload_key_packages()).await,
            TimedTaskKind::UsernameRefresh => self.refresh_usernames().await,
            TimedTaskKind::SelfUpdate => self.self_update(run_token).await,
            TimedTaskKind::TokenReplenishment { operation_type } => {
                self.replenish_tokens(operation_type, &mut context.loaded_credentials)
                    .await
            }
        }
    }

    /// Refresh usernames whose `refreshed_at` is older than `USERNAME_REFRESH_THRESHOLD`.
    ///
    /// This ensures usernames are refreshed on the server well before they expire (server sets
    /// a `USERNAME_VALIDITY_PERIOD` window from creation/refresh time).
    async fn refresh_usernames(&self) -> anyhow::Result<Duration> {
        use crate::privacy_pass;

        let now = Utc::now();
        let threshold = now - USERNAME_REFRESH_THRESHOLD;
        let usernames =
            UsernameRecord::load_needing_refresh(self.db.read().await?, threshold).await?;

        if !usernames.is_empty() {
            let api_client = self.api_clients.default_client()?;
            for username_record in usernames {
                let token = match privacy_pass::consume_token(
                    self.db.write().await?,
                    OperationType::AddUsername,
                )
                .await
                {
                    Ok(Some(t)) => t,
                    Ok(None) => {
                        info!("skipping username refresh: no tokens available");
                        break;
                    }
                    Err(e) => {
                        error!(%e, "failed to consume token for username refresh");
                        break;
                    }
                };
                info!("refreshing username");
                let result = api_client
                    .as_refresh_username(username_record.hash, &username_record.signing_key, token)
                    .await;

                if let Err(e) = &result {
                    if e.is_unknown_token_key_id() {
                        warn!("unknown token key ID, purging stale tokens");
                        privacy_pass::purge_and_replenish(
                            &self.db,
                            &api_client,
                            self.user_id().clone(),
                            OperationType::AddUsername,
                            self.signing_key(),
                        )
                        .await?;
                        // Don't consume and retry immediately — that would
                        // let the server correlate issuance with redemption
                        // by timing. Break and let the next task iteration
                        // retry with decorrelated tokens.
                        break;
                    }
                    result?;
                }

                UsernameRecord::update_refreshed_at(
                    self.db.write().await?,
                    &username_record.hash,
                    now,
                )
                .await?;
            }
        }

        Ok(Duration::weeks(1))
    }

    /// Ensures the client has Privacy Pass tokens available for all
    /// operations. Fetches VOPRF public keys from the server on every run.
    ///
    /// Returns a short interval (5 min) while something still has to converge,
    /// and a long interval (6 h) once there is nothing left to fetch.
    async fn replenish_tokens(
        &self,
        operation_type: OperationType,
        loaded_credentials: &mut bool,
    ) -> anyhow::Result<Duration> {
        use crate::privacy_pass::{self, ReplenishOutcome};

        let api_client = self.api_clients.default_client()?;

        // Refresh the key set before looking at the cache depth: a client whose
        // cache never runs low would otherwise sleep through the AS rotation
        // overlap window and end up holding tokens no key can redeem.
        if !*loaded_credentials {
            let credentials_response = api_client.as_as_credentials().await?;
            self.db
                .with_write_transaction(async move |txn| {
                    privacy_pass::store_batched_token_keys(
                        txn,
                        &credentials_response.batched_token_keys,
                    )
                    .await
                })
                .await?;
            *loaded_credentials = true;
        }

        let outcome = privacy_pass::replenish(
            &self.db,
            &api_client,
            self.user_id().clone(),
            self.signing_key(),
            operation_type,
        )
        .await?;

        Ok(match outcome {
            ReplenishOutcome::Settled => Duration::hours(6),
            ReplenishOutcome::RetrySoon => Duration::minutes(5),
        })
    }

    async fn self_update(&self, run_token: &CancellationToken) -> anyhow::Result<Duration> {
        const PARTIAL_UPDATE_INTERVAL: Duration = Duration::minutes(5);
        const BATCH_SIZE: usize = 5;

        let now = Utc::now();
        let threshold = now - SELF_UPDATE_INTERVAL;

        let chat_ids = Chat::load_ids_for_self_update(self.db.read().await?, threshold).await?;
        let num_chats = chat_ids.len();

        info!(num_chats, "Running self-updates");

        let mut num_updated = 0;
        let mut num_failed = 0;

        for chat_id in chat_ids {
            if run_token.is_cancelled() {
                debug!("Stopping self-update task due to cancellation");
                return Ok(Duration::zero()); // Continue as soon as possible
            }
            if num_updated >= BATCH_SIZE {
                info!(
                    num_updated,
                    "Self-update successful for a partial batch of chats"
                );
                return Ok(PARTIAL_UPDATE_INTERVAL); // Continue after a partial batch
            }
            match self.self_update_in_chat(chat_id).await {
                Ok(SelfUpdateOutcome::Updated) => num_updated += 1,
                Ok(SelfUpdateOutcome::Skipped) => (),
                Err(OutboundServiceError::Fatal(error)) => {
                    num_failed += 1;
                    warn!(?chat_id, %error, "Skipping self-update in chat due to unexpected error");
                }
                Err(OutboundServiceError::Recoverable(error)) => return Err(error),
            }
        }

        let skipped = num_chats.wrapping_sub(num_updated).wrapping_sub(num_failed);
        info!(
            num_chats,
            num_updated, skipped, num_failed, "Full self-update finished"
        );
        Ok(SELF_UPDATE_INTERVAL)
    }

    /// Performs the self-update in a single chat.
    ///
    /// Failures that only concern this chat are reported as
    /// [`OutboundServiceError::Fatal`], so that the batch can continue with the
    /// next chat. [`OutboundServiceError::Recoverable`] is reserved for failures
    /// that affect every chat, e.g. an unreachable database or network, where
    /// retrying the whole task is the only useful thing to do.
    async fn self_update_in_chat(
        &self,
        chat_id: ChatId,
    ) -> Result<SelfUpdateOutcome, OutboundServiceError> {
        debug!(?chat_id, "Self-update in chat");

        let (group, is_connection, erase_attributes, pq_due) = {
            let mut read = self
                .db
                .read()
                .await
                .map_err(OutboundServiceError::recoverable)?;
            let mut read_txn = read
                .begin()
                .await
                .map_err(OutboundServiceError::recoverable)?;

            // Loading can fail for a single chat, e.g. when its persisted group state can no
            // longer be decoded. Such a chat must not hold up the rest of the batch.
            let group = match Group::load_with_chat_id(&mut read_txn, chat_id).await {
                Ok(Some(group)) => group,
                Ok(None) => {
                    debug!(
                        ?chat_id,
                        "Skipping self-update in chat because group is not found"
                    );
                    return Ok(SelfUpdateOutcome::Skipped);
                }
                Err(error) => return Err(OutboundServiceError::fatal(error)),
            };

            if group.mls_group().pending_commit().is_some()
                || group
                    .pq()
                    .is_some_and(|pq| pq.mls_group.pending_commit().is_some())
            {
                debug!(
                    ?chat_id,
                    "Skipping self-update in chat because there is a pending commit"
                );
                return Ok(SelfUpdateOutcome::Skipped);
            }

            let now = Utc::now();
            let t_self_update_at: DateTime<Utc> =
                group.self_updated_at.map(From::from).unwrap_or_default();
            let t_due = t_self_update_at + SELF_UPDATE_INTERVAL < now;

            let pq_due = group.pq().is_some_and(|pq| {
                let pq_self_update_at: DateTime<Utc> =
                    pq.self_updated_at.map(From::from).unwrap_or_default();
                pq_self_update_at + PQ_SELF_UPDATE_INTERVAL < now
            });

            if !t_due && !pq_due {
                return Ok(SelfUpdateOutcome::Skipped);
            }

            // If a chat operation is pending, we skip updating this chat
            match PendingChatOperation::is_pending_for_chat(&mut read_txn, chat_id).await {
                Ok(true) => return Ok(SelfUpdateOutcome::Skipped),
                Ok(false) => (),
                Err(error) => return Err(OutboundServiceError::fatal(error)),
            }

            let chat = match Chat::load(&mut read_txn, &chat_id).await {
                Ok(Some(chat)) => chat,
                Ok(None) => {
                    debug!(
                        ?chat_id,
                        "Skipping self-update in chat because chat is not found"
                    );
                    return Ok(SelfUpdateOutcome::Skipped);
                }
                Err(error) => return Err(OutboundServiceError::fatal(error)),
            };

            // For connection chats, that support empty connection group titles, we can erase the data.
            let is_connection = chat.is_connection();
            let erase_attributes = if is_connection {
                group.members_app_data().all(|app_data| {
                    app_data
                        .is_some_and(|app_data| app_data.features.empty_connection_group_attributes)
                })
            } else {
                false
            };

            (group, is_connection, erase_attributes, pq_due)
        };

        let migration_attrs = legacy_group_data_migration(&group, is_connection, erase_attributes);

        // The periodic self-update of the emulation group, i.e. the self group,
        // doubles as the rotation of its derivation epoch. openmls rejects the
        // marker on any other group.
        let derivation_epoch = if group.mls_group().is_emulation_group() {
            DerivationEpoch::Rotate
        } else {
            // A self group without a registered derivation epoch loads as a
            // non-emulation group and cannot register one. An unmarked
            // self-update would only bounce off the DS, so skip it and leave a
            // diagnosable trace.
            if GroupAppData::is_self_group_context(group.mls_group().extensions()) {
                error!(
                    %chat_id,
                    "self group has no derivation epoch and cannot register one, \
                     skipping its self-update"
                );
                return Ok(SelfUpdateOutcome::Skipped);
            }
            DerivationEpoch::Keep
        };

        let job = if migration_attrs.is_some() {
            // Migration takes precedence over PQ self-update (PQ interval is long, so this is
            // fine).
            info!(%chat_id, "Migrating legacy group data");
            ChatOperation::update(chat_id, migration_attrs, derivation_epoch)
        } else if pq_due {
            // Both T and PQ are due and no migration is needed, so the joint APQ update covers
            // both.
            info!(%chat_id, "Performing joint APQ self-update");
            ChatOperation::apq_update(chat_id, derivation_epoch)
        } else {
            // Pure T-only update
            ChatOperation::update(chat_id, None, derivation_epoch)
        };
        match self.execute_job(job).await {
            Ok(_messages) => Ok(SelfUpdateOutcome::Updated),
            // A network error is likely something transient that would affect
            // all chats, so we retry the whole task with backoff.
            Err(error @ JobError::NetworkError) => Err(OutboundServiceError::recoverable(error)),
            // The operation is no longer applicable to this chat, so we skip
            // it.
            Err(JobError::NotFound | JobError::Blocked) => Ok(SelfUpdateOutcome::Skipped),
            Err(error @ (JobError::Domain(_) | JobError::Fatal(_))) => {
                Err(OutboundServiceError::fatal(error))
            }
        }
    }
}

/// Result of a self-update attempt in a single chat.
enum SelfUpdateOutcome {
    Updated,
    /// The update does not apply to this chat right now.
    Skipped,
}

/// Migrates the group data from the legacy format to the new format.
///
/// The legacy format is the format where title and picture were stored in the group data verbatim.
///
/// If this is a connection chat and it supports empty connection group titles, the data is erased.
fn legacy_group_data_migration(
    group: &Group,
    is_connection: bool,
    erase_attributes: bool,
) -> Option<ChatAttributes> {
    if is_connection && !erase_attributes {
        // No migration is done for connection chats that don't need to erase data.
        return None;
    }

    let group_data_bytes = group.group_data()?;
    let group_data = GroupData::decode(&group_data_bytes).ok()?;

    if erase_attributes {
        // Erase the group data if it is not empty
        return (!group_data.is_empty()).then(ChatAttributes::empty);
    }

    let has_encrypted_title = group_data.encrypted_title.is_some();
    let (title, profile) = group_data.into_parts(group.identity_link_wrapper_key());

    let Some(title) = title else {
        return None; // Ignore groups without title
    };

    let legacy_picture = match profile {
        Some(GroupDataProfilePart::LegacyPicture(picture)) => Some(picture),
        _ if has_encrypted_title => return None, // Already migrated
        _ => None,
    };
    Some(ChatAttributes::new(title, legacy_picture))
}
