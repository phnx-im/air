// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use apqmls::messages::ApqKeyPackage;
use chrono::Duration;
use openmls::prelude::{KeyPackage, KeyPackageRef};
use openmls_rust_crypto::OpenMlsRustCrypto;
use openmls_traits::OpenMlsProvider;
use tracing::{error, info};

use crate::{
    clients::own_client_info::OwnClientInfo,
    db::access::DbAccess,
    groups::Group,
    key_stores::MemoryUserKeyStore,
    outbound_service::{APQ_KEY_PACKAGES, KEY_PACKAGES, OutboundServiceContext},
};

impl OutboundServiceContext {
    pub(super) async fn upload_key_packages(&self) -> anyhow::Result<Duration> {
        let batch = self.generate_key_packages().await?; // shared: plain + APQ
        match self.load_self_group().await? {
            Some(group) => self.upload_via_self_group(group, batch).await,
            None => self.upload_via_publish(batch).await,
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
                batch.plain.clone(),
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
                super::timed_tasks::persistence::mark_key_packages_as_live(
                    txn,
                    key_package_refs.plain.iter(),
                    false,
                )
                .await?;
                Ok(())
            })
            .await?;

        // Publish APQ key packages
        if let Err(error) = api_client
            .qs_publish_apq_key_packages(
                self.qs_client_id,
                batch.apq.clone(),
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
                super::timed_tasks::persistence::mark_key_packages_as_live(
                    txn,
                    key_package_refs.apq.iter(),
                    true,
                )
                .await?;
                Ok(())
            })
            .await?;

        info!("Uploaded key packages");

        Ok(Duration::weeks(1))
    }

    async fn upload_via_self_group(
        &self,
        self_group: Group,
        batch: KeyPackageBatch,
    ) -> anyhow::Result<Duration> {
        // if self_group.has_pending_commit() {
        //     return Ok(Duration::minutes(5));
        // }

        // let key_package_refs = batch.references();
        //
        // let batch_id = KeyPackageBatchId {
        //     epoch_id: EpochIdExt::from_bytes(self_group.mls_group().epoch().as_u64().to_be_bytes()),
        //     leaf_index: self_group.mls_group().own_leaf_index(),
        //     generation: todo!(),
        // };

        Ok(Duration::weeks(1))
    }

    /// Loads the self-group if one has been provisioned for this client.
    async fn load_self_group(&self) -> anyhow::Result<Option<Group>> {
        let mut read = self.db.read().await?;
        let own_client_info = OwnClientInfo::load(&mut read).await?;
        let Some(group_id) = own_client_info.self_group_id else {
            return Ok(None);
        };
        Ok(Group::load(read, &group_id).await?)
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
