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
    qs::{METRIC_AIR_QS_DAU_USERS, METRIC_AIR_QS_MAU_USERS, METRIC_AIR_QS_TOTAL_USERS},
};

#[derive(Debug, Clone, PartialEq)]
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
    /// Number of users connected in the last 24 hours
    pub(crate) active_last_day_users: Option<i64>,
}

impl UserMetrics {
    pub(crate) fn report(&self) {
        gauge!(METRIC_AIR_QS_TOTAL_USERS).set(self.total_users.unwrap_or(0) as i32);
        gauge!(METRIC_AIR_QS_MAU_USERS).set(self.active_last_month_users.unwrap_or(0) as i32);
        gauge!(METRIC_AIR_QS_DAU_USERS).set(self.active_last_day_users.unwrap_or(0) as i32);
    }
}

pub(crate) mod persistence {
    use aircommon::identifiers::QsUserId;
    use sqlx::{PgExecutor, query_as};

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
                    user_id = $1"#,
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

        pub(in crate::qs) async fn delete(
            connection: impl PgExecutor<'_>,
            user_id: QsUserId,
        ) -> Result<(), StorageError> {
            sqlx::query!(
                "DELETE FROM qs_user_record WHERE user_id = $1",
                &user_id as &QsUserId
            )
            .execute(connection)
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
                    SELECT MAX(activity_time) AS last_activity_time
                    FROM qs_client_record
                    GROUP BY user_id
                )
                SELECT
                    (SELECT COUNT(user_id) FROM qs_user_record) AS total_users,
                    COUNT(last_activity_time) FILTER (
                        WHERE last_activity_time >= (NOW() - INTERVAL '1 month')
                    ) AS active_last_month_users,
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
        use sqlx::PgPool;

        use crate::qs::client_record::{
            QsClientRecord, persistence::tests::store_random_client_record,
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

        #[sqlx::test]
        async fn delete(pool: PgPool) -> anyhow::Result<()> {
            let user_record = store_random_user_record(&pool).await?;

            let loaded = UserRecord::load(&pool, &user_record.user_id)
                .await?
                .expect("missing user record");
            assert_eq!(loaded, user_record);

            UserRecord::delete(&pool, user_record.user_id).await?;
            let loaded = UserRecord::load(&pool, &user_record.user_id).await?;
            assert_eq!(loaded, None);

            Ok(())
        }

        #[sqlx::test]
        async fn metrics(pool: PgPool) -> anyhow::Result<()> {
            // active user right now
            let user_record_1 = store_random_user_record(&pool).await?;
            let _client_record_1 = store_random_client_record(&pool, user_record_1.user_id).await?;

            // acitive user in the last month
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

            let metrics = UserRecord::metrics(&pool).await?;
            assert_eq!(metrics.total_users, Some(3));
            assert_eq!(metrics.active_last_month_users, Some(2));
            assert_eq!(metrics.active_last_day_users, Some(1));

            Ok(())
        }
    }
}
