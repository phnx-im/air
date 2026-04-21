// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::messages::client_as::SerializedToken;
use airprotos::auth_service::v1::OperationType;
use chrono::{DateTime, Utc};
use sqlx::SqliteExecutor;

use crate::privacy_pass::TokenId;

pub(crate) async fn load_token_ids(
    executor: impl SqliteExecutor<'_>,
    operation_type: OperationType,
) -> sqlx::Result<Vec<TokenId>> {
    let operation_type = i32::from(operation_type);
    sqlx::query_as!(
        TokenId,
        "SELECT id, created_at as 'created_at: DateTime<Utc>'
         FROM privacy_pass_token WHERE operation_type = ?",
        operation_type,
    )
    .fetch_all(executor)
    .await
}

/// Stores a serialized Privacy Pass token.
pub(crate) async fn store_token(
    executor: impl SqliteExecutor<'_>,
    operation_type: OperationType,
    token: &[u8],
) -> Result<(), sqlx::Error> {
    let operation_type = i32::from(operation_type);
    let now = Utc::now();
    sqlx::query!(
        "INSERT INTO privacy_pass_token (
            operation_type, token, created_at
        )
        VALUES (?, ?, ?)",
        operation_type,
        token,
        now
    )
    .execute(executor)
    .await?;
    Ok(())
}

impl TokenId {
    pub(crate) async fn load(
        executor: impl SqliteExecutor<'_>,
        token_id: &TokenId,
    ) -> sqlx::Result<Option<SerializedToken>> {
        sqlx::query_scalar!(
            "SELECT token FROM privacy_pass_token WHERE id = ?",
            token_id.id
        )
        .fetch_optional(executor)
        .await
        .map(|bytes| bytes.map(SerializedToken::new))
    }

    pub(crate) async fn delete(
        executor: impl SqliteExecutor<'_>,
        token_id: &TokenId,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!("DELETE FROM privacy_pass_token WHERE id = ?", token_id.id)
            .execute(executor)
            .await?;
        Ok(())
    }
}

/// Loads and deletes one token (FIFO order).
pub(crate) async fn consume_token(
    executor: impl SqliteExecutor<'_>,
    operation_type: OperationType,
) -> Result<Option<Vec<u8>>, sqlx::Error> {
    let operation_type = i32::from(operation_type);
    let row = sqlx::query_scalar!(
        "DELETE FROM privacy_pass_token
         WHERE
            operation_type = $1 AND
            id = (SELECT MIN(id)
                    FROM privacy_pass_token
                    WHERE operation_type = $1)
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
) -> sqlx::Result<u16> {
    let operation_type = i32::from(operation_type);
    sqlx::query_scalar!(
        "SELECT COUNT(*) FROM privacy_pass_token WHERE operation_type = ?",
        operation_type
    )
    .fetch_one(executor)
    .await?
    .try_into()
    .map_err(|error| sqlx::Error::Decode(Box::new(error)))
}

/// Stores or updates a batched token public key.
pub(crate) async fn store_batched_token_key(
    executor: impl SqliteExecutor<'_>,
    token_key_id: u8,
    operation_type: OperationType,
    public_key: &[u8],
) -> Result<(), sqlx::Error> {
    let key_id = token_key_id as i32;
    let operation_type = i32::from(operation_type);
    sqlx::query!(
        "INSERT INTO batched_token_key
            (token_key_id, operation_type, public_key)
            VALUES (?, ?, ?)
            ON CONFLICT (operation_type, token_key_id)
            DO UPDATE SET public_key = excluded.public_key",
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
    let operation_type = i32::from(operation_type);
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
    let operation_type = i32::from(operation_type);
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
    let operation_type = i32::from(operation_type);
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

    const OP1: OperationType = OperationType::AddUsername;
    const OP2: OperationType = OperationType::GetInviteCode;

    /// Tokens are consumed in FIFO order.
    #[sqlx::test]
    async fn store_and_consume_fifo(pool: SqlitePool) -> anyhow::Result<()> {
        let token_a = b"token_aaa".to_vec();
        let token_b = b"token_bbb".to_vec();

        store_token(&pool, OP1, &token_a).await?;
        store_token(&pool, OP1, &token_b).await?;

        assert_eq!(token_count(&pool, OP1).await?, 2);

        // Consume returns FIFO order.
        let first = consume_token(&pool, OP1)
            .await?
            .expect("should have a token");
        assert_eq!(first, token_a);
        let second = consume_token(&pool, OP1)
            .await?
            .expect("should have a token");
        assert_eq!(second, token_b);

        // Empty after consuming both.
        assert_eq!(token_count(&pool, OP1).await?, 0);
        assert!(consume_token(&pool, OP1).await?.is_none());

        Ok(())
    }

    /// Consuming from an empty store returns `None`.
    #[sqlx::test]
    async fn consume_from_empty(pool: SqlitePool) -> anyhow::Result<()> {
        assert!(consume_token(&pool, OP1).await?.is_none());
        assert_eq!(token_count(&pool, OP1).await?, 0);
        Ok(())
    }

    /// Store and load multiple batched token public keys.
    #[sqlx::test]
    async fn batched_key_store_load(pool: SqlitePool) -> anyhow::Result<()> {
        let pk_a = b"public_key_a_32_bytes_padding!!".to_vec();
        let pk_b = b"public_key_b_32_bytes_padding!!".to_vec();

        store_batched_token_key(&pool, 1, OP1, &pk_a).await?;
        store_batched_token_key(&pool, 2, OP1, &pk_b).await?;

        let keys = load_batched_token_keys(&pool, OP1).await?;
        assert_eq!(keys.len(), 2);
        assert!(keys.contains(&(1u8, pk_a.clone())));
        assert!(keys.contains(&(2u8, pk_b)));

        Ok(())
    }

    /// `delete_all_tokens` removes every stored token.
    #[sqlx::test]
    async fn delete_all_tokens_clears_store(pool: SqlitePool) -> anyhow::Result<()> {
        store_token(&pool, OP1, b"aaa").await?;
        store_token(&pool, OP1, b"bbb").await?;
        assert_eq!(token_count(&pool, OP1).await?, 2);

        delete_all_tokens(&pool, OP1).await?;
        assert_eq!(token_count(&pool, OP1).await?, 0);
        assert!(consume_token(&pool, OP1).await?.is_none());

        Ok(())
    }

    /// `delete_all_batched_token_keys` removes every stored key.
    #[sqlx::test]
    async fn delete_all_keys_clears_store(pool: SqlitePool) -> anyhow::Result<()> {
        store_batched_token_key(&pool, 1, OP1, b"pk1").await?;
        store_batched_token_key(&pool, 2, OP1, b"pk2").await?;
        assert_eq!(load_batched_token_keys(&pool, OP1).await?.len(), 2);

        delete_all_batched_token_keys(&pool, OP1).await?;
        assert!(load_batched_token_keys(&pool, OP1).await?.is_empty());

        Ok(())
    }

    /// Re-inserting a key with the same ID updates the public key (upsert).
    #[sqlx::test]
    async fn batched_key_upsert(pool: SqlitePool) -> anyhow::Result<()> {
        let pk_old = b"old_key_padded_to_32_bytes!!!!!".to_vec();
        let pk_new = b"new_key_padded_to_32_bytes!!!!!".to_vec();

        store_batched_token_key(&pool, 1, OP1, &pk_old).await?;
        store_batched_token_key(&pool, 1, OP1, &pk_new).await?;

        let keys = load_batched_token_keys(&pool, OP1).await?;
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0], (1u8, pk_new));

        Ok(())
    }

    /// Tokens stored under OP1 are not visible to OP2 and vice-versa.
    #[sqlx::test]
    async fn tokens_are_isolated_between_operation_types(pool: SqlitePool) -> anyhow::Result<()> {
        let token_op1 = b"token_op1".to_vec();
        let token_op2 = b"token_op2".to_vec();

        store_token(&pool, OP1, &token_op1).await?;
        store_token(&pool, OP2, &token_op2).await?;

        // Each operation type sees exactly its own token.
        assert_eq!(token_count(&pool, OP1).await?, 1);
        assert_eq!(token_count(&pool, OP2).await?, 1);

        // Consuming OP1 returns only the OP1 token and leaves OP2 untouched.
        let consumed = consume_token(&pool, OP1)
            .await?
            .expect("should have a token");
        assert_eq!(consumed, token_op1);
        assert_eq!(token_count(&pool, OP1).await?, 0);
        assert_eq!(token_count(&pool, OP2).await?, 1);

        // Consuming OP2 returns only the OP2 token.
        let consumed = consume_token(&pool, OP2)
            .await?
            .expect("should have a token");
        assert_eq!(consumed, token_op2);
        assert_eq!(token_count(&pool, OP2).await?, 0);

        Ok(())
    }

    /// `delete_all_tokens` for OP1 does not remove OP2 tokens.
    #[sqlx::test]
    async fn delete_all_tokens_does_not_affect_other_operation_type(
        pool: SqlitePool,
    ) -> anyhow::Result<()> {
        store_token(&pool, OP1, b"op1_token").await?;
        store_token(&pool, OP2, b"op2_token").await?;

        delete_all_tokens(&pool, OP1).await?;

        assert_eq!(token_count(&pool, OP1).await?, 0);
        assert_eq!(token_count(&pool, OP2).await?, 1);

        Ok(())
    }

    /// Batched token keys stored under OP1 are not visible to OP2 and vice-versa.
    #[sqlx::test]
    async fn batched_keys_are_isolated_between_operation_types(
        pool: SqlitePool,
    ) -> anyhow::Result<()> {
        let pk_op1 = b"public_key_op1_padded_32bytes!!".to_vec();
        let pk_op2 = b"public_key_op2_padded_32bytes!!".to_vec();

        // Same key ID, different operation types — must not collide.
        store_batched_token_key(&pool, 1, OP1, &pk_op1).await?;
        store_batched_token_key(&pool, 1, OP2, &pk_op2).await?;

        let keys_op1 = load_batched_token_keys(&pool, OP1).await?;
        let keys_op2 = load_batched_token_keys(&pool, OP2).await?;

        assert_eq!(keys_op1, vec![(1u8, pk_op1)]);
        assert_eq!(keys_op2, vec![(1u8, pk_op2)]);

        Ok(())
    }

    /// `delete_all_batched_token_keys` for OP1 does not remove OP2 keys.
    #[sqlx::test]
    async fn delete_all_keys_does_not_affect_other_operation_type(
        pool: SqlitePool,
    ) -> anyhow::Result<()> {
        store_batched_token_key(&pool, 1, OP1, b"pk_op1").await?;
        store_batched_token_key(&pool, 1, OP2, b"pk_op2").await?;

        delete_all_batched_token_keys(&pool, OP1).await?;

        assert!(load_batched_token_keys(&pool, OP1).await?.is_empty());
        assert_eq!(load_batched_token_keys(&pool, OP2).await?.len(), 1);

        Ok(())
    }

    /// Upserting a key under OP1 does not overwrite the same key ID stored under OP2.
    #[sqlx::test]
    async fn batched_key_upsert_does_not_affect_other_operation_type(
        pool: SqlitePool,
    ) -> anyhow::Result<()> {
        let pk_op1_v1 = b"op1_old_key_padded_32_bytes!!!!".to_vec();
        let pk_op1_v2 = b"op1_new_key_padded_32_bytes!!!!".to_vec();
        let pk_op2 = b"op2_key_should_not_change!!!!!!".to_vec();

        store_batched_token_key(&pool, 1, OP1, &pk_op1_v1).await?;
        store_batched_token_key(&pool, 1, OP2, &pk_op2).await?;

        // Upsert key ID 1 for OP1 — must not touch OP2's key ID 1.
        store_batched_token_key(&pool, 1, OP1, &pk_op1_v2).await?;

        let keys_op1 = load_batched_token_keys(&pool, OP1).await?;
        let keys_op2 = load_batched_token_keys(&pool, OP2).await?;

        assert_eq!(keys_op1, vec![(1u8, pk_op1_v2)]);
        assert_eq!(keys_op2, vec![(1u8, pk_op2)]);

        Ok(())
    }
}
