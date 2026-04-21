// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later
use sqlx::{PgExecutor, PgPool, query, query_as, query_scalar};

use super::*;

#[cfg_attr(test, derive(Debug, PartialEq, Eq, Clone))]
pub(crate) struct UsernameRecord {
    pub(crate) username_hash: UsernameHash,
    pub(crate) verifying_key: UsernameVerifyingKey,
    pub(crate) expiration_data: ExpirationData,
}

impl UsernameRecord {
    pub(crate) async fn load_all(
        executor: impl PgExecutor<'_>,
    ) -> sqlx::Result<Vec<UsernameRecord>> {
        query_as!(
            UsernameRecord,
            r#"
                SELECT
                    hash AS "username_hash: UsernameHash",
                    verifying_key AS "verifying_key: UsernameVerifyingKey",
                    expiration_data AS "expiration_data: ExpirationData"
                FROM as_user_handle
            "#
        )
        .fetch_all(executor)
        .await
    }

    pub(crate) async fn check_exists(pool: &PgPool, hash: &UsernameHash) -> sqlx::Result<bool> {
        Self::load_expiration_data(pool, hash)
            .await
            .map(|opt| opt.is_some())
    }

    pub(crate) async fn store(&self, executor: impl PgExecutor<'_>) -> sqlx::Result<bool> {
        let res = query!(
            "INSERT INTO as_user_handle (
                hash,
                verifying_key,
                expiration_data
            ) VALUES ($1, $2, $3)
            ON CONFLICT (hash) DO NOTHING",
            self.username_hash.as_bytes(),
            self.verifying_key as _,
            self.expiration_data as _,
        )
        .execute(executor)
        .await?;
        Ok(res.rows_affected() > 0)
    }

    pub(crate) async fn update(&self, executor: impl PgExecutor<'_>) -> sqlx::Result<()> {
        query!(
            "UPDATE as_user_handle SET
                verifying_key = $2,
                expiration_data = $3
            WHERE hash = $1",
            self.username_hash.as_bytes(),
            self.verifying_key as _,
            self.expiration_data as _,
        )
        .execute(executor)
        .await?;
        Ok(())
    }

    pub(crate) async fn load_verifying_key(
        executor: impl PgExecutor<'_>,
        hash: &UsernameHash,
    ) -> sqlx::Result<Option<UsernameVerifyingKey>> {
        query_scalar!(
            r#"SELECT verifying_key AS "verifying_key: UsernameVerifyingKey"
                FROM as_user_handle WHERE hash = $1"#,
            hash.as_bytes(),
        )
        .fetch_optional(executor)
        .await
    }

    /// Deletes a username record from the database.
    ///
    /// Returns `true` if the record was deleted, otherwise `false`.
    pub(super) async fn delete(
        executor: impl PgExecutor<'_>,
        hash: &UsernameHash,
    ) -> sqlx::Result<bool> {
        let res = query!(
            "DELETE FROM as_user_handle WHERE hash = $1",
            hash.as_bytes(),
        )
        .execute(executor)
        .await?;
        let deleted = res.rows_affected() > 0;
        Ok(deleted)
    }

    pub(crate) async fn load_expiration_data(
        executor: impl PgExecutor<'_>,
        hash: &UsernameHash,
    ) -> sqlx::Result<Option<ExpirationData>> {
        query_scalar!(
            r#"SELECT expiration_data AS "expiration_data: ExpirationData"
            FROM as_user_handle WHERE hash = $1"#,
            hash.as_bytes(),
        )
        .fetch_optional(executor)
        .await
    }

    pub(crate) async fn load_expiration_data_for_update(
        executor: impl PgExecutor<'_>,
        hash: &UsernameHash,
    ) -> sqlx::Result<Option<ExpirationData>> {
        query_scalar!(
            r#"SELECT expiration_data AS "expiration_data: ExpirationData"
            FROM as_user_handle WHERE hash = $1
            FOR UPDATE"#,
            hash.as_bytes(),
        )
        .fetch_optional(executor)
        .await
    }

    pub(crate) async fn update_expiration_data(
        executor: impl PgExecutor<'_>,
        hash: &UsernameHash,
        expiration_data: ExpirationData,
    ) -> sqlx::Result<()> {
        query!(
            "UPDATE as_user_handle SET expiration_data = $1 WHERE hash = $2",
            expiration_data as _,
            hash.as_bytes(),
        )
        .execute(executor)
        .await?;
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use aircommon::time::Duration;
    use sqlx::PgPool;

    use super::*;

    #[sqlx::test]
    async fn test_store_and_load_username_record(pool: PgPool) -> anyhow::Result<()> {
        let username_hash = UsernameHash::new([1; 32]);
        let verifying_key = UsernameVerifyingKey::from_bytes(vec![1]);
        let expiration_data = ExpirationData::new(Duration::zero());

        let record = UsernameRecord {
            username_hash,
            verifying_key: verifying_key.clone(),
            expiration_data: expiration_data.clone(),
        };

        let inserted = record.store(&pool).await?;
        assert!(inserted, "First store should insert the record");

        let loaded_verifying_key =
            UsernameRecord::load_verifying_key(&pool, &username_hash).await?;
        assert_eq!(loaded_verifying_key.as_ref(), Some(&verifying_key));

        let loaded_expiration_data =
            UsernameRecord::load_expiration_data(&pool, &username_hash).await?;
        assert_eq!(loaded_expiration_data.as_ref(), Some(&expiration_data));

        let loaded_expiration_data =
            UsernameRecord::load_expiration_data_for_update(&pool, &username_hash).await?;
        assert_eq!(loaded_expiration_data.as_ref(), Some(&expiration_data));

        // Storing the same hash again does nothing (ON CONFLICT DO NOTHING)
        let different_verifying_key = UsernameVerifyingKey::from_bytes(vec![2]);
        assert_ne!(verifying_key, different_verifying_key);
        let inserted_again = UsernameRecord {
            username_hash,
            verifying_key: different_verifying_key,
            expiration_data: ExpirationData::new(Duration::days(1)),
        }
        .store(&pool)
        .await?;
        assert!(!inserted_again, "Store for existing hash should return false");

        let loaded_verifying_key =
            UsernameRecord::load_verifying_key(&pool, &username_hash).await?;
        assert_eq!(
            loaded_verifying_key.as_ref(),
            Some(&verifying_key),
            "Verifying key should not change"
        );

        // Non-existent hash returns None
        let non_existent_hash = UsernameHash::new([2; 32]);
        assert_eq!(
            UsernameRecord::load_verifying_key(&pool, &non_existent_hash).await?,
            None
        );
        assert_eq!(
            UsernameRecord::load_expiration_data(&pool, &non_existent_hash).await?,
            None
        );

        Ok(())
    }

    #[sqlx::test]
    async fn test_delete_username_record(pool: PgPool) -> anyhow::Result<()> {
        let username_hash = UsernameHash::new([1; 32]);
        let verifying_key = UsernameVerifyingKey::from_bytes(vec![1, 2, 3, 4, 5]);
        let expiration_data = ExpirationData::new(Duration::days(1));

        let record = UsernameRecord {
            username_hash,
            verifying_key,
            expiration_data,
        };

        let mut txn = pool.begin().await?;
        record.store(txn.as_mut()).await?;
        txn.commit().await?;

        let deleted = UsernameRecord::delete(&pool, &username_hash).await?;
        assert!(deleted, "Record should be deleted successfully");

        let loaded_after_delete = UsernameRecord::load_verifying_key(&pool, &username_hash).await?;
        assert_eq!(loaded_after_delete, None, "Record should not exist after deletion");

        let non_existent_hash = UsernameHash::new([2; 32]);
        let deleted_non_existent = UsernameRecord::delete(&pool, &non_existent_hash).await?;
        assert!(!deleted_non_existent, "Deleting non-existent record should return false");

        Ok(())
    }

    #[sqlx::test]
    async fn test_update_expiration_data(pool: PgPool) -> anyhow::Result<()> {
        let username_hash = UsernameHash::new([1; 32]);
        let verifying_key = UsernameVerifyingKey::from_bytes(vec![1, 2, 3, 4, 5]);
        let initial_expiration_data = ExpirationData::new(Duration::days(1));
        let updated_expiration_data = ExpirationData::new(Duration::days(2));

        let record = UsernameRecord {
            username_hash,
            verifying_key: verifying_key.clone(),
            expiration_data: initial_expiration_data.clone(),
        };

        let mut txn = pool.begin().await?;

        record.store(txn.as_mut()).await?;

        UsernameRecord::update_expiration_data(
            txn.as_mut(),
            &username_hash,
            updated_expiration_data.clone(),
        )
        .await?;

        let loaded_expiration_data =
            UsernameRecord::load_expiration_data(txn.as_mut(), &username_hash)
                .await?
                .unwrap();
        assert_eq!(loaded_expiration_data, updated_expiration_data);

        Ok(())
    }
}
