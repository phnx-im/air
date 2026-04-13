// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::{HashMap, HashSet};

use aircommon::codec::{BlobDecoded, BlobEncoded};
use airprotos::common::v1::OperationType;
use async_trait::async_trait;
use privacypass::{
    Nonce, NonceStore, TruncatedTokenKeyId,
    amortized_tokens::server::Server,
    common::{private::serialize_public_key, store::PrivateKeyStore},
    private_tokens::{Ristretto255, VoprfServer},
};
use sqlx::{Acquire, PgConnection, PgExecutor, PgPool};
use tokio::sync::Mutex;
use tracing::{error, info};

pub(super) struct AuthServiceBatchedKeyStoreProvider<'a> {
    /// Shared mutex over a single connection. Safe because the privacypass
    /// library accesses key store and nonce store sequentially, never
    /// holding borrows on both simultaneously.
    connection: &'a Mutex<&'a mut PgConnection>,
}

impl<'a> AuthServiceBatchedKeyStoreProvider<'a> {
    pub(super) fn new(connection: &'a Mutex<&'a mut PgConnection>) -> Self {
        Self { connection }
    }
}

#[async_trait]
impl PrivateKeyStore for AuthServiceBatchedKeyStoreProvider<'_> {
    type CS = Ristretto255;
    /// Inserts a keypair with a given `truncated_token_key_id` into the key store.
    ///
    /// On conflict, an error is logged and the value is not inserted.
    async fn insert(
        &self,
        truncated_token_key_id: TruncatedTokenKeyId,
        server: VoprfServer<Ristretto255>,
    ) -> bool {
        let server = BlobEncoded(server);
        match sqlx::query!(
            "INSERT INTO as_batched_key (token_key_id, voprf_server)
            VALUES ($1, $2)",
            truncated_token_key_id as i16,
            server as _
        )
        .execute(&mut **self.connection.lock().await)
        .await
        {
            Ok(res) => res.rows_affected() > 0,
            Err(error) => {
                error!(%error, "Failed to insert key into batched key store");
                false
            }
        }
    }

    /// Returns a keypair with a given `truncated_token_key_id` from the key store.
    async fn get(
        &self,
        truncated_token_key_id: &TruncatedTokenKeyId,
    ) -> Option<VoprfServer<Ristretto255>> {
        let token_key_id: i16 = (*truncated_token_key_id).into();
        sqlx::query_scalar!(
            r#"SELECT voprf_server AS "voprf_server: BlobDecoded<VoprfServer<Ristretto255>>"
            FROM as_batched_key
            WHERE token_key_id = $1"#,
            token_key_id
        )
        .fetch_optional(&mut **self.connection.lock().await)
        .await
        .inspect_err(|error| error!(%error, "Failed to fetch key from batched key store"))
        .ok()?
        .map(|BlobDecoded(voprf_server)| voprf_server)
    }

    async fn remove(&self, truncated_token_key_id: &TruncatedTokenKeyId) -> bool {
        let token_key_id: i16 = (*truncated_token_key_id).into();
        match sqlx::query!(
            "DELETE FROM as_batched_key WHERE token_key_id = $1",
            token_key_id
        )
        .execute(&mut **self.connection.lock().await)
        .await
        {
            Ok(res) => res.rows_affected() > 0,
            Err(error) => {
                error!(%error, "Failed to remove key from batched key store");
                false
            }
        }
    }
}

// --- Nonce Store ---

pub(super) struct AuthServiceNonceStore<'a> {
    connection: &'a Mutex<&'a mut PgConnection>,
}

impl<'a> AuthServiceNonceStore<'a> {
    pub(super) fn new(connection: &'a Mutex<&'a mut PgConnection>) -> Self {
        Self { connection }
    }
}

#[async_trait]
impl NonceStore for AuthServiceNonceStore<'_> {
    async fn reserve(&self, nonce: &Nonce) -> bool {
        let nonce_bytes = nonce.as_slice();
        match sqlx::query!(
            "INSERT INTO as_token_nonce (nonce, status) \
             VALUES ($1, 'reserved') \
             ON CONFLICT DO NOTHING",
            nonce_bytes
        )
        .execute(&mut **self.connection.lock().await)
        .await
        {
            Ok(res) => res.rows_affected() > 0,
            Err(error) => {
                error!(%error, "Failed to reserve nonce");
                false
            }
        }
    }

    async fn commit(&self, nonce: &Nonce) {
        let nonce_bytes = nonce.as_slice();
        if let Err(error) = sqlx::query!(
            "UPDATE as_token_nonce \
             SET status = 'committed' \
             WHERE nonce = $1 AND status = 'reserved'",
            nonce_bytes
        )
        .execute(&mut **self.connection.lock().await)
        .await
        {
            error!(%error, "Failed to commit nonce");
        }
    }

    async fn release(&self, nonce: &Nonce) {
        let nonce_bytes = nonce.as_slice();
        if let Err(error) = sqlx::query!(
            "DELETE FROM as_token_nonce \
             WHERE nonce = $1 AND status = 'reserved'",
            nonce_bytes
        )
        .execute(&mut **self.connection.lock().await)
        .await
        {
            error!(%error, "Failed to release nonce");
        }
    }
}

// --- Key Rotation ---

/// Duration after which a VOPRF key is considered stale and a new key should
/// be generated.
const KEY_ROTATION_PERIOD_DAYS: i64 = 90;

/// Grace period after rotation during which the old key remains valid for
/// token redemption.
const KEY_OVERLAP_DAYS: i64 = 7;

/// Returns the `token_key_id` of the most recently created VOPRF key.
pub(crate) async fn load_current_key_id(
    executor: impl PgExecutor<'_>,
    operation_type: OperationType,
) -> Result<Option<i16>, sqlx::Error> {
    sqlx::query_scalar!(
        "
        SELECT token_key_id
        FROM as_batched_key
        WHERE operation_type = $1
        ORDER BY created_at DESC LIMIT 1
        ",
        operation_type as i16,
    )
    .fetch_optional(executor)
    .await
}

/// Checks whether key rotation is needed and performs it if so.
///
/// Creates a new VOPRF keypair if no key exists or if the current key is older
/// than [`KEY_ROTATION_PERIOD_DAYS`]. Removes keys older than the rotation
/// period plus the overlap window.
///
/// Returns `true` if a new key was created.
pub async fn rotate_keys_if_needed(
    pool: &PgPool,
) -> Result<HashSet<OperationType>, RotateKeysError> {
    let mut results = HashSet::new();
    for operation_type in OperationType::all() {
        if rotate_keys_if_needed_for_operation_type(pool, operation_type).await? {
            results.insert(operation_type);
        }
    }
    Ok(results)
}

/// Checks whether key rotation is needed and performs it if so.
///
/// Creates a new VOPRF keypair if no key exists or if the current key is older
/// than [`KEY_ROTATION_PERIOD_DAYS`]. Removes keys older than the rotation
/// period plus the overlap window.
///
/// Returns `true` if a new key was created.
async fn rotate_keys_if_needed_for_operation_type(
    pool: &PgPool,
    operation_type: OperationType,
) -> Result<bool, RotateKeysError> {
    let needs_rotation = sqlx::query_scalar!(
        "SELECT NOT EXISTS(
            SELECT 1 FROM as_batched_key 
            WHERE operation_type = $1 AND created_at > now() - make_interval(days => $2)
        )",
        operation_type as i16,
        KEY_ROTATION_PERIOD_DAYS as i32
    )
    .fetch_one(pool)
    .await
    .map_err(RotateKeysError::Storage)?
    .unwrap_or(true);

    let rotated = if needs_rotation {
        let mut transaction = pool.begin().await.map_err(RotateKeysError::Storage)?;
        {
            let conn_mutex = Mutex::new(&mut *transaction);
            let key_store = AuthServiceBatchedKeyStoreProvider::new(&conn_mutex);
            let server = Server::<Ristretto255>::new();
            server
                .create_keypair(&key_store)
                .await
                .map_err(RotateKeysError::KeyGeneration)?;
        }
        transaction
            .commit()
            .await
            .map_err(RotateKeysError::Storage)?;
        info!(%operation_type, "created new VOPRF keypair");
        true
    } else {
        false
    };

    // Remove keys past the overlap window.
    let max_age_days = (KEY_ROTATION_PERIOD_DAYS + KEY_OVERLAP_DAYS) as i32;
    let removed = sqlx::query!(
        "DELETE FROM as_batched_key \
         WHERE created_at < now() - make_interval(days => $1)",
        max_age_days
    )
    .execute(pool)
    .await
    .map_err(RotateKeysError::Storage)?;

    if removed.rows_affected() > 0 {
        info!(
            removed = removed.rows_affected(),
            "removed expired VOPRF keys"
        );
    }

    // Prune committed nonces older than the overlap window. Tokens issued
    // under expired keys can no longer be redeemed, so their nonces are
    // no longer needed for double-spend protection.
    let nonces_removed = sqlx::query!(
        "DELETE FROM as_token_nonce \
         WHERE created_at < now() - make_interval(days => $1)",
        max_age_days
    )
    .execute(pool)
    .await
    .map_err(RotateKeysError::Storage)?;

    if nonces_removed.rows_affected() > 0 {
        info!(
            removed = nonces_removed.rows_affected(),
            "removed expired nonces"
        );
    }

    Ok(rotated)
}

#[derive(Debug, thiserror::Error)]
pub enum RotateKeysError {
    #[error("storage error: {0}")]
    Storage(sqlx::Error),
    #[error("key generation error: {0}")]
    KeyGeneration(privacypass::common::errors::CreateKeypairError),
}

// --- Public Key Retrieval ---

/// Batched token key for distribution to clients.
pub(super) struct BatchedTokenKeyRecord {
    pub(super) token_key_id: u8,
    pub(super) public_key: Vec<u8>,
}

/// Loads all VOPRF public keys from the batched key store.
pub(super) async fn load_batched_token_keys(
    pool: &PgPool,
) -> Result<Vec<BatchedTokenKeyRecord>, sqlx::Error> {
    let rows = sqlx::query_as!(
        BatchedKeyRow,
        r#"SELECT
            token_key_id,
            voprf_server AS "voprf_server: BlobDecoded<VoprfServer<Ristretto255>>"
        FROM as_batched_key
        ORDER BY created_at DESC"#
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| {
            let public_key =
                serialize_public_key::<Ristretto255>(row.voprf_server.0.get_public_key());
            BatchedTokenKeyRecord {
                token_key_id: row.token_key_id as u8,
                public_key,
            }
        })
        .collect())
}

struct BatchedKeyRow {
    token_key_id: i16,
    voprf_server: BlobDecoded<VoprfServer<Ristretto255>>,
}

#[derive(Debug)]
pub(in crate::auth_service) struct TokenAllowance {
    pub(super) operation_type: OperationType,
    pub(super) remaining: i32,
    pub(super) epoch: i16,
}

impl TokenAllowance {
    pub(super) fn new(operation_type: OperationType, epoch: i16) -> Self {
        Self {
            operation_type,
            remaining: operation_type.tokens_allowance(),
            epoch,
        }
    }
}

mod persistence {
    use aircommon::identifiers::UserId;
    use airprotos::common::v1::OperationType;
    use sqlx::{PgExecutor, query};

    use super::TokenAllowance;
    use crate::errors::StorageError;

    impl TokenAllowance {
        pub(in crate::auth_service) async fn store(
            &self,
            connection: impl PgExecutor<'_>,
            user_id: &UserId,
        ) -> Result<(), StorageError> {
            query!(
                "INSERT INTO as_token_allowance (
                    user_uuid,
                    user_domain,
                    operation_type,
                    remaining,
                    epoch
                ) VALUES ($1, $2, $3, $4, $5) ON CONFLICT DO NOTHING",
                user_id.uuid(),
                user_id.domain() as _,
                self.operation_type as i64,
                self.remaining,
                self.epoch,
            )
            .execute(connection)
            .await?;
            Ok(())
        }

        pub(in crate::auth_service) async fn update(
            &self,
            connection: impl PgExecutor<'_>,
            user_id: &UserId,
        ) -> Result<(), StorageError> {
            query!(
                "UPDATE as_token_allowance SET
                    remaining = $4,
                    epoch = $5
                WHERE user_uuid = $1 AND user_domain = $2 AND operation_type = $3",
                user_id.uuid(),
                user_id.domain() as _,
                self.operation_type as i16,
                self.remaining,
                self.epoch
            )
            .execute(connection)
            .await?;
            Ok(())
        }

        #[cfg(test)]
        pub(in crate::auth_service) async fn load(
            connection: impl PgExecutor<'_>,
            user_id: &UserId,
            operation_type: OperationType,
        ) -> Result<Option<Self>, StorageError> {
            query!(
                r#"SELECT
                    remaining,
                    epoch
                FROM as_token_allowance
                WHERE user_uuid = $1 AND user_domain = $2 AND operation_type = $3"#,
                user_id.uuid(),
                user_id.domain() as _,
                operation_type as i16,
            )
            .fetch_optional(connection)
            .await?
            .map(|record| {
                Ok(Self {
                    operation_type,
                    remaining: record.remaining,
                    epoch: record.epoch,
                })
            })
            .transpose()
        }

        pub(in crate::auth_service) async fn load_for_update(
            connection: impl PgExecutor<'_>,
            user_id: &UserId,
            operation_type: OperationType,
        ) -> Result<Option<Self>, StorageError> {
            query!(
                r#"SELECT
                    remaining,
                    epoch
                FROM as_token_allowance
                WHERE user_uuid = $1 AND user_domain = $2 AND operation_type = $3
                FOR UPDATE"#,
                user_id.uuid(),
                user_id.domain() as _,
                operation_type as i16,
            )
            .fetch_optional(connection)
            .await?
            .map(|record| {
                Ok(Self {
                    operation_type,
                    remaining: record.remaining,
                    epoch: record.epoch,
                })
            })
            .transpose()
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::LazyLock;

    use aircommon::codec::PersistenceCodec;
    use rand::{SeedableRng, rngs::StdRng};
    use sqlx::PgPool;

    use super::*;

    /// Insert two VOPRF keys and retrieve each by ID.
    #[sqlx::test]
    async fn insert_get(pool: PgPool) -> anyhow::Result<()> {
        let mut connection = pool.acquire().await?;
        let conn_mutex = Mutex::new(&mut *connection);
        let provider = AuthServiceBatchedKeyStoreProvider::new(&conn_mutex);

        let mut rng = rand::thread_rng();

        let value = VoprfServer::new(&mut rng).unwrap();
        provider.insert(1, value.clone()).await;

        let loaded = provider.get(&1).await.unwrap();
        assert_eq!(loaded, value);

        let value = VoprfServer::new(&mut rng).unwrap();
        provider.insert(2, value.clone()).await;

        let loaded = provider.get(&2).await.unwrap();
        assert_eq!(loaded, value);

        Ok(())
    }

    /// Inserting a key with a duplicate ID is a no-op; the original is kept.
    #[sqlx::test]
    async fn no_insert_on_conflict(pool: PgPool) -> anyhow::Result<()> {
        let mut connection = pool.acquire().await?;
        let conn_mutex = Mutex::new(&mut *connection);
        let provider = AuthServiceBatchedKeyStoreProvider::new(&conn_mutex);

        let mut rng = rand::thread_rng();

        let value_a = VoprfServer::new(&mut rng).unwrap();
        provider.insert(1, value_a.clone()).await;

        let loaded = provider.get(&1).await.unwrap();
        assert_eq!(loaded, value_a);

        let value_b = VoprfServer::new(&mut rng).unwrap();
        provider.insert(1, value_b.clone()).await;

        let loaded = provider.get(&1).await.unwrap();
        assert_eq!(loaded, value_a);

        Ok(())
    }

    /// Removing a key deletes it; removing a non-existent key returns false.
    #[sqlx::test]
    async fn remove_key(pool: PgPool) -> anyhow::Result<()> {
        let mut connection = pool.acquire().await?;
        let conn_mutex = Mutex::new(&mut *connection);
        let provider = AuthServiceBatchedKeyStoreProvider::new(&conn_mutex);
        let mut rng = rand::thread_rng();

        let value = VoprfServer::new(&mut rng).unwrap();
        provider.insert(1, value).await;
        assert!(provider.get(&1).await.is_some());

        assert!(provider.remove(&1).await);
        assert!(provider.get(&1).await.is_none());

        // Removing a non-existent key returns false.
        assert!(!provider.remove(&1).await);

        Ok(())
    }

    /// Reserve → commit lifecycle: a committed nonce cannot be re-reserved.
    #[sqlx::test]
    async fn nonce_reserve_commit(pool: PgPool) -> anyhow::Result<()> {
        let mut connection = pool.acquire().await?;
        let conn_mutex = Mutex::new(&mut *connection);
        let store = AuthServiceNonceStore::new(&conn_mutex);
        let nonce: Nonce = [42u8; 32];

        // First reserve succeeds.
        assert!(store.reserve(&nonce).await);
        // Second reserve of the same nonce fails (already reserved).
        assert!(!store.reserve(&nonce).await);

        // Commit the nonce.
        store.commit(&nonce).await;

        // Reserve still fails after commit (nonce is committed, not absent).
        assert!(!store.reserve(&nonce).await);

        Ok(())
    }

    /// Reserve → release lifecycle: a released nonce can be reserved again.
    #[sqlx::test]
    async fn nonce_reserve_release(pool: PgPool) -> anyhow::Result<()> {
        let mut connection = pool.acquire().await?;
        let conn_mutex = Mutex::new(&mut *connection);
        let store = AuthServiceNonceStore::new(&conn_mutex);
        let nonce: Nonce = [7u8; 32];

        assert!(store.reserve(&nonce).await);

        // Release frees the reservation.
        store.release(&nonce).await;

        // Nonce can be reserved again after release.
        assert!(store.reserve(&nonce).await);

        Ok(())
    }

    /// Stored VOPRF keys can be loaded as public key records (32-byte Ristretto points).
    #[sqlx::test]
    async fn load_public_keys(pool: PgPool) -> anyhow::Result<()> {
        {
            let mut connection = pool.acquire().await?;
            let conn_mutex = Mutex::new(&mut *connection);
            let provider = AuthServiceBatchedKeyStoreProvider::new(&conn_mutex);
            let mut rng = rand::thread_rng();

            let server_a = VoprfServer::new(&mut rng).unwrap();
            let server_b = VoprfServer::new(&mut rng).unwrap();
            provider.insert(1, server_a).await;
            provider.insert(2, server_b).await;
        }

        let keys = load_batched_token_keys(&pool).await?;
        assert_eq!(keys.len(), 2);
        // Public keys should be non-empty Ristretto255 group elements (32 bytes)
        for key in &keys {
            assert_eq!(key.public_key.len(), 32);
        }

        Ok(())
    }

    static SERVER: LazyLock<VoprfServer<Ristretto255>> = LazyLock::new(|| {
        VoprfServer::new(&mut StdRng::seed_from_u64(0x0DDB1A5E5BAD5EEDu64)).unwrap()
    });

    #[test]
    fn test_server_serde_codec() {
        let bytes = PersistenceCodec::to_vec(&*SERVER).unwrap();
        let diag = cbor_diag::parse_bytes(&bytes[1..]).unwrap().to_hex();
        insta::assert_snapshot!(diag);
    }

    #[test]
    fn test_server_serde_json() {
        insta::assert_json_snapshot!(&*SERVER);
    }
}
