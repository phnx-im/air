// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::{
    crypto::ConnectionDecryptionKey,
    identifiers::Username,
    messages::connection_package::{ConnectionPackageHash, ConnectionPackageMetadata},
    time::TimeStamp,
};
use sqlx::{Result, query, query_scalar};

use crate::db::access::{ReadConnection, WriteConnection};

pub(crate) struct ConnectionPackageRecord {
    pub(crate) hash: ConnectionPackageHash,
    pub(crate) expires_at: TimeStamp,
    pub(crate) is_last_resort: bool,
}

impl From<ConnectionPackageMetadata> for ConnectionPackageRecord {
    fn from(
        ConnectionPackageMetadata {
            hash,
            lifetime,
            is_last_resort,
        }: ConnectionPackageMetadata,
    ) -> Self {
        Self {
            hash,
            expires_at: lifetime.not_after(),
            is_last_resort,
        }
    }
}

impl ConnectionPackageRecord {
    /// Store the connection package in the database.
    ///
    /// Returns an error if the storage fails.
    pub(crate) async fn store_for_username(
        &self,
        mut connection: impl WriteConnection,
        username: &Username,
        decryption_key: &ConnectionDecryptionKey,
    ) -> Result<()> {
        query!(
            "INSERT INTO connection_package
                 (connection_package_hash, handle, decryption_key, expires_at, is_last_resort)
                 VALUES ($1, $2, $3, $4, $5)",
            self.hash,
            username,
            decryption_key,
            self.expires_at,
            self.is_last_resort
        )
        .execute(connection.as_mut())
        .await?;

        Ok(())
    }

    pub(crate) async fn load_decryption_key(
        mut connection: impl ReadConnection,
        hash: &ConnectionPackageHash,
    ) -> Result<Option<ConnectionDecryptionKey>> {
        query_scalar!(
            r#"SELECT decryption_key
                AS "decryption_key: _"
            FROM connection_package
            WHERE connection_package_hash = $1"#,
            hash
        )
        .fetch_optional(connection.as_mut())
        .await
    }

    pub(crate) async fn delete(
        mut connection: impl WriteConnection,
        hash: &ConnectionPackageHash,
    ) -> Result<()> {
        query!(
            "DELETE FROM connection_package WHERE connection_package_hash = $1",
            hash
        )
        .execute(connection.as_mut())
        .await?;
        Ok(())
    }

    pub(crate) async fn load_is_last_resort(
        mut connection: impl ReadConnection,
        hash: &ConnectionPackageHash,
    ) -> Result<Option<bool>> {
        query_scalar!(
            r#"SELECT is_last_resort
            FROM connection_package
            WHERE connection_package_hash = $1"#,
            hash
        )
        .fetch_one(connection.as_mut())
        .await
    }
}

#[cfg(test)]
mod tests {
    use crate::{UsernameRecord, db::access::DbAccess};

    use super::*;

    use aircommon::{
        credentials::keys::UsernameSigningKey, messages::connection_package::ConnectionPackage,
    };

    use sqlx::SqlitePool;

    #[sqlx::test]
    async fn test_store_and_load_connection_package(pool: SqlitePool) {
        let pool = DbAccess::for_tests(pool);
        let mut connection = pool.write().await.unwrap();

        let username = Username::new("test-handle".to_string()).unwrap();
        let signing_key = UsernameSigningKey::generate().unwrap();
        let hash = username.calculate_hash().unwrap();
        let username_record = UsernameRecord::new(username, hash, signing_key);
        username_record.store(&mut connection).await.unwrap();
        let (decryption_key, _package, metadata) =
            ConnectionPackage::generate(username_record.hash, &username_record.signing_key, false)
                .unwrap();
        let record = ConnectionPackageRecord::from(metadata);

        record
            .store_for_username(&mut connection, &username_record.username, &decryption_key)
            .await
            .unwrap();

        let loaded_decryption_key =
            ConnectionPackageRecord::load_decryption_key(&mut connection, &record.hash)
                .await
                .unwrap()
                .unwrap();
        assert_eq!(loaded_decryption_key, decryption_key);
        ConnectionPackageRecord::delete(&mut connection, &record.hash)
            .await
            .unwrap();
        let loaded_decryption_key_after_delete =
            ConnectionPackageRecord::load_decryption_key(&mut connection, &record.hash)
                .await
                .unwrap();
        assert!(loaded_decryption_key_after_delete.is_none());
    }
}
