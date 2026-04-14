// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use airprotos::common::v1::OperationType;
use sqlx::SqliteExecutor;

/// Stores a serialized Privacy Pass token.
pub(crate) async fn store_token(
    executor: impl SqliteExecutor<'_>,
    operation_type: OperationType,
    token: &[u8],
) -> Result<(), sqlx::Error> {
    let operation_type = operation_type as i32;
    sqlx::query!(
        "INSERT INTO privacy_pass_token (operation_type, token) VALUES (?, ?)",
        operation_type,
        token
    )
    .execute(executor)
    .await?;
    Ok(())
}

/// Loads and deletes one token (FIFO order).
pub(crate) async fn consume_token(
    executor: impl SqliteExecutor<'_>,
    operation_type: OperationType,
) -> Result<Option<Vec<u8>>, sqlx::Error> {
    let operation_type = operation_type as i32;
    let row = sqlx::query_scalar!(
        "DELETE FROM privacy_pass_token
         WHERE operation_type = ? AND id = (SELECT MIN(id) FROM privacy_pass_token)
         RETURNING token",
        operation_type
    )
    .fetch_optional(executor)
    .await?;
    Ok(row)
}

/// Returns the number of stored tokens.
pub(crate) async fn token_count(
    executor: impl SqliteExecutor<'_>,
    operation_type: OperationType,
) -> Result<u16, sqlx::Error> {
    let operation_type = operation_type as i32;
    let count = sqlx::query_scalar!(
        "SELECT COUNT(*) FROM privacy_pass_token WHERE operation_type = ?",
        operation_type
    )
    .fetch_one(executor)
    .await?;
    Ok(count as u16)
}

/// Stores or updates a batched token public key.
pub(crate) async fn store_batched_token_key(
    executor: impl SqliteExecutor<'_>,
    operation_type: OperationType,
    token_key_id: u8,
    public_key: &[u8],
) -> Result<(), sqlx::Error> {
    let key_id = token_key_id as i32;
    let operation_type = operation_type as i32;
    sqlx::query!(
        "INSERT INTO batched_token_key (token_key_id, operation_type, public_key) \
         VALUES (?, ?, ?) \
         ON CONFLICT (operation_type, token_key_id) DO UPDATE SET public_key = excluded.public_key",
        key_id,
        operation_type,
        public_key
    )
    .execute(executor)
    .await?;
    Ok(())
}

/// Deletes all stored tokens.
pub(crate) async fn delete_all_tokens(
    executor: impl SqliteExecutor<'_>,
    operation_type: OperationType,
) -> Result<(), sqlx::Error> {
    let operation_type = operation_type as i32;
    sqlx::query!(
        "DELETE FROM privacy_pass_token WHERE operation_type = ?",
        operation_type
    )
    .execute(executor)
    .await?;
    Ok(())
}

/// Deletes all stored batched token keys.
pub(crate) async fn delete_all_batched_token_keys(
    executor: impl SqliteExecutor<'_>,
    operation_type: OperationType,
) -> Result<(), sqlx::Error> {
    let operation_type = operation_type as i32;
    sqlx::query!(
        "DELETE FROM batched_token_key WHERE operation_type = ?",
        operation_type
    )
    .execute(executor)
    .await?;
    Ok(())
}

/// Loads all batched token public keys for a specific operation type.
pub(crate) async fn load_batched_token_keys(
    executor: impl SqliteExecutor<'_>,
    operation_type: OperationType,
) -> Result<Vec<(u8, Vec<u8>)>, sqlx::Error> {
    let operation_type = operation_type as i32;
    let rows = sqlx::query!(
        "
        SELECT token_key_id, public_key
        FROM batched_token_key
        WHERE operation_type = ?
        ORDER BY token_key_id DESC
        ",
        operation_type,
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

    const OP: OperationType = OperationType::GetInviteCode;

    /// Tokens are consumed in FIFO order.
    #[sqlx::test]
    async fn store_and_consume_fifo(pool: SqlitePool) -> anyhow::Result<()> {
        let token_a = b"token_aaa".to_vec();
        let token_b = b"token_bbb".to_vec();

        store_token(&pool, OP, &token_a).await?;
        store_token(&pool, OP, &token_b).await?;

        assert_eq!(token_count(&pool, OP).await?, 2);

        // Consume returns FIFO order.
        let first = consume_token(&pool, OP)
            .await?
            .expect("should have a token");
        assert_eq!(first, token_a);
        let second = consume_token(&pool, OP)
            .await?
            .expect("should have a token");
        assert_eq!(second, token_b);

        // Empty after consuming both.
        assert_eq!(token_count(&pool, OP).await?, 0);
        assert!(consume_token(&pool, OP).await?.is_none());

        Ok(())
    }

    /// Consuming from an empty store returns `None`.
    #[sqlx::test]
    async fn consume_from_empty(pool: SqlitePool) -> anyhow::Result<()> {
        assert!(consume_token(&pool, OP).await?.is_none());
        assert_eq!(token_count(&pool, OP).await?, 0);
        Ok(())
    }

    /// Store and load multiple batched token public keys.
    #[sqlx::test]
    async fn batched_key_store_load(pool: SqlitePool) -> anyhow::Result<()> {
        let pk_a = b"public_key_a_32_bytes_padding!!".to_vec();
        let pk_b = b"public_key_b_32_bytes_padding!!".to_vec();

        store_batched_token_key(&pool, 1, OP, &pk_a).await?;
        store_batched_token_key(&pool, 2, OP, &pk_b).await?;

        let keys = load_batched_token_keys(&pool, OP).await?;
        assert_eq!(keys.len(), 2);
        assert!(keys.contains(&(1u8, pk_a.clone())));
        assert!(keys.contains(&(2u8, pk_b)));

        Ok(())
    }

    /// `delete_all_tokens` removes every stored token.
    #[sqlx::test]
    async fn delete_all_tokens_clears_store(pool: SqlitePool) -> anyhow::Result<()> {
        store_token(&pool, OP, b"aaa").await?;
        store_token(&pool, OP, b"bbb").await?;
        assert_eq!(token_count(&pool, OP).await?, 2);

        delete_all_tokens(&pool, OP).await?;
        assert_eq!(token_count(&pool, OP).await?, 0);
        assert!(consume_token(&pool, OP).await?.is_none());

        Ok(())
    }

    /// `delete_all_batched_token_keys` removes every stored key.
    #[sqlx::test]
    async fn delete_all_keys_clears_store(pool: SqlitePool) -> anyhow::Result<()> {
        store_batched_token_key(&pool, 1, OP, b"pk1").await?;
        store_batched_token_key(&pool, 2, OP, b"pk2").await?;
        assert_eq!(load_batched_token_keys(&pool, OP).await?.len(), 2);

        delete_all_batched_token_keys(&pool, OP).await?;
        assert!(load_batched_token_keys(&pool, OP).await?.is_empty());

        Ok(())
    }

    /// Re-inserting a key with the same ID updates the public key (upsert).
    #[sqlx::test]
    async fn batched_key_upsert(pool: SqlitePool) -> anyhow::Result<()> {
        let pk_old = b"old_key_padded_to_32_bytes!!!!!".to_vec();
        let pk_new = b"new_key_padded_to_32_bytes!!!!!".to_vec();

        store_batched_token_key(&pool, 1, OP, &pk_old).await?;
        store_batched_token_key(&pool, 1, OP, &pk_new).await?;

        let keys = load_batched_token_keys(&pool, OP).await?;
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0], (1u8, pk_new));

        Ok(())
    }
}
