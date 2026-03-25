// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::identifiers::USER_HANDLE_REFRESH_THRESHOLD;
use chrono::{DateTime, Duration, Utc};
use openmls::prelude::OpenMlsProvider;
use openmls_rust_crypto::OpenMlsRustCrypto;
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info};
use uuid::Uuid;

use crate::{
    Chat, ChatId,
    groups::Group,
    job::{
        JobError,
        chat_operation::ChatOperation,
        operation::{Operation, OperationData, OperationId, OperationKind},
        pending_chat_operation::PendingChatOperation,
    },
    user_handles::UserHandleRecord,
    utils::connection_ext::StoreExt,
};

use super::OutboundServiceContext;

pub const KEY_PACKAGES: usize = 100;

/// Interval at which the self-update in a group is executed
const SELF_UPDATE_INTERVAL: Duration = Duration::days(1);

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
        id.push(self.kind as u8);
        OperationId(id)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum TimedTaskKind {
    KeyPackageUpload,
    HandleRefresh,
    SelfUpdate,
}

impl TimedTaskKind {
    pub(super) fn default_retry_interval(&self) -> Duration {
        match self {
            TimedTaskKind::KeyPackageUpload => Duration::minutes(5),
            TimedTaskKind::HandleRefresh => Duration::minutes(5),
            TimedTaskKind::SelfUpdate => Duration::minutes(5),
        }
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
                .enqueue(&self.context.pool)
                .await
        }

        pub async fn schedule_self_update(&self, due_at: DateTime<Utc>) -> sqlx::Result<()> {
            TimedTask::new(TimedTaskKind::SelfUpdate)
                .into_operation()
                .schedule_at(due_at)
                .enqueue(&self.context.pool)
                .await
        }
    }
}

impl OutboundServiceContext {
    pub(super) async fn execute_timed_tasks(
        &self,
        run_token: &CancellationToken,
    ) -> anyhow::Result<()> {
        self.ensure_timed_tasks_exist().await?;

        // Used to identify locked receipts by this task
        let task_id = Uuid::new_v4();
        loop {
            if run_token.is_cancelled() {
                return Ok(()); // the task is being stopped
            }

            let now = Utc::now();

            let Some(mut op) = self
                .with_transaction(async |txn| {
                    Operation::<TimedTask>::dequeue(txn, task_id, now).await
                })
                .await?
            else {
                return Ok(());
            };
            let task_kind = op.data.kind;
            debug!(?task_kind, "dequeued task");

            let res = self.handle_task(run_token, task_kind).await;

            let interval = match res {
                Ok(interval) => interval,
                Err(error) => {
                    error!(%error, "Failed to execute timed task");
                    task_kind.default_retry_interval()
                }
            };

            // Schedule next run
            op.reschedule(&self.pool, Utc::now() + interval).await?;
        }
    }

    async fn ensure_timed_tasks_exist(&self) -> Result<(), anyhow::Error> {
        TimedTask::new(TimedTaskKind::KeyPackageUpload)
            .into_operation()
            .enqueue_if_not_exists(&self.pool)
            .await?;
        TimedTask::new(TimedTaskKind::HandleRefresh)
            .into_operation()
            .enqueue_if_not_exists(&self.pool)
            .await?;
        TimedTask::new(TimedTaskKind::SelfUpdate)
            .into_operation()
            .enqueue_if_not_exists(&self.pool)
            .await?;
        Ok(())
    }

    /// On success, returns the next due time for the task.
    async fn handle_task(
        &self,
        run_token: &CancellationToken,
        task_kind: TimedTaskKind,
    ) -> anyhow::Result<Duration> {
        debug!(?task_kind, "handling task");

        match task_kind {
            TimedTaskKind::KeyPackageUpload => self.upload_key_packages().await,
            TimedTaskKind::HandleRefresh => self.refresh_handles().await,
            TimedTaskKind::SelfUpdate => self.self_update(run_token).await,
        }
    }

    /// Refresh handles whose `refreshed_at` is older than USER_HANDLE_REFRESH_THRESHOLD`.
    ///
    /// This ensures handles are refreshed on the server well before they expire (server sets
    /// a `USER_HANDLE_VALIDITY_PERIOD` window from creation/refresh time).
    async fn refresh_handles(&self) -> anyhow::Result<Duration> {
        let now = Utc::now();
        let threshold = now - USER_HANDLE_REFRESH_THRESHOLD;
        let handles = UserHandleRecord::load_needing_refresh(&self.pool, threshold).await?;

        if !handles.is_empty() {
            let api_client = self.api_clients.default_client()?;
            for handle in handles {
                info!("refreshing handle");
                api_client
                    .as_refresh_handle(handle.hash, &handle.signing_key)
                    .await?;
                UserHandleRecord::update_refreshed_at(&self.pool, &handle.hash, now).await?;
            }
        }

        Ok(Duration::weeks(1))
    }

    /// This function does the following:
    /// 1. Generate a number of new key packages
    /// 2. Upload them to the QS (and clean up on failure)
    /// 3. Delete key packages that are marked stale
    /// 4. Mark key packages stale that were previously marked live
    /// 5. Marks the uploaded key packages as live in the database
    async fn upload_key_packages(&self) -> anyhow::Result<Duration> {
        let key_packages = self
            .with_transaction(async |txn| {
                let mut key_packages = Vec::with_capacity(KEY_PACKAGES);
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
                        .generate_key_package(&mut *txn, &self.qs_client_id, true)?;
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
            let connection = &mut self.pool.acquire().await?;
            for key_package_ref in key_package_refs {
                if let Err(error) = self
                    .key_store
                    .delete_key_package(connection, key_package_ref)
                {
                    error!(%error, "Failed to delete key package after upload failure");
                }
            }

            return Err(error.into());
        }
        info!("Uploaded key packages");

        // If the upload was successful, we mark the uploaded ones as live and
        // mark the others as stale.
        self.with_transaction(async |txn| {
            persistence::mark_key_packages_as_live(txn, &key_package_refs).await
        })
        .await?;

        Ok(Duration::weeks(1))
    }

    async fn self_update(&self, run_token: &CancellationToken) -> anyhow::Result<Duration> {
        const PARTIAL_UPDATE_INTERVAL: Duration = Duration::minutes(5);
        const BATCH_SIZE: usize = 5;

        let now = Utc::now();
        let threshold = now - SELF_UPDATE_INTERVAL;

        let chat_ids = Chat::load_ids_for_self_update(&self.pool, threshold).await?;
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

        let Some(group) =
            Group::load_with_chat_id(self.pool.acquire().await?.as_mut(), chat_id).await?
        else {
            debug!(
                ?chat_id,
                "Skipping self-update in chat because group is not found"
            );
            return Ok(false);
        };

        if group.mls_group().pending_commit().is_some() {
            debug!(
                ?chat_id,
                "Skipping self-update in chat because there is a pending commit"
            );
            return Ok(false);
        }

        if group.mls_group().pending_proposals().next().is_some() {
            debug!(
                ?chat_id,
                "Skipping self-update in chat because there are pending proposals"
            );
            return Ok(false);
        }

        let self_update_at: DateTime<Utc> =
            group.self_updated_at.map(From::from).unwrap_or_default();
        if Utc::now() <= self_update_at + SELF_UPDATE_INTERVAL {
            return Ok(false);
        }

        // If a chat operation is pending, we skip updating this chat
        if PendingChatOperation::is_pending_for_chat(&self.pool, chat_id).await? {
            return Ok(false);
        }

        let job = ChatOperation::update(chat_id, None);
        let res = self.execute_job(job).await;

        match res {
            Ok(_messages) => Ok(true),
            Err(JobError::NotFound | JobError::Blocked) => Ok(false),
            // Fatal or network errors abort the whole batch because we assume that they will
            // persist for the next chat in the batch.
            Err(error @ (JobError::Fatal(_) | JobError::NetworkError | JobError::Domain(_))) => {
                debug!(?chat_id, %error, "Failed to self-update in chat");
                Err(error.into())
            }
        }
    }
}

mod persistence {
    use openmls::prelude::KeyPackageRef;
    use sqlx::{QueryBuilder, SqliteTransaction};

    use crate::groups::openmls_provider::KeyRefWrapper;

    pub(super) async fn mark_key_packages_as_live(
        txn: &mut SqliteTransaction<'_>,
        key_package_refs: &[KeyPackageRef],
    ) -> anyhow::Result<()> {
        // Delete all key packages that are not marked as live
        sqlx::query!(
            "DELETE FROM key_package
            WHERE key_package_ref IN (
              SELECT key_package_ref
              FROM key_package_refs
              WHERE is_live = 0
            )"
        )
        .execute(txn.as_mut())
        .await?;

        // Mark all key packages as stale
        sqlx::query!(
            "UPDATE key_package_refs
            SET is_live = 0
            WHERE is_live = 1"
        )
        .execute(txn.as_mut())
        .await?;

        // Add the newly uploaded ones as 'live'.
        let mut qb =
            QueryBuilder::new("INSERT INTO key_package_refs (key_package_ref, is_live) VALUES ");
        let mut vals = qb.separated(", ");
        for r in key_package_refs {
            let r = KeyRefWrapper(r);
            vals.push("(")
                .push_bind_unseparated(r)
                .push_unseparated(", 1)");
        }
        qb.build().execute(txn.as_mut()).await?;

        // Delete orphaned key packages (usually this is a no-op)
        sqlx::query!(
            "DELETE FROM key_package WHERE key_package_ref NOT IN (
                SELECT key_package_ref
                FROM key_package_refs
            )"
        )
        .execute(txn.as_mut())
        .await?;

        Ok(())
    }

    #[cfg(test)]
    mod test {
        use std::slice;

        use aircommon::{
            codec::PersistenceCodec, credentials::test_utils::create_test_credentials,
            identifiers::UserId,
        };
        use openmls::prelude::{CredentialWithKey, KeyPackage, SignaturePublicKey};
        use openmls_traits::OpenMlsProvider;
        use sqlx::{Row, SqlitePool, query, query_scalar};
        use url::Host;

        use crate::{clients::CIPHERSUITE, groups::openmls_provider::AirOpenMlsProvider};

        use super::*;

        #[tokio::test(flavor = "multi_thread")]
        async fn test_mark_key_packages_as_live() -> anyhow::Result<()> {
            // Note: We don't use `sqlx::test` and instead create manually a pool, because we must
            // run on a multi-threaded flavor of tokio runtime, because `AirOpenMlsProvider` blocks
            // the current thread.
            let pool = SqlitePool::connect("sqlite://:memory:").await?;
            sqlx::migrate!("./migrations").run(&pool).await?;

            let mut connection = pool.acquire().await?;
            let provider = AirOpenMlsProvider::new(&mut connection);

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
                .execute(&pool)
                .await?;
            query("INSERT INTO key_package_refs (key_package_ref, is_live) VALUES (?1, 0)")
                .bind(KeyRefWrapper(&stale_key_package_ref))
                .execute(&pool)
                .await?;

            let mut txn = pool.begin().await?;
            mark_key_packages_as_live(&mut txn, slice::from_ref(&new_key_package_ref)).await?;
            txn.commit().await?;

            let rows = query(
                "SELECT key_package_ref, is_live \
                FROM key_package kp \
                LEFT JOIN key_package_refs kpr USING (key_package_ref)
                ORDER BY is_live ASC",
            )
            .fetch_all(&pool)
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
                .fetch_one(&pool)
                .await?;
            assert_eq!(num_refs, 2);

            Ok(())
        }
    }
}
