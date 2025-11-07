// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::borrow::Borrow;

use aircommon::{
    crypto::{ConnectionDecryptionKey, hash::Hashable},
    identifiers::UserHandle,
    messages::connection_package::{ConnectionPackage, ConnectionPackageHash},
};
use sqlx::{Result, SqliteConnection, query, query_scalar};

pub(crate) trait StorableConnectionPackage: Sized + Borrow<ConnectionPackage> {
    /// Store the connection package in the database.
    ///
    /// Returns an error if the storage fails.
    async fn store_for_handle(
        &self,
        connection: &mut SqliteConnection,
        handle: &UserHandle,
        decryption_key: &ConnectionDecryptionKey,
    ) -> Result<()> {
        let cp = self.borrow();
        let hash = cp.hash();
        let not_after = cp.expires_at();
        let is_last_resort = cp.is_last_resort();
        query!(
            "INSERT INTO connection_package
                 (connection_package_hash, handle, decryption_key, expires_at, is_last_resort)
                 VALUES ($1, $2, $3, $4, $5)",
            hash,
            handle,
            decryption_key,
            not_after,
            is_last_resort
        )
        .execute(connection)
        .await?;

        Ok(())
    }

    async fn load_decryption_key(
        connection: &mut SqliteConnection,
        hash: &ConnectionPackageHash,
    ) -> Result<Option<ConnectionDecryptionKey>> {
        query_scalar!(
            r#"SELECT decryption_key
                AS "decryption_key: _"
            FROM connection_package
            WHERE connection_package_hash = $1"#,
            hash
        )
        .fetch_optional(connection)
        .await
    }

    async fn delete(connection: &mut SqliteConnection, hash: &ConnectionPackageHash) -> Result<()> {
        query!(
            "DELETE FROM connection_package WHERE connection_package_hash = $1",
            hash
        )
        .execute(connection)
        .await?;
        Ok(())
    }

    async fn is_last_resort(
        connection: &mut SqliteConnection,
        hash: &ConnectionPackageHash,
    ) -> Result<Option<bool>> {
        query_scalar!(
            r#"SELECT is_last_resort
            FROM connection_package
            WHERE connection_package_hash = $1"#,
            hash
        )
        .fetch_one(connection)
        .await
    }
}

impl StorableConnectionPackage for ConnectionPackage {}

#[cfg(test)]
mod tests {
    use crate::UserHandleRecord;

    use super::*;

    use aircommon::credentials::keys::HandleSigningKey;

    use sqlx::SqlitePool;

    #[sqlx::test]
    async fn test_store_and_load_connection_package(pool: SqlitePool) {
        let handle = UserHandle::new("test_handle".to_string()).unwrap();
        let signing_key = HandleSigningKey::generate().unwrap();
        let hash = handle.calculate_hash().unwrap();
        let record = UserHandleRecord::new(handle, hash, signing_key);
        record.store(&pool).await.unwrap();
        let (decryption_key, connection_package) =
            ConnectionPackage::new(record.hash, &record.signing_key, false).unwrap();

        let mut connection = pool.acquire().await.unwrap();
        connection_package
            .store_for_handle(&mut connection, &record.handle, &decryption_key)
            .await
            .unwrap();

        let loaded_decryption_key =
            ConnectionPackage::load_decryption_key(&mut connection, &connection_package.hash())
                .await
                .unwrap()
                .unwrap();
        assert_eq!(loaded_decryption_key, decryption_key);
        ConnectionPackage::delete(&mut connection, &connection_package.hash())
            .await
            .unwrap();
        let loaded_decryption_key_after_delete =
            ConnectionPackage::load_decryption_key(&mut connection, &connection_package.hash())
                .await
                .unwrap();
        assert!(loaded_decryption_key_after_delete.is_none());
    }
}
