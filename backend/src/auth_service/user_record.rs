// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::{
    crypto::indexed_aead::keys::UserProfileKeyIndex, identifiers::UserId,
    messages::client_as_out::EncryptedUserProfile,
};
use thiserror::Error;

use crate::errors::StorageError;

#[derive(Debug, Error)]
pub enum UserProfileMergingError {
    /// The user profile is not staged.
    #[error("No staged user profile")]
    NoStagedUserProfile,
}

#[derive(Debug, Clone)]
#[cfg_attr(test, derive(PartialEq, Eq))]
pub struct UserRecord {
    user_id: UserId,
    encrypted_user_profile: EncryptedUserProfile,
    staged_user_profile: Option<EncryptedUserProfile>,
}

impl UserRecord {
    pub fn new(user_id: UserId, encrypted_user_profile: EncryptedUserProfile) -> Self {
        Self {
            user_id,
            encrypted_user_profile,
            staged_user_profile: None,
        }
    }

    #[cfg(test)]
    pub fn user_id(&self) -> &UserId {
        &self.user_id
    }

    pub(super) async fn new_and_store(
        connection: impl sqlx::PgExecutor<'_>,
        user_id: &UserId,
        encrypted_user_profile: &EncryptedUserProfile,
    ) -> Result<Self, StorageError> {
        let user_record = Self::new(user_id.clone(), encrypted_user_profile.clone());
        user_record.store(connection).await?;
        Ok(user_record)
    }

    pub fn into_user_profile(
        self,
        key_index: &UserProfileKeyIndex,
    ) -> Option<EncryptedUserProfile> {
        if key_index == self.encrypted_user_profile.key_index() {
            return Some(self.encrypted_user_profile);
        } else if let Some(staged_user_profile) = self.staged_user_profile
            && key_index == staged_user_profile.key_index()
        {
            return Some(staged_user_profile);
        }
        None
    }

    /// Stage a new user profile for the user. If a user profile is already
    /// staged, it will be replaced.
    pub fn stage_user_profile(&mut self, encrypted_user_profile: EncryptedUserProfile) {
        self.staged_user_profile = Some(encrypted_user_profile);
    }

    pub fn merge_user_profile(&mut self) -> Result<(), UserProfileMergingError> {
        let Some(staged_user_profile) = self.staged_user_profile.take() else {
            return Err(UserProfileMergingError::NoStagedUserProfile);
        };
        self.encrypted_user_profile = staged_user_profile;
        Ok(())
    }
}

pub(crate) mod persistence {
    use aircommon::{identifiers::UserId, messages::client_as_out::EncryptedUserProfile};
    use sqlx::{PgExecutor, query, query_as};

    use crate::errors::StorageError;

    use super::UserRecord;

    impl UserRecord {
        /// Loads the AsUserRecord for a given UserName. Returns None if no AsUserRecord
        /// exists for the given UserId.
        pub(in crate::auth_service) async fn load(
            connection: impl PgExecutor<'_>,
            user_id: &UserId,
        ) -> Result<Option<UserRecord>, StorageError> {
            struct AsUserRecord {
                encrypted_user_profile: EncryptedUserProfile,
                staged_user_profile: Option<EncryptedUserProfile>,
            }

            let record = query_as!(
                AsUserRecord,
                r#"SELECT
                    encrypted_user_profile AS "encrypted_user_profile: _",
                    staged_user_profile AS "staged_user_profile: _"
                FROM as_user_record
                WHERE user_uuid = $1 AND user_domain = $2"#,
                user_id.uuid(),
                user_id.domain() as _,
            )
            .fetch_optional(connection)
            .await?;
            Ok(record.map(|record| UserRecord {
                user_id: user_id.clone(),
                encrypted_user_profile: record.encrypted_user_profile,
                staged_user_profile: record.staged_user_profile,
            }))
        }

        /// Update the AsUserRecord for a given UserId.
        pub(crate) async fn update(
            &self,
            connection: impl PgExecutor<'_>,
        ) -> Result<(), StorageError> {
            query!(
                "UPDATE as_user_record
                SET encrypted_user_profile = $1, staged_user_profile = $2
                WHERE user_uuid = $3 AND user_domain = $4",
                self.encrypted_user_profile as _,
                self.staged_user_profile as _,
                self.user_id.uuid(),
                self.user_id.domain() as _,
            )
            .execute(connection)
            .await?;
            Ok(())
        }

        /// Create a new user with the given user name. If a user with the given user
        /// name already exists, an error is returned.
        pub(super) async fn store(
            &self,
            connection: impl PgExecutor<'_>,
        ) -> Result<(), StorageError> {
            query!(
                "INSERT INTO as_user_record
                    (user_uuid, user_domain, encrypted_user_profile, staged_user_profile)
                    VALUES ($1, $2, $3, $4)",
                self.user_id.uuid(),
                self.user_id.domain() as _,
                self.encrypted_user_profile as _,
                self.staged_user_profile as _,
            )
            .execute(connection)
            .await?;
            Ok(())
        }

        /// Deletes the AsUserRecord for a given UserId. Returns true if a AsUserRecord
        /// was deleted, false if no AsUserRecord existed for the given UserId.
        ///
        /// The storage provider must also delete the following:
        ///  - All clients of the user
        ///  - All enqueued messages for the respective clients
        ///  - All key packages for the respective clients
        pub(in crate::auth_service) async fn delete(
            connection: impl PgExecutor<'_>,
            user_id: &UserId,
        ) -> Result<(), sqlx::Error> {
            // The database cascades the delete to the clients and their connection packages.
            query!(
                "DELETE FROM as_user_record WHERE user_uuid = $1 AND user_domain = $2",
                user_id.uuid(),
                user_id.domain() as _,
            )
            .execute(connection)
            .await?;
            Ok(())
        }
    }

    #[cfg(test)]
    pub(crate) mod tests {
        use aircommon::messages::client_as_out::EncryptedUserProfile;
        use sqlx::PgPool;

        use super::*;

        pub(crate) async fn store_random_user_record(pool: &PgPool) -> anyhow::Result<UserRecord> {
            let user_id = UserId::random("example.com".parse()?);
            let encrypted_user_profile = EncryptedUserProfile::dummy();
            let record = UserRecord {
                user_id,
                encrypted_user_profile,
                staged_user_profile: None,
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
        async fn delete(pool: PgPool) -> anyhow::Result<()> {
            let user_record = store_random_user_record(&pool).await?;

            let loaded = UserRecord::load(&pool, &user_record.user_id)
                .await?
                .expect("missing user record");
            assert_eq!(loaded, user_record);

            UserRecord::delete(&pool, &user_record.user_id).await?;

            let loaded = UserRecord::load(&pool, &user_record.user_id).await?;
            assert!(loaded.is_none());

            Ok(())
        }
    }
}
