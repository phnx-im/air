// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use chrono::Utc;
use openmls::prelude::OpenMlsProvider;
use openmls_rust_crypto::OpenMlsRustCrypto;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error};
use uuid::Uuid;

use crate::{
    outbound_service::timed_tasks_queue::{TaskKind, TimedTaskQueue},
    utils::connection_ext::StoreExt,
};

use super::OutboundServiceContext;

pub const KEY_PACKAGES: usize = 100;

impl OutboundServiceContext {
    pub(super) async fn execute_timed_tasks(
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

            let Some(task_kind) = TimedTaskQueue::dequeue(&self.pool, task_id, now).await? else {
                return Ok(());
            };
            debug!(?task_kind, "dequeued task");

            let res = self.handle_task(task_kind).await;

            let interval = match res {
                Ok(_) => {
                    // Task was successful, schedule next run
                    task_kind.default_interval()
                }
                Err(error) => {
                    error!(%error, "Failed to execute timed task");
                    task_kind.default_retry_interval()
                }
            };

            // Schedule next run
            let now = Utc::now();
            let due_at = now + interval;
            TimedTaskQueue::set_due_date(&self.pool, task_kind, due_at).await?;
        }
    }

    async fn handle_task(&self, task_kind: TaskKind) -> Result<(), anyhow::Error> {
        debug!(?task_kind, "handling task");

        match task_kind {
            TaskKind::KeyPackageUpload => self.upload_key_packages().await?,
        }
        Ok(())
    }

    /// This function does the following:
    /// 1. Generate a number of new key packages
    /// 2. Upload them to the QS (and clean up on failure)
    /// 3. Delete key packages that are marked stale
    /// 4. Mark key packages stale that were previously marked live
    /// 5. Marks the uploaded key packages as live in the database
    async fn upload_key_packages(&self) -> anyhow::Result<()> {
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

        debug!("Uploading key packages");
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

        // If the upload was successful, we mark the uploaded ones as live and
        // mark the others as stale.
        self.with_transaction(async |txn| {
            persistence::mark_key_packages_as_live(txn, &key_package_refs).await
        })
        .await?;

        Ok(())
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
