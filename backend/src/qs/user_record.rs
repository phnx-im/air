// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::{
    crypto::signatures::keys::QsUserVerifyingKey, identifiers::QsUserId, messages::FriendshipToken,
};
use metrics::gauge;
use sqlx::PgExecutor;

use crate::{
    errors::StorageError,
    qs::{
        METRIC_AIR_QS_DAU_USERS, METRIC_AIR_QS_MAU_USERS, METRIC_AIR_QS_TOTAL_USERS,
        METRIC_AIR_QS_WAU_USERS,
    },
};

#[derive(Debug, PartialEq)]
pub(super) struct UserRecord {
    pub(super) user_id: QsUserId,
    pub(super) verifying_key: QsUserVerifyingKey,
    pub(super) friendship_token: FriendshipToken,
}

impl UserRecord {
    pub(in crate::qs) async fn new_and_store(
        connection: impl PgExecutor<'_>,
        verifying_key: QsUserVerifyingKey,
        friendship_token: FriendshipToken,
    ) -> Result<Self, StorageError> {
        let user_id = QsUserId::random();
        let user_record = Self {
            user_id,
            verifying_key,
            friendship_token,
        };
        user_record.store(connection).await?;
        Ok(user_record)
    }
}

pub(crate) struct UserMetrics {
    /// Total number of users
    pub(crate) total_users: Option<i64>,
    /// Number of users connected in the last 30 days
    pub(crate) active_last_month_users: Option<i64>,
    /// Number of users connected in the last 7 days
    pub(crate) active_last_week_users: Option<i64>,
    /// Number of users connected in the last 24 hours
    pub(crate) active_last_day_users: Option<i64>,
}

impl UserMetrics {
    pub(crate) fn report(&self) {
        gauge!(METRIC_AIR_QS_TOTAL_USERS).set(self.total_users.unwrap_or(0) as i32);
        gauge!(METRIC_AIR_QS_MAU_USERS).set(self.active_last_month_users.unwrap_or(0) as i32);
        gauge!(METRIC_AIR_QS_WAU_USERS).set(self.active_last_week_users.unwrap_or(0) as i32);
        gauge!(METRIC_AIR_QS_DAU_USERS).set(self.active_last_day_users.unwrap_or(0) as i32);
    }
}

pub(crate) mod persistence {
    use aircommon::identifiers::{QsClientId, QsUserId};
    use sqlx::{PgConnection, PgExecutor, PgPool, query, query_as, query_scalar};

    use crate::errors::StorageError;

    use super::*;

    impl UserRecord {
        pub(super) async fn store(
            &self,
            connection: impl PgExecutor<'_>,
        ) -> Result<(), StorageError> {
            sqlx::query!(
                "INSERT INTO
                    qs_user_record
                    (user_id, verifying_key, friendship_token)
                VALUES
                    ($1, $2, $3)",
                &self.user_id as &QsUserId,
                &self.verifying_key as &QsUserVerifyingKey,
                &self.friendship_token as &FriendshipToken,
            )
            .execute(connection)
            .await?;
            Ok(())
        }

        #[cfg(test)]
        pub(in crate::qs) async fn load(
            connection: impl PgExecutor<'_>,
            user_id: &QsUserId,
        ) -> Result<Option<UserRecord>, StorageError> {
            sqlx::query!(
                r#"SELECT
                    verifying_key as "verifying_key: QsUserVerifyingKey",
                    friendship_token as "friendship_token: FriendshipToken"
                FROM
                    qs_user_record
                WHERE
                    user_id = $1
                    AND deleted_at IS NULL"#,
                user_id.as_uuid(),
            )
            .fetch_optional(connection)
            .await?
            .map(|record| {
                Ok(UserRecord {
                    user_id: *user_id,
                    verifying_key: record.verifying_key,
                    friendship_token: record.friendship_token,
                })
            })
            .transpose()
        }

        pub(in crate::qs) async fn load_verifying_key(
            connection: impl PgExecutor<'_>,
            user_id: &QsUserId,
        ) -> Result<Option<QsUserVerifyingKey>, StorageError> {
            sqlx::query_scalar!(
                r#"SELECT
                    verifying_key as "verifying_key: QsUserVerifyingKey"
                FROM
                    qs_user_record
                WHERE
                    user_id = $1
                    AND deleted_at IS NULL"#,
                user_id.as_uuid(),
            )
            .fetch_optional(connection)
            .await
            .map_err(From::from)
        }

        /// Locks the record of an active user with `FOR SHARE`. Returns `false`
        /// if there is no such record.
        ///
        /// Writers of the user's data take this lock before locking any other
        /// row of the user, so that they don't interleave with
        /// [`Self::soft_delete`].
        pub(in crate::qs) async fn lock_active(
            connection: impl PgExecutor<'_>,
            user_id: QsUserId,
        ) -> sqlx::Result<bool> {
            let row = query_scalar!(
                r#"SELECT 1 AS "locked!" FROM qs_user_record
                WHERE user_id = $1 AND deleted_at IS NULL
                FOR SHARE"#,
                &user_id as &QsUserId,
            )
            .fetch_optional(connection)
            .await?;
            Ok(row.is_some())
        }

        /// Like [`Self::lock_active`], but for the user owning an active client
        /// record. Returns the user's id.
        pub(in crate::qs) async fn lock_active_owner(
            connection: impl PgExecutor<'_>,
            client_id: &QsClientId,
        ) -> sqlx::Result<Option<QsUserId>> {
            query_scalar!(
                r#"SELECT u.user_id AS "user_id: QsUserId"
                FROM qs_user_record u
                WHERE u.user_id = (
                        SELECT c.user_id FROM qs_client_record c
                        WHERE c.client_id = $1 AND c.deleted_at IS NULL
                    )
                    AND u.deleted_at IS NULL
                FOR SHARE OF u"#,
                client_id.as_uuid(),
            )
            .fetch_optional(connection)
            .await
        }

        /// Marks the user as deleted and removes KeyPackages and push tokens.  Does nothing if the
        /// user is already deleted or does not exist.
        pub(in crate::qs) async fn soft_delete(
            pool: &PgPool,
            user_id: QsUserId,
        ) -> Result<(), StorageError> {
            let mut txn = pool.begin().await?;
            Self::soft_delete_in(&mut txn, user_id).await?;
            txn.commit().await?;
            Ok(())
        }

        /// [`Self::soft_delete`] inside the given transaction.
        pub(in crate::qs) async fn soft_delete_in(
            connection: &mut PgConnection,
            user_id: QsUserId,
        ) -> Result<(), StorageError> {
            let is_locked = query_scalar!(
                r#"SELECT 1 AS "locked!" FROM qs_user_record
                WHERE user_id = $1 AND deleted_at IS NULL
                FOR UPDATE"#,
                &user_id as &QsUserId,
            )
            .fetch_optional(&mut *connection)
            .await?;
            if is_locked.is_none() {
                return Ok(());
            }
            query!(
                "UPDATE qs_user_record SET deleted_at = NOW() WHERE user_id = $1",
                &user_id as &QsUserId,
            )
            .execute(&mut *connection)
            .await?;
            query!(
                "DELETE FROM key_package WHERE user_id = $1",
                &user_id as &QsUserId,
            )
            .execute(&mut *connection)
            .await?;
            query!(
                "DELETE FROM apq_key_package WHERE user_id = $1",
                &user_id as &QsUserId,
            )
            .execute(&mut *connection)
            .await?;
            query!(
                "DELETE FROM qs_staged_key_package_batch WHERE user_id = $1",
                &user_id as &QsUserId,
            )
            .execute(&mut *connection)
            .await?;
            query!(
                "UPDATE qs_client_record SET encrypted_push_token = NULL WHERE user_id = $1",
                &user_id as &QsUserId,
            )
            .execute(&mut *connection)
            .await?;
            Ok(())
        }

        /// Updates the record of an active user. Returns `false` if there is no
        /// such record.
        pub(in crate::qs) async fn update(
            &self,
            connection: impl PgExecutor<'_>,
        ) -> Result<bool, StorageError> {
            let result = sqlx::query!(
                "UPDATE
                    qs_user_record
                SET
                    verifying_key = $2, friendship_token = $3
                WHERE
                    user_id = $1
                    AND deleted_at IS NULL",
                &self.user_id as &QsUserId,
                &self.verifying_key as &QsUserVerifyingKey,
                self.friendship_token.token(),
            )
            .execute(connection)
            .await?;
            Ok(result.rows_affected() > 0)
        }

        pub(in crate::qs) async fn metrics(
            connection: impl PgExecutor<'_>,
        ) -> sqlx::Result<UserMetrics> {
            query_as!(
                UserMetrics,
                "WITH last_activity_time AS (
                    SELECT MAX(c.activity_time) AS last_activity_time
                    FROM qs_client_record c
                    JOIN qs_user_record u ON u.user_id = c.user_id
                    WHERE u.deleted_at IS NULL
                    GROUP BY c.user_id
                )
                SELECT
                    (
                        SELECT COUNT(user_id) FROM qs_user_record
                        WHERE deleted_at IS NULL
                    ) AS total_users,
                    COUNT(last_activity_time) FILTER (
                        WHERE last_activity_time >= (NOW() - INTERVAL '1 month')
                    ) AS active_last_month_users,
                    COUNT(last_activity_time) FILTER (
                        WHERE last_activity_time >= (NOW() - INTERVAL '1 week')
                    ) AS active_last_week_users,
                    COUNT(last_activity_time) FILTER (
                        WHERE last_activity_time >= (NOW() - INTERVAL '1 day')
                    ) AS active_last_day_users
                FROM last_activity_time"
            )
            .fetch_one(connection)
            .await
        }
    }

    #[cfg(test)]
    pub(crate) mod tests {
        use chrono::{Duration, Utc};
        use sqlx::{AssertSqlSafe, PgPool};

        use crate::qs::{
            client_record::{QsClientRecord, persistence::tests::store_random_client_record},
            queue::tests::{enqueue_test_messages, queue_len},
        };

        use super::*;

        pub(crate) async fn store_random_user_record(pool: &PgPool) -> anyhow::Result<UserRecord> {
            let record = UserRecord {
                user_id: QsUserId::random(),
                verifying_key: QsUserVerifyingKey::new_for_test(b"some_key".to_vec()),
                friendship_token: FriendshipToken::random().unwrap(),
            };
            record.store(pool).await?;
            Ok(record)
        }

        #[sqlx::test]
        async fn load(pool: PgPool) -> anyhow::Result<()> {
            let user_record = store_random_user_record(&pool).await?;

            let loaded = UserRecord::load(&pool, &user_record.user_id)
                .await?
                .expect("missing user record");
            assert_eq!(loaded, user_record);

            let verifying_key = UserRecord::load_verifying_key(&pool, &user_record.user_id)
                .await?
                .expect("missing user verifying key");
            assert_eq!(verifying_key, user_record.verifying_key);

            Ok(())
        }

        #[sqlx::test]
        async fn update(pool: PgPool) -> anyhow::Result<()> {
            let user_record = store_random_user_record(&pool).await?;

            let loaded = UserRecord::load(&pool, &user_record.user_id)
                .await?
                .expect("missing user record");
            assert_eq!(loaded, user_record);

            let user_record = UserRecord {
                user_id: user_record.user_id,
                verifying_key: QsUserVerifyingKey::new_for_test(b"some_other_key".to_vec()),
                friendship_token: FriendshipToken::random().unwrap(),
            };

            assert!(user_record.update(&pool).await?);
            let loaded = UserRecord::load(&pool, &user_record.user_id)
                .await?
                .expect("missing user record");
            assert_eq!(loaded, user_record);

            Ok(())
        }

        /// Waits until a statement on the test database waits for a lock.
        async fn wait_for_lock_waiter(pool: &PgPool) -> anyhow::Result<()> {
            tokio::time::timeout(std::time::Duration::from_secs(10), async {
                loop {
                    let is_waiting: bool = sqlx::query_scalar(
                        "SELECT EXISTS (
                            SELECT 1 FROM pg_stat_activity
                            WHERE datname = current_database() AND wait_event_type = 'Lock'
                        )",
                    )
                    .fetch_one(pool)
                    .await?;
                    if is_waiting {
                        return Ok(());
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                }
            })
            .await?
        }

        /// Runs `operation` while the soft delete of the user is uncommitted,
        /// and commits the soft delete once `operation` waits for a lock.
        pub(crate) async fn run_during_soft_delete<T: Send + 'static>(
            pool: &PgPool,
            user_id: QsUserId,
            operation: impl Future<Output = T> + Send + 'static,
        ) -> anyhow::Result<T> {
            let mut txn = pool.begin().await?;
            UserRecord::soft_delete_in(&mut txn, user_id).await?;
            let operation = tokio::spawn(operation);
            wait_for_lock_waiter(pool).await?;
            txn.commit().await?;
            Ok(operation.await?)
        }

        pub(crate) async fn count_rows(
            pool: &PgPool,
            table: &str,
            user_id: QsUserId,
        ) -> sqlx::Result<i64> {
            sqlx::query_scalar(AssertSqlSafe(format!(
                "SELECT COUNT(*) FROM {table} WHERE user_id = $1"
            )))
            .bind(user_id)
            .fetch_one(pool)
            .await
        }

        #[sqlx::test]
        async fn soft_delete(pool: PgPool) -> anyhow::Result<()> {
            let user_record = store_random_user_record(&pool).await?;
            let client_a = store_random_client_record(&pool, user_record.user_id).await?;
            let client_b = store_random_client_record(&pool, user_record.user_id).await?;
            let other_user = store_random_user_record(&pool).await?;
            let other_client = store_random_client_record(&pool, other_user.user_id).await?;

            for user_id in [user_record.user_id, other_user.user_id] {
                for table in ["key_package", "apq_key_package"] {
                    sqlx::query(AssertSqlSafe(format!(
                        "INSERT INTO {table} (user_id, key_package, is_last_resort)
                        VALUES ($1, '\\x00', FALSE), ($1, '\\x01', TRUE)"
                    )))
                    .bind(user_id)
                    .execute(&pool)
                    .await?;
                }
                sqlx::query(
                    "INSERT INTO qs_staged_key_package_batch
                        (user_id, epoch_id, leaf_index, generation)
                    VALUES ($1, '\\x00', 0, 0)",
                )
                .bind(user_id)
                .execute(&pool)
                .await?;
            }

            UserRecord::soft_delete(&pool, user_record.user_id).await?;

            assert_eq!(UserRecord::load(&pool, &user_record.user_id).await?, None);
            assert_eq!(
                UserRecord::load_verifying_key(&pool, &user_record.user_id).await?,
                None
            );

            // The client records keep their keys and lose their push tokens.
            for client in [client_a, client_b] {
                let loaded = QsClientRecord::load(&pool, &client.client_id)
                    .await?
                    .expect("missing client record");
                assert_eq!(
                    loaded,
                    QsClientRecord {
                        encrypted_push_token: None,
                        ..client
                    }
                );
            }

            for table in [
                "key_package",
                "apq_key_package",
                "qs_staged_key_package_batch",
            ] {
                assert_eq!(count_rows(&pool, table, user_record.user_id).await?, 0);
            }

            // Other users are not affected.
            let other_user_id = other_user.user_id;
            assert_eq!(
                UserRecord::load(&pool, &other_user_id).await?,
                Some(other_user)
            );
            assert_eq!(
                QsClientRecord::load(&pool, &other_client.client_id).await?,
                Some(other_client)
            );
            assert_eq!(count_rows(&pool, "key_package", other_user_id).await?, 2);
            assert_eq!(
                count_rows(&pool, "apq_key_package", other_user_id).await?,
                2
            );
            assert_eq!(
                count_rows(&pool, "qs_staged_key_package_batch", other_user_id).await?,
                1
            );

            Ok(())
        }

        #[sqlx::test]
        async fn update_waits_for_soft_delete(pool: PgPool) -> anyhow::Result<()> {
            let user = store_random_user_record(&pool).await?;
            let user_id = user.user_id;
            let updated = UserRecord {
                user_id,
                verifying_key: QsUserVerifyingKey::new_for_test(b"some_other_key".to_vec()),
                friendship_token: FriendshipToken::random().unwrap(),
            };

            let update_pool = pool.clone();
            let is_updated = run_during_soft_delete(&pool, user_id, async move {
                updated.update(&update_pool).await
            })
            .await??;

            assert!(!is_updated);
            let verifying_key: QsUserVerifyingKey =
                sqlx::query_scalar("SELECT verifying_key FROM qs_user_record WHERE user_id = $1")
                    .bind(user_id)
                    .fetch_one(&pool)
                    .await?;
            assert_eq!(verifying_key, user.verifying_key);

            Ok(())
        }

        #[sqlx::test]
        async fn revert_migration_removes_deleted_users(pool: PgPool) -> anyhow::Result<()> {
            let deleted_user = store_random_user_record(&pool).await?;
            let pending_client = store_random_client_record(&pool, deleted_user.user_id).await?;
            enqueue_test_messages(&pool, pending_client.client_id, 1).await?;
            UserRecord::soft_delete(&pool, deleted_user.user_id).await?;
            let user = store_random_user_record(&pool).await?;
            let client = store_random_client_record(&pool, user.user_id).await?;
            enqueue_test_messages(&pool, client.client_id, 1).await?;

            sqlx::migrate!("./migrations")
                .undo(&pool, 20260929120000 - 1)
                .await?;

            assert_eq!(
                count_rows(&pool, "qs_user_record", deleted_user.user_id).await?,
                0
            );
            assert_eq!(
                count_rows(&pool, "qs_client_record", deleted_user.user_id).await?,
                0
            );
            assert_eq!(queue_len(&pool, pending_client.client_id).await?, 0);
            assert_eq!(count_rows(&pool, "qs_user_record", user.user_id).await?, 1);
            assert_eq!(
                count_rows(&pool, "qs_client_record", user.user_id).await?,
                1
            );
            assert_eq!(queue_len(&pool, client.client_id).await?, 1);

            Ok(())
        }

        #[sqlx::test]
        async fn metrics(pool: PgPool) -> anyhow::Result<()> {
            // active user right now
            let user_record_1 = store_random_user_record(&pool).await?;
            let _client_record_1 = store_random_client_record(&pool, user_record_1.user_id).await?;

            // active user in the last month
            let user_record_2 = store_random_user_record(&pool).await?;
            let client_record_2 = store_random_client_record(&pool, user_record_2.user_id).await?;
            QsClientRecord::update_activity_time(
                &pool,
                client_record_2.client_id,
                (Utc::now() - Duration::days(2)).into(),
            )
            .await?;

            // active user more than a month ago
            let user_record_3 = store_random_user_record(&pool).await?;
            let client_record_3 = store_random_client_record(&pool, user_record_3.user_id).await?;
            QsClientRecord::update_activity_time(
                &pool,
                client_record_3.client_id,
                (Utc::now() - Duration::days(31)).into(),
            )
            .await?;

            // deleted user active right now
            let user_record_4 = store_random_user_record(&pool).await?;
            let _client_record_4 = store_random_client_record(&pool, user_record_4.user_id).await?;
            UserRecord::soft_delete(&pool, user_record_4.user_id).await?;

            let metrics = UserRecord::metrics(&pool).await?;
            assert_eq!(metrics.total_users, Some(3));
            assert_eq!(metrics.active_last_month_users, Some(2));
            assert_eq!(metrics.active_last_day_users, Some(1));

            Ok(())
        }
    }
}
