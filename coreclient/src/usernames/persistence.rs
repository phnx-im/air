// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::{
    codec::{BlobDecoded, BlobEncoded},
    credentials::keys::UsernameSigningKey,
    identifiers::{Username, UsernameHash},
};
use anyhow::bail;
use chrono::{DateTime, Utc};
use sqlx::{query, query_as, query_scalar};

use crate::{
    db::{
        access::{ReadConnection, WriteConnection, WriteDbTransaction},
        notification::DbEntityId,
    },
    job::operation::{Operation, OperationData},
    outbound_service::timed_tasks::{PARKED_AT, TimedTask, TimedTaskKind},
};

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

    pub(crate) async fn load(
        connection: impl ReadConnection,
        username: &Username,
    ) -> sqlx::Result<Option<Self>> {
        let record = Self::load_with_timestamps(connection, username).await?;
        Ok(record.map(|(record, ..)| record))
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

    pub(crate) async fn store(&self, connection: impl WriteConnection) -> sqlx::Result<()> {
        let created_at = Utc::now();
        self.store_with_timestamps(connection, created_at, created_at)
            .await
    }

    pub(super) async fn store_with_timestamps(
        &self,
        mut connection: impl WriteConnection,
        created_at: DateTime<Utc>,
        refreshed_at: DateTime<Utc>,
    ) -> sqlx::Result<()> {
        let signing_key = BlobEncoded(&self.signing_key);
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
        connection
            .notifier()
            .add(DbEntityId::Username(self.username.clone()));
        Ok(())
    }

    /// Loads a username with its creation and last refresh times.
    pub(crate) async fn load_with_timestamps(
        mut connection: impl ReadConnection,
        username: &Username,
    ) -> sqlx::Result<Option<(Self, DateTime<Utc>, DateTime<Utc>)>> {
        let record = query!(
            r#"
                SELECT
                    handle AS "username: Username",
                    hash AS "hash: UsernameHash",
                    signing_key AS "signing_key: BlobDecoded<UsernameSigningKey>",
                    created_at AS "created_at: DateTime<Utc>",
                    refreshed_at AS "refreshed_at: DateTime<Utc>"
                FROM user_handle
                WHERE handle = ?
            "#,
            username
        )
        .fetch_optional(connection.as_mut())
        .await?;
        Ok(record.map(|record| {
            let username_record = Self {
                username: record.username,
                hash: record.hash,
                signing_key: record.signing_key.into_inner(),
            };
            (username_record, record.created_at, record.refreshed_at)
        }))
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

    /// Moves `refreshed_at` forward to the given time, never back.
    pub(crate) async fn advance_refreshed_at(
        mut connection: impl WriteConnection,
        username: &Username,
        refreshed_at: DateTime<Utc>,
    ) -> sqlx::Result<()> {
        query!(
            r#"
                UPDATE user_handle
                SET refreshed_at = ?1
                WHERE handle = ?2 AND refreshed_at < ?1
            "#,
            refreshed_at,
            username,
        )
        .execute(connection.as_mut())
        .await?;
        Ok(())
    }

    /// Deletes the username together with its connection packages and its
    /// pending key.
    pub(crate) async fn delete(
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
        PendingKey::delete(&mut connection, username).await?;
        connection
            .notifier()
            .remove(DbEntityId::Username(username.clone()));
        Ok(())
    }
}

/// The signing key of a username registration that is still in progress.
#[derive(Debug)]
pub(crate) struct PendingKey {
    pub(super) signing_key: UsernameSigningKey,
    pub(super) origin: PendingKeyOrigin,
}

#[derive(Debug)]
pub(crate) enum PendingKeyOrigin {
    /// The key is new and the AS has not seen it.
    New,
    /// An earlier attempt stored the key and may have registered the username
    /// with it.
    Resumed,
}

impl PendingKey {
    /// Loads the pending key of `username`, or stores a new one.
    pub(super) async fn load_or_create(
        txn: &mut WriteDbTransaction<'_>,
        username: &Username,
    ) -> anyhow::Result<Self> {
        let task_id = TimedTask::pending_username_id(username);
        if let Some(task) = Operation::<TimedTask>::load(&mut *txn, &task_id).await? {
            let TimedTaskKind::PendingUsername { signing_key, .. } = task.data.kind else {
                bail!("unexpected pending username task: {:?}", task.data.kind);
            };
            return Ok(Self {
                signing_key,
                origin: PendingKeyOrigin::Resumed,
            });
        }

        let signing_key = UsernameSigningKey::generate()?;
        TimedTask::new(TimedTaskKind::PendingUsername {
            username: username.clone(),
            signing_key: signing_key.clone(),
        })
        .into_operation()
        .schedule_at(PARKED_AT)
        .enqueue(txn)
        .await?;
        Ok(Self {
            signing_key,
            origin: PendingKeyOrigin::New,
        })
    }

    pub(super) async fn delete(
        connection: impl WriteConnection,
        username: &Username,
    ) -> sqlx::Result<()> {
        let task_id = TimedTask::pending_username_id(username);
        Operation::<TimedTask>::delete_by_id(connection, &task_id).await
    }
}

#[cfg(test)]
mod test {
    use std::assert_matches;

    use sqlx::SqlitePool;
    use uuid::Uuid;

    use crate::db::access::DbAccess;

    use super::*;

    #[sqlx::test]
    async fn user_handle_record_store_load(pool: SqlitePool) -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(pool);
        let username = Username::new("ellie-03".to_owned())?;
        let hash = username.calculate_hash()?;
        let signing_key = UsernameSigningKey::generate()?;
        let record = UsernameRecord::new(username.clone(), hash, signing_key);
        record.store(pool.write().await?).await?;

        let loaded_record = UsernameRecord::load(pool.read().await?, &username)
            .await?
            .unwrap();
        assert_eq!(loaded_record, record);
        Ok(())
    }

    #[sqlx::test]
    async fn user_handle_record_load_all(pool: SqlitePool) -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(pool);
        let username1 = Username::new("ellie-03".to_owned())?;
        let hash1 = username1.calculate_hash()?;
        let signing_key1 = UsernameSigningKey::generate()?;
        let record1 = UsernameRecord::new(username1.clone(), hash1, signing_key1);
        record1.store(pool.write().await?).await?;

        let username2 = Username::new("joel-03".to_owned())?;
        let hash2 = username2.calculate_hash()?;
        let signing_key2 = UsernameSigningKey::generate()?;
        let record2 = UsernameRecord::new(username2.clone(), hash2, signing_key2);
        record2.store(pool.write().await?).await?;

        let loaded_records = UsernameRecord::load_all(pool.read().await?).await?;
        assert_eq!(loaded_records.len(), 2);
        assert!(loaded_records.contains(&record1));
        assert!(loaded_records.contains(&record2));
        Ok(())
    }

    #[sqlx::test]
    async fn username_record_load_all_usernames(pool: SqlitePool) -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(pool);
        let username1 = Username::new("ellie-03".to_owned())?;
        let hash1 = username1.calculate_hash()?;
        let signing_key1 = UsernameSigningKey::generate()?;
        let record1 = UsernameRecord::new(username1.clone(), hash1, signing_key1);
        record1.store(pool.write().await?).await?;

        let username2 = Username::new("joel-03".to_owned())?;
        let hash2 = username2.calculate_hash()?;
        let signing_key2 = UsernameSigningKey::generate()?;
        let record2 = UsernameRecord::new(username2.clone(), hash2, signing_key2);
        record2.store(pool.write().await?).await?;

        let loaded_usernames = UsernameRecord::load_all_usernames(pool.read().await?).await?;
        assert_eq!(loaded_usernames.len(), 2);
        assert!(loaded_usernames.contains(&username1));
        assert!(loaded_usernames.contains(&username2));
        Ok(())
    }

    #[sqlx::test]
    async fn user_handle_record_load_needing_refresh(pool: SqlitePool) -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(pool);
        use chrono::Duration;

        // Create a username with old refreshed_at (> 90 days ago)
        let username_old = Username::new("old-handle".to_owned())?;
        let hash_old = username_old.calculate_hash()?;
        let signing_key_old = UsernameSigningKey::generate()?;
        let record_old = UsernameRecord::new(username_old.clone(), hash_old, signing_key_old);
        record_old.store(pool.write().await?).await?;

        // Manually set refreshed_at to 100 days ago
        let old_time = Utc::now() - Duration::days(100);
        sqlx::query("UPDATE user_handle SET refreshed_at = ? WHERE handle = ?")
            .bind(old_time)
            .bind(&username_old)
            .execute(pool.read().await?.as_mut())
            .await?;

        // Create a username with recent refreshed_at
        let username_new = Username::new("new-handle".to_owned())?;
        let hash_new = username_new.calculate_hash()?;
        let signing_key_new = UsernameSigningKey::generate()?;
        let record_new = UsernameRecord::new(username_new.clone(), hash_new, signing_key_new);
        record_new.store(pool.write().await?).await?;

        // Query usernames needing refresh (threshold = now - 90 days)
        let threshold = Utc::now() - Duration::days(90);
        let needing_refresh =
            UsernameRecord::load_needing_refresh(pool.read().await?, threshold).await?;
        assert_eq!(needing_refresh.len(), 1);
        assert_eq!(needing_refresh[0].username, username_old);

        // Update refreshed_at for the old username
        let now = Utc::now();
        UsernameRecord::update_refreshed_at(pool.write().await?, &hash_old, now).await?;

        // Now it should no longer need refresh
        let needing_refresh =
            UsernameRecord::load_needing_refresh(pool.read().await?, threshold).await?;
        assert!(needing_refresh.is_empty());

        Ok(())
    }

    #[sqlx::test]
    async fn user_handle_record_delete(pool: SqlitePool) -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(pool);
        let username = Username::new("ellie-03".to_owned())?;
        let hash = username.calculate_hash()?;
        let signing_key = UsernameSigningKey::generate()?;
        let record = UsernameRecord::new(username.clone(), hash, signing_key);
        record.store(pool.write().await?).await?;

        UsernameRecord::delete(pool.write().await?, &username).await?;
        let loaded_record = UsernameRecord::load(pool.read().await?, &username).await?;
        assert!(loaded_record.is_none());
        Ok(())
    }

    async fn load_or_create_pending_key(
        pool: &DbAccess,
        username: &Username,
    ) -> anyhow::Result<PendingKey> {
        pool.with_write_transaction(async |txn| PendingKey::load_or_create(txn, username).await)
            .await
    }

    #[sqlx::test]
    async fn pending_key_is_resumed_until_deleted(pool: SqlitePool) -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(pool);
        let username = Username::new("ellie-03".to_owned())?;

        let first = load_or_create_pending_key(&pool, &username).await?;
        assert_matches!(first.origin, PendingKeyOrigin::New);
        let resumed = load_or_create_pending_key(&pool, &username).await?;
        assert_matches!(resumed.origin, PendingKeyOrigin::Resumed);
        assert_eq!(
            resumed.signing_key.verifying_key(),
            first.signing_key.verifying_key()
        );

        PendingKey::delete(pool.write().await?, &username).await?;
        let fresh = load_or_create_pending_key(&pool, &username).await?;
        assert_matches!(fresh.origin, PendingKeyOrigin::New);
        assert_ne!(
            fresh.signing_key.verifying_key(),
            first.signing_key.verifying_key()
        );
        Ok(())
    }

    #[sqlx::test]
    async fn a_deleted_username_takes_its_pending_key_along(
        pool: SqlitePool,
    ) -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(pool);
        let username = Username::new("ellie-03".to_owned())?;
        let signing_key = load_or_create_pending_key(&pool, &username)
            .await?
            .signing_key;
        UsernameRecord::new(username.clone(), username.calculate_hash()?, signing_key)
            .store(pool.write().await?)
            .await?;

        UsernameRecord::delete(pool.write().await?, &username).await?;
        let fresh = load_or_create_pending_key(&pool, &username).await?;
        assert_matches!(fresh.origin, PendingKeyOrigin::New);
        Ok(())
    }

    async fn dequeue_timed_task(pool: &DbAccess) -> anyhow::Result<Option<TimedTaskKind>> {
        let task = pool
            .with_write_transaction(async |txn| {
                Operation::<TimedTask>::dequeue(txn, Uuid::new_v4(), Utc::now()).await
            })
            .await?;
        Ok(task.map(|task| task.data.kind))
    }

    #[sqlx::test]
    async fn pending_key_is_due_only_once_scheduled(pool: SqlitePool) -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(pool);
        let username = Username::new("ellie-03".to_owned())?;
        let signing_key = load_or_create_pending_key(&pool, &username)
            .await?
            .signing_key;
        assert_eq!(dequeue_timed_task(&pool).await?, None);

        let kind = TimedTaskKind::PendingUsername {
            username,
            signing_key,
        };
        TimedTask::schedule_now(pool.write().await?, kind.clone()).await?;
        assert_eq!(dequeue_timed_task(&pool).await?, Some(kind));
        Ok(())
    }
}
