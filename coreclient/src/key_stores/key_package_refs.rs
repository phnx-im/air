// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use openmls::prelude::KeyPackageRef;
use sqlx::{AssertSqlSafe, QueryBuilder};

use crate::{db::access::WriteDbTransaction, groups::openmls_provider::KeyRefWrapper};

pub(crate) async fn mark_key_packages_as_live(
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

    // Mark all key packages as stale
    sqlx::query(AssertSqlSafe(format!(
        "UPDATE {refs_table}
            SET is_live = 0
            WHERE is_live = 1",
    )))
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
}
