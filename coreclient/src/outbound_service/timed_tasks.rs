// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::identifiers::USERNAME_REFRESH_THRESHOLD;
use airprotos::{auth_service::v1::OperationType, client::group::GroupData};
use chrono::{DateTime, Duration, Utc};
use openmls::prelude::OpenMlsProvider;
use openmls_rust_crypto::OpenMlsRustCrypto;
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
        chat_operation::ChatOperation,
        operation::{Operation, OperationData, OperationId, OperationKind},
        pending_chat_operation::PendingChatOperation,
    },
    privacy_pass::RequestTokensError,
    usernames::UsernameRecord,
};

use super::OutboundServiceContext;

/// Number of key packages to upload (excluding the last resort key package)
pub const KEY_PACKAGES: usize = 100;

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
            TimedTaskKind::ApqKeyPackageUpload => id.push(4),
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
    ApqKeyPackageUpload,
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
            TimedTaskKind::ApqKeyPackageUpload => Duration::minutes(5),
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

            let res = self
                .handle_task(run_token, task_kind, &mut timed_task_context)
                .await;

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
        // For now, the APQ feature is not fully released, so we don't enable APQ key package
        // uploads yet.
        #[cfg(any(feature = "test_utils", test))]
        TimedTask::new(TimedTaskKind::ApqKeyPackageUpload)
            .into_operation()
            .enqueue_if_not_exists(self.db.write().await?)
            .await?;
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
            TimedTaskKind::KeyPackageUpload => self.upload_key_packages().await,
            TimedTaskKind::ApqKeyPackageUpload => self.upload_apq_key_packages().await,
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
    /// operations. Fetches VOPRF public keys from the server and requests
    /// tokens if the local store is running low.
    ///
    /// Returns a short interval (5 min) when tokens are still below the
    /// threshold, and a long interval (6 h) when fully stocked.
    async fn replenish_tokens(
        &self,
        operation_type: OperationType,
        loaded_credentials: &mut bool,
    ) -> anyhow::Result<Duration> {
        use crate::privacy_pass;

        let api_client = self.api_clients.default_client()?;

        let Some(replenish_count) =
            privacy_pass::needs_replenishment(self.db.read().await?, operation_type).await?
        else {
            return Ok(Duration::hours(6));
        };

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

        match privacy_pass::request_and_store_tokens(
            &self.db,
            &api_client,
            self.user_id().clone(),
            self.signing_key(),
            operation_type,
            replenish_count,
        )
        .await?
        {
            Ok(count) => {
                if count < usize::from(operation_type.low_tokens_threshold()) {
                    Ok(Duration::minutes(5))
                } else {
                    Ok(Duration::hours(6))
                }
            }
            Err(RequestTokensError::QuotaExceeded {
                retry_after,
                tokens_available,
            }) => {
                warn!(
                    %operation_type,
                    retry_after_secs = retry_after.num_seconds(),
                    tokens_available,
                    "quota exceeded"
                );
                if tokens_available > 0 && retry_after.is_zero() {
                    // Partial quota: some tokens are available right now. Retry immediately with
                    // the reduced count.
                    match privacy_pass::request_and_store_tokens(
                        &self.db,
                        &api_client,
                        self.user_id().clone(),
                        self.signing_key(),
                        operation_type,
                        tokens_available,
                    )
                    .await?
                    {
                        Ok(_) => Ok(Duration::hours(6)),
                        Err(RequestTokensError::QuotaExceeded { retry_after, .. }) => {
                            Ok(retry_after.max(Duration::minutes(5)))
                        }
                    }
                } else {
                    Ok(retry_after.max(Duration::minutes(5)))
                }
            }
        }
    }

    /// This function does the following:
    /// 1. Generate a number of new key packages
    /// 2. Upload them to the QS (and clean up on failure)
    /// 3. Delete key packages that are marked stale
    /// 4. Mark key packages stale that were previously marked live
    /// 5. Marks the uploaded key packages as live in the database
    async fn upload_key_packages(&self) -> anyhow::Result<Duration> {
        let key_packages = self
            .db
            .with_write_transaction(async |txn| {
                let mut key_packages = Vec::with_capacity(KEY_PACKAGES + 1);
                for _ in 0..KEY_PACKAGES {
                    let kp = self.key_store.generate_key_package(
                        &mut *txn,
                        &self.qs_client_id,
                        false,
                    )?;
                    key_packages.push(kp);
                }

                let last_resort_kp =
                    self.key_store
                        .generate_key_package(txn, &self.qs_client_id, true)?;
                key_packages.push(last_resort_kp);

                Ok::<_, anyhow::Error>(key_packages)
            })
            .await?;

        let crypto_provider = OpenMlsRustCrypto::default();
        let key_package_refs = key_packages
            .iter()
            .map(|kp| kp.hash_ref(crypto_provider.crypto()))
            .collect::<Result<Vec<_>, _>>()?;

        info!(n = key_packages.len(), "Uploading key packages");
        if let Err(error) = self
            .api_clients
            .default_client()?
            .qs_publish_key_packages(
                self.qs_client_id,
                key_packages,
                &self.key_store.qs_client_signing_key,
            )
            .await
        {
            error!(%error, "Failed to upload key packages");
            // Clean up previously created key packages
            for key_package_ref in key_package_refs {
                if let Err(error) = self
                    .key_store
                    .delete_key_package(self.db.write().await?, key_package_ref)
                {
                    error!(%error, "Failed to delete key package after upload failure");
                }
            }

            return Err(error.into());
        }
        info!("Uploaded key packages");

        // If the upload was successful, we mark the uploaded ones as live and
        // mark the others as stale.
        self.db
            .with_write_transaction(async |txn| {
                let is_apq = false;
                persistence::mark_key_packages_as_live(txn, &key_package_refs, is_apq).await
            })
            .await?;

        Ok(Duration::weeks(1))
    }

    /// Same as [`Self::upload_key_packages`], but for APQ key packages.
    ///
    /// For now, we only upload one last resort APQ key package.
    async fn upload_apq_key_packages(&self) -> anyhow::Result<Duration> {
        let last_resort_key_package = self
            .db
            .with_write_transaction(async |txn| {
                self.key_store
                    .generate_apq_key_package(&mut *txn, &self.qs_client_id, true)
            })
            .await?;
        let key_packages = vec![last_resort_key_package];

        let crypto_provider = OpenMlsRustCrypto::default();
        let key_package_refs = key_packages
            .iter()
            .map(|kp| {
                let t_ref = kp.t_key_package().hash_ref(crypto_provider.crypto())?;
                let pq_ref = kp.pq_key_package().hash_ref(crypto_provider.crypto())?;
                Ok([t_ref, pq_ref])
            })
            .collect::<anyhow::Result<Vec<_>>>()?;

        info!(n = key_packages.len(), "Uploading key packages");
        if let Err(error) = self
            .api_clients
            .default_client()?
            .qs_publish_apq_key_packages(
                self.qs_client_id,
                key_packages,
                &self.key_store.qs_client_signing_key,
            )
            .await
        {
            error!(%error, "Failed to upload key packages");
            // Clean up previously created key packages
            for key_package_ref in key_package_refs.into_iter().flatten() {
                if let Err(error) = self
                    .key_store
                    .delete_key_package(self.db.write().await?, key_package_ref)
                {
                    error!(%error, "Failed to delete key package after upload failure");
                }
            }

            return Err(error.into());
        }
        info!("Uploaded key packages");

        // If the upload was successful, we mark the uploaded ones as live and
        // mark the others as stale.
        self.db
            .with_write_transaction(async |txn| {
                let is_apq = true;
                persistence::mark_key_packages_as_live(
                    txn,
                    key_package_refs.iter().flatten(),
                    is_apq,
                )
                .await
            })
            .await?;

        Ok(Duration::weeks(1))
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
            if self.self_update_in_chat(chat_id).await? {
                num_updated += 1;
            }
        }

        let skipped = num_chats.wrapping_sub(num_updated);
        info!(num_chats, skipped, "Full self-update successful");
        Ok(SELF_UPDATE_INTERVAL)
    }

    async fn self_update_in_chat(&self, chat_id: ChatId) -> anyhow::Result<bool> {
        debug!(?chat_id, "Self-update in chat");

        let (group, is_connection, erase_attributes, pq_due) = {
            let mut read = self.db.read().await?;
            let mut read_txn = read.begin().await?;

            let Some(group) = Group::load_with_chat_id(&mut read_txn, chat_id).await? else {
                debug!(
                    ?chat_id,
                    "Skipping self-update in chat because group is not found"
                );
                return Ok(false);
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
                return Ok(false);
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
                return Ok(false);
            }

            // If a chat operation is pending, we skip updating this chat
            if PendingChatOperation::is_pending_for_chat(&mut read_txn, chat_id).await? {
                return Ok(false);
            }

            let Some(chat) = Chat::load(&mut read_txn, &chat_id).await? else {
                debug!(
                    ?chat_id,
                    "Skipping self-update in chat because chat is not found"
                );
                return Ok(false);
            };

            // For connection chats, that support empty connection group titles, we can erase the data.
            let is_connection = chat.is_connection();
            let erase_attributes = if is_connection {
                group.members_air_component().all(|component| {
                    component
                        .map(|component| component.features.empty_connection_group_attributes)
                        .unwrap_or(false)
                })
            } else {
                false
            };

            (group, is_connection, erase_attributes, pq_due)
        };

        let migration_attrs = legacy_group_data_migration(&group, is_connection, erase_attributes);
        if migration_attrs.is_some() {
            info!(%chat_id, "Migrating legacy group data");
        }

        let job = if migration_attrs.is_some() {
            // Migration takes precedence over PQ self-update (PQ interval is long, so this is
            // fine).
            info!(%chat_id, "Migrating legacy group data");
            ChatOperation::update(chat_id, migration_attrs)
        } else if pq_due {
            // Both T and PQ are due and no migration is needed, so the joint APQ update covers
            // both.
            info!(%chat_id, "Performing joint APQ self-update");
            ChatOperation::apq_update(chat_id)
        } else {
            // Pure T-only update
            ChatOperation::update(chat_id, None)
        };
        let res = self.execute_job(job).await;

        match res {
            Ok(_messages) => Ok(true),
            // A network error is likely something transient that would affect
            // all chats, so we return the error to retry the task with backoff.
            Err(error @ JobError::NetworkError) => Err(error.into()),
            // The operation is no longer applicable to this chat, so we skip
            // it.
            Err(JobError::NotFound | JobError::Blocked) => Ok(false),
            // Any other failure is specific to this chat, so we log and skip
            // it, but continue with the rest of the batch.
            Err(error) => {
                warn!(?chat_id, %error, "Skipping self-update in chat due to unexpected error");
                Ok(false)
            }
        }
    }
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

mod persistence {
    use openmls::prelude::KeyPackageRef;
    use sqlx::QueryBuilder;

    use crate::{db::access::WriteDbTransaction, groups::openmls_provider::KeyRefWrapper};

    pub(super) async fn mark_key_packages_as_live(
        txn: &mut WriteDbTransaction<'_>,
        key_package_refs: impl IntoIterator<Item = &KeyPackageRef>,
        is_apq: bool,
    ) -> anyhow::Result<()> {
        let refs_table = if is_apq {
            "apq_key_package_refs"
        } else {
            "key_package_refs"
        };
        mark_key_packages_as_live_impl(txn, refs_table, key_package_refs).await
    }

    async fn mark_key_packages_as_live_impl(
        txn: &mut WriteDbTransaction<'_>,
        refs_table: &'static str,
        key_package_refs: impl IntoIterator<Item = &KeyPackageRef>,
    ) -> anyhow::Result<()> {
        // Delete all key packages that are not marked as live
        sqlx::query(&format!(
            "DELETE FROM key_package
            WHERE key_package_ref IN (
              SELECT key_package_ref
              FROM {refs_table}
              WHERE is_live = 0
            )"
        ))
        .execute(txn.as_mut())
        .await?;

        // Mark all key packages as stale
        sqlx::query(&format!(
            "UPDATE {refs_table}
            SET is_live = 0
            WHERE is_live = 1",
        ))
        .execute(txn.as_mut())
        .await?;

        // Add the newly uploaded ones as 'live'.
        let mut qb = QueryBuilder::new(format!(
            "INSERT INTO {refs_table} (key_package_ref, is_live) VALUES "
        ));
        let mut vals = qb.separated(", ");
        for r in key_package_refs {
            let r = KeyRefWrapper(r);
            vals.push("(")
                .push_bind_unseparated(r)
                .push_unseparated(", 1)");
        }
        qb.build().execute(txn.as_mut()).await?;

        // Delete orphaned key packages (usually this is a no-op).
        // Must check both tables so regular and APQ key packages don't clobber each other.
        sqlx::query(
            "DELETE FROM key_package WHERE key_package_ref NOT IN (
                SELECT key_package_ref FROM key_package_refs
                UNION
                SELECT key_package_ref FROM apq_key_package_refs
            )",
        )
        .execute(txn.as_mut())
        .await?;

        Ok(())
    }

    #[cfg(test)]
    mod test {
        use aircommon::{
            codec::PersistenceCodec, credentials::test_utils::create_test_credentials,
            identifiers::UserId,
        };
        use openmls::prelude::{CredentialWithKey, KeyPackage, SignaturePublicKey};
        use openmls_traits::OpenMlsProvider;
        use sqlx::{Row, SqlitePool, query, query_scalar};
        use url::Host;

        use crate::{
            clients::CIPHERSUITE, db::access::DbAccess,
            groups::openmls_provider::AirOpenMlsProvider,
        };

        use super::*;

        #[tokio::test(flavor = "multi_thread")]
        async fn test_mark_key_packages_as_live() -> anyhow::Result<()> {
            // Note: We don't use `sqlx::test` and instead create manually a pool, because we must
            // run on a multi-threaded flavor of tokio runtime, because `AirOpenMlsProvider` blocks
            // the current thread.
            let pool = SqlitePool::connect("sqlite://:memory:").await?;
            sqlx::migrate!("./migrations").run(&pool).await?;

            let pool = DbAccess::for_tests(pool);

            let mut connection = pool.write().await?;
            let provider = AirOpenMlsProvider::new(connection.as_mut());

            let user_id = UserId::random(Host::Domain("example.com".to_string()).into());
            let (_aic_sk, client_sk) = create_test_credentials(user_id);

            let credential_with_key = CredentialWithKey {
                credential: client_sk.credential().try_into().unwrap(),
                signature_key: SignaturePublicKey::from(
                    client_sk.credential().verifying_key().clone(),
                ),
            };

            let key_packages: Vec<KeyPackage> = (0..3)
                .map(|_| {
                    let bundle = KeyPackage::builder()
                        .build(
                            CIPHERSUITE,
                            &provider,
                            &client_sk,
                            credential_with_key.clone(),
                        )
                        .unwrap();
                    bundle.key_package().clone()
                })
                .collect();

            let live_key_package_ref = key_packages[0].hash_ref(provider.crypto())?;
            let stale_key_package_ref = key_packages[1].hash_ref(provider.crypto())?;
            let new_key_package_ref = key_packages[2].hash_ref(provider.crypto())?;

            query("INSERT INTO key_package_refs (key_package_ref, is_live) VALUES (?1, 1)")
                .bind(KeyRefWrapper(&live_key_package_ref))
                .execute(pool.write().await?.as_mut())
                .await?;
            query("INSERT INTO key_package_refs (key_package_ref, is_live) VALUES (?1, 0)")
                .bind(KeyRefWrapper(&stale_key_package_ref))
                .execute(pool.write().await?.as_mut())
                .await?;

            pool.with_write_transaction(async |txn| {
                let is_apq = false;
                mark_key_packages_as_live(txn, [&new_key_package_ref], is_apq).await
            })
            .await?;

            let rows = query(
                "SELECT key_package_ref, is_live \
                FROM key_package kp \
                LEFT JOIN key_package_refs kpr USING (key_package_ref)
                ORDER BY is_live ASC",
            )
            .fetch_all(pool.read().await?.as_mut())
            .await?;

            let key_packages: Vec<(KeyPackageRef, Option<bool>)> = rows
                .into_iter()
                .map(|row| {
                    let bytes: Vec<u8> = row.get(0);
                    let key_package_ref: KeyPackageRef =
                        PersistenceCodec::from_slice(&bytes).unwrap();
                    let is_live: Option<bool> = row.get(1);
                    (key_package_ref, is_live)
                })
                .collect();

            assert_eq!(key_packages.len(), 2); // stale key package is deleted

            let (key_package_ref, is_live) = &key_packages[0];
            assert_eq!(key_package_ref, &live_key_package_ref);
            assert_eq!(is_live, &Some(false));

            let (key_package_ref, is_live) = &key_packages[1];
            assert_eq!(key_package_ref, &new_key_package_ref);
            assert_eq!(is_live, &Some(true));

            let num_refs: i32 = query_scalar("SELECT COUNT(*) FROM key_package_refs")
                .fetch_one(pool.read().await?.as_mut())
                .await?;
            assert_eq!(num_refs, 2);

            Ok(())
        }
    }
}
