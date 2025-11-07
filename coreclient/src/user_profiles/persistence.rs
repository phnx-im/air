// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::{crypto::indexed_aead::keys::UserProfileKeyIndex, identifiers::UserId};
use sqlx::{SqliteExecutor, query, query_as};

use crate::store::StoreNotifier;

use super::{Asset, IndexedUserProfile, UserProfile, display_name::BaseDisplayName};

impl IndexedUserProfile {
    /// Stores this [`BaseIndexedUserProfile`].
    ///
    /// Will return an error if there already exists a user profile with the same user id.
    pub(super) async fn store(
        &self,
        executor: impl SqliteExecutor<'_>,
        notifier: &mut StoreNotifier,
    ) -> sqlx::Result<()> {
        let uuid = self.user_id.uuid();
        let domain = self.user_id.domain();
        let epoch = self.epoch as i64;
        query!(
            "INSERT INTO user (
                user_uuid,
                user_domain,
                epoch,
                decryption_key_index,
                display_name,
                profile_picture
            ) VALUES (?, ?, ?, ?, ?, ?)",
            uuid,
            domain,
            epoch,
            self.decryption_key_index,
            self.display_name,
            self.profile_picture,
        )
        .execute(executor)
        .await?;
        notifier.update(self.user_id.clone());
        Ok(())
    }

    /// Update the user's display name and profile picture in the database.
    pub(crate) async fn update(
        &self,
        executor: impl SqliteExecutor<'_>,
        notifier: &mut StoreNotifier,
    ) -> sqlx::Result<()> {
        let uuid = self.user_id.uuid();
        let domain = self.user_id.domain();
        let epoch = self.epoch as i64;
        query!(
            "UPDATE user SET
                epoch = ?3,
                decryption_key_index = ?4,
                display_name = ?5,
                profile_picture = ?6
            WHERE user_uuid = ?1 AND user_domain = ?2",
            uuid,
            domain,
            epoch,
            self.decryption_key_index,
            self.display_name,
            self.profile_picture
        )
        .execute(executor)
        .await?;
        notifier.update(self.user_id.clone());
        Ok(())
    }
}

struct SqlUser {
    epoch: u64,
    decryption_key_index: UserProfileKeyIndex,
    display_name: BaseDisplayName<true>,
    profile_picture: Option<Asset>,
}

impl From<(UserId, SqlUser)> for IndexedUserProfile {
    fn from(
        (
            user_id,
            SqlUser {
                epoch,
                decryption_key_index,
                display_name,
                profile_picture,
            },
        ): (UserId, SqlUser),
    ) -> Self {
        Self {
            user_id,
            epoch,
            decryption_key_index,
            display_name,
            profile_picture,
        }
    }
}

impl IndexedUserProfile {
    pub(crate) async fn load(
        executor: impl SqliteExecutor<'_>,
        user_id: &UserId,
    ) -> sqlx::Result<Option<Self>> {
        let uuid = user_id.uuid();
        let domain = user_id.domain();
        query_as!(
            SqlUser,
            r#"SELECT
                epoch AS "epoch: _",
                decryption_key_index AS "decryption_key_index: _",
                display_name AS "display_name: _",
                profile_picture AS "profile_picture: _"
            FROM user
            WHERE user_uuid = ? AND user_domain = ?"#,
            uuid,
            domain,
        )
        .fetch_optional(executor)
        .await
        .map(|res| res.map(|user| (user_id.clone(), user).into()))
    }
}

impl UserProfile {
    pub async fn load(
        executor: impl SqliteExecutor<'_>,
        user_id: &UserId,
    ) -> sqlx::Result<Option<Self>> {
        IndexedUserProfile::load(executor, user_id)
            .await
            .map(|res| res.map(From::from))
    }
}

#[cfg(test)]
mod tests {
    use aircommon::crypto::indexed_aead::keys::UserProfileKey;
    use sqlx::SqlitePool;

    use crate::{Asset, key_stores::indexed_keys::StorableIndexedKey};

    use super::*;

    fn test_profile() -> (IndexedUserProfile, UserProfileKey) {
        let user_id = UserId::random("localhost".parse().unwrap());
        let user_profile_key = UserProfileKey::random(&user_id).unwrap();
        let user_profile = IndexedUserProfile {
            user_id,
            epoch: 0,
            decryption_key_index: user_profile_key.index().clone(),
            display_name: "Alice".parse().unwrap(),
            profile_picture: Some(Asset::Value(vec![1, 2, 3])),
        };
        (user_profile, user_profile_key)
    }

    #[sqlx::test]
    async fn store_load(pool: SqlitePool) -> anyhow::Result<()> {
        let mut notifier = StoreNotifier::noop();

        let (profile, key) = test_profile();

        key.store(&pool).await?;

        profile.store(&pool, &mut notifier).await?;
        let loaded = IndexedUserProfile::load(&pool, &profile.user_id)
            .await?
            .expect("profile exists");
        assert_eq!(loaded, profile);

        let mut new_profile = profile.clone();
        new_profile.display_name = "Alice In Wonderland".parse()?;
        new_profile.profile_picture = None;

        // store again doesn't work
        let store_err = new_profile
            .store(&pool, &mut notifier)
            .await
            .expect_err("profile does not exist");
        assert!(matches!(store_err, sqlx::Error::Database(_)));

        Ok(())
    }

    #[sqlx::test]
    async fn update_load(pool: SqlitePool) -> anyhow::Result<()> {
        let mut notifier = StoreNotifier::noop();

        let (profile, key) = test_profile();
        key.store(&pool).await?;

        profile.store(&pool, &mut notifier).await?;
        let loaded = IndexedUserProfile::load(&pool, &profile.user_id)
            .await?
            .expect("profile exists");
        assert_eq!(loaded, profile);

        let mut new_profile = profile.clone();
        new_profile.display_name = "Alice In Wonderland".parse()?;
        new_profile.profile_picture = None;

        new_profile.update(&pool, &mut notifier).await?;
        let loaded = IndexedUserProfile::load(&pool, &profile.user_id)
            .await?
            .expect("profile exists");
        assert_ne!(loaded, profile);
        assert_eq!(loaded, new_profile);

        Ok(())
    }
}
