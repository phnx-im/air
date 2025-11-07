// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::fmt::Debug;

use aircommon::crypto::indexed_aead::keys::{
    BaseSecret, Index, IndexedAeadKey, IndexedKeyType, Key, KeyTypeInstance,
};
use sqlx::{Connection, SqliteConnection, SqliteExecutor, query, query_as};

pub(crate) struct SqlIndexedAeadKey<KT> {
    base_secret: BaseSecret<KT>,
    key: Key<KT>,
    index: Index<KT>,
}

impl<KT: IndexedKeyType> From<SqlIndexedAeadKey<KT>> for IndexedAeadKey<KT> {
    fn from(sql_key: SqlIndexedAeadKey<KT>) -> Self {
        IndexedAeadKey::from_parts(sql_key.base_secret, sql_key.key, sql_key.index)
    }
}

impl<KT: IndexedKeyType + Send + Unpin + Debug> StorableIndexedKey<KT> for IndexedAeadKey<KT> {
    fn base_secret(&self) -> &BaseSecret<KT> {
        self.base_secret()
    }

    fn key(&self) -> &Key<KT> {
        self.key()
    }

    fn index(&self) -> &Index<KT> {
        self.index()
    }
}

pub(crate) trait StorableIndexedKey<KT: IndexedKeyType + Send + Unpin + Debug>:
    From<SqlIndexedAeadKey<KT>>
{
    fn base_secret(&self) -> &BaseSecret<KT>;
    fn key(&self) -> &Key<KT>;
    fn index(&self) -> &Index<KT>;

    async fn store(&self, connection: impl SqliteExecutor<'_>) -> Result<(), sqlx::Error> {
        let base_secret = self.base_secret();
        let key = self.key();
        let index = self.index();
        query!(
            "INSERT OR IGNORE INTO indexed_key (base_secret, key_value, key_index)
                VALUES ($1, $2, $3)",
            base_secret,
            key,
            index
        )
        .execute(connection)
        .await?;
        Ok(())
    }

    async fn store_own(&self, connection: &mut SqliteConnection) -> Result<(), sqlx::Error> {
        let base_secret = self.base_secret();
        let key = self.key();
        let index = self.index();
        let key_type = KeyTypeInstance::<KT>::new();
        let mut transaction = connection.begin().await?;
        // Delete the old own key
        query!(
            "DELETE FROM indexed_key
               WHERE key_index IN (
                   SELECT key_index FROM own_key_index WHERE key_type = ?
               )",
            key_type
        )
        .execute(&mut *transaction)
        .await?;
        query!("DELETE FROM own_key_index WHERE key_type = ?", key_type)
            .execute(&mut *transaction)
            .await?;
        query!(
            "INSERT OR REPLACE INTO indexed_key (base_secret, key_value, key_index)
                VALUES ($1, $2, $3)",
            base_secret,
            key,
            index
        )
        .execute(&mut *transaction)
        .await?;
        query!(
            "INSERT OR REPLACE INTO own_key_index (key_index, key_type) VALUES ($1, $2)",
            index,
            key_type
        )
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;
        Ok(())
    }

    async fn load(
        connection: impl SqliteExecutor<'_>,
        index: &Index<KT>,
    ) -> Result<Self, sqlx::Error> {
        query_as!(
            SqlIndexedAeadKey,
            r#"
                SELECT
                    base_secret AS "base_secret: _",
                    key_value AS "key: _",
                    key_index AS "index: _"
                FROM indexed_key
                WHERE key_index = ?
                LIMIT 1"#,
            index,
        )
        .fetch_one(connection)
        .await
        .map(From::from)
    }

    async fn load_own(connection: impl SqliteExecutor<'_>) -> Result<Self, sqlx::Error> {
        let key_type = KeyTypeInstance::<KT>::new();
        query_as!(
            SqlIndexedAeadKey,
            r#"SELECT
                    ik.key_index as "index: _",
                    ik.key_value as "key: _",
                    ik.base_secret as "base_secret: _"
                FROM own_key_index oki
                JOIN indexed_key ik ON oki.key_index = ik.key_index
                WHERE oki.key_type = ?"#,
            key_type
        )
        .fetch_one(connection)
        .await
        .map(From::from)
    }

    async fn delete(
        connection: impl SqliteExecutor<'_>,
        index: &Index<KT>,
    ) -> Result<(), sqlx::Error> {
        query!("DELETE FROM indexed_key WHERE key_index = ?", index)
            .execute(connection)
            .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use aircommon::{crypto::indexed_aead::keys::UserProfileKey, identifiers::UserId};
    use sqlx::SqlitePool;

    use crate::key_stores::indexed_keys::StorableIndexedKey;

    #[sqlx::test]
    fn user_profile_key_storage(pool: SqlitePool) {
        let mut connection = pool.acquire().await.unwrap();
        let user_id = UserId::random("example.com".parse().unwrap());
        let key = UserProfileKey::random(&user_id).unwrap();
        let index = key.index().clone();
        key.store_own(&mut connection).await.unwrap();

        let loaded_key = UserProfileKey::load_own(connection.as_mut()).await.unwrap();
        assert_eq!(key, loaded_key);

        let loaded_key = UserProfileKey::load(connection.as_mut(), &index)
            .await
            .unwrap();
        assert_eq!(key, loaded_key);

        // Update key
        let new_key = UserProfileKey::random(&user_id).unwrap();
        new_key.store_own(&mut connection).await.unwrap();
        let loaded_key = UserProfileKey::load_own(connection.as_mut()).await.unwrap();
        assert_eq!(new_key, loaded_key);
        let loaded_key = UserProfileKey::load(connection.as_mut(), new_key.index())
            .await
            .unwrap();
        assert_eq!(new_key, loaded_key);

        // Delete key
        UserProfileKey::delete(connection.as_mut(), new_key.index())
            .await
            .unwrap();
        let loaded_key = UserProfileKey::load(connection.as_mut(), new_key.index())
            .await
            .unwrap_err();
        assert!(matches!(loaded_key, sqlx::Error::RowNotFound));
    }
}
