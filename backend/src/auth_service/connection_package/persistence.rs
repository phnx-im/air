// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::{
    codec::{BlobDecoded, BlobEncoded},
    identifiers::UserHandleHash,
    messages::connection_package::VersionedConnectionPackage,
};
use sqlx::{Arguments, PgExecutor, postgres::PgArguments};

use crate::errors::StorageError;

use super::{StorableConnectionPackage, StorableConnectionPackageRef};

impl StorableConnectionPackage {
    pub(in crate::auth_service) async fn store_multiple_for_handle(
        connection: impl PgExecutor<'_>,
        connection_packages: impl IntoIterator<Item = &VersionedConnectionPackage>,
        hash: &UserHandleHash,
    ) -> Result<(), StorageError> {
        let mut query_args = PgArguments::default();
        let mut query_string = String::from(
            "INSERT INTO handle_connection_package (hash, connection_package, is_last_resort) VALUES",
        );

        for (i, connection_package) in connection_packages.into_iter().enumerate() {
            let is_last_resort = connection_package.is_last_resort();
            let connection_package: StorableConnectionPackageRef = connection_package.into();

            // Add values to the query arguments. None of these should throw an error.
            query_args.add(hash.as_bytes())?;
            query_args.add(BlobEncoded(connection_package))?;
            query_args.add(is_last_resort)?;

            if i > 0 {
                query_string.push(',');
            }

            // Add placeholders for each value
            query_string.push_str(&format!(
                " (${}, ${}, ${})",
                i * 3 + 1,
                i * 3 + 2,
                i * 3 + 3
            ));
        }

        // Finalize the query string
        query_string.push(';');

        // Execute the query
        sqlx::query_with(&query_string, query_args)
            .execute(connection)
            .await?;

        Ok(())
    }

    pub(crate) async fn load_for_handle(
        connection: impl PgExecutor<'_>,
        hash: &UserHandleHash,
    ) -> sqlx::Result<VersionedConnectionPackage> {
        // This is to ensure that counting and deletion happen atomically. If we
        // don't do this, two concurrent queries might both count 2 and delete,
        // leaving us with 0 packages.
        let connection_package = sqlx::query_scalar!(
            r#"WITH next_connection_package AS (
                SELECT id, connection_package
                FROM handle_connection_package
                WHERE hash = $1
                ORDER BY is_last_resort ASC
                LIMIT 1
                FOR UPDATE -- make sure two concurrent queries don't return the same package
                SKIP LOCKED -- skip rows that are already locked by other processes
            ),
            deleted_package AS (
                DELETE FROM handle_connection_package
                WHERE id = (
                    SELECT id
                    FROM next_connection_package
                )
                AND is_last_resort IS NOT TRUE
                AND (SELECT COUNT(*) FROM handle_connection_package WHERE hash = $1) > 1
            )
            SELECT connection_package
                AS "connection_package: BlobDecoded<StorableConnectionPackage>"
            FROM next_connection_package"#,
            hash.as_bytes(),
        )
        .fetch_one(connection)
        .await
        .map(|BlobDecoded(connection_package)| connection_package)?;
        Ok(connection_package.into())
    }

    #[cfg(test)]
    async fn packages_left_for_handle(
        connection: impl PgExecutor<'_>,
        hash: &UserHandleHash,
    ) -> sqlx::Result<usize> {
        let count = sqlx::query_scalar!(
            r#"SELECT COUNT(*) FROM handle_connection_package WHERE hash = $1"#,
            hash.as_bytes(),
        )
        .fetch_one(connection)
        .await?;
        Ok(count.unwrap_or(0) as usize)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use aircommon::{
        credentials::keys::{self, HandleVerifyingKey},
        crypto::{ConnectionDecryptionKey, signatures::signable::Signature},
        messages::{
            AirProtocolVersion,
            connection_package::{ConnectionPackage, ConnectionPackagePayload},
            connection_package_v1::{ConnectionPackageV1, ConnectionPackageV1Payload},
        },
        time::{Duration, ExpirationData},
    };
    use sqlx::PgPool;

    use crate::auth_service::user_handles::UserHandleRecord;

    use super::*;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub(crate) enum ConnectionPackageType {
        V1,
        V2 { is_last_resort: bool }, // is_last_resort
    }

    async fn store_connection_packages_for_handle(
        pool: &PgPool,
        hash: &UserHandleHash,
        verifying_key: HandleVerifyingKey,
        number_of_packages: usize,
        package_type: ConnectionPackageType,
    ) -> anyhow::Result<Vec<VersionedConnectionPackage>> {
        let pkgs = (0..number_of_packages)
            .map(|_| random_connection_package(verifying_key.clone(), package_type))
            .collect::<Vec<_>>();
        StorableConnectionPackage::store_multiple_for_handle(pool, pkgs.iter(), hash).await?;
        Ok(pkgs)
    }

    pub(crate) fn random_connection_package(
        verifying_key: HandleVerifyingKey,
        package_type: ConnectionPackageType,
    ) -> VersionedConnectionPackage {
        match package_type {
            ConnectionPackageType::V2 { is_last_resort } => {
                VersionedConnectionPackage::V2(ConnectionPackage::new_for_test(
                    ConnectionPackagePayload {
                        verifying_key,
                        protocol_version: AirProtocolVersion::Alpha,
                        encryption_key: ConnectionDecryptionKey::generate()
                            .unwrap()
                            .encryption_key()
                            .clone(),
                        lifetime: ExpirationData::new(Duration::days(90)),
                        user_handle_hash: UserHandleHash::new([1; 32]),
                        is_last_resort: is_last_resort.into(),
                    },
                    Signature::new_for_test(b"signature".to_vec()),
                ))
            }
            ConnectionPackageType::V1 => {
                VersionedConnectionPackage::V1(ConnectionPackageV1::new_for_test(
                    ConnectionPackageV1Payload {
                        verifying_key,
                        protocol_version: AirProtocolVersion::Alpha,
                        encryption_key: ConnectionDecryptionKey::generate()
                            .unwrap()
                            .encryption_key()
                            .clone(),
                        lifetime: ExpirationData::new(Duration::days(90)),
                        user_handle_hash: UserHandleHash::new([1; 32]),
                    },
                    Signature::new_for_test(b"signature".to_vec()),
                ))
            }
        }
    }

    async fn setup_user_record(
        pool: &PgPool,
    ) -> anyhow::Result<(UserHandleHash, HandleVerifyingKey)> {
        let hash = UserHandleHash::new([1; 32]);
        let verifying_key = keys::HandleVerifyingKey::from_bytes(vec![1, 2, 3, 4, 5]);
        let record = UserHandleRecord {
            user_handle_hash: hash,
            verifying_key: verifying_key.clone(),
            expiration_data: ExpirationData::new(Duration::days(1)),
        };
        record.store(pool).await?;
        Ok((hash, verifying_key))
    }

    async fn test_loading_and_deleting_connection_packages(
        pool: &PgPool,
        package_type: ConnectionPackageType,
    ) -> anyhow::Result<()> {
        let (hash, verifying_key) = setup_user_record(pool).await?;

        let mut pkgs =
            store_connection_packages_for_handle(pool, &hash, verifying_key, 2, package_type)
                .await?;

        // There should be 2 packages now
        let expected_num_packages = pkgs.len();
        let packages_remaining = StorableConnectionPackage::packages_left_for_handle(
            pool.acquire().await?.as_mut(),
            &hash,
        )
        .await?;
        assert_eq!(packages_remaining, expected_num_packages);

        let loaded =
            StorableConnectionPackage::load_for_handle(pool.acquire().await?.as_mut(), &hash)
                .await?;

        // first or second package is loaded
        assert!(loaded == pkgs[0] || loaded == pkgs[1]);
        if loaded == pkgs[0] {
            pkgs.remove(0);
        } else {
            pkgs.remove(1);
        }

        match package_type {
            ConnectionPackageType::V1 => {
                assert!(matches!(loaded, VersionedConnectionPackage::V1(_)))
            }
            ConnectionPackageType::V2 { is_last_resort } => {
                let VersionedConnectionPackage::V2(pkg) = &loaded else {
                    panic!("Expected V2 package");
                };
                assert_eq!(pkg.is_last_resort(), is_last_resort);
            }
        }

        // There should be 1 package now
        let packages_remaining = StorableConnectionPackage::packages_left_for_handle(
            pool.acquire().await?.as_mut(),
            &hash,
        )
        .await?;
        assert_eq!(packages_remaining, 1);

        // remaing package is loaded
        let loaded =
            StorableConnectionPackage::load_for_handle(pool.acquire().await?.as_mut(), &hash)
                .await?;
        assert_eq!(loaded, pkgs[0]);

        // last package is not deleted
        let loaded =
            StorableConnectionPackage::load_for_handle(pool.acquire().await?.as_mut(), &hash)
                .await?;
        assert_eq!(loaded, pkgs[0]);

        Ok(())
    }

    #[sqlx::test]
    async fn handle_legacy_connection_packages(pool: PgPool) -> anyhow::Result<()> {
        test_loading_and_deleting_connection_packages(&pool, ConnectionPackageType::V1).await
    }

    #[sqlx::test]
    async fn handle_connection_packages(pool: PgPool) -> anyhow::Result<()> {
        test_loading_and_deleting_connection_packages(
            &pool,
            ConnectionPackageType::V2 {
                is_last_resort: false,
            },
        )
        .await
    }

    #[sqlx::test]
    async fn handle_last_resort_connection_package(pool: PgPool) -> anyhow::Result<()> {
        let (hash, verifying_key) = setup_user_record(&pool).await?;

        // Store two regular and one last resort package
        let pkgs = store_connection_packages_for_handle(
            &pool,
            &hash,
            verifying_key.clone(),
            2,
            ConnectionPackageType::V2 {
                is_last_resort: false,
            },
        )
        .await?;
        let lr_pkgs = store_connection_packages_for_handle(
            &pool,
            &hash,
            verifying_key,
            1,
            ConnectionPackageType::V2 {
                is_last_resort: true,
            },
        )
        .await?;
        let mut expected_num_packages = pkgs.len() + lr_pkgs.len();

        // There should be 3 packages now
        let packages_remaining = StorableConnectionPackage::packages_left_for_handle(
            pool.acquire().await?.as_mut(),
            &hash,
        )
        .await?;
        assert!(packages_remaining == expected_num_packages);

        // The first and second package are loaded and deleted
        for _ in 0..2 {
            let first_loaded =
                StorableConnectionPackage::load_for_handle(pool.acquire().await?.as_mut(), &hash)
                    .await?;
            let packages_remaining = StorableConnectionPackage::packages_left_for_handle(
                pool.acquire().await?.as_mut(),
                &hash,
            )
            .await?;
            // Two packages remaining
            expected_num_packages -= 1;
            assert!(packages_remaining == expected_num_packages);
            // Loaded package is not last resort
            let VersionedConnectionPackage::V2(pkg) = &first_loaded else {
                panic!("Expected V2 package");
            };
            assert!(!pkg.is_last_resort());
        }

        // The last resort package is loaded but not deleted
        let last_loaded =
            StorableConnectionPackage::load_for_handle(pool.acquire().await?.as_mut(), &hash)
                .await?;
        let packages_remaining = StorableConnectionPackage::packages_left_for_handle(
            pool.acquire().await?.as_mut(),
            &hash,
        )
        .await?;
        assert!(packages_remaining == expected_num_packages);
        let VersionedConnectionPackage::V2(pkg) = &last_loaded else {
            panic!("Expected V2 package");
        };
        assert!(pkg.is_last_resort());

        Ok(())
    }
}
