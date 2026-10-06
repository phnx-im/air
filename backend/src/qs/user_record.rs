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
    use aircommon::identifiers::QsUserId;
    use sqlx::{Acquire, PgConnection, PgExecutor, query, query_as, query_scalar};

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

        /// Locks the record of a deleted user. Returns `false` if there is no
        /// such record.
        pub(in crate::qs) async fn lock_deleted(
            connection: impl PgExecutor<'_>,
            user_id: QsUserId,
        ) -> Result<bool, StorageError> {
            let row = query_scalar!(
                r#"SELECT 1 AS "locked!" FROM qs_user_record
                WHERE user_id = $1 AND deleted_at IS NOT NULL
                FOR UPDATE"#,
                &user_id as &QsUserId,
            )
            .fetch_optional(connection)
            .await?;
            Ok(row.is_some())
        }

        /// Marks the user as deleted.
        ///
        /// The client records with a non-empty queue stay, so that the user's
        /// clients can still read their queues. The others are removed right
        /// away, see [`Self::delete_drained`]. The user's key packages and
        /// push tokens are removed.
        pub(in crate::qs) async fn soft_delete(
            connection: &mut PgConnection,
            user_id: QsUserId,
        ) -> Result<(), StorageError> {
            let mut txn = connection.begin().await?;
            query!(
                "UPDATE qs_user_record SET deleted_at = NOW()
                WHERE user_id = $1 AND deleted_at IS NULL",
                &user_id as &QsUserId,
            )
            .execute(&mut *txn)
            .await?;
            query!(
                "DELETE FROM key_package WHERE user_id = $1",
                &user_id as &QsUserId,
            )
            .execute(&mut *txn)
            .await?;
            query!(
                "DELETE FROM apq_key_package WHERE user_id = $1",
                &user_id as &QsUserId,
            )
            .execute(&mut *txn)
            .await?;
            query!(
                "DELETE FROM qs_staged_key_package_batch WHERE user_id = $1",
                &user_id as &QsUserId,
            )
            .execute(&mut *txn)
            .await?;
            // Lock the client records in ascending client id order, the same
            // order as the staged key package promotion.
            query!(
                "UPDATE qs_client_record SET encrypted_push_token = NULL
                WHERE client_id IN (
                    SELECT client_id FROM qs_client_record
                    WHERE user_id = $1
                    ORDER BY client_id
                    FOR UPDATE
                )",
                &user_id as &QsUserId,
            )
            .execute(&mut *txn)
            .await?;
            Self::delete_drained(&mut txn, user_id).await?;
            txn.commit().await?;
            Ok(())
        }

        /// Deletes the client records of a deleted user whose queue is empty.
        /// Then deletes the user record if no active client record is left,
        /// which cascades to the remaining tombstoned client records, together
        /// with their queues.
        ///
        /// Does nothing for a user that is not deleted. The caller must hold
        /// the lock on the user record, so that concurrent calls for the same
        /// user are serialized.
        pub(in crate::qs) async fn delete_drained(
            connection: &mut PgConnection,
            user_id: QsUserId,
        ) -> Result<(), StorageError> {
            // Lock the client records in ascending client id order, the same
            // order as the staged key package promotion. With the locks held,
            // the queue check below sees every committed enqueue, and a later
            // enqueue finds no client record.
            query!(
                "SELECT client_id FROM qs_client_record
                WHERE user_id = $1
                ORDER BY client_id
                FOR UPDATE",
                &user_id as &QsUserId,
            )
            .fetch_all(&mut *connection)
            .await?;
            query!(
                "DELETE FROM qs_client_record c
                USING qs_user_record u
                WHERE c.user_id = $1
                    AND u.user_id = c.user_id
                    AND u.deleted_at IS NOT NULL
                    AND NOT EXISTS (
                        SELECT 1 FROM qs_queues q WHERE q.queue_id = c.client_id
                    )",
                &user_id as &QsUserId,
            )
            .execute(&mut *connection)
            .await?;
            let is_drained = query_scalar!(
                r#"SELECT EXISTS (
                    SELECT 1 FROM qs_user_record u
                    WHERE u.user_id = $1
                        AND u.deleted_at IS NOT NULL
                        AND NOT EXISTS (
                            SELECT 1 FROM qs_client_record c
                            WHERE c.user_id = u.user_id AND c.deleted_at IS NULL
                        )
                ) AS "is_drained!""#,
                &user_id as &QsUserId,
            )
            .fetch_one(&mut *connection)
            .await?;
            if !is_drained {
                return Ok(());
            }
            // The queues have no foreign key to the client records, so the
            // cascade below leaves them behind.
            query!(
                "DELETE FROM qs_queues
                WHERE queue_id IN (
                    SELECT client_id FROM qs_client_record WHERE user_id = $1
                )",
                &user_id as &QsUserId,
            )
            .execute(&mut *connection)
            .await?;
            query!(
                "DELETE FROM qs_user_record WHERE user_id = $1",
                &user_id as &QsUserId,
            )
            .execute(&mut *connection)
            .await?;
            Ok(())
        }

        pub(in crate::qs) async fn update(
            &self,
            connection: impl PgExecutor<'_>,
        ) -> Result<(), StorageError> {
            sqlx::query!(
                "UPDATE
                    qs_user_record
                SET
                    verifying_key = $2, friendship_token = $3
                WHERE
                    user_id = $1",
                &self.user_id as &QsUserId,
                &self.verifying_key as &QsUserVerifyingKey,
                self.friendship_token.token(),
            )
            .execute(connection)
            .await?;
            Ok(())
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

            user_record.update(&pool).await?;
            let loaded = UserRecord::load(&pool, &user_record.user_id)
                .await?
                .expect("missing user record");
            assert_eq!(loaded, user_record);

            Ok(())
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
            for client in [&client_a, &client_b] {
                enqueue_test_messages(&pool, client.client_id, 1).await?;
            }

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

            UserRecord::soft_delete(&mut *pool.acquire().await?, user_record.user_id).await?;

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
        async fn soft_delete_drained_user(pool: PgPool) -> anyhow::Result<()> {
            let user_record = store_random_user_record(&pool).await?;
            let user_id = user_record.user_id;
            let client = store_random_client_record(&pool, user_id).await?;
            let tombstoned_client = store_random_client_record(&pool, user_id).await?;
            enqueue_test_messages(&pool, tombstoned_client.client_id, 1).await?;
            QsClientRecord::soft_delete(&pool, &tombstoned_client.client_id).await?;
            let other_user = store_random_user_record(&pool).await?;
            let other_client = store_random_client_record(&pool, other_user.user_id).await?;

            UserRecord::soft_delete(&mut *pool.acquire().await?, user_id).await?;

            assert_eq!(count_rows(&pool, "qs_user_record", user_id).await?, 0);
            assert_eq!(count_rows(&pool, "qs_client_record", user_id).await?, 0);
            assert_eq!(QsClientRecord::load(&pool, &client.client_id).await?, None);
            assert_eq!(queue_len(&pool, tombstoned_client.client_id).await?, 0);

            // Other users are not affected.
            assert_eq!(
                UserRecord::load(&pool, &other_user.user_id).await?,
                Some(other_user)
            );
            assert_eq!(
                QsClientRecord::load(&pool, &other_client.client_id).await?,
                Some(other_client)
            );

            Ok(())
        }

        #[sqlx::test]
        async fn soft_delete_keeps_pending_client(pool: PgPool) -> anyhow::Result<()> {
            let user_record = store_random_user_record(&pool).await?;
            let user_id = user_record.user_id;
            let pending_client = store_random_client_record(&pool, user_id).await?;
            enqueue_test_messages(&pool, pending_client.client_id, 1).await?;
            let drained_client = store_random_client_record(&pool, user_id).await?;

            UserRecord::soft_delete(&mut *pool.acquire().await?, user_id).await?;

            assert_eq!(count_rows(&pool, "qs_user_record", user_id).await?, 1);
            assert_eq!(count_rows(&pool, "qs_client_record", user_id).await?, 1);
            assert_eq!(
                QsClientRecord::load(&pool, &pending_client.client_id).await?,
                Some(QsClientRecord {
                    encrypted_push_token: None,
                    ..pending_client
                })
            );
            assert_eq!(
                QsClientRecord::load(&pool, &drained_client.client_id).await?,
                None
            );

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

            // deleted user active right now, kept by a pending message
            let user_record_4 = store_random_user_record(&pool).await?;
            let client_record_4 = store_random_client_record(&pool, user_record_4.user_id).await?;
            enqueue_test_messages(&pool, client_record_4.client_id, 1).await?;
            UserRecord::soft_delete(&mut *pool.acquire().await?, user_record_4.user_id).await?;

            let metrics = UserRecord::metrics(&pool).await?;
            assert_eq!(metrics.total_users, Some(3));
            assert_eq!(metrics.active_last_month_users, Some(2));
            assert_eq!(metrics.active_last_day_users, Some(1));

            Ok(())
        }
    }
}
