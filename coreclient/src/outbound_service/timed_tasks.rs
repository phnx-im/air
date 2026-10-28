// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use airapiclient::as_api::UsernameRefresh;
use aircommon::{
    identifiers::{USERNAME_REFRESH_THRESHOLD, USERNAME_VALIDITY_PERIOD},
    mls_group_config::VC_DERIVATION_EPOCH_RETENTION_WINDOW,
};
use airprotos::{auth_service::v1::OperationType, client::app_data::GroupAppData};
use chrono::{DateTime, Duration, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use crate::{
    Chat, ChatId,
    chats::request_expiry,
    db::access::WriteConnection,
    groups::{Group, vc_epoch_retention::sweep_vc_derivation_epochs},
    job::{
        JobError,
        chat_operation::{ChatOperation, DerivationEpoch},
        operation::{Operation, OperationData, OperationId, OperationKind},
        pending_chat_operation::PendingChatOperation,
        recoverable::Recoverable,
    },
    privacy_pass::{self, ConsumedToken},
    usernames::{self, UsernameRecord},
};

use super::{OutboundServiceContext, error::OutboundServiceError, resync::Resync};

/// Scheduled time of a task that never runs again.
pub(crate) const PARKED_AT: DateTime<Utc> = DateTime::from_naive_utc_and_offset(
    NaiveDate::from_ymd_opt(9999, 1, 1)
        .expect("valid date")
        .and_hms_opt(0, 0, 0)
        .expect("valid time"),
    Utc,
);

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

/// Interval at which expired connection requests are closed. The cards count
/// down to the minute.
const CONNECTION_REQUEST_EXPIRY_INTERVAL: Duration = Duration::minutes(1);

/// A task to be executed at some point in the future
#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct TimedTask {
    pub(crate) kind: TimedTaskKind,
}

impl TimedTask {
    pub(crate) fn new(kind: TimedTaskKind) -> Self {
        Self { kind }
    }

    pub(crate) async fn schedule_now(
        connection: impl WriteConnection,
        kind: TimedTaskKind,
    ) -> sqlx::Result<()> {
        Self::new(kind).into_operation().enqueue(connection).await
    }
}

impl OperationData for TimedTask {
    fn kind() -> OperationKind {
        OperationKind::TimedTask
    }

    fn generate_id(&self) -> OperationId {
        let mut id = Vec::new();
        id.extend_from_slice(b"timed_task");
        match &self.kind {
            TimedTaskKind::KeyPackageUpload => id.push(0),
            TimedTaskKind::UsernameRefresh => id.push(1),
            TimedTaskKind::SelfUpdate => id.push(2),
            TimedTaskKind::TokenReplenishment { operation_type } => {
                id.push(3);
                id.extend(i32::from(*operation_type).to_le_bytes());
            }
            TimedTaskKind::ConnectionRequestExpiry => id.push(6),
        }
        OperationId(id)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
    // reserved 5 = removed SignedConnectionPackageUpload
    /// Closes the connection requests whose deadline passed.
    ConnectionRequestExpiry,
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
                OperationType::ConnectUsername => Duration::minutes(5),
            },
            TimedTaskKind::ConnectionRequestExpiry => CONNECTION_REQUEST_EXPIRY_INTERVAL,
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

        pub async fn schedule_connection_request_expiry(
            &self,
            due_at: DateTime<Utc>,
        ) -> sqlx::Result<()> {
            TimedTask::new(TimedTaskKind::ConnectionRequestExpiry)
                .into_operation()
                .schedule_at(due_at)
                .enqueue(self.context.db.write().await?)
                .await
        }

        pub async fn schedule_username_refresh(&self, due_at: DateTime<Utc>) -> sqlx::Result<()> {
            TimedTask::new(TimedTaskKind::UsernameRefresh)
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
    ) -> Result<(), OutboundServiceError> {
        self.ensure_timed_tasks_exist()
            .await
            .map_err(OutboundServiceError::fatal)?;

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
            debug!(kind = ?op.data.kind, "dequeued task");

            let res = Box::pin(self.handle_task(run_token, &mut op, &mut timed_task_context)).await;

            let interval = match res {
                Ok(interval) => interval,
                // Leave the task due so it is the first one retried in the next run
                Err(
                    error @ (OutboundServiceError::NetworkError
                    | OutboundServiceError::RateLimited { .. }),
                ) => return Err(error),
                Err(error) => {
                    error!(%error, "Failed to execute timed task");
                    op.data.kind.default_retry_interval()
                }
            };
            let schedule_at = Utc::now().checked_add_signed(interval).unwrap_or(PARKED_AT);
            op.reschedule(self.db.write().await?, schedule_at).await?;
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
        TimedTask::new(TimedTaskKind::ConnectionRequestExpiry)
            .into_operation()
            .enqueue_if_not_exists(self.db.write().await?)
            .await?;
        Ok(())
    }

    /// On success, returns the next due time for the task.
    async fn handle_task(
        &self,
        run_token: &CancellationToken,
        op: &mut Operation<TimedTask>,
        context: &mut TimedTaskContext,
    ) -> Result<Duration, OutboundServiceError> {
        debug!(kind = ?op.data.kind, "handling task");

        match &op.data.kind {
            TimedTaskKind::KeyPackageUpload => Box::pin(self.upload_key_packages()).await,
            TimedTaskKind::UsernameRefresh => self
                .refresh_usernames()
                .await
                .map_err(OutboundServiceError::fatal),
            TimedTaskKind::SelfUpdate => self.self_update(run_token).await,
            TimedTaskKind::TokenReplenishment { operation_type } => self
                .replenish_tokens(*operation_type, &mut context.loaded_credentials)
                .await
                .map_err(OutboundServiceError::fatal),
            TimedTaskKind::ConnectionRequestExpiry => {
                self.expire_connection_requests()
                    .await
                    .map_err(OutboundServiceError::fatal)?;
                Ok(CONNECTION_REQUEST_EXPIRY_INTERVAL)
            }
        }
    }

    /// Closes the connection requests whose deadline passed. The deletes of
    /// their groups on the DS go out with the pending chat operations of the
    /// next run.
    async fn expire_connection_requests(&self) -> anyhow::Result<()> {
        let now = Utc::now();
        let is_due = self
            .db
            .with_read_transaction(async |txn| request_expiry::is_due(txn, now).await)
            .await?;
        if !is_due {
            return Ok(());
        }
        self.db
            .with_write_transaction(async |txn| {
                request_expiry::expire_due(txn, &self.key_store.signing_key, now).await
            })
            .await
    }

    /// Refresh usernames whose `refreshed_at` is older than `USERNAME_REFRESH_THRESHOLD`.
    ///
    /// This ensures usernames are refreshed on the server well before they expire (server sets
    /// a `USERNAME_VALIDITY_PERIOD` window from creation/refresh time).
    async fn refresh_usernames(&self) -> anyhow::Result<Duration> {
        let now = Utc::now();
        let threshold = now - USERNAME_REFRESH_THRESHOLD;
        let usernames =
            UsernameRecord::load_needing_refresh(self.db.read().await?, threshold).await?;

        if !usernames.is_empty() {
            let api_client = self.api_clients.default_client()?;
            for username_record in usernames {
                let consumed = match privacy_pass::consume_token(
                    self.db.write().await?,
                    OperationType::AddUsername,
                )
                .await
                {
                    Ok(Some(consumed)) => consumed,
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
                    .as_refresh_username(
                        username_record.hash,
                        &username_record.signing_key,
                        consumed.token.clone(),
                    )
                    .await;

                let refresh = match result {
                    Ok(refresh) => refresh,
                    Err(error) if error.is_unknown_token_key_id() => {
                        warn!("unknown token key ID, purging stale tokens");
                        privacy_pass::purge_and_replenish(
                            &self.db,
                            &api_client,
                            self.user_id().clone(),
                            OperationType::AddUsername,
                            self.signing_key(),
                        )
                        .await?;
                        // Don't consume and retry immediately. That would
                        // let the server correlate issuance with redemption
                        // by timing. Break and let the next task iteration
                        // retry with decorrelated tokens.
                        break;
                    }
                    Err(error) if error.is_not_found() => {
                        // When a username is not found on the server we should
                        // keep the token.
                        warn!(%error, "username not found on the server, skipping its refresh");
                        privacy_pass::restore_unredeemed_token(
                            &self.db,
                            OperationType::AddUsername,
                            consumed,
                        )
                        .await;
                        continue;
                    }
                    Err(error) => return Err(error.into()),
                };

                self.store_username_refresh(&username_record, consumed, refresh, now)
                    .await?;
            }
        }

        Ok(Duration::days(1))
    }

    async fn store_username_refresh(
        &self,
        username_record: &UsernameRecord,
        consumed: ConsumedToken,
        refresh: UsernameRefresh,
        now: DateTime<Utc>,
    ) -> anyhow::Result<()> {
        match refresh {
            UsernameRefresh::Refreshed => {
                // The redemption and the refresh should be atomic. We tell the
                // siblings about the refresh. Otherwise each sibling tries to
                // also refresh that username.
                self.db
                    .with_write_transaction(async |txn| -> anyhow::Result<()> {
                        if let Some(position) = &consumed.position {
                            privacy_pass::mark_redeemed(&mut *txn, position).await?;
                        }
                        UsernameRecord::update_refreshed_at(&mut *txn, &username_record.hash, now)
                            .await?;
                        usernames::sync::park_claimed(txn, &username_record.username).await?;
                        Ok(())
                    })
                    .await?;
            }
            UsernameRefresh::Skipped { expires_at } => {
                // The commit of a sibling's refresh has not reached us yet.
                info!("username refresh skipped, a sibling refreshed it");
                privacy_pass::restore_unredeemed_token(
                    &self.db,
                    OperationType::AddUsername,
                    consumed,
                )
                .await;
                let refreshed_at = *expires_at - USERNAME_VALIDITY_PERIOD;
                self.db
                    .with_write_transaction(async |txn| {
                        UsernameRecord::advance_refreshed_at(
                            txn,
                            &username_record.username,
                            refreshed_at,
                        )
                        .await
                    })
                    .await?;
            }
        }
        Ok(())
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
        use crate::privacy_pass::ReplenishOutcome;

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

    async fn self_update(
        &self,
        run_token: &CancellationToken,
    ) -> Result<Duration, OutboundServiceError> {
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
                Err(
                    OutboundServiceError::Fatal(error)
                    | OutboundServiceError::Recoverable(Recoverable { error, .. }),
                ) => {
                    num_failed += 1;
                    warn!(?chat_id, %error, "Skipping self-update in chat due to unexpected error");
                }
                Err(error) => return Err(error),
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
    /// [`OutboundServiceError::Fatal`], so that the batch can continue with
    /// the next chat. All other errors are reserved for failures that affect
    /// every chat, e.g. an unreachable database or network, where retrying the
    /// whole task is the only useful thing to do.
    async fn self_update_in_chat(
        &self,
        chat_id: ChatId,
    ) -> Result<SelfUpdateOutcome, OutboundServiceError> {
        debug!(?chat_id, "Self-update in chat");

        let (mut group, pq_due) = {
            let mut read = self.db.read().await?;
            let mut read_txn = read.begin().await?;

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

            // A commit on a desynced group would only be rejected.
            match Resync::status_for_chat(&mut read_txn, &chat_id).await {
                Ok(Some(status)) => {
                    debug!(
                        ?chat_id,
                        ?status,
                        "Skipping self-update in chat due to resync"
                    );
                    return Ok(SelfUpdateOutcome::Skipped);
                }
                Ok(None) => (),
                Err(error) => return Err(OutboundServiceError::fatal(error)),
            }

            (group, pq_due)
        };

        // The periodic self-update of the emulation group, i.e. the self group,
        // doubles as the rotation of its derivation epoch. openmls rejects the
        // marker on any other group.
        let derivation_epoch = if group.mls_group().is_emulation_group() {
            // The rotation below leaves the current derivation epoch behind, so
            // this is also where the epochs that aged out of the retention
            // window are released. A failure here only leaves state around, so
            // it must not hold up the self-update.
            if let Err(error) = self.sweep_derivation_epochs(&mut group).await {
                warn!(%chat_id, %error, "Failed to sweep derivation epochs of the emulation group");
            }
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

        let job = if pq_due {
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
            // The operation is no longer applicable to this chat, so we skip it.
            Err(JobError::NotFound | JobError::GroupClosed | JobError::Blocked) => {
                Ok(SelfUpdateOutcome::Skipped)
            }
            Err(error) => Err(error.into()),
        }
    }

    /// Prunes the derivation epochs of the emulation group that aged out of
    /// [`VC_DERIVATION_EPOCH_RETENTION_WINDOW`].
    async fn sweep_derivation_epochs(&self, group: &mut Group) -> anyhow::Result<()> {
        let mut write = self.db.write().await?;
        let mut txn = write.begin().await?;
        sweep_vc_derivation_epochs(&mut txn, group, VC_DERIVATION_EPOCH_RETENTION_WINDOW)?;
        txn.commit().await?;
        Ok(())
    }
}

/// Result of a self-update attempt in a single chat.
enum SelfUpdateOutcome {
    Updated,
    /// The update does not apply to this chat right now.
    Skipped,
}
