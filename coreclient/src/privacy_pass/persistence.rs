// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use sqlx::SqliteExecutor;

/// Stores a serialized Privacy Pass token.
pub(crate) async fn store_token(
    executor: impl SqliteExecutor<'_>,
    token: &[u8],
) -> Result<(), sqlx::Error> {
    sqlx::query!("INSERT INTO privacy_pass_token (token) VALUES (?)", token)
        .execute(executor)
        .await?;
    Ok(())
}

/// Loads and deletes one token (FIFO order).
pub(crate) async fn consume_token(
    executor: impl SqliteExecutor<'_>,
) -> Result<Option<Vec<u8>>, sqlx::Error> {
    let row = sqlx::query_scalar!(
        "DELETE FROM privacy_pass_token \
         WHERE id = (SELECT MIN(id) FROM privacy_pass_token) \
         RETURNING token"
    )
    .fetch_optional(executor)
    .await?;
    Ok(row)
}

/// Returns the number of stored tokens.
pub(crate) async fn token_count(executor: impl SqliteExecutor<'_>) -> Result<i64, sqlx::Error> {
    let count = sqlx::query_scalar!("SELECT COUNT(*) FROM privacy_pass_token")
        .fetch_one(executor)
        .await?;
    Ok(count)
}

/// Stores or updates a batched token public key.
pub(crate) async fn store_batched_token_key(
    executor: impl SqliteExecutor<'_>,
    token_key_id: u8,
    public_key: &[u8],
) -> Result<(), sqlx::Error> {
    let key_id = token_key_id as i32;
    sqlx::query!(
        "INSERT INTO batched_token_key (token_key_id, public_key) \
         VALUES (?, ?) \
         ON CONFLICT (token_key_id) DO UPDATE SET public_key = excluded.public_key",
        key_id,
        public_key
    )
    .execute(executor)
    .await?;
    Ok(())
}

/// Deletes all stored tokens.
pub(crate) async fn delete_all_tokens(
    executor: impl SqliteExecutor<'_>,
) -> Result<(), sqlx::Error> {
    sqlx::query!("DELETE FROM privacy_pass_token")
        .execute(executor)
        .await?;
    Ok(())
}

/// Deletes all stored batched token keys.
pub(crate) async fn delete_all_batched_token_keys(
    executor: impl SqliteExecutor<'_>,
) -> Result<(), sqlx::Error> {
    sqlx::query!("DELETE FROM batched_token_key")
        .execute(executor)
        .await?;
    Ok(())
}

/// Loads all batched token public keys.
pub(crate) async fn load_batched_token_keys(
    executor: impl SqliteExecutor<'_>,
) -> Result<Vec<(u8, Vec<u8>)>, sqlx::Error> {
    let rows = sqlx::query!(
        "SELECT token_key_id, public_key FROM batched_token_key \
         ORDER BY token_key_id DESC"
    )
    .fetch_all(executor)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| (r.token_key_id as u8, r.public_key))
        .collect())
}

#[cfg(test)]
mod tests {
    use sqlx::SqlitePool;

    use super::*;

    /// Tokens are consumed in FIFO order.
    #[sqlx::test]
    async fn store_and_consume_fifo(pool: SqlitePool) -> anyhow::Result<()> {
        let token_a = b"token_aaa".to_vec();
        let token_b = b"token_bbb".to_vec();

        store_token(&pool, &token_a).await?;
        store_token(&pool, &token_b).await?;

        assert_eq!(token_count(&pool).await?, 2);

        // Consume returns FIFO order.
        let first = consume_token(&pool).await?.expect("should have a token");
        assert_eq!(first, token_a);
        let second = consume_token(&pool).await?.expect("should have a token");
        assert_eq!(second, token_b);

        // Empty after consuming both.
        assert_eq!(token_count(&pool).await?, 0);
        assert!(consume_token(&pool).await?.is_none());

        Ok(())
    }

    /// Consuming from an empty store returns `None`.
    #[sqlx::test]
    async fn consume_from_empty(pool: SqlitePool) -> anyhow::Result<()> {
        assert!(consume_token(&pool).await?.is_none());
        assert_eq!(token_count(&pool).await?, 0);
        Ok(())
    }

    /// Store and load multiple batched token public keys.
    #[sqlx::test]
    async fn batched_key_store_load(pool: SqlitePool) -> anyhow::Result<()> {
        let pk_a = b"public_key_a_32_bytes_padding!!".to_vec();
        let pk_b = b"public_key_b_32_bytes_padding!!".to_vec();

        store_batched_token_key(&pool, 1, &pk_a).await?;
        store_batched_token_key(&pool, 2, &pk_b).await?;

        let keys = load_batched_token_keys(&pool).await?;
        assert_eq!(keys.len(), 2);
        assert!(keys.contains(&(1u8, pk_a.clone())));
        assert!(keys.contains(&(2u8, pk_b)));

        Ok(())
    }

    /// `delete_all_tokens` removes every stored token.
    #[sqlx::test]
    async fn delete_all_tokens_clears_store(pool: SqlitePool) -> anyhow::Result<()> {
        store_token(&pool, b"aaa").await?;
        store_token(&pool, b"bbb").await?;
        assert_eq!(token_count(&pool).await?, 2);

        delete_all_tokens(&pool).await?;
        assert_eq!(token_count(&pool).await?, 0);
        assert!(consume_token(&pool).await?.is_none());

        Ok(())
    }

    /// `delete_all_batched_token_keys` removes every stored key.
    #[sqlx::test]
    async fn delete_all_keys_clears_store(pool: SqlitePool) -> anyhow::Result<()> {
        store_batched_token_key(&pool, 1, b"pk1").await?;
        store_batched_token_key(&pool, 2, b"pk2").await?;
        assert_eq!(load_batched_token_keys(&pool).await?.len(), 2);

        delete_all_batched_token_keys(&pool).await?;
        assert!(load_batched_token_keys(&pool).await?.is_empty());

        Ok(())
    }

    /// Re-inserting a key with the same ID updates the public key (upsert).
    #[sqlx::test]
    async fn batched_key_upsert(pool: SqlitePool) -> anyhow::Result<()> {
        let pk_old = b"old_key_padded_to_32_bytes!!!!!".to_vec();
        let pk_new = b"new_key_padded_to_32_bytes!!!!!".to_vec();

        store_batched_token_key(&pool, 1, &pk_old).await?;
        store_batched_token_key(&pool, 1, &pk_new).await?;

        let keys = load_batched_token_keys(&pool).await?;
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0], (1u8, pk_new));

        Ok(())
    }
}
