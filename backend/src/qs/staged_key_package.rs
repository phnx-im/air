// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::time::Duration;

use aircommon::{
    identifiers::QsUserId,
    virtual_client::{EpochIdExt, KeyPackageBatchId},
};
use apqmls::messages::ApqKeyPackage;
use chrono::{DateTime, Utc};
use mls_assist::openmls::key_packages::KeyPackage;
use sqlx::{PgExecutor, PgPool, PgTransaction, query, query_as, query_scalar};
use tokio_stream::StreamExt;
use tokio_util::sync::CancellationToken;
use tracing::error;

/// How often to run the cleanup task
const CLEANUP_INTERVAL: Duration = Duration::from_secs(60);
/// How long to keep a batch of staged key packages around
const STAGED_KEY_PACKAGES_BATCH_TTL: chrono::Duration = chrono::Duration::minutes(5);

/// A batch of key packages to be staged.
pub(super) struct StagedKeyPackages {
    pub user_id: QsUserId,
    pub batch_id: KeyPackageBatchId,
    /// Traditional key packages incl. their storage bytes
    pub key_packages: Vec<(KeyPackage, Vec<u8>)>,
    /// APQ key packages incl. their TLS bytes
    pub apq_key_packages: Vec<(ApqKeyPackage, Vec<u8>)>,
}

impl StagedKeyPackages {
    /// Stages key packages for the given `(user, epoch, leaf_index, generation)` tuple.
    ///
    /// This function is idempotent in the following sense: if the batch already exists for the
    /// given tuple, it is checked that the key packages are exactly the same (the order of the key
    /// packages is *important*). If the key packages are different, an error is returned.
    pub(super) async fn stage(
        &self,
        txn: &mut PgTransaction<'_>,
    ) -> Result<(), StageKeyPackageError> {
        let epoch_id_bytes = self.batch_id.epoch_id.to_bytes();
        let batch_id = query_scalar!(
            "INSERT INTO qs_staged_key_package_batch (user_id, epoch_id, leaf_index, generation)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (user_id, epoch_id, leaf_index, generation) DO NOTHING
            RETURNING id",
            self.user_id as _,
            epoch_id_bytes,
            self.batch_id.leaf_index.u32() as i64,
            self.batch_id.generation as i64,
        )
        .fetch_optional(txn.as_mut())
        .await?;

        if let Some(batch_id) = batch_id {
            // Fresh batch
            for (key_package, tls) in &self.key_packages {
                let is_last_resort = key_package.last_resort();
                query!(
                    "INSERT INTO qs_staged_key_package (
                        batch_id, key_package, is_last_resort, is_apq
                    )
                    VALUES ($1, $2, $3, false)",
                    batch_id,
                    tls,
                    is_last_resort,
                )
                .execute(txn.as_mut())
                .await?;
            }

            for (apq_key_package, tls) in &self.apq_key_packages {
                let is_last_resort = apq_key_package.t_key_package().last_resort()
                    && apq_key_package.pq_key_package().last_resort();
                query!(
                    "INSERT INTO qs_staged_key_package (
                        batch_id, key_package, is_last_resort, is_apq
                    )
                    VALUES ($1, $2, $3, true)",
                    batch_id,
                    tls,
                    is_last_resort,
                )
                .execute(txn.as_mut())
                .await?;
            }
        } else {
            let epoch_id_bytes = self.batch_id.epoch_id.to_bytes();
            let local_batch_id = query_scalar!(
                "SELECT id FROM qs_staged_key_package_batch
                WHERE user_id = $1 AND epoch_id = $2 AND leaf_index = $3 AND generation = $4",
                self.user_id as _,
                epoch_id_bytes,
                self.batch_id.leaf_index.u32() as i64,
                self.batch_id.generation as i64,
            )
            .fetch_one(txn.as_mut())
            .await?;

            // Existing batch => check that the key packages are the same => idempotency
            struct Row {
                key_package: Vec<u8>,
                is_last_resort: bool,
                is_apq: bool,
            }

            let mut rows = query_as!(
                Row,
                "SELECT key_package, is_last_resort, is_apq
                FROM qs_staged_key_package
                WHERE batch_id = $1
                ORDER BY id ASC",
                local_batch_id
            )
            .fetch(txn.as_mut());

            let mut num_t = 0;
            let mut num_apq = 0;

            while let Some(row) = rows.next().await {
                let row = row?;
                let (tls, is_last_resort) = if !row.is_apq {
                    let (key_package, tls) = self
                        .key_packages
                        .get(num_t)
                        .ok_or(StageKeyPackageError::MissingKeyPackage)?;
                    num_t += 1;
                    (tls, key_package.last_resort())
                } else {
                    let (key_package, tls) = self
                        .apq_key_packages
                        .get(num_apq)
                        .ok_or(StageKeyPackageError::MissingKeyPackage)?;
                    num_apq += 1;
                    let is_last_resort = key_package.t_key_package().last_resort()
                        && key_package.pq_key_package().last_resort();
                    (tls, is_last_resort)
                };
                if row.is_last_resort != is_last_resort || &row.key_package != tls {
                    return Err(StageKeyPackageError::KeyPackageMismatch);
                }
            }

            if num_t != self.key_packages.len() || num_apq != self.apq_key_packages.len() {
                return Err(StageKeyPackageError::MissingKeyPackage);
            }
        }

        Ok(())
    }

    /// Delete all staged key packages that are older than the given `DateTime`.
    pub(super) async fn delete_expired(
        executor: impl PgExecutor<'_>,
        older_than: DateTime<Utc>,
    ) -> sqlx::Result<()> {
        query!(
            "DELETE FROM qs_staged_key_package_batch WHERE created_at < $1",
            older_than,
        )
        .execute(executor)
        .await?;
        Ok(())
    }

    pub(crate) async fn promote(
        txn: &mut PgTransaction<'_>,
        user_id: &QsUserId,
        batch_id: &KeyPackageBatchId,
    ) -> sqlx::Result<()> {
        // Get the batch ID and lock it for update.

        let epoch_id_bytes = batch_id.epoch_id.to_bytes();
        let Some(batch_id) = query_scalar!(
            "SELECT id FROM qs_staged_key_package_batch
            where user_id = $1 and epoch_id = $2 and leaf_index = $3 and generation = $4
            FOR UPDATE",
            user_id as _,
            epoch_id_bytes,
            batch_id.leaf_index.u32() as i64,
            batch_id.generation as i64,
        )
        .fetch_optional(txn.as_mut())
        .await?
        else {
            // No batch found, nothing to promote => replay defense
            return Ok(());
        };

        // Delete all T key packages for all clients of this user; last resort packages are deleted
        // if the batch contains at least one T last resort key package.
        query!(
            "WITH lr AS (
                SELECT
                    coalesce(bool_or(is_last_resort)
                        FILTER (WHERE is_apq = false), false) AS has
                FROM qs_staged_key_package
                WHERE batch_id = $1
            )
            DELETE FROM key_package
            WHERE client_id IN (
                SELECT client_id FROM qs_client_record
                    WHERE user_id = $2 AND deleted_at IS NULL
                )
                AND (NOT is_last_resort OR (SELECT has FROM lr))
            ",
            batch_id,
            user_id as _,
        )
        .execute(txn.as_mut())
        .await?;

        // Delete all APQ key packages for all clients of this user; last resort packages are
        // deleted if the batch contains at least one APQ last resort key package.
        query!(
            "WITH lr AS (
                SELECT
                    coalesce(bool_or(is_last_resort)
                        FILTER (WHERE is_apq = true), false) AS has
                FROM qs_staged_key_package
                WHERE batch_id = $1
            )
            DELETE FROM apq_key_package
            WHERE client_id IN (
                SELECT client_id FROM qs_client_record
                    WHERE user_id = $2 AND deleted_at IS NULL
                )
                AND (NOT is_last_resort OR (SELECT has FROM lr))
            ",
            batch_id,
            user_id as _,
        )
        .execute(txn.as_mut())
        .await?;

        // Copy all T key packages for all clients of this user from the staged table to the
        // T key package table.
        query!(
            "INSERT INTO key_package (client_id, key_package, is_last_resort)
            SELECT c.client_id, s.key_package, s.is_last_resort
            FROM qs_staged_key_package s
            JOIN qs_client_record c ON c.user_id = $1 AND c.deleted_at IS NULL
            WHERE s.batch_id = $2 AND s.is_apq = false
            ",
            user_id as _,
            batch_id,
        )
        .execute(txn.as_mut())
        .await?;

        // Copy all APQ key packages for all clients of this user from the staged table to the
        // APQ key package table.
        query!(
            "INSERT INTO apq_key_package (client_id, key_package, is_last_resort)
            SELECT c.client_id, s.key_package, s.is_last_resort
            FROM qs_staged_key_package s
            JOIN qs_client_record c ON c.user_id = $1 AND c.deleted_at IS NULL
            WHERE s.batch_id = $2 AND s.is_apq = true
            ",
            user_id as _,
            batch_id,
        )
        .execute(txn.as_mut())
        .await?;

        // Delete the staged key packages batch.
        query!(
            "DELETE FROM qs_staged_key_package_batch WHERE id = $1",
            batch_id,
        )
        .execute(txn.as_mut())
        .await?;

        Ok(())
    }

    pub(crate) fn spawn_periodic_cleanup(db_pool: PgPool, stop: CancellationToken) {
        tokio::spawn(stop.run_until_cancelled_owned(async move {
            let mut interval = tokio::time::interval(CLEANUP_INTERVAL);
            loop {
                interval.tick().await;
                let cutoff = Utc::now() - STAGED_KEY_PACKAGES_BATCH_TTL;
                if let Err(error) = Self::delete_expired(&db_pool, cutoff).await {
                    error!(%error, "Failed to delete expired staged key packages");
                }
            }
        }));
    }
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum StageKeyPackageError {
    #[error(transparent)]
    Sqlx(#[from] sqlx::Error),
    #[error("missing key package")]
    MissingKeyPackage,
    #[error("key package mismatch")]
    KeyPackageMismatch,
}

#[cfg(test)]
mod tests {
    use aircommon::codec::PersistenceCodec;
    use apqmls::{
        ApqCiphersuite,
        authentication::{ApqCredentialWithKey, ApqSignatureKeyPair, ApqSignatureScheme},
    };
    use chrono::Duration;
    use mls_assist::{openmls::prelude::LeafNodeIndex, openmls_rust_crypto::OpenMlsRustCrypto};
    use sqlx::PgPool;
    use tls_codec::Serialize;

    use crate::qs::{
        client_record::persistence::tests::store_random_client_record,
        key_package::StorableKeyPackage, user_record::persistence::tests::store_random_user_record,
    };

    use super::*;

    fn build_apq_key_package(last_resort: bool) -> ApqKeyPackage {
        let provider = OpenMlsRustCrypto::default();
        let ciphersuite = ApqCiphersuite::default_pq_conf_and_auth();
        let scheme = ApqSignatureScheme::from(ciphersuite);

        // Generating a PQ signature explodes the stack because it needs about 2 MiB.
        // The exact problem is in `ml_dsa::SigningKey::<MlDsa87>::generate())`.
        let signer = std::thread::Builder::new()
            .stack_size(4 * 1024 * 1024)
            .spawn(move || ApqSignatureKeyPair::new(scheme).unwrap())
            .unwrap()
            .join()
            .unwrap();

        let credential = ApqCredentialWithKey::new(b"test-client", &signer);
        let mut builder = ApqKeyPackage::builder();
        if last_resort {
            builder = builder.mark_as_last_resort();
        }
        builder
            .build(&provider, ciphersuite, &signer, credential)
            .unwrap()
            .into_key_package()
    }

    /// A plain key package is just the traditional half of a freshly built APQ key package.
    fn build_key_package(last_resort: bool) -> KeyPackage {
        build_apq_key_package(last_resort).t_key_package().clone()
    }

    fn staged(
        user_id: QsUserId,
        generation: u32,
        key_packages: Vec<KeyPackage>,
        apq_key_packages: Vec<ApqKeyPackage>,
    ) -> StagedKeyPackages {
        StagedKeyPackages {
            user_id,
            batch_id: KeyPackageBatchId {
                epoch_id: EpochIdExt::from_bytes(b"epoch-1"),
                leaf_index: LeafNodeIndex::new(0),
                generation,
            },
            key_packages: key_packages
                .into_iter()
                .map(|key_package| {
                    let encoded = PersistenceCodec::to_vec(&key_package).unwrap();
                    (key_package, encoded)
                })
                .collect(),
            apq_key_packages: apq_key_packages
                .into_iter()
                .map(|key_package| {
                    let tls = [
                        key_package
                            .t_key_package()
                            .tls_serialize_detached()
                            .unwrap(),
                        key_package
                            .pq_key_package()
                            .tls_serialize_detached()
                            .unwrap(),
                    ]
                    .concat();
                    (key_package, tls)
                })
                .collect(),
        }
    }

    /// Returns `(batch_count, key_package_count)`.
    async fn counts(pool: &PgPool) -> anyhow::Result<(i64, i64)> {
        let batches = sqlx::query_scalar!("SELECT count(*) FROM qs_staged_key_package_batch")
            .fetch_one(pool)
            .await?
            .unwrap_or(0);
        let entries = sqlx::query_scalar!("SELECT count(*) FROM qs_staged_key_package")
            .fetch_one(pool)
            .await?
            .unwrap_or(0);
        Ok((batches, entries))
    }

    #[sqlx::test]
    async fn stage_fresh_batch(pool: PgPool) -> anyhow::Result<()> {
        let user = store_random_user_record(&pool).await?;
        let skp = staged(
            user.user_id,
            1,
            vec![build_key_package(false), build_key_package(true)],
            vec![build_apq_key_package(false)],
        );

        let mut txn = pool.begin().await?;
        skp.stage(&mut txn).await?;
        txn.commit().await?;

        // One batch, three entries (two plain + one APQ).
        assert_eq!(counts(&pool).await?, (1, 3));
        Ok(())
    }

    #[sqlx::test]
    async fn restage_identical_is_noop(pool: PgPool) -> anyhow::Result<()> {
        let user = store_random_user_record(&pool).await?;
        let skp = staged(
            user.user_id,
            1,
            vec![build_key_package(false), build_key_package(true)],
            vec![build_apq_key_package(true), build_apq_key_package(false)],
        );

        for _ in 0..2 {
            let mut txn = pool.begin().await?;
            skp.stage(&mut txn).await?;
            txn.commit().await?;
        }

        // The second stage is a no-op
        assert_eq!(counts(&pool).await?, (1, 4));
        Ok(())
    }

    #[sqlx::test]
    async fn restage_different_rejects(pool: PgPool) -> anyhow::Result<()> {
        let user = store_random_user_record(&pool).await?;

        let first = staged(user.user_id, 1, vec![build_key_package(false)], vec![]);
        let mut txn = pool.begin().await?;
        first.stage(&mut txn).await?;
        txn.commit().await?;

        // Same (user, epoch, leaf_index, generation), but a different key package => reject.
        let second = staged(user.user_id, 1, vec![build_key_package(false)], vec![]);
        let mut txn = pool.begin().await?;
        let result = second.stage(&mut txn).await;
        assert!(matches!(
            result,
            Err(StageKeyPackageError::KeyPackageMismatch)
        ));
        Ok(())
    }

    #[sqlx::test]
    async fn restage_fewer_packages_rejects(pool: PgPool) -> anyhow::Result<()> {
        let user = store_random_user_record(&pool).await?;
        let kp = build_key_package(false);

        let first = staged(
            user.user_id,
            1,
            vec![kp.clone(), build_key_package(false)],
            vec![],
        );
        let mut txn = pool.begin().await?;
        first.stage(&mut txn).await?;
        txn.commit().await?;

        // Same leading package, but the re-staged batch is shorter => reject.
        let second = staged(user.user_id, 1, vec![kp], vec![]);
        let mut txn = pool.begin().await?;
        let result = second.stage(&mut txn).await;
        assert!(matches!(
            result,
            Err(StageKeyPackageError::MissingKeyPackage)
        ));
        Ok(())
    }

    #[sqlx::test]
    async fn delete_expired_removes_old_batches_and_cascades(pool: PgPool) -> anyhow::Result<()> {
        let user = store_random_user_record(&pool).await?;

        let old = staged(
            user.user_id,
            1,
            vec![build_key_package(false)],
            vec![build_apq_key_package(false)],
        );
        let fresh = staged(user.user_id, 2, vec![build_key_package(false)], vec![]);

        let mut txn = pool.begin().await?;
        old.stage(&mut txn).await?;
        fresh.stage(&mut txn).await?;
        txn.commit().await?;

        // Backdate the "old" batch past the TTL.
        sqlx::query!(
            "UPDATE qs_staged_key_package_batch SET created_at = $1 WHERE generation = $2",
            Utc::now() - Duration::hours(1),
            old.batch_id.generation as i64,
        )
        .execute(&pool)
        .await?;

        StagedKeyPackages::delete_expired(&pool, Utc::now() - Duration::minutes(5)).await?;

        // Only the fresh batch and its single entry remain; old entries cascaded away.
        assert_eq!(counts(&pool).await?, (1, 1));
        Ok(())
    }

    /// Returns `(key_package_count, apq_key_package_count)` in the live tables.
    async fn live_counts(pool: &PgPool) -> anyhow::Result<(i64, i64)> {
        let plain = sqlx::query_scalar!("SELECT count(*) FROM key_package")
            .fetch_one(pool)
            .await?
            .unwrap_or(0);
        let apq = sqlx::query_scalar!("SELECT count(*) FROM apq_key_package")
            .fetch_one(pool)
            .await?
            .unwrap_or(0);
        Ok((plain, apq))
    }

    /// Storage bytes of all live (plain) last-resort key packages.
    async fn live_last_resort(pool: &PgPool) -> anyhow::Result<Vec<Vec<u8>>> {
        Ok(
            sqlx::query_scalar!("SELECT key_package FROM key_package WHERE is_last_resort = true")
                .fetch_all(pool)
                .await?,
        )
    }

    async fn stage_and_promote(pool: &PgPool, batch: &StagedKeyPackages) -> anyhow::Result<()> {
        let mut txn = pool.begin().await?;
        batch.stage(&mut txn).await?;
        txn.commit().await?;

        let mut txn = pool.begin().await?;
        StagedKeyPackages::promote(&mut txn, &batch.user_id, &batch.batch_id).await?;
        txn.commit().await?;
        Ok(())
    }

    #[sqlx::test]
    async fn promote_populates_live_for_all_clients(pool: PgPool) -> anyhow::Result<()> {
        let user = store_random_user_record(&pool).await?;
        store_random_client_record(&pool, user.user_id).await?;
        store_random_client_record(&pool, user.user_id).await?;

        let batch = staged(
            user.user_id,
            1,
            vec![build_key_package(false)],
            vec![build_apq_key_package(false)],
        );
        stage_and_promote(&pool, &batch).await?;

        // One plain + one APQ KP, hosted under each of the two clients.
        assert_eq!(live_counts(&pool).await?, (2, 2));
        // Staging consumed.
        assert_eq!(counts(&pool).await?, (0, 0));
        Ok(())
    }

    #[sqlx::test]
    async fn promoted_packages_are_servable(pool: PgPool) -> anyhow::Result<()> {
        let user = store_random_user_record(&pool).await?;
        store_random_client_record(&pool, user.user_id).await?;

        let key_package = build_key_package(false);
        let apq_key_package = build_apq_key_package(false);
        let batch = staged(
            user.user_id,
            1,
            vec![key_package.clone()],
            vec![apq_key_package.clone()],
        );
        stage_and_promote(&pool, &batch).await?;

        // Promoted packages must round-trip through the same load path that
        // serves published packages.
        let loaded = KeyPackage::load_user_key_package(
            pool.acquire().await?.as_mut(),
            &user.friendship_token,
        )
        .await?;
        assert_eq!(
            loaded.tls_serialize_detached()?,
            key_package.tls_serialize_detached()?
        );

        let loaded = ApqKeyPackage::load_user_key_package(
            pool.acquire().await?.as_mut(),
            &user.friendship_token,
        )
        .await?;
        assert_eq!(
            loaded.t_key_package().tls_serialize_detached()?,
            apq_key_package.t_key_package().tls_serialize_detached()?
        );
        assert_eq!(
            loaded.pq_key_package().tls_serialize_detached()?,
            apq_key_package.pq_key_package().tls_serialize_detached()?
        );

        Ok(())
    }

    #[sqlx::test]
    async fn promote_replaces_non_last_resort(pool: PgPool) -> anyhow::Result<()> {
        let user = store_random_user_record(&pool).await?;
        store_random_client_record(&pool, user.user_id).await?;

        let first = staged(
            user.user_id,
            1,
            vec![build_key_package(false), build_key_package(false)],
            vec![],
        );
        stage_and_promote(&pool, &first).await?;
        assert_eq!(live_counts(&pool).await?, (2, 0));

        let second = staged(user.user_id, 2, vec![build_key_package(false)], vec![]);
        stage_and_promote(&pool, &second).await?;

        // The two from `first` were replaced by the single one from `second`.
        assert_eq!(live_counts(&pool).await?, (1, 0));
        Ok(())
    }

    #[sqlx::test]
    async fn promote_keeps_last_resort_when_batch_has_none(pool: PgPool) -> anyhow::Result<()> {
        let user = store_random_user_record(&pool).await?;
        store_random_client_record(&pool, user.user_id).await?;

        // Batch 1 brings a non-last-resort and a last-resort.
        let lr = build_key_package(true);
        let lr_encoded = PersistenceCodec::to_vec(&lr)?;
        let first = staged(user.user_id, 1, vec![build_key_package(false), lr], vec![]);
        stage_and_promote(&pool, &first).await?;
        assert_eq!(live_counts(&pool).await?, (2, 0));

        // Batch 2 has no last-resort: it replaces the non-last-resort but keeps the last-resort.
        let second = staged(user.user_id, 2, vec![build_key_package(false)], vec![]);
        stage_and_promote(&pool, &second).await?;

        // One fresh non-last-resort + the surviving last-resort from batch 1.
        assert_eq!(live_counts(&pool).await?, (2, 0));
        assert_eq!(live_last_resort(&pool).await?, vec![lr_encoded]);
        Ok(())
    }

    #[sqlx::test]
    async fn promote_replaces_last_resort_when_batch_has_one(pool: PgPool) -> anyhow::Result<()> {
        let user = store_random_user_record(&pool).await?;
        store_random_client_record(&pool, user.user_id).await?;

        let first = staged(user.user_id, 1, vec![build_key_package(true)], vec![]);
        stage_and_promote(&pool, &first).await?;

        let new_lr = build_key_package(true);
        let new_lr_encoded = PersistenceCodec::to_vec(&new_lr)?;
        let second = staged(user.user_id, 2, vec![new_lr], vec![]);
        stage_and_promote(&pool, &second).await?;

        // Exactly one last-resort remains, and it's the one from batch 2.
        assert_eq!(live_counts(&pool).await?, (1, 0));
        assert_eq!(live_last_resort(&pool).await?, vec![new_lr_encoded]);
        Ok(())
    }

    #[sqlx::test]
    async fn promote_missing_batch_is_noop(pool: PgPool) -> anyhow::Result<()> {
        let user = store_random_user_record(&pool).await?;
        store_random_client_record(&pool, user.user_id).await?;

        // No batch staged for this (epoch, leaf_index, generation): replayed/stale hint => no-op.
        let mut txn = pool.begin().await?;
        let batch_id = KeyPackageBatchId {
            epoch_id: EpochIdExt::from_bytes(b"epoch-1"),
            leaf_index: LeafNodeIndex::new(0),
            generation: 999,
        };
        StagedKeyPackages::promote(&mut txn, &user.user_id, &batch_id).await?;
        txn.commit().await?;

        assert_eq!(live_counts(&pool).await?, (0, 0));
        Ok(())
    }
}
