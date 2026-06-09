// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::identifiers::QsUserId;
use apqmls::messages::ApqKeyPackage;
use chrono::{DateTime, Utc};
use mls_assist::openmls::key_packages::KeyPackage;
use sqlx::{PgExecutor, PgTransaction, query, query_as, query_scalar};
use tokio_stream::StreamExt;

/// A batch of key packages to be staged.
pub(super) struct StagedKeyPackages {
    pub user_id: QsUserId,
    pub epoch_id: Vec<u8>,
    pub random: Vec<u8>,
    /// Traditional key packages incl. their TLS bytes
    pub key_packages: Vec<(KeyPackage, Vec<u8>)>,
    /// APQ key packages incl. their TLS bytes
    pub apq_key_packages: Vec<(ApqKeyPackage, Vec<u8>)>,
}

impl StagedKeyPackages {
    /// Stages key packages for the given `(user, epoch, random)` triple.
    ///
    /// This function is idempotent in the following sense: if the batch already exists for the
    /// given triple, it is checked that the key packages are exactly the same (the order of the key
    /// packages is *important*). If the key packages are different, an error is returned.
    pub(super) async fn stage(
        &self,
        txn: &mut PgTransaction<'_>,
    ) -> Result<(), StageKeyPackageError> {
        let batch_id = query_scalar!(
            "INSERT INTO qs_staged_key_package_batch (user_id, epoch_id, random)
            VALUES ($1, $2, $3)
            ON CONFLICT (user_id, epoch_id, random) DO NOTHING
            RETURNING id",
            self.user_id as _,
            self.epoch_id,
            self.random,
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
            let batch_id = query_scalar!(
                "SELECT id FROM qs_staged_key_package_batch
                WHERE user_id = $1 AND epoch_id = $2 AND random = $3",
                self.user_id as _,
                self.epoch_id,
                self.random,
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
                batch_id
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
    use apqmls::{
        ApqCiphersuite,
        authentication::{ApqCredentialWithKey, ApqSignatureKeyPair, ApqSignatureScheme},
    };
    use chrono::Duration;
    use mls_assist::openmls_rust_crypto::OpenMlsRustCrypto;
    use sqlx::PgPool;
    use tls_codec::Serialize;

    use crate::qs::user_record::persistence::tests::store_random_user_record;

    use super::*;

    fn build_apq_key_package(last_resort: bool) -> ApqKeyPackage {
        let provider = OpenMlsRustCrypto::default();
        let ciphersuite = ApqCiphersuite::default_pq_conf_and_auth();
        let scheme = ApqSignatureScheme::from(ciphersuite);
        let signer = ApqSignatureKeyPair::new(scheme).unwrap();
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
        random: &[u8],
        key_packages: Vec<KeyPackage>,
        apq_key_packages: Vec<ApqKeyPackage>,
    ) -> StagedKeyPackages {
        StagedKeyPackages {
            user_id,
            epoch_id: b"epoch-1".to_vec(),
            random: random.to_vec(),
            key_packages: key_packages
                .into_iter()
                .map(|key_package| {
                    let tls = key_package.tls_serialize_detached().unwrap();
                    (key_package, tls)
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
            b"r1",
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
            b"r1",
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

        let first = staged(user.user_id, b"r1", vec![build_key_package(false)], vec![]);
        let mut txn = pool.begin().await?;
        first.stage(&mut txn).await?;
        txn.commit().await?;

        // Same (user, epoch, random), but a different key package => reject.
        let second = staged(user.user_id, b"r1", vec![build_key_package(false)], vec![]);
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
            b"r1",
            vec![kp.clone(), build_key_package(false)],
            vec![],
        );
        let mut txn = pool.begin().await?;
        first.stage(&mut txn).await?;
        txn.commit().await?;

        // Same leading package, but the re-staged batch is shorter => reject.
        let second = staged(user.user_id, b"r1", vec![kp], vec![]);
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
            b"old",
            vec![build_key_package(false)],
            vec![build_apq_key_package(false)],
        );
        let fresh = staged(
            user.user_id,
            b"fresh",
            vec![build_key_package(false)],
            vec![],
        );

        let mut txn = pool.begin().await?;
        old.stage(&mut txn).await?;
        fresh.stage(&mut txn).await?;
        txn.commit().await?;

        // Backdate the "old" batch past the TTL.
        sqlx::query!(
            "UPDATE qs_staged_key_package_batch SET created_at = $1 WHERE random = $2",
            Utc::now() - Duration::hours(1),
            b"old".as_slice(),
        )
        .execute(&pool)
        .await?;

        StagedKeyPackages::delete_expired(&pool, Utc::now() - Duration::minutes(5)).await?;

        // Only the fresh batch and its single entry remain; old entries cascaded away.
        assert_eq!(counts(&pool).await?, (1, 1));
        Ok(())
    }
}
