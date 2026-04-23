// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::{
    codec::{BlobDecoded, BlobEncoded},
    credentials::keys::UsernameSigningKey,
    identifiers::{Username, UsernameHash},
};
use chrono::{DateTime, Utc};
use sqlx::{query, query_as, query_scalar};

use crate::db_access::{ReadConnection, WriteConnection};

/// A username record stored in the client database.
///
/// Contains additional information about the username, such as hash and signing key.
#[derive(Debug, Clone)]
pub struct UsernameRecord {
    pub username: Username,
    pub hash: UsernameHash,
    pub signing_key: UsernameSigningKey,
}

#[cfg(test)]
impl PartialEq for UsernameRecord {
    fn eq(&self, other: &Self) -> bool {
        // Note: only the verifying key part of the signing key is compared.
        self.username == other.username
            && self.hash == other.hash
            && self.signing_key.verifying_key() == other.signing_key.verifying_key()
    }
}

struct SqlUsernameRecord {
    username: Username,
    hash: UsernameHash,
    signing_key: BlobDecoded<UsernameSigningKey>,
}

impl From<SqlUsernameRecord> for UsernameRecord {
    fn from(record: SqlUsernameRecord) -> Self {
        Self {
            username: record.username,
            hash: record.hash,
            signing_key: record.signing_key.into_inner(),
        }
    }
}

impl UsernameRecord {
    pub fn new(username: Username, hash: UsernameHash, signing_key: UsernameSigningKey) -> Self {
        Self {
            username,
            hash,
            signing_key,
        }
    }

    pub(super) async fn load(
        mut connection: impl ReadConnection,
        username: &Username,
    ) -> sqlx::Result<Option<Self>> {
        let record = query_as!(
            SqlUsernameRecord,
            r#"
                SELECT
                    handle AS "username: _",
                    hash AS "hash: _",
                    signing_key AS "signing_key: _"
                FROM user_handle
                WHERE handle = ?
            "#,
            username
        )
        .fetch_optional(connection.as_mut())
        .await?;
        Ok(record.map(From::from))
    }

    pub(crate) async fn load_all(mut connection: impl ReadConnection) -> sqlx::Result<Vec<Self>> {
        let records = query_as!(
            SqlUsernameRecord,
            r#"
                SELECT
                    handle AS "username: _",
                    hash AS "hash: _",
                    signing_key AS "signing_key: _"
                FROM user_handle
                ORDER BY created_at ASC
            "#,
        )
        .fetch_all(connection.as_mut())
        .await?;
        Ok(records.into_iter().map(From::from).collect())
    }

    pub(crate) async fn load_all_usernames(
        mut connection: impl ReadConnection,
    ) -> sqlx::Result<Vec<Username>> {
        query_scalar!(
            r#"
                SELECT handle AS "username: _"
                FROM user_handle
                ORDER BY created_at ASC
            "#
        )
        .fetch_all(connection.as_mut())
        .await
    }

    pub(super) async fn store(&self, mut connection: impl WriteConnection) -> sqlx::Result<()> {
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
            self.username,
            self.hash,
            signing_key,
            created_at,
            refreshed_at,
        )
        .execute(connection.as_mut())
        .await?;
        Ok(())
    }

    /// Load usernames where `refreshed_at` is older than the given threshold.
    pub(crate) async fn load_needing_refresh(
        mut connection: impl ReadConnection,
        threshold: DateTime<Utc>,
    ) -> sqlx::Result<Vec<Self>> {
        let records = query_as!(
            SqlUsernameRecord,
            r#"
                SELECT
                    handle AS "username: _",
                    hash AS "hash: _",
                    signing_key AS "signing_key: _"
                FROM user_handle
                WHERE refreshed_at < ?
            "#,
            threshold
        )
        .fetch_all(connection.as_mut())
        .await?;
        Ok(records.into_iter().map(From::from).collect())
    }

    /// Update `refreshed_at` for a username identified by its hash.
    pub(crate) async fn update_refreshed_at(
        mut connection: impl WriteConnection,
        hash: &UsernameHash,
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
        .execute(connection.as_mut())
        .await?;
        Ok(())
    }

    pub(super) async fn delete(
        mut connection: impl WriteConnection,
        username: &Username,
    ) -> sqlx::Result<()> {
        query!(
            r#"
                DELETE FROM user_handle
                WHERE handle = ?
            "#,
            username,
        )
        .execute(connection.as_mut())
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
        let username = Username::new("ellie-03".to_owned())?;
        let hash = username.calculate_hash()?;
        let signing_key = UsernameSigningKey::generate()?;
        let record = UsernameRecord::new(username.clone(), hash, signing_key);
        record.store(&pool).await?;

        let loaded_record = UsernameRecord::load(&pool, &username).await?.unwrap();
        assert_eq!(loaded_record, record);
        Ok(())
    }

    #[sqlx::test]
    async fn user_handle_record_load_all(pool: SqlitePool) -> anyhow::Result<()> {
        let username1 = Username::new("ellie-03".to_owned())?;
        let hash1 = username1.calculate_hash()?;
        let signing_key1 = UsernameSigningKey::generate()?;
        let record1 = UsernameRecord::new(username1.clone(), hash1, signing_key1);
        record1.store(&pool).await?;

        let username2 = Username::new("joel-03".to_owned())?;
        let hash2 = username2.calculate_hash()?;
        let signing_key2 = UsernameSigningKey::generate()?;
        let record2 = UsernameRecord::new(username2.clone(), hash2, signing_key2);
        record2.store(&pool).await?;

        let loaded_records = UsernameRecord::load_all(&pool).await?;
        assert_eq!(loaded_records.len(), 2);
        assert!(loaded_records.contains(&record1));
        assert!(loaded_records.contains(&record2));
        Ok(())
    }

    #[sqlx::test]
    async fn username_record_load_all_usernames(pool: SqlitePool) -> anyhow::Result<()> {
        let username1 = Username::new("ellie-03".to_owned())?;
        let hash1 = username1.calculate_hash()?;
        let signing_key1 = UsernameSigningKey::generate()?;
        let record1 = UsernameRecord::new(username1.clone(), hash1, signing_key1);
        record1.store(&pool).await?;

        let username2 = Username::new("joel-03".to_owned())?;
        let hash2 = username2.calculate_hash()?;
        let signing_key2 = UsernameSigningKey::generate()?;
        let record2 = UsernameRecord::new(username2.clone(), hash2, signing_key2);
        record2.store(&pool).await?;

        let loaded_usernames = UsernameRecord::load_all_usernames(&pool).await?;
        assert_eq!(loaded_usernames.len(), 2);
        assert!(loaded_usernames.contains(&username1));
        assert!(loaded_usernames.contains(&username2));
        Ok(())
    }

    #[sqlx::test]
    async fn user_handle_record_load_needing_refresh(pool: SqlitePool) -> anyhow::Result<()> {
        use chrono::Duration;

        // Create a username with old refreshed_at (> 90 days ago)
        let username_old = Username::new("old-handle".to_owned())?;
        let hash_old = username_old.calculate_hash()?;
        let signing_key_old = UsernameSigningKey::generate()?;
        let record_old = UsernameRecord::new(username_old.clone(), hash_old, signing_key_old);
        record_old.store(&pool).await?;

        // Manually set refreshed_at to 100 days ago
        let old_time = Utc::now() - Duration::days(100);
        sqlx::query("UPDATE user_handle SET refreshed_at = ? WHERE handle = ?")
            .bind(old_time)
            .bind(&username_old)
            .execute(&pool)
            .await?;

        // Create a username with recent refreshed_at
        let username_new = Username::new("new-handle".to_owned())?;
        let hash_new = username_new.calculate_hash()?;
        let signing_key_new = UsernameSigningKey::generate()?;
        let record_new = UsernameRecord::new(username_new.clone(), hash_new, signing_key_new);
        record_new.store(&pool).await?;

        // Query usernames needing refresh (threshold = now - 90 days)
        let threshold = Utc::now() - Duration::days(90);
        let needing_refresh = UsernameRecord::load_needing_refresh(&pool, threshold).await?;
        assert_eq!(needing_refresh.len(), 1);
        assert_eq!(needing_refresh[0].username, username_old);

        // Update refreshed_at for the old username
        let now = Utc::now();
        UsernameRecord::update_refreshed_at(&pool, &hash_old, now).await?;

        // Now it should no longer need refresh
        let needing_refresh = UsernameRecord::load_needing_refresh(&pool, threshold).await?;
        assert!(needing_refresh.is_empty());

        Ok(())
    }

    #[sqlx::test]
    async fn user_handle_record_delete(pool: SqlitePool) -> anyhow::Result<()> {
        let username = Username::new("ellie-03".to_owned())?;
        let hash = username.calculate_hash()?;
        let signing_key = UsernameSigningKey::generate()?;
        let record = UsernameRecord::new(username.clone(), hash, signing_key);
        record.store(&pool).await?;

        UsernameRecord::delete(&pool, &username).await?;
        let loaded_record = UsernameRecord::load(&pool, &username).await?;
        assert!(loaded_record.is_none());
        Ok(())
    }
}
