// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later
use sqlx::{PgExecutor, PgPool, query, query_scalar};

use super::*;

#[cfg_attr(test, derive(Debug, PartialEq, Eq, Clone))]
pub(crate) struct UserHandleRecord {
    pub(crate) user_handle_hash: UserHandleHash,
    pub(crate) verifying_key: HandleVerifyingKey,
    pub(crate) expiration_data: ExpirationData,
}

impl UserHandleRecord {
    /// Upserts a user handle record in the database.
    ///
    /// Upsert is only done when the record is expired.
    ///
    /// Returns `true` if the record was upserrted, otherwise `false`.
    pub(crate) async fn store(&self, pool: &PgPool) -> sqlx::Result<bool> {
        let mut txn = pool.begin().await?;

        if let Some(record) =
            Self::load_expiration_data(txn.as_mut(), &self.user_handle_hash).await?
            && record.validate()
        {
            // A record already exists and is not expired
            // => it can't be reclaimed yet
            return Ok(false);
        }

        query!(
            "INSERT INTO as_user_handle (
                hash,
                verifying_key,
                expiration_data
            ) VALUES ($1, $2, $3)
            ON CONFLICT (hash) DO UPDATE
                SET verifying_key = $2, expiration_data = $3",
            self.user_handle_hash.as_bytes(),
            self.verifying_key as _,
            self.expiration_data as _,
        )
        .execute(txn.as_mut())
        .await?;

        txn.commit().await?;

        Ok(true)
    }

    pub(crate) async fn load_verifying_key(
        executor: impl PgExecutor<'_>,
        hash: &UserHandleHash,
    ) -> sqlx::Result<Option<HandleVerifyingKey>> {
        query_scalar!(
            r#"SELECT verifying_key AS "verifying_key: HandleVerifyingKey"
                FROM as_user_handle WHERE hash = $1"#,
            hash.as_bytes(),
        )
        .fetch_optional(executor)
        .await
    }

    /// Deletes a user handle record from the database.
    ///
    /// Returns `true` if the record was deleted, otherwise `false`.
    pub(super) async fn delete(
        executor: impl PgExecutor<'_>,
        hash: &UserHandleHash,
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
        hash: &UserHandleHash,
    ) -> sqlx::Result<Option<ExpirationData>> {
        query_scalar!(
            r#"SELECT expiration_data AS "expiration_data: ExpirationData"
            FROM as_user_handle WHERE hash = $1"#,
            hash.as_bytes(),
        )
        .fetch_optional(executor)
        .await
    }

    pub(super) async fn update_expiration_data(
        db_pool: &PgPool,
        hash: &UserHandleHash,
        expiration_data: ExpirationData,
    ) -> sqlx::Result<UpdateExpirationDataResult> {
        let mut txn = db_pool.begin().await?;

        let Some(stored_expiration_data) = Self::load_expiration_data(&mut *txn, hash).await?
        else {
            return Ok(UpdateExpirationDataResult::NotFound);
        };

        let res = if !stored_expiration_data.validate() {
            // Delete the record if the expiration date has passed
            Self::delete(&mut *txn, hash).await?;
            UpdateExpirationDataResult::Deleted
        } else {
            query!(
                "UPDATE as_user_handle SET expiration_data = $1 WHERE hash = $2",
                expiration_data as _,
                hash.as_bytes(),
            )
            .execute(db_pool)
            .await?;
            UpdateExpirationDataResult::Updated
        };

        txn.commit().await?;

        Ok(res)
    }
}

pub(super) enum UpdateExpirationDataResult {
    Updated,
    Deleted,
    NotFound,
}

#[cfg(test)]
mod test {
    use aircommon::time::Duration;
    use sqlx::PgPool;

    use super::*;

    #[sqlx::test]
    async fn test_store_and_load_user_handle_record(pool: PgPool) -> anyhow::Result<()> {
        let user_handle_hash = UserHandleHash::new([1; 32]);
        let verifying_key = HandleVerifyingKey::from_bytes(vec![1]);
        let expiration_data = ExpirationData::new(Duration::zero());

        let record = UserHandleRecord {
            user_handle_hash,
            verifying_key: verifying_key.clone(),
            expiration_data: expiration_data.clone(),
        };

        // Test storing a new record (which expires immediately)
        let inserted = record.store(&pool).await?;
        assert!(inserted, "Record should be inserted successfully");

        // Test loading the verifying key
        let loaded_verifying_key =
            UserHandleRecord::load_verifying_key(&pool, &user_handle_hash).await?;
        assert_eq!(
            loaded_verifying_key.as_ref(),
            Some(&verifying_key),
            "Loaded verifying key should match"
        );
        // Test loading the expiration data
        let loaded_expiration_data =
            UserHandleRecord::load_expiration_data(&pool, &user_handle_hash).await?;
        assert_eq!(
            loaded_expiration_data.as_ref(),
            Some(&expiration_data),
            "Loaded expiration data should match"
        );

        // Test storing the same hash (previous record is expired now)
        let different_verifying_key = HandleVerifyingKey::from_bytes(vec![2]);
        assert_ne!(verifying_key, different_verifying_key);
        let record = UserHandleRecord {
            user_handle_hash,
            verifying_key: different_verifying_key,
            expiration_data: ExpirationData::new(Duration::days(1)),
        };
        let inserted_again = record.store(&pool).await?;
        assert!(inserted_again, "Expired hash is reclaimed");
        let loaded_verifying_key =
            UserHandleRecord::load_verifying_key(&pool, &user_handle_hash).await?;
        assert_eq!(
            loaded_verifying_key.as_ref(),
            Some(&record.verifying_key),
            "Loaded verifying key should match"
        );
        let loaded_expiration_data =
            UserHandleRecord::load_expiration_data(&pool, &user_handle_hash).await?;
        assert_eq!(
            loaded_expiration_data.as_ref(),
            Some(&record.expiration_data),
            "Loaded expiration data should match"
        );

        // Test storing a different hash (previous record is not expired)
        let different_verifying_key = HandleVerifyingKey::from_bytes(vec![3]);
        assert_ne!(verifying_key, different_verifying_key);
        let different_record = UserHandleRecord {
            user_handle_hash,
            verifying_key: different_verifying_key,
            expiration_data: ExpirationData::new(Duration::days(1)),
        };
        let inserted_again = different_record.store(&pool).await?;
        assert!(!inserted_again, "Non-expired hash is not reclaimed");
        assert_eq!(
            loaded_verifying_key.as_ref(),
            Some(&record.verifying_key),
            "Verifying key should not change"
        );
        let loaded_expiration_data =
            UserHandleRecord::load_expiration_data(&pool, &user_handle_hash).await?;
        assert_eq!(
            loaded_expiration_data.as_ref(),
            Some(&record.expiration_data),
            "Expiration data should not change"
        );

        // Test loading a non-existent key
        let non_existent_hash = UserHandleHash::new([2; 32]);
        let loaded_non_existent =
            UserHandleRecord::load_verifying_key(&pool, &non_existent_hash).await?;
        assert_eq!(
            loaded_non_existent, None,
            "Loading non-existent key should return None"
        );
        let loaded_non_existent =
            UserHandleRecord::load_expiration_data(&pool, &non_existent_hash).await?;
        assert_eq!(
            loaded_non_existent, None,
            "Loading non-existent key should return None"
        );

        Ok(())
    }

    #[sqlx::test]
    async fn test_delete_user_handle_record(pool: PgPool) -> anyhow::Result<()> {
        let user_handle_hash = UserHandleHash::new([1; 32]);
        let verifying_key = HandleVerifyingKey::from_bytes(vec![1, 2, 3, 4, 5]);
        let expiration_data = ExpirationData::new(Duration::days(1));

        let record = UserHandleRecord {
            user_handle_hash,
            verifying_key,
            expiration_data,
        };

        // Store the record first
        record.store(&pool).await?;

        // Test deleting an existing record
        let deleted = UserHandleRecord::delete(&pool, &user_handle_hash).await?;
        assert!(deleted, "Record should be deleted successfully");

        // Verify it's gone
        let loaded_after_delete =
            UserHandleRecord::load_verifying_key(&pool, &user_handle_hash).await?;
        assert_eq!(
            loaded_after_delete, None,
            "Record should not exist after deletion"
        );

        // Test deleting a non-existent record
        let non_existent_hash = UserHandleHash::new([2; 32]);
        let deleted_non_existent = UserHandleRecord::delete(&pool, &non_existent_hash).await?;
        assert!(
            !deleted_non_existent,
            "Deleting non-existent record should return false"
        );

        Ok(())
    }

    #[sqlx::test]
    async fn test_update_expiration_data(pool: PgPool) -> anyhow::Result<()> {
        let user_handle_hash = UserHandleHash::new([1; 32]);
        let verifying_key = HandleVerifyingKey::from_bytes(vec![1, 2, 3, 4, 5]);
        let initial_expiration_data = ExpirationData::new(Duration::days(1));
        let updated_expiration_data = ExpirationData::new(Duration::days(2));

        let record = UserHandleRecord {
            user_handle_hash,
            verifying_key: verifying_key.clone(),
            expiration_data: initial_expiration_data.clone(),
        };

        // Store the record first
        record.store(&pool).await?;

        let res = UserHandleRecord::update_expiration_data(
            &pool,
            &user_handle_hash,
            updated_expiration_data.clone(),
        )
        .await?;
        assert!(
            matches!(res, UpdateExpirationDataResult::Updated),
            "Expiration data should be updated successfully"
        );

        // Verify the expiration data has been updated
        let loaded_expiration_data =
            UserHandleRecord::load_expiration_data(&pool, &user_handle_hash)
                .await?
                .unwrap();
        assert_eq!(
            loaded_expiration_data, updated_expiration_data,
            "Expiration data should be updated"
        );

        let loaded_verifying_key = UserHandleRecord::load_verifying_key(&pool, &user_handle_hash)
            .await?
            .unwrap();
        assert_eq!(
            loaded_verifying_key, verifying_key,
            "Verifying key should remain unchanged"
        );

        // Test updating an expired record
        let res = UserHandleRecord::update_expiration_data(
            &pool,
            &user_handle_hash,
            ExpirationData::new(Duration::zero()),
        )
        .await?;
        assert!(
            matches!(res, UpdateExpirationDataResult::Updated),
            "Updating expired record should return false"
        );

        let res = UserHandleRecord::update_expiration_data(
            &pool,
            &user_handle_hash,
            ExpirationData::new(Duration::days(1)),
        )
        .await?;
        assert!(
            matches!(res, UpdateExpirationDataResult::Deleted),
            "Updating expired should delete the record"
        );
        let res = UserHandleRecord::load_expiration_data(&pool, &user_handle_hash).await?;
        assert!(res.is_none(), "Expired record should be deleted");

        // Test updating a non-existent record
        let non_existent_hash = UserHandleHash::new([2; 32]);
        let res = UserHandleRecord::update_expiration_data(
            &pool,
            &non_existent_hash,
            ExpirationData::new(Duration::days(1)),
        )
        .await?;
        assert!(
            matches!(res, UpdateExpirationDataResult::NotFound),
            "Updating non-existent record should return false"
        );

        Ok(())
    }
}
