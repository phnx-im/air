// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use phnxtypes::{
    crypto::signatures::keys::QsUserVerifyingKey, identifiers::QsUserId, messages::FriendshipToken,
};
use sqlx::PgExecutor;

use crate::errors::StorageError;

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

pub(crate) mod persistence {
    use phnxtypes::identifiers::QsUserId;
    use sqlx::PgExecutor;

    use crate::errors::StorageError;

    use super::*;

    impl UserRecord {
        pub(super) async fn store(
            &self,
            connection: impl PgExecutor<'_>,
        ) -> Result<(), StorageError> {
            sqlx::query!(
                "INSERT INTO
                    qs_user_records
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
                    qs_user_records
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
                "DELETE FROM qs_user_records WHERE user_id = $1",
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
                    qs_user_records
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
    }

    #[cfg(test)]
    pub(crate) mod tests {
        use sqlx::PgPool;

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
    }
}
