// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::messages::client_as::SerializedToken;
use airprotos::auth_service::v1::OperationType;
use chrono::{DateTime, Utc};

use crate::{
    db::access::{ReadConnection, WriteConnection},
    privacy_pass::{
        KeyFingerprint, SeedRecord, SeedState, StoredSeed, TokenId, derivation::SEED_LEN,
    },
};

impl SeedState {
    pub(super) const PROPOSED: &'static str = "proposed";
    const COMMITTED: &'static str = "committed";

    fn as_str(self) -> &'static str {
        match self {
            SeedState::Proposed => Self::PROPOSED,
            SeedState::Committed => Self::COMMITTED,
        }
    }

    fn parse(value: &str) -> sqlx::Result<Self> {
        match value {
            Self::PROPOSED => Ok(SeedState::Proposed),
            Self::COMMITTED => Ok(SeedState::Committed),
            other => Err(sqlx::Error::Decode(
                format!("unknown token seed state {other}").into(),
            )),
        }
    }
}

impl StoredSeed {
    fn new(seed: Vec<u8>, state: &str) -> sqlx::Result<Self> {
        Ok(Self {
            seed: decode_seed(seed)?,
            state: SeedState::parse(state)?,
        })
    }
}

pub(crate) async fn load_token_ids(
    mut connection: impl ReadConnection,
    operation_type: OperationType,
) -> sqlx::Result<Vec<TokenId>> {
    let operation_type = i32::from(operation_type);
    sqlx::query_as!(
        TokenId,
        "SELECT id, created_at as 'created_at: DateTime<Utc>'
         FROM privacy_pass_token WHERE operation_type = ?",
        operation_type,
    )
    .fetch_all(connection.as_mut())
    .await
}

/// Stores a serialized Privacy Pass token issued under `token_key_id`.
pub(crate) async fn store_token(
    mut connection: impl WriteConnection,
    operation_type: OperationType,
    token_key_id: u8,
    token: &[u8],
) -> Result<(), sqlx::Error> {
    let operation_type = i32::from(operation_type);
    let key_id = i32::from(token_key_id);
    let now = Utc::now();
    sqlx::query!(
        "INSERT INTO privacy_pass_token (
            operation_type, token_key_id, token, created_at
        )
        VALUES (?, ?, ?, ?)",
        operation_type,
        key_id,
        token,
        now
    )
    .execute(connection.as_mut())
    .await?;
    Ok(())
}

/// Stores one token of a deterministically derived batch.
///
/// Re-fetching a batch yields byte-identical tokens, so a conflict on the
/// unique token index means the token is already stored and the insert is a
/// no-op.
pub(crate) async fn store_batch_token(
    mut connection: impl WriteConnection,
    operation_type: OperationType,
    token_key_id: u8,
    allowance_epoch: u32,
    token_index: u16,
    token: &[u8],
) -> Result<(), sqlx::Error> {
    let operation_type = i32::from(operation_type);
    let key_id = i32::from(token_key_id);
    let allowance_epoch = i64::from(allowance_epoch);
    let token_index = i64::from(token_index);
    let now = Utc::now();
    sqlx::query!(
        "INSERT INTO privacy_pass_token (
            operation_type, token_key_id, token, created_at,
            allowance_epoch, token_index
        )
        VALUES (?, ?, ?, ?, ?, ?)
        ON CONFLICT (token) DO NOTHING",
        operation_type,
        key_id,
        token,
        now,
        allowance_epoch,
        token_index
    )
    .execute(connection.as_mut())
    .await?;
    Ok(())
}

impl TokenId {
    pub(crate) async fn load(
        mut connection: impl ReadConnection,
        token_id: &TokenId,
    ) -> sqlx::Result<Option<SerializedToken>> {
        sqlx::query_scalar!(
            "SELECT token FROM privacy_pass_token WHERE id = ?",
            token_id.id
        )
        .fetch_optional(connection.as_mut())
        .await
        .map(|bytes| bytes.map(SerializedToken::new))
    }

    pub(crate) async fn delete(
        mut connection: impl WriteConnection,
        token_id: &TokenId,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!("DELETE FROM privacy_pass_token WHERE id = ?", token_id.id)
            .execute(connection.as_mut())
            .await?;
        Ok(())
    }
}

/// Loads and deletes one token (FIFO order).
pub(crate) async fn consume_token(
    mut connection: impl WriteConnection,
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
    .fetch_optional(connection.as_mut())
    .await?;
    Ok(row)
}

/// Returns the number of stored tokens.
pub(crate) async fn token_count(
    mut connection: impl ReadConnection,
    operation_type: OperationType,
) -> sqlx::Result<u16> {
    let operation_type = i32::from(operation_type);
    sqlx::query_scalar!(
        "SELECT COUNT(*) FROM privacy_pass_token WHERE operation_type = ?",
        operation_type
    )
    .fetch_one(connection.as_mut())
    .await?
    .try_into()
    .map_err(|error| sqlx::Error::Decode(Box::new(error)))
}

/// Stores or updates a batched token public key.
pub(crate) async fn store_batched_token_key(
    mut connection: impl WriteConnection,
    token_key_id: u8,
    operation_type: OperationType,
    public_key: &[u8],
    is_current: bool,
) -> Result<(), sqlx::Error> {
    let key_id = token_key_id as i32;
    let operation_type = i32::from(operation_type);
    sqlx::query!(
        "INSERT INTO batched_token_key
            (token_key_id, operation_type, public_key, is_current)
            VALUES (?, ?, ?, ?)
            ON CONFLICT (operation_type, token_key_id)
            DO UPDATE SET
                public_key = excluded.public_key,
                is_current = excluded.is_current",
        key_id,
        operation_type,
        public_key,
        is_current
    )
    .execute(connection.as_mut())
    .await?;
    Ok(())
}

/// Deletes all stored tokens.
pub(crate) async fn delete_all_tokens(
    mut connection: impl WriteConnection,
    operation_type: OperationType,
) -> Result<(), sqlx::Error> {
    let operation_type = i32::from(operation_type);
    sqlx::query!(
        "DELETE FROM privacy_pass_token WHERE operation_type = ?",
        operation_type
    )
    .execute(connection.as_mut())
    .await?;
    Ok(())
}

/// Returns the distinct key IDs the stored tokens were issued under.
pub(crate) async fn load_token_key_ids(
    mut connection: impl ReadConnection,
    operation_type: OperationType,
) -> sqlx::Result<Vec<u8>> {
    let operation_type = i32::from(operation_type);
    let rows = sqlx::query_scalar!(
        "SELECT DISTINCT token_key_id
         FROM privacy_pass_token
         WHERE operation_type = ?",
        operation_type
    )
    .fetch_all(connection.as_mut())
    .await?;
    Ok(rows.into_iter().map(|id| id as u8).collect())
}

/// Deletes the stored tokens issued under a single key.
///
/// Returns the number of deleted tokens.
pub(crate) async fn delete_tokens_for_key(
    mut connection: impl WriteConnection,
    operation_type: OperationType,
    token_key_id: u8,
) -> sqlx::Result<u64> {
    let operation_type = i32::from(operation_type);
    let key_id = i32::from(token_key_id);
    let result = sqlx::query!(
        "DELETE FROM privacy_pass_token
         WHERE operation_type = ? AND token_key_id = ?",
        operation_type,
        key_id
    )
    .execute(connection.as_mut())
    .await?;
    Ok(result.rows_affected())
}

/// Deletes all stored batched token keys.
pub(crate) async fn delete_all_batched_token_keys(
    mut connection: impl WriteConnection,
    operation_type: OperationType,
) -> Result<(), sqlx::Error> {
    let operation_type = i32::from(operation_type);
    sqlx::query!(
        "DELETE FROM batched_token_key WHERE operation_type = ?",
        operation_type
    )
    .execute(connection.as_mut())
    .await?;
    Ok(())
}

/// Loads all batched token public keys for a specific operation type, the key
/// the AS advertised as current first.
pub(crate) async fn load_batched_token_keys(
    mut connection: impl ReadConnection,
    operation_type: OperationType,
) -> Result<Vec<(u8, Vec<u8>)>, sqlx::Error> {
    let operation_type = i32::from(operation_type);
    let rows = sqlx::query!(
        "
        SELECT token_key_id, public_key
        FROM batched_token_key
        WHERE operation_type = ?
        ORDER BY is_current DESC, token_key_id DESC
        ",
        operation_type,
    )
    .fetch_all(connection.as_mut())
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| (r.token_key_id as u8, r.public_key))
        .collect())
}

/// Inserts `candidate` as the token seed of a key unless one is already stored,
/// and returns the seed record that is now stored.
///
/// Set once, so two concurrent runs converge on one seed instead of each
/// deriving from its own: the AS locks an allowance epoch to the first request
/// hash it sees, and a run that kept a losing candidate would only ever get
/// conflicts back. Returning the stored row lets the caller see whether its
/// candidate won and whether the winner is usable for issuance yet.
pub(crate) async fn insert_seed(
    mut connection: impl WriteConnection,
    operation_type: OperationType,
    key_fingerprint: &KeyFingerprint,
    candidate: &[u8; SEED_LEN],
    state: SeedState,
) -> sqlx::Result<StoredSeed> {
    let operation_type = i32::from(operation_type);
    let key_fingerprint = key_fingerprint.as_slice();
    let candidate = candidate.as_slice();
    let state = state.as_str();
    // A fresh proposal has to go out on a commit; a seed committed locally is
    // already agreed, because the only device that commits locally is alone.
    let needs_broadcast = state == SeedState::PROPOSED;
    let now = Utc::now();
    let record = sqlx::query!(
        r#"INSERT INTO privacy_pass_seed (
            operation_type, key_fingerprint, seed, state, needs_broadcast, created_at
        )
        VALUES (?, ?, ?, ?, ?, ?)
        ON CONFLICT (operation_type, key_fingerprint)
        -- Keeping the stored seed and returning it makes the whole set-once
        -- decision one statement, so no writer can interleave with it.
        DO UPDATE SET seed = privacy_pass_seed.seed
        RETURNING seed, state"#,
        operation_type,
        key_fingerprint,
        candidate,
        state,
        needs_broadcast,
        now
    )
    .fetch_one(connection.as_mut())
    .await?;
    StoredSeed::new(record.seed, &record.state)
}

/// Stores `seed` as the committed seed of a key, replacing whatever was there.
///
/// Used when an incoming seed wins: a sibling's commit is ordered ahead of our
/// proposal, or it carries the lower seed of a divergence. Unlike
/// [`insert_seed`] this is not set-once, because the incoming seed is exactly
/// the one the fleet has to converge on.
pub(crate) async fn adopt_seed(
    mut connection: impl WriteConnection,
    operation_type: OperationType,
    key_fingerprint: &KeyFingerprint,
    seed: &[u8; SEED_LEN],
) -> sqlx::Result<()> {
    let operation_type = i32::from(operation_type);
    let key_fingerprint = key_fingerprint.as_slice();
    let seed = seed.as_slice();
    let committed = SeedState::Committed.as_str();
    let now = Utc::now();
    sqlx::query!(
        "INSERT INTO privacy_pass_seed (
            operation_type, key_fingerprint, seed, state, needs_broadcast, created_at
        )
        VALUES (?, ?, ?, ?, FALSE, ?)
        ON CONFLICT (operation_type, key_fingerprint)
        DO UPDATE SET
            seed = excluded.seed,
            state = excluded.state,
            needs_broadcast = FALSE",
        operation_type,
        key_fingerprint,
        seed,
        committed,
        now
    )
    .execute(connection.as_mut())
    .await?;
    Ok(())
}

/// Marks a proposed seed committed, once the commit carrying it was accepted.
///
/// Matches on the seed bytes and on the proposed state, so an accepted commit
/// cannot promote a seed the fleet has meanwhile agreed to replace. Returns
/// whether a row was promoted.
pub(crate) async fn mark_seed_committed(
    mut connection: impl WriteConnection,
    operation_type: OperationType,
    key_fingerprint: &KeyFingerprint,
    seed: &[u8; SEED_LEN],
) -> sqlx::Result<bool> {
    let operation_type = i32::from(operation_type);
    let key_fingerprint = key_fingerprint.as_slice();
    let seed = seed.as_slice();
    let proposed = SeedState::PROPOSED;
    let committed = SeedState::Committed.as_str();
    let result = sqlx::query!(
        "UPDATE privacy_pass_seed
         SET state = ?, needs_broadcast = FALSE
         WHERE operation_type = ? AND key_fingerprint = ? AND seed = ? AND state = ?",
        committed,
        operation_type,
        key_fingerprint,
        seed,
        proposed
    )
    .execute(connection.as_mut())
    .await?;
    Ok(result.rows_affected() > 0)
}

/// Clears the broadcast flag of a seed once the commit carrying it landed.
pub(crate) async fn clear_needs_broadcast(
    mut connection: impl WriteConnection,
    operation_type: OperationType,
    key_fingerprint: &KeyFingerprint,
    seed: &[u8; SEED_LEN],
) -> sqlx::Result<()> {
    let operation_type = i32::from(operation_type);
    let key_fingerprint = key_fingerprint.as_slice();
    let seed = seed.as_slice();
    sqlx::query!(
        "UPDATE privacy_pass_seed
         SET needs_broadcast = FALSE
         WHERE operation_type = ? AND key_fingerprint = ? AND seed = ?",
        operation_type,
        key_fingerprint,
        seed
    )
    .execute(connection.as_mut())
    .await?;
    Ok(())
}

/// Marks a committed seed for re-broadcast, so a sibling holding a higher seed
/// converges on it.
pub(crate) async fn mark_needs_broadcast(
    mut connection: impl WriteConnection,
    operation_type: OperationType,
    key_fingerprint: &KeyFingerprint,
) -> sqlx::Result<()> {
    let operation_type = i32::from(operation_type);
    let key_fingerprint = key_fingerprint.as_slice();
    sqlx::query!(
        "UPDATE privacy_pass_seed
         SET needs_broadcast = TRUE
         WHERE operation_type = ? AND key_fingerprint = ?",
        operation_type,
        key_fingerprint
    )
    .execute(connection.as_mut())
    .await?;
    Ok(())
}

/// Deletes a seed that is still proposed and still holds these bytes.
///
/// Rollback for a proposal whose commit failed terminally: the seed was never
/// agreed, so dropping it lets a later run propose again. A seed the fleet has
/// meanwhile committed does not match and survives.
pub(crate) async fn delete_proposed_seed(
    mut connection: impl WriteConnection,
    operation_type: OperationType,
    key_fingerprint: &KeyFingerprint,
    seed: &[u8; SEED_LEN],
) -> sqlx::Result<()> {
    let operation_type = i32::from(operation_type);
    let key_fingerprint = key_fingerprint.as_slice();
    let seed = seed.as_slice();
    let proposed = SeedState::PROPOSED;
    sqlx::query!(
        "DELETE FROM privacy_pass_seed
         WHERE operation_type = ? AND key_fingerprint = ? AND seed = ? AND state = ?",
        operation_type,
        key_fingerprint,
        seed,
        proposed
    )
    .execute(connection.as_mut())
    .await?;
    Ok(())
}

/// Loads the stored seed of a (operation type, key), whatever its state.
pub(crate) async fn load_seed(
    mut connection: impl ReadConnection,
    operation_type: OperationType,
    key_fingerprint: &KeyFingerprint,
) -> sqlx::Result<Option<StoredSeed>> {
    let operation_type = i32::from(operation_type);
    let key_fingerprint = key_fingerprint.as_slice();
    let record = sqlx::query!(
        "SELECT seed, state FROM privacy_pass_seed
         WHERE operation_type = ? AND key_fingerprint = ?",
        operation_type,
        key_fingerprint
    )
    .fetch_optional(connection.as_mut())
    .await?;
    record
        .map(|record| StoredSeed::new(record.seed, &record.state))
        .transpose()
}

/// Loads every seed that still has to go out on a self-group commit.
pub(crate) async fn load_seeds_needing_broadcast(
    mut connection: impl ReadConnection,
) -> sqlx::Result<Vec<SeedRecord>> {
    let rows = sqlx::query!(
        "SELECT operation_type, key_fingerprint, seed FROM privacy_pass_seed
         WHERE needs_broadcast
         ORDER BY operation_type, key_fingerprint"
    )
    .fetch_all(connection.as_mut())
    .await?;
    rows.into_iter()
        .map(|row| SeedRecord::decode(row.operation_type, row.key_fingerprint, row.seed))
        .collect()
}

/// Loads every committed seed, for the provisioning snapshot a new device gets.
pub(crate) async fn load_committed_seeds(
    mut connection: impl ReadConnection,
) -> sqlx::Result<Vec<SeedRecord>> {
    let committed = SeedState::COMMITTED;
    let rows = sqlx::query!(
        "SELECT operation_type, key_fingerprint, seed FROM privacy_pass_seed
         WHERE state = ?
         ORDER BY operation_type, key_fingerprint",
        committed
    )
    .fetch_all(connection.as_mut())
    .await?;
    rows.into_iter()
        .map(|row| SeedRecord::decode(row.operation_type, row.key_fingerprint, row.seed))
        .collect()
}

impl SeedRecord {
    fn decode(operation_type: i64, key_fingerprint: Vec<u8>, seed: Vec<u8>) -> sqlx::Result<Self> {
        let decoded = i32::try_from(operation_type)
            .ok()
            .and_then(|value| OperationType::try_from(value).ok())
            .ok_or_else(|| {
                sqlx::Error::Decode(format!("unknown operation type {operation_type}").into())
            })?;
        Ok(Self {
            operation_type: decoded,
            key_fingerprint: decode_fingerprint(key_fingerprint)?,
            seed: decode_seed(seed)?,
        })
    }
}

fn decode_seed(seed: Vec<u8>) -> sqlx::Result<[u8; SEED_LEN]> {
    seed.try_into()
        .map_err(|_| sqlx::Error::Decode("token seed is not 32 bytes".into()))
}

fn decode_fingerprint(fingerprint: Vec<u8>) -> sqlx::Result<KeyFingerprint> {
    fingerprint
        .try_into()
        .map_err(|_| sqlx::Error::Decode("key fingerprint is not 32 bytes".into()))
}

/// Deletes the token seed of a (operation type, key).
pub(crate) async fn delete_seed(
    mut connection: impl WriteConnection,
    operation_type: OperationType,
    key_fingerprint: &KeyFingerprint,
) -> sqlx::Result<()> {
    let operation_type = i32::from(operation_type);
    let key_fingerprint = key_fingerprint.as_slice();
    sqlx::query!(
        "DELETE FROM privacy_pass_seed
         WHERE operation_type = ? AND key_fingerprint = ?",
        operation_type,
        key_fingerprint
    )
    .execute(connection.as_mut())
    .await?;
    Ok(())
}

/// Records that the batch of an allowance epoch was fetched.
pub(crate) async fn store_batch(
    mut connection: impl WriteConnection,
    operation_type: OperationType,
    key_fingerprint: &KeyFingerprint,
    allowance_epoch: u32,
) -> sqlx::Result<()> {
    let operation_type = i32::from(operation_type);
    let key_fingerprint = key_fingerprint.as_slice();
    let allowance_epoch = i64::from(allowance_epoch);
    let now = Utc::now();
    sqlx::query!(
        "INSERT INTO privacy_pass_batch (
            operation_type, key_fingerprint, allowance_epoch, fetched_at
        )
        VALUES (?, ?, ?, ?)
        ON CONFLICT (operation_type, key_fingerprint, allowance_epoch)
        DO NOTHING",
        operation_type,
        key_fingerprint,
        allowance_epoch,
        now
    )
    .execute(connection.as_mut())
    .await?;
    Ok(())
}

/// Returns whether the batch of an allowance epoch was already fetched.
pub(crate) async fn batch_was_fetched(
    mut connection: impl ReadConnection,
    operation_type: OperationType,
    key_fingerprint: &KeyFingerprint,
    allowance_epoch: u32,
) -> sqlx::Result<bool> {
    let operation_type = i32::from(operation_type);
    let key_fingerprint = key_fingerprint.as_slice();
    let allowance_epoch = i64::from(allowance_epoch);
    let count = sqlx::query_scalar!(
        "SELECT COUNT(*) FROM privacy_pass_batch
         WHERE operation_type = ? AND key_fingerprint = ? AND allowance_epoch = ?",
        operation_type,
        key_fingerprint,
        allowance_epoch
    )
    .fetch_one(connection.as_mut())
    .await?;
    Ok(count > 0)
}

/// Deletes the fetch records of a single key.
pub(crate) async fn delete_batches_for_key(
    mut connection: impl WriteConnection,
    operation_type: OperationType,
    key_fingerprint: &KeyFingerprint,
) -> sqlx::Result<()> {
    let operation_type = i32::from(operation_type);
    let key_fingerprint = key_fingerprint.as_slice();
    sqlx::query!(
        "DELETE FROM privacy_pass_batch
         WHERE operation_type = ? AND key_fingerprint = ?",
        operation_type,
        key_fingerprint
    )
    .execute(connection.as_mut())
    .await?;
    Ok(())
}

/// Deletes all fetch records of an operation type, so that the next run
/// re-fetches the batch.
pub(crate) async fn delete_all_batches(
    mut connection: impl WriteConnection,
    operation_type: OperationType,
) -> sqlx::Result<()> {
    let operation_type = i32::from(operation_type);
    sqlx::query!(
        "DELETE FROM privacy_pass_batch WHERE operation_type = ?",
        operation_type
    )
    .execute(connection.as_mut())
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use sqlx::SqlitePool;

    use crate::db::access::DbAccess;

    use super::*;

    const OP1: OperationType = OperationType::AddUsername;
    const OP2: OperationType = OperationType::GetInviteCode;

    /// Tokens are consumed in FIFO order.
    #[sqlx::test]
    async fn store_and_consume_fifo(pool: SqlitePool) -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(pool);
        let token_a = b"token_aaa".to_vec();
        let token_b = b"token_bbb".to_vec();

        store_token(pool.write().await?, OP1, 1, &token_a).await?;
        store_token(pool.write().await?, OP1, 1, &token_b).await?;

        assert_eq!(token_count(pool.read().await?, OP1).await?, 2);

        // Consume returns FIFO order.
        let first = consume_token(pool.write().await?, OP1)
            .await?
            .expect("should have a token");
        assert_eq!(first, token_a);
        let second = consume_token(pool.write().await?, OP1)
            .await?
            .expect("should have a token");
        assert_eq!(second, token_b);

        // Empty after consuming both.
        assert_eq!(token_count(pool.read().await?, OP1).await?, 0);
        assert!(consume_token(pool.write().await?, OP1).await?.is_none());

        Ok(())
    }

    /// Consuming from an empty store returns `None`.
    #[sqlx::test]
    async fn consume_from_empty(pool: SqlitePool) -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(pool);

        assert!(consume_token(pool.write().await?, OP1).await?.is_none());
        assert_eq!(token_count(pool.read().await?, OP1).await?, 0);
        Ok(())
    }

    /// Store and load multiple batched token public keys.
    #[sqlx::test]
    async fn batched_key_store_load(pool: SqlitePool) -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(pool);

        let pk_a = b"public_key_a_32_bytes_padding!!".to_vec();
        let pk_b = b"public_key_b_32_bytes_padding!!".to_vec();

        store_batched_token_key(pool.write().await?, 1, OP1, &pk_a, true).await?;
        store_batched_token_key(pool.write().await?, 2, OP1, &pk_b, true).await?;

        let keys = load_batched_token_keys(pool.read().await?, OP1).await?;
        assert_eq!(keys.len(), 2);
        assert!(keys.contains(&(1u8, pk_a.clone())));
        assert!(keys.contains(&(2u8, pk_b)));

        Ok(())
    }

    /// The current key is loaded first even when another key has a higher ID.
    #[sqlx::test]
    async fn batched_key_current_first(pool: SqlitePool) -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(pool);

        let pk_outgoing = b"outgoing_key_padded_32_bytes!!!".to_vec();
        let pk_current = b"current_key_padded_32_bytes!!!!".to_vec();

        store_batched_token_key(pool.write().await?, 5, OP1, &pk_outgoing, false).await?;
        store_batched_token_key(pool.write().await?, 3, OP1, &pk_current, true).await?;

        let keys = load_batched_token_keys(pool.read().await?, OP1).await?;
        assert_eq!(keys, vec![(3u8, pk_current), (5u8, pk_outgoing)]);

        Ok(())
    }

    /// `delete_all_tokens` removes every stored token.
    #[sqlx::test]
    async fn delete_all_tokens_clears_store(pool: SqlitePool) -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(pool);

        store_token(pool.write().await?, OP1, 1, b"aaa").await?;
        store_token(pool.write().await?, OP1, 1, b"bbb").await?;
        assert_eq!(token_count(pool.read().await?, OP1).await?, 2);

        delete_all_tokens(pool.write().await?, OP1).await?;
        assert_eq!(token_count(pool.read().await?, OP1).await?, 0);
        assert!(consume_token(pool.write().await?, OP1).await?.is_none());

        Ok(())
    }

    /// `delete_all_batched_token_keys` removes every stored key.
    #[sqlx::test]
    async fn delete_all_keys_clears_store(pool: SqlitePool) -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(pool);

        store_batched_token_key(pool.write().await?, 1, OP1, b"pk1", true).await?;
        store_batched_token_key(pool.write().await?, 2, OP1, b"pk2", true).await?;
        assert_eq!(
            load_batched_token_keys(pool.read().await?, OP1)
                .await?
                .len(),
            2
        );

        delete_all_batched_token_keys(pool.write().await?, OP1).await?;
        assert!(
            load_batched_token_keys(pool.read().await?, OP1)
                .await?
                .is_empty()
        );

        Ok(())
    }

    /// Re-inserting a key with the same ID updates the public key (upsert).
    #[sqlx::test]
    async fn batched_key_upsert(pool: SqlitePool) -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(pool);

        let pk_old = b"old_key_padded_to_32_bytes!!!!!".to_vec();
        let pk_new = b"new_key_padded_to_32_bytes!!!!!".to_vec();

        store_batched_token_key(pool.write().await?, 1, OP1, &pk_old, true).await?;
        store_batched_token_key(pool.write().await?, 1, OP1, &pk_new, true).await?;

        let keys = load_batched_token_keys(pool.read().await?, OP1).await?;
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0], (1u8, pk_new));

        Ok(())
    }

    /// Tokens stored under OP1 are not visible to OP2 and vice-versa.
    #[sqlx::test]
    async fn tokens_are_isolated_between_operation_types(pool: SqlitePool) -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(pool);

        let token_op1 = b"token_op1".to_vec();
        let token_op2 = b"token_op2".to_vec();

        store_token(pool.write().await?, OP1, 1, &token_op1).await?;
        store_token(pool.write().await?, OP2, 1, &token_op2).await?;

        // Each operation type sees exactly its own token.
        assert_eq!(token_count(pool.read().await?, OP1).await?, 1);
        assert_eq!(token_count(pool.read().await?, OP2).await?, 1);

        // Consuming OP1 returns only the OP1 token and leaves OP2 untouched.
        let consumed = consume_token(pool.write().await?, OP1)
            .await?
            .expect("should have a token");
        assert_eq!(consumed, token_op1);
        assert_eq!(token_count(pool.read().await?, OP1).await?, 0);
        assert_eq!(token_count(pool.read().await?, OP2).await?, 1);

        // Consuming OP2 returns only the OP2 token.
        let consumed = consume_token(pool.write().await?, OP2)
            .await?
            .expect("should have a token");
        assert_eq!(consumed, token_op2);
        assert_eq!(token_count(pool.read().await?, OP2).await?, 0);

        Ok(())
    }

    /// `delete_all_tokens` for OP1 does not remove OP2 tokens.
    #[sqlx::test]
    async fn delete_all_tokens_does_not_affect_other_operation_type(
        pool: SqlitePool,
    ) -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(pool);

        store_token(pool.write().await?, OP1, 1, b"op1_token").await?;
        store_token(pool.write().await?, OP2, 1, b"op2_token").await?;

        delete_all_tokens(pool.write().await?, OP1).await?;

        assert_eq!(token_count(pool.read().await?, OP1).await?, 0);
        assert_eq!(token_count(pool.read().await?, OP2).await?, 1);

        Ok(())
    }

    /// Batched token keys stored under OP1 are not visible to OP2 and vice-versa.
    #[sqlx::test]
    async fn batched_keys_are_isolated_between_operation_types(
        pool: SqlitePool,
    ) -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(pool);

        let pk_op1 = b"public_key_op1_padded_32bytes!!".to_vec();
        let pk_op2 = b"public_key_op2_padded_32bytes!!".to_vec();

        // Same key ID, different operation types — must not collide.
        store_batched_token_key(pool.write().await?, 1, OP1, &pk_op1, true).await?;
        store_batched_token_key(pool.write().await?, 1, OP2, &pk_op2, true).await?;

        let keys_op1 = load_batched_token_keys(pool.read().await?, OP1).await?;
        let keys_op2 = load_batched_token_keys(pool.read().await?, OP2).await?;

        assert_eq!(keys_op1, vec![(1u8, pk_op1)]);
        assert_eq!(keys_op2, vec![(1u8, pk_op2)]);

        Ok(())
    }

    /// `delete_all_batched_token_keys` for OP1 does not remove OP2 keys.
    #[sqlx::test]
    async fn delete_all_keys_does_not_affect_other_operation_type(
        pool: SqlitePool,
    ) -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(pool);

        store_batched_token_key(pool.write().await?, 1, OP1, b"pk_op1", true).await?;
        store_batched_token_key(pool.write().await?, 1, OP2, b"pk_op2", true).await?;

        delete_all_batched_token_keys(pool.write().await?, OP1).await?;

        assert!(
            load_batched_token_keys(pool.read().await?, OP1)
                .await?
                .is_empty()
        );
        assert_eq!(
            load_batched_token_keys(pool.read().await?, OP2)
                .await?
                .len(),
            1
        );

        Ok(())
    }

    /// Upserting a key under OP1 does not overwrite the same key ID stored under OP2.
    #[sqlx::test]
    async fn batched_key_upsert_does_not_affect_other_operation_type(
        pool: SqlitePool,
    ) -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(pool);

        let pk_op1_v1 = b"op1_old_key_padded_32_bytes!!!!".to_vec();
        let pk_op1_v2 = b"op1_new_key_padded_32_bytes!!!!".to_vec();
        let pk_op2 = b"op2_key_should_not_change!!!!!!".to_vec();

        store_batched_token_key(pool.write().await?, 1, OP1, &pk_op1_v1, true).await?;
        store_batched_token_key(pool.write().await?, 1, OP2, &pk_op2, true).await?;

        // Upsert key ID 1 for OP1 — must not touch OP2's key ID 1.
        store_batched_token_key(pool.write().await?, 1, OP1, &pk_op1_v2, true).await?;

        let keys_op1 = load_batched_token_keys(pool.read().await?, OP1).await?;
        let keys_op2 = load_batched_token_keys(pool.read().await?, OP2).await?;

        assert_eq!(keys_op1, vec![(1u8, pk_op1_v2)]);
        assert_eq!(keys_op2, vec![(1u8, pk_op2)]);

        Ok(())
    }
}
