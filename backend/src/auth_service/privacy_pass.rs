// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::codec::{BlobDecoded, BlobEncoded};
use async_trait::async_trait;
use privacypass::{
    TruncatedTokenKeyId,
    common::store::PrivateKeyStore,
    private_tokens::{Ristretto255, VoprfServer},
};
use sqlx::PgConnection;
use tokio::sync::Mutex;
use tracing::error;

pub(super) struct AuthServiceBatchedKeyStoreProvider<'a> {
    // Note: PgPool is not used here, because we need to use this provider in a transactional
    // context.
    connection: Mutex<&'a mut PgConnection>,
}

impl<'a> AuthServiceBatchedKeyStoreProvider<'a> {
    pub(super) fn new(connection: &'a mut PgConnection) -> Self {
        Self {
            connection: connection.into(),
        }
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
    ) {
        let server = BlobEncoded(server);
        if let Err(error) = sqlx::query!(
            "INSERT INTO as_batched_key (token_key_id, voprf_server)
            VALUES ($1, $2)",
            truncated_token_key_id as i16,
            server as _
        )
        .execute(&mut **self.connection.lock().await)
        .await
        {
            error!(%error, "Failed to insert key into batched key store");
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
}

#[cfg(test)]
mod tests {
    use std::sync::LazyLock;

    use aircommon::codec::PersistenceCodec;
    use rand::{SeedableRng, rngs::StdRng};
    use sqlx::PgPool;

    use super::*;

    #[sqlx::test]
    async fn insert_get(pool: PgPool) -> anyhow::Result<()> {
        let mut connection = pool.acquire().await?;
        let provider = AuthServiceBatchedKeyStoreProvider::new(&mut connection);

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

    #[sqlx::test]
    async fn no_insert_on_conflict(pool: PgPool) -> anyhow::Result<()> {
        let mut connection = pool.acquire().await?;
        let provider = AuthServiceBatchedKeyStoreProvider::new(&mut connection);

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

    static SERVER: LazyLock<VoprfServer<Ristretto255>> = LazyLock::new(|| {
        VoprfServer::new(&mut StdRng::seed_from_u64(0x0DDB1A5E5BAD5EEDu64)).unwrap()
    });

    #[test]
    fn test_server_serde_codec() {
        insta::assert_binary_snapshot!(".cbor", PersistenceCodec::to_vec(&*SERVER).unwrap());
    }

    #[test]
    fn test_server_serde_json() {
        insta::assert_json_snapshot!(&*SERVER);
    }
}
