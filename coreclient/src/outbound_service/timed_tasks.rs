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

const KEY_PACKAGES: usize = 100;

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

            match self.handle_task(task_kind).await {
                Ok(_) => {
                    // Task was successful, schedule next run
                    let now = Utc::now();
                    let due_at = now + task_kind.default_interval();
                    TimedTaskQueue::set_due_date(&self.pool, task_kind, due_at).await?;
                }
                Err(error) => {
                    error!(%error, "Failed to execute timed task");
                    continue;
                }
            };
        }
    }

    async fn handle_task(&self, task_kind: TaskKind) -> Result<(), anyhow::Error> {
        debug!(?task_kind, "handling task");

        match task_kind {
            TaskKind::KeyPackageUpload => self.upload_key_packages().await?,
        }
        Ok(())
    }

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

    pub(super) async fn mark_key_packages_as_live(
        txn: &mut SqliteTransaction<'_>,
        key_package_refs: &[KeyPackageRef],
    ) -> anyhow::Result<()> {
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

        // Mark the remaining ones as 'stale'.
        sqlx::query!(
            "UPDATE key_package_refs
            SET is_live = 0
            WHERE is_live = 1"
        )
        .execute(txn.as_mut())
        .await?;

        // Add the newly uploaded ones as 'live'.
        let mut qb =
            QueryBuilder::new("INSERT INTO key_package_refs (key_package_ref, is_live) VALUES");

        let mut vals = qb.separated(", ");
        for r in key_package_refs {
            vals.push("(").push_bind(r.as_slice()).push(", 1)");
        }

        qb.build().execute(txn.as_mut()).await?;

        Ok(())
    }
}
