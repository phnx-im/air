// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::{
    codec::{BlobDecoded, BlobEncoded},
    credentials::keys::HandleSigningKey,
    identifiers::{UserHandle, UserHandleHash},
};
use chrono::{DateTime, Utc};
use sqlx::{SqliteExecutor, query, query_as, query_scalar};

/// A user handle record stored in the client database.
///
/// Contains additional information about the handle, such as hash and signing key.
#[derive(Debug, Clone)]
pub struct UserHandleRecord {
    pub handle: UserHandle,
    pub hash: UserHandleHash,
    pub signing_key: HandleSigningKey,
}

#[cfg(test)]
impl PartialEq for UserHandleRecord {
    fn eq(&self, other: &Self) -> bool {
        // Note: only the verifying key part of the signing key is compared.
        self.handle == other.handle
            && self.hash == other.hash
            && self.signing_key.verifying_key() == other.signing_key.verifying_key()
    }
}

struct SqlUserHandleRecord {
    handle: UserHandle,
    hash: UserHandleHash,
    signing_key: BlobDecoded<HandleSigningKey>,
}

impl From<SqlUserHandleRecord> for UserHandleRecord {
    fn from(record: SqlUserHandleRecord) -> Self {
        Self {
            handle: record.handle,
            hash: record.hash,
            signing_key: record.signing_key.into_inner(),
        }
    }
}

impl UserHandleRecord {
    pub fn new(handle: UserHandle, hash: UserHandleHash, signing_key: HandleSigningKey) -> Self {
        Self {
            handle,
            hash,
            signing_key,
        }
    }

    pub(super) async fn load(
        executor: impl SqliteExecutor<'_>,
        handle: &UserHandle,
    ) -> sqlx::Result<Option<Self>> {
        let record = query_as!(
            SqlUserHandleRecord,
            r#"
                SELECT
                    handle AS "handle: _",
                    hash AS "hash: _",
                    signing_key AS "signing_key: _"
                FROM user_handle
                WHERE handle = ?
            "#,
            handle
        )
        .fetch_optional(executor)
        .await?;
        Ok(record.map(From::from))
    }

    pub(crate) async fn load_all(executor: impl SqliteExecutor<'_>) -> sqlx::Result<Vec<Self>> {
        let records = query_as!(
            SqlUserHandleRecord,
            r#"
                SELECT
                    handle AS "handle: _",
                    hash AS "hash: _",
                    signing_key AS "signing_key: _"
                FROM user_handle
                ORDER BY created_at ASC
            "#,
        )
        .fetch_all(executor)
        .await?;
        Ok(records.into_iter().map(From::from).collect())
    }

    pub(crate) async fn load_all_handles(
        executor: impl SqliteExecutor<'_>,
    ) -> sqlx::Result<Vec<UserHandle>> {
        query_scalar!(
            r#"
                SELECT handle AS "handle: _"
                FROM user_handle
                ORDER BY created_at ASC
            "#
        )
        .fetch_all(executor)
        .await
    }

    pub(super) async fn store(&self, executor: impl SqliteExecutor<'_>) -> sqlx::Result<()> {
        let signing_key = BlobEncoded(&self.signing_key);
        let created_at = Utc::now();
        let refreshed_at = created_at;
        query!(
            r#"
                INSERT INTO user_handle (
                    handle,
                    hash,
                    signing_key,
                    created_at,
                    refreshed_at
                ) VALUES (?, ?, ?, ?, ?)
            "#,
            self.handle,
            self.hash,
            signing_key,
            created_at,
            refreshed_at,
        )
        .execute(executor)
        .await?;
        Ok(())
    }

    /// Load handles where `refreshed_at` is older than the given threshold.
    pub(crate) async fn load_needing_refresh(
        executor: impl SqliteExecutor<'_>,
        threshold: DateTime<Utc>,
    ) -> sqlx::Result<Vec<Self>> {
        let records = query_as!(
            SqlUserHandleRecord,
            r#"
                SELECT
                    handle AS "handle: _",
                    hash AS "hash: _",
                    signing_key AS "signing_key: _"
                FROM user_handle
                WHERE refreshed_at < ?
            "#,
            threshold
        )
        .fetch_all(executor)
        .await?;
        Ok(records.into_iter().map(From::from).collect())
    }

    /// Update `refreshed_at` for a handle identified by its hash.
    pub(crate) async fn update_refreshed_at(
        executor: impl SqliteExecutor<'_>,
        hash: &UserHandleHash,
        refreshed_at: DateTime<Utc>,
    ) -> sqlx::Result<()> {
        query!(
            r#"
                UPDATE user_handle
                SET refreshed_at = ?
                WHERE hash = ?
            "#,
            refreshed_at,
            hash,
        )
        .execute(executor)
        .await?;
        Ok(())
    }

    pub(super) async fn delete(
        executor: impl SqliteExecutor<'_>,
        handle: &UserHandle,
    ) -> sqlx::Result<()> {
        query!(
            r#"
                DELETE FROM user_handle
                WHERE handle = ?
            "#,
            handle,
        )
        .execute(executor)
        .await?;
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use sqlx::SqlitePool;

    use super::*;

    #[sqlx::test]
    async fn user_handle_record_store_load(pool: SqlitePool) -> anyhow::Result<()> {
        let handle = UserHandle::new("ellie-03".to_owned())?;
        let hash = handle.calculate_hash()?;
        let signing_key = HandleSigningKey::generate()?;
        let record = UserHandleRecord::new(handle.clone(), hash, signing_key);
        record.store(&pool).await?;

        let loaded_record = UserHandleRecord::load(&pool, &handle).await?.unwrap();
        assert_eq!(loaded_record, record);
        Ok(())
    }

    #[sqlx::test]
    async fn user_handle_record_load_all(pool: SqlitePool) -> anyhow::Result<()> {
        let handle1 = UserHandle::new("ellie-03".to_owned())?;
        let hash1 = handle1.calculate_hash()?;
        let signing_key1 = HandleSigningKey::generate()?;
        let record1 = UserHandleRecord::new(handle1.clone(), hash1, signing_key1);
        record1.store(&pool).await?;

        let handle2 = UserHandle::new("joel-03".to_owned())?;
        let hash2 = handle2.calculate_hash()?;
        let signing_key2 = HandleSigningKey::generate()?;
        let record2 = UserHandleRecord::new(handle2.clone(), hash2, signing_key2);
        record2.store(&pool).await?;

        let loaded_records = UserHandleRecord::load_all(&pool).await?;
        assert_eq!(loaded_records.len(), 2);
        assert!(loaded_records.contains(&record1));
        assert!(loaded_records.contains(&record2));
        Ok(())
    }

    #[sqlx::test]
    async fn user_handle_record_load_all_handles(pool: SqlitePool) -> anyhow::Result<()> {
        let handle1 = UserHandle::new("ellie-03".to_owned())?;
        let hash1 = handle1.calculate_hash()?;
        let signing_key1 = HandleSigningKey::generate()?;
        let record1 = UserHandleRecord::new(handle1.clone(), hash1, signing_key1);
        record1.store(&pool).await?;

        let handle2 = UserHandle::new("joel-03".to_owned())?;
        let hash2 = handle2.calculate_hash()?;
        let signing_key2 = HandleSigningKey::generate()?;
        let record2 = UserHandleRecord::new(handle2.clone(), hash2, signing_key2);
        record2.store(&pool).await?;

        let loaded_handles = UserHandleRecord::load_all_handles(&pool).await?;
        assert_eq!(loaded_handles.len(), 2);
        assert!(loaded_handles.contains(&handle1));
        assert!(loaded_handles.contains(&handle2));
        Ok(())
    }

    #[sqlx::test]
    async fn user_handle_record_load_needing_refresh(pool: SqlitePool) -> anyhow::Result<()> {
        use chrono::Duration;

        // Create a handle with old refreshed_at (> 90 days ago)
        let handle_old = UserHandle::new("old-handle".to_owned())?;
        let hash_old = handle_old.calculate_hash()?;
        let signing_key_old = HandleSigningKey::generate()?;
        let record_old = UserHandleRecord::new(handle_old.clone(), hash_old, signing_key_old);
        record_old.store(&pool).await?;

        // Manually set refreshed_at to 100 days ago
        let old_time = Utc::now() - Duration::days(100);
        sqlx::query("UPDATE user_handle SET refreshed_at = ? WHERE handle = ?")
            .bind(old_time)
            .bind(&handle_old)
            .execute(&pool)
            .await?;

        // Create a handle with recent refreshed_at
        let handle_new = UserHandle::new("new-handle".to_owned())?;
        let hash_new = handle_new.calculate_hash()?;
        let signing_key_new = HandleSigningKey::generate()?;
        let record_new = UserHandleRecord::new(handle_new.clone(), hash_new, signing_key_new);
        record_new.store(&pool).await?;

        // Query handles needing refresh (threshold = now - 90 days)
        let threshold = Utc::now() - Duration::days(90);
        let needing_refresh = UserHandleRecord::load_needing_refresh(&pool, threshold).await?;
        assert_eq!(needing_refresh.len(), 1);
        assert_eq!(needing_refresh[0].handle, handle_old);

        // Update refreshed_at for the old handle
        let now = Utc::now();
        UserHandleRecord::update_refreshed_at(&pool, &hash_old, now).await?;

        // Now it should no longer need refresh
        let needing_refresh = UserHandleRecord::load_needing_refresh(&pool, threshold).await?;
        assert!(needing_refresh.is_empty());

        Ok(())
    }

    #[sqlx::test]
    async fn user_handle_record_delete(pool: SqlitePool) -> anyhow::Result<()> {
        let handle = UserHandle::new("ellie-03".to_owned())?;
        let hash = handle.calculate_hash()?;
        let signing_key = HandleSigningKey::generate()?;
        let record = UserHandleRecord::new(handle.clone(), hash, signing_key);
        record.store(&pool).await?;

        UserHandleRecord::delete(&pool, &handle).await?;
        let loaded_record = UserHandleRecord::load(&pool, &handle).await?;
        assert!(loaded_record.is_none());
        Ok(())
    }
}
