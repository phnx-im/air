// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use apqmls::messages::{ApqKeyPackage, ApqKeyPackageIn};
use mls_assist::openmls::prelude::KeyPackage;

use aircommon::{
    codec::{BlobDecoded, BlobEncoded},
    identifiers::QsUserId,
    messages::FriendshipToken,
};
use sqlx::{
    Arguments, AssertSqlSafe, Connection, PgConnection, PgTransaction, Postgres, encode::IsNull,
    error::BoxDynError, postgres::PgArguments, query,
};
use tls_codec::Serialize as _;
use tonic::async_trait;

use crate::errors::StorageError;

#[async_trait]
pub(super) trait StorableKeyPackage<'q>: Sized + Send + Sync + Unpin {
    const TABLE_NAME: &'static str;

    type BlobEncoded<'a>: sqlx::Encode<'a, Postgres> + sqlx::Type<Postgres>
    where
        Self: 'a;

    fn encoded<'a>(&'a self) -> Self::BlobEncoded<'a>;

    type BlobDecoded: for<'a> sqlx::Decode<'a, Postgres> + sqlx::Type<Postgres> + Send + Unpin;

    fn decoded(decoded: Self::BlobDecoded) -> sqlx::Result<Self>;

    async fn replace_multiple(
        txn: &mut PgTransaction<'_>,
        user_id: &QsUserId,
        key_packages: &[Self],
    ) -> Result<(), StorageError> {
        Self::replace_multiple_internal(txn, user_id, key_packages, false).await
    }

    async fn replace_last_resort(
        &self,
        txn: &mut PgTransaction<'_>,
        user_id: &QsUserId,
    ) -> Result<(), StorageError> {
        Self::replace_multiple_internal(txn, user_id, std::slice::from_ref(self), true).await
    }

    async fn replace_multiple_internal(
        txn: &mut PgTransaction<'_>,
        user_id: &QsUserId,
        key_packages: &[Self],
        is_last_resort: bool,
    ) -> Result<(), StorageError> {
        if key_packages.is_empty() {
            return Ok(());
        }

        query(AssertSqlSafe(format!(
            "DELETE FROM {table_name} WHERE user_id = $1 AND is_last_resort = $2",
            table_name = Self::TABLE_NAME
        )))
        .bind(user_id)
        .bind(is_last_resort)
        .execute(txn.as_mut())
        .await?;

        let mut query_args = PgArguments::default();
        let mut query_string = format!(
            "INSERT INTO {table_name} (user_id, key_package, is_last_resort) VALUES",
            table_name = Self::TABLE_NAME
        );

        for (i, key_package) in key_packages.iter().enumerate() {
            // Add values to the query arguments. None of these should throw an error.
            query_args.add(user_id)?;
            query_args.add(key_package.encoded())?;
            query_args.add(is_last_resort)?;

            if i > 0 {
                query_string.push(',');
            }

            // Add placeholders for each value
            query_string.push_str(&format!(
                " (${}, ${}, ${})",
                i * 3 + 1,
                i * 3 + 2,
                i * 3 + 3,
            ));
        }

        // Finalize the query string
        query_string.push(';');

        // Execute the query
        sqlx::query_with(AssertSqlSafe(query_string), query_args)
            .execute(txn.as_mut())
            .await?;

        Ok(())
    }

    async fn load_user_key_package(
        connection: &mut PgConnection,
        friendship_token: &FriendshipToken,
    ) -> Result<Self, StorageError> {
        let mut transaction = connection.begin().await?;

        let key_package = sqlx::query_scalar(AssertSqlSafe(format!(
            r#"WITH selected_key_package AS (
                    SELECT p.id, p.key_package, p.is_last_resort
                    FROM {table_name} p
                    INNER JOIN qs_user_record u ON p.user_id = u.user_id
                    WHERE u.friendship_token = $1
                    ORDER BY p.is_last_resort ASC
                    LIMIT 1
                    FOR UPDATE OF p SKIP LOCKED
                ),

                deleted_packages AS (
                    DELETE FROM {table_name}
                    WHERE id IN (SELECT id FROM selected_key_package
                        WHERE is_last_resort = FALSE)
                    RETURNING key_package
                )

                SELECT key_package as "key_package: Self::BlobDecoded"
                FROM selected_key_package"#,
            table_name = Self::TABLE_NAME
        )))
        .bind(friendship_token)
        .fetch_one(&mut *transaction)
        .await
        .inspect_err(|error| {
            tracing::error!(%error, "Failed to fetch key package");
        })
        .map(|blob| Self::decoded(blob))??;

        transaction.commit().await?;

        Ok(key_package)
    }
}

impl StorableKeyPackage<'_> for KeyPackage {
    const TABLE_NAME: &'static str = "key_package";

    type BlobEncoded<'a> = BlobEncoded<&'a Self>;

    fn encoded(&self) -> Self::BlobEncoded<'_> {
        BlobEncoded(self)
    }

    type BlobDecoded = BlobDecoded<Self>;

    fn decoded<'a>(decoded: Self::BlobDecoded) -> sqlx::Result<Self> {
        Ok(BlobDecoded::into_inner(decoded))
    }
}

impl StorableKeyPackage<'_> for ApqKeyPackage {
    const TABLE_NAME: &'static str = "apq_key_package";

    type BlobEncoded<'a> = StorableApqKeyPackage<'a>;

    fn encoded(&self) -> Self::BlobEncoded<'_> {
        StorableApqKeyPackage(self)
    }

    type BlobDecoded = StoredApqKeyPackage;

    fn decoded(decoded: Self::BlobDecoded) -> sqlx::Result<Self> {
        Ok(decoded.0.unwrap_verified())
    }
}

pub(super) struct StorableApqKeyPackage<'a>(&'a ApqKeyPackage);

impl sqlx::Type<Postgres> for StorableApqKeyPackage<'_> {
    fn type_info() -> <Postgres as sqlx::Database>::TypeInfo {
        <Vec<u8> as sqlx::Type<Postgres>>::type_info()
    }
}

impl sqlx::Encode<'_, Postgres> for StorableApqKeyPackage<'_> {
    fn encode_by_ref(
        &self,
        buf: &mut <Postgres as sqlx::Database>::ArgumentBuffer,
    ) -> Result<IsNull, BoxDynError> {
        let buf: &mut Vec<u8> = &mut *buf;
        self.0.t_key_package().tls_serialize(buf)?;
        self.0.pq_key_package().tls_serialize(buf)?;
        Ok(IsNull::No)
    }
}

pub(super) struct StoredApqKeyPackage(ApqKeyPackageIn);

impl sqlx::Type<Postgres> for StoredApqKeyPackage {
    fn type_info() -> <Postgres as sqlx::Database>::TypeInfo {
        <Vec<u8> as sqlx::Type<Postgres>>::type_info()
    }
}

impl sqlx::Decode<'_, Postgres> for StoredApqKeyPackage {
    fn decode(value: <Postgres as sqlx::Database>::ValueRef<'_>) -> Result<Self, BoxDynError> {
        let bytes = value.as_bytes()?;
        let (t_key_package, remaining) = tls_codec::DeserializeBytes::tls_deserialize_bytes(bytes)?;
        let pq_key_package = tls_codec::DeserializeBytes::tls_deserialize_exact_bytes(remaining)?;
        Ok(StoredApqKeyPackage(ApqKeyPackageIn::new(
            t_key_package,
            pq_key_package,
        )))
    }
}

#[cfg(test)]
mod tests {

    use std::collections::HashSet;

    use rand::RngExt;
    use sqlx::PgPool;

    use crate::qs::user_record::persistence::tests::store_random_user_record;

    use super::*;

    type DummyKeyPackage = Vec<u8>;

    impl StorableKeyPackage<'_> for DummyKeyPackage {
        const TABLE_NAME: &'static str = "key_package";

        type BlobEncoded<'a> = BlobEncoded<&'a Self>;

        fn encoded(&self) -> Self::BlobEncoded<'_> {
            BlobEncoded(self)
        }

        type BlobDecoded = BlobDecoded<Self>;

        fn decoded(decoded: Self::BlobDecoded) -> sqlx::Result<Self> {
            Ok(BlobDecoded::into_inner(decoded))
        }
    }

    #[sqlx::test]
    async fn load_user_key_package(pool: PgPool) -> anyhow::Result<()> {
        let user_record = store_random_user_record(&pool).await?;
        let packages = store_random_key_packages(&pool, &user_record.user_id).await?;

        let mut loaded = [None, None];

        for _ in 0..2 {
            let pkg = DummyKeyPackage::load_user_key_package(
                pool.acquire().await?.as_mut(),
                &user_record.friendship_token,
            )
            .await?;
            if pkg == packages[0] {
                loaded[0] = Some(pkg);
            } else if pkg == packages[1] {
                loaded[1] = Some(pkg);
            } else {
                panic!("Unexpected key package loaded");
            }
        }

        assert_eq!(loaded[0].as_ref().unwrap(), &packages[0]);
        assert_eq!(loaded[1].as_ref().unwrap(), &packages[1]);

        // There should be no more key packages left to load that are not marked as last resort.
        for _ in 0..10 {
            let pkg = DummyKeyPackage::load_user_key_package(
                pool.acquire().await?.as_mut(),
                &user_record.friendship_token,
            )
            .await?;
            if pkg != packages[2] {
                panic!("Unexpected key package loaded");
            }
        }

        Ok(())
    }

    #[sqlx::test]
    async fn packages_are_replaced(pool: PgPool) -> anyhow::Result<()> {
        let user_record = store_random_user_record(&pool).await?;

        for _ in 0..2 {
            let packages = store_random_key_packages(&pool, &user_record.user_id).await?;
            let loaded: Vec<BlobDecoded<Vec<u8>>> = sqlx::query_scalar(
                r#"SELECT key_package as "key_package: BlobDecoded<Vec<u8>>" FROM key_package"#,
            )
            .fetch_all(&pool)
            .await?;
            let loaded: HashSet<_> = loaded
                .into_iter()
                .map(|BlobDecoded(key_package)| key_package)
                .collect();
            let packages: HashSet<_> = packages.into_iter().collect();
            assert_eq!(loaded, packages);
        }

        Ok(())
    }

    async fn store_random_key_packages(
        pool: &PgPool,
        user_id: &QsUserId,
    ) -> anyhow::Result<Vec<DummyKeyPackage>> {
        let mut rng = rand::rng();

        let a: [u8; 4] = rng.random();
        let b: [u8; 4] = rng.random();
        let last_resort: [u8; 4] = rng.random();

        let pkg_a = a.to_vec();
        let pkg_b = b.to_vec();
        let pkg_last_resort = last_resort.to_vec();

        let mut txn = pool.begin().await?;
        DummyKeyPackage::replace_multiple(&mut txn, user_id, &[pkg_a.clone(), pkg_b.clone()])
            .await?;
        pkg_last_resort
            .replace_last_resort(&mut txn, user_id)
            .await?;
        txn.commit().await?;

        Ok(vec![pkg_a, pkg_b, pkg_last_resort])
    }
}
