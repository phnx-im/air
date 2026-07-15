// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::virtual_client::KeyPackageBatchId;
use anyhow::Context;
use apqmls::messages::ApqKeyPackage;
use chrono::Duration;
use openmls::{
    components::vc_derivation_info::KeyPackageUpload,
    prelude::{KeyPackage, KeyPackageRef},
};
use openmls_rust_crypto::OpenMlsRustCrypto;
use openmls_traits::OpenMlsProvider;
use tracing::{error, info};

use crate::{
    db::access::DbAccess,
    groups::{openmls_provider::AirOpenMlsProvider, self_group::SelfGroup},
    job::pending_chat_operation::PendingChatOperation,
    key_stores::{
        HeterogeneousVcKeyPackageBatch, MemoryUserKeyStore, VcKeyPackageBatchConfig,
        key_package_refs::{delete_orphaned_key_packages, mark_key_packages_as_live},
    },
    outbound_service::{APQ_KEY_PACKAGES, KEY_PACKAGES, OutboundServiceContext},
};

impl OutboundServiceContext {
    pub(super) async fn upload_key_packages(&self) -> anyhow::Result<Duration> {
        match SelfGroup::load(self.db.read().await?).await? {
            Some(group) => self.upload_via_self_group(group).await,
            None => {
                let batch = self.generate_key_packages().await?; // shared: plain + APQ
                self.upload_via_publish(batch).await
            }
        }
    }

    async fn upload_via_publish(&self, batch: KeyPackageBatch) -> anyhow::Result<Duration> {
        info!(
            plain = batch.plain.len(),
            apq = batch.apq.len(),
            "Uploading key packages via publish"
        );

        let api_client = self.api_clients.default_client()?;

        let key_package_refs = batch.references();

        // Publish plain key packages
        if let Err(error) = api_client
            .qs_publish_key_packages(
                self.qs_client_id,
                batch.plain,
                &self.key_store.qs_client_signing_key,
            )
            .await
        {
            error!(%error, "Failed to upload key packages via publish");
            key_package_refs
                .cleanup(&self.db, &self.key_store, Cleanup::All)
                .await;
            return Err(error.into());
        }
        self.db
            .with_write_transaction(async |txn| -> anyhow::Result<()> {
                mark_key_packages_as_live(txn, key_package_refs.plain.iter(), false).await?;
                Ok(())
            })
            .await?;

        // Publish APQ key packages
        if let Err(error) = api_client
            .qs_publish_apq_key_packages(
                self.qs_client_id,
                batch.apq,
                &self.key_store.qs_client_signing_key,
            )
            .await
        {
            error!(%error, "Failed to upload APQ key packages via publish");
            key_package_refs
                .cleanup(&self.db, &self.key_store, Cleanup::ApqOnly)
                .await;
            return Err(error.into());
        }
        self.db
            .with_write_transaction(async |txn| -> anyhow::Result<()> {
                mark_key_packages_as_live(txn, key_package_refs.apq.iter(), true).await?;
                delete_orphaned_key_packages(txn).await?;
                Ok(())
            })
            .await?;

        info!("Uploaded key packages");

        Ok(Duration::weeks(1))
    }

    async fn upload_via_self_group(&self, mut self_group: SelfGroup) -> anyhow::Result<Duration> {
        // Generate key packages
        let Some((epoch_id, generated)) = self
            .db
            .with_write_transaction(async |txn| -> anyhow::Result<_> {
                // Pending chat operation guard
                if PendingChatOperation::is_pending_for_group(&mut *txn, self_group.group_id())
                    .await?
                {
                    // Don't upload if there is a pending operation for the self-group.
                    return Ok(None);
                }

                let provider = AirOpenMlsProvider::new(txn.as_mut());

                // No pending chat operation => clear orphaned pending commits
                self_group
                    .mls_group_mut()
                    .clear_pending_commit(provider.storage())?;
                self_group
                    .pq_mut()
                    .context("Self-group is not APQ")?
                    .mls_group
                    .clear_pending_commit(provider.storage())?;

                let epoch_id = self_group
                    .mls_group_mut()
                    .register_vc_emulation_epoch(provider.crypto(), provider.storage())?;

                // TODO: Heavy CPU operation, do we have to spawn_blocking here?
                let generated = self.key_store.generate_vc_key_package_batch(
                    &mut *txn,
                    &self.qs_client_id,
                    epoch_id.clone(),
                    VcKeyPackageBatchConfig {
                        key_packages: KEY_PACKAGES,
                        last_resort: true,
                        apq_key_packages: APQ_KEY_PACKAGES,
                        apq_last_resort: true,
                    },
                )?;

                Ok(Some((epoch_id, generated)))
            })
            .await?
        else {
            return Ok(Duration::minutes(5));
        };

        // Upload and stage key packages
        let HeterogeneousVcKeyPackageBatch {
            generation,
            key_packages,
            apq_key_packages,
            infos,
        } = generated;

        let leaf_index = self_group.mls_group().own_leaf_index();
        let batch = KeyPackageBatch {
            plain: key_packages,
            apq: apq_key_packages,
        };
        let key_package_refs = batch.references();

        info!(
            plain = batch.plain.len(),
            apq = batch.apq.len(),
            "Uploading key packages via self-group"
        );

        let api_client = self.api_clients.default_client()?;
        let batch_id = KeyPackageBatchId {
            epoch_id,
            leaf_index,
            generation,
        };
        if let Err(error) = api_client
            .qs_stage_key_packages(
                self.qs_client_id,
                &batch_id,
                batch.plain,
                batch.apq,
                &self.key_store.qs_client_signing_key,
            )
            .await
        {
            error!(%error, "Failed to stage key packages");
            key_package_refs
                .cleanup(&self.db, &self.key_store, Cleanup::All)
                .await;
            return Err(error.into());
        }

        // Send commit to confirm uploaded key packages
        let job = Box::pin(
            self.db
                .with_write_transaction(async move |txn| -> anyhow::Result<_> {
                    let params = self_group.stage_key_package_upload(
                        &mut *txn,
                        self.signing_key(),
                        KeyPackageUpload {
                            epoch_id: batch_id.epoch_id.clone(),
                            leaf_index: batch_id.leaf_index,
                            generation: batch_id.generation,
                            key_package_info: infos,
                        },
                    )?;
                    PendingChatOperation::create_self_group_key_package_upload(
                        txn,
                        params,
                        batch_id,
                        key_package_refs.plain,
                        key_package_refs.apq,
                    )
                    .await
                }),
        )
        .await?;

        self.execute_job(job).await?;
        Ok(Duration::weeks(1))
    }

    async fn generate_key_packages(&self) -> anyhow::Result<KeyPackageBatch> {
        self.db
            .with_write_transaction(async move |txn| {
                let mut plain = Vec::with_capacity(KEY_PACKAGES + 1);
                // Non-last resort key packages
                for _ in 0..KEY_PACKAGES {
                    plain.push(self.key_store.generate_key_package(
                        &mut *txn,
                        &self.qs_client_id,
                        false,
                    )?);
                }
                // Last resort key package
                plain.push(self.key_store.generate_key_package(
                    &mut *txn,
                    &self.qs_client_id,
                    true,
                )?);

                // Non-last resort APQ key packages
                let mut apq = Vec::with_capacity(APQ_KEY_PACKAGES + 1);
                #[expect(clippy::reversed_empty_ranges)]
                for _ in 0..APQ_KEY_PACKAGES {
                    apq.push(self.key_store.generate_apq_key_package(
                        &mut *txn,
                        &self.qs_client_id,
                        false,
                    )?);
                }
                // Last resort APQ key package
                apq.push(
                    self.key_store
                        .generate_apq_key_package(txn, &self.qs_client_id, true)?,
                );

                Ok(KeyPackageBatch { plain, apq })
            })
            .await
    }
}

struct KeyPackageBatch {
    plain: Vec<KeyPackage>,
    apq: Vec<ApqKeyPackage>,
}

struct KeyPackageRefsBatch {
    plain: Vec<KeyPackageRef>,
    apq: Vec<KeyPackageRef>,
}

impl KeyPackageBatch {
    /// Collect all key package references.
    fn references(&self) -> KeyPackageRefsBatch {
        let crypto_provider = OpenMlsRustCrypto::default();
        let plain = self
            .plain
            .iter()
            .filter_map(|kp| kp.hash_ref(crypto_provider.crypto()).ok())
            .collect();
        let apq = self
            .apq
            .iter()
            .flat_map(|apq_kp| {
                [
                    apq_kp
                        .t_key_package()
                        .hash_ref(crypto_provider.crypto())
                        .ok(),
                    apq_kp
                        .pq_key_package()
                        .hash_ref(crypto_provider.crypto())
                        .ok(),
                ]
            })
            .flatten()
            .collect();
        KeyPackageRefsBatch { plain, apq }
    }
}

enum Cleanup {
    All,
    ApqOnly,
}

impl KeyPackageRefsBatch {
    async fn cleanup(self, db: &DbAccess, key_store: &MemoryUserKeyStore, cleanup: Cleanup) {
        let Ok(mut write) = db.write().await else {
            error!("Failed to acquire write lock for key package cleanup");
            return;
        };
        let packages = match cleanup {
            Cleanup::All => self.plain.into_iter().chain(self.apq),
            Cleanup::ApqOnly => self.apq.into_iter().chain(Vec::new()),
        };
        for key_package_ref in packages {
            if let Err(error) = key_store.delete_key_package(&mut write, key_package_ref) {
                error!(%error, "Failed to delete key package after upload failure");
            }
        }
    }
}
