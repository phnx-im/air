// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use openmls::prelude::KeyPackageRef;
use sqlx::{AssertSqlSafe, QueryBuilder};

use crate::{
    db::access::{WriteConnection, WriteDbTransaction},
    groups::openmls_provider::KeyRefWrapper,
};

pub(crate) async fn mark_key_packages_as_live(
    txn: &mut WriteDbTransaction<'_>,
    key_package_refs: &[KeyPackageRef],
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
    key_package_refs: &[KeyPackageRef],
) -> anyhow::Result<()> {
    // Delete all key packages that are not marked as live
    sqlx::query(AssertSqlSafe(format!(
        "DELETE FROM key_package
            WHERE key_package_ref IN (
              SELECT key_package_ref
              FROM {refs_table}
              WHERE is_live = 0
            )"
    )))
    .execute(txn.as_mut())
    .await?;

    // A sibling's key packages are not stored as bundles but as the material
    // to rederive them, which keeps their derivation epoch alive. It follows
    // the same lifecycle as the bundles.
    sqlx::query(AssertSqlSafe(format!(
        "DELETE FROM vc_retained_key_package_material
            WHERE key_package_ref IN (
              SELECT key_package_ref
              FROM {refs_table}
              WHERE is_live = 0
            )"
    )))
    .execute(txn.as_mut())
    .await?;

    // Their refs are dead weight now. Delete them explicitly (there is no foreign key
    // cascade).
    sqlx::query(AssertSqlSafe(format!(
        "DELETE FROM {refs_table} WHERE is_live = 0"
    )))
    .execute(txn.as_mut())
    .await?;

    // Mark all key packages as stale
    sqlx::query(AssertSqlSafe(format!(
        "UPDATE {refs_table}
            SET is_live = 0
            WHERE is_live = 1",
    )))
    .execute(txn.as_mut())
    .await?;

    if key_package_refs.is_empty() {
        return Ok(());
    }

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

    Ok(())
}

/// Delete key packages and retained sibling key package material not
/// referenced in either refs table (usually a no-op).
///
/// Must only run after *both* the regular and the APQ refs of a batch are
/// inserted: key package bundles and retained material are written to storage
/// when the batch is generated or processed, before their refs are registered,
/// so an early sweep would delete the not yet referenced half of the batch.
pub(crate) async fn delete_orphaned_key_packages(
    mut write: impl WriteConnection,
) -> anyhow::Result<()> {
    sqlx::query!(
        "DELETE FROM key_package WHERE key_package_ref NOT IN (
            SELECT key_package_ref FROM key_package_refs
            UNION
            SELECT key_package_ref FROM apq_key_package_refs
        )",
    )
    .execute(write.as_mut())
    .await?;
    sqlx::query!(
        "DELETE FROM vc_retained_key_package_material WHERE key_package_ref NOT IN (
            SELECT key_package_ref FROM key_package_refs
            UNION
            SELECT key_package_ref FROM apq_key_package_refs
        )",
    )
    .execute(write.as_mut())
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
    use openmls_rust_crypto::RustCrypto;
    use openmls_traits::OpenMlsProvider;
    use sqlx::{Row, SqlitePool, query, query_scalar};
    use url::Host;

    use crate::{
        clients::CIPHERSUITE, db::access::DbAccess, groups::openmls_provider::AirOpenMlsProvider,
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
            signature_key: SignaturePublicKey::from(client_sk.credential().verifying_key().clone()),
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
            mark_key_packages_as_live(txn, std::slice::from_ref(&new_key_package_ref), is_apq).await
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
                let key_package_ref: KeyPackageRef = PersistenceCodec::from_slice(&bytes).unwrap();
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

    fn key_package_ref(name: &[u8]) -> KeyPackageRef {
        KeyPackageRef::new(name, CIPHERSUITE, &RustCrypto::default(), b"test").unwrap()
    }

    async fn insert_ref(pool: &DbAccess, r: &KeyPackageRef, is_live: bool) -> anyhow::Result<()> {
        query("INSERT INTO key_package_refs (key_package_ref, is_live) VALUES (?1, ?2)")
            .bind(KeyRefWrapper(r))
            .bind(is_live)
            .execute(pool.write().await?.as_mut())
            .await?;
        Ok(())
    }

    async fn insert_retained_material(pool: &DbAccess, r: &KeyPackageRef) -> anyhow::Result<()> {
        query(
            "INSERT INTO vc_retained_key_package_material (key_package_ref, epoch_id, record)
            VALUES (?1, ?2, ?3)",
        )
        .bind(KeyRefWrapper(r))
        .bind(b"epoch".to_vec())
        .bind(b"record".to_vec())
        .execute(pool.write().await?.as_mut())
        .await?;
        Ok(())
    }

    async fn retained_refs(pool: &DbAccess) -> anyhow::Result<Vec<Vec<u8>>> {
        Ok(query_scalar(
            "SELECT key_package_ref FROM vc_retained_key_package_material ORDER BY key_package_ref",
        )
        .fetch_all(pool.read().await?.as_mut())
        .await?)
    }

    async fn mark_live(pool: &DbAccess, r: &KeyPackageRef) -> anyhow::Result<()> {
        pool.with_write_transaction(async |txn| {
            mark_key_packages_as_live(txn, std::slice::from_ref(r), false).await
        })
        .await
    }

    #[tokio::test]
    async fn stale_retained_material_is_deleted_with_its_refs() -> anyhow::Result<()> {
        let pool = SqlitePool::connect("sqlite://:memory:").await?;
        sqlx::migrate!("./migrations").run(&pool).await?;
        let pool = DbAccess::for_tests(pool);

        let live = key_package_ref(b"live");
        let stale = key_package_ref(b"stale");
        let new = key_package_ref(b"new");
        insert_ref(&pool, &live, true).await?;
        insert_ref(&pool, &stale, false).await?;
        for r in [&live, &stale, &new] {
            insert_retained_material(&pool, r).await?;
        }

        mark_live(&pool, &new).await?;
        let mut expected = vec![
            PersistenceCodec::to_vec(&live)?,
            PersistenceCodec::to_vec(&new)?,
        ];
        expected.sort();
        assert_eq!(retained_refs(&pool).await?, expected);

        // The next replacement takes the material that just went stale.
        mark_live(&pool, &key_package_ref(b"newer")).await?;
        assert_eq!(
            retained_refs(&pool).await?,
            vec![PersistenceCodec::to_vec(&new)?]
        );

        Ok(())
    }

    #[tokio::test]
    async fn orphaned_retained_material_is_deleted() -> anyhow::Result<()> {
        let pool = SqlitePool::connect("sqlite://:memory:").await?;
        sqlx::migrate!("./migrations").run(&pool).await?;
        let pool = DbAccess::for_tests(pool);

        let referenced = key_package_ref(b"referenced");
        insert_ref(&pool, &referenced, true).await?;
        insert_retained_material(&pool, &referenced).await?;
        insert_retained_material(&pool, &key_package_ref(b"orphan")).await?;

        delete_orphaned_key_packages(pool.write().await?).await?;

        assert_eq!(
            retained_refs(&pool).await?,
            vec![PersistenceCodec::to_vec(&referenced)?]
        );
        Ok(())
    }
}
