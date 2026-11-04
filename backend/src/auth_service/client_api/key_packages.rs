// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::{
    credentials::keys::UsernameVerifyingKey, crypto::signatures::signable::Verifiable,
    identifiers::UsernameHash,
};
use airprotos::client::signed_connection_package::{
    SignedConnectionPackageIn, VerifiedSignedConnectionPackage,
};
use thiserror::Error;
use tonic::Status;
use tracing::{error, warn};

use crate::auth_service::{
    AuthService,
    connection_package::{StorableConnectionPackage, signed::StorableSignedConnectionPackage},
    usernames::UsernameRecord,
};

impl AuthService {
    pub(crate) async fn as_publish_connection_packages_for_handle(
        &self,
        hash: &UsernameHash,
        verifying_key: &UsernameVerifyingKey,
        signed_connection_packages: Vec<SignedConnectionPackageIn>,
    ) -> Result<(), PublishConnectionPackageError> {
        let signed_connection_packages = signed_connection_packages
            .into_iter()
            .map(|cp| {
                let verified: VerifiedSignedConnectionPackage = cp
                    .verify(verifying_key)
                    .map_err(|_| PublishConnectionPackageError::InvalidKeyPackage)?;
                if verified.verifying_key() != verifying_key {
                    return Err(PublishConnectionPackageError::VerifyingKeyMismatch);
                }
                if verified.username_hash() != hash {
                    return Err(PublishConnectionPackageError::UsernameHashMismatch);
                }
                Ok(verified)
            })
            .collect::<Result<Vec<VerifiedSignedConnectionPackage>, _>>()?;

        let mut txn = self.db_pool.begin().await?;
        if !UsernameRecord::lock_for_write(txn.as_mut(), hash).await? {
            return Err(PublishConnectionPackageError::UsernameNotFound);
        }
        StorableSignedConnectionPackage::store_multiple_for_username(
            &mut txn,
            hash,
            signed_connection_packages,
        )
        .await?;
        StorableConnectionPackage::delete_unreachable_for_username(txn.as_mut(), hash).await?;
        txn.commit().await?;

        Ok(())
    }
}

#[derive(Debug, Error)]
pub(crate) enum PublishConnectionPackageError {
    /// Storage provider error
    #[error("Storage provider error")]
    StorageError,
    /// Invalid KeyPackage
    #[error("Invalid KeyPackage")]
    InvalidKeyPackage,
    /// The username hash does not match the one in the connection package
    #[error("Username hash mismatch")]
    UsernameHashMismatch,
    /// The verifying key does not match the one in the connection package
    #[error("Verifying key mismatch")]
    VerifyingKeyMismatch,
    /// The username was deleted in the meantime
    #[error("Username not found")]
    UsernameNotFound,
}

impl From<sqlx::Error> for PublishConnectionPackageError {
    fn from(error: sqlx::Error) -> Self {
        match error {
            // The connection package tables reference the username, which was deleted concurrently
            sqlx::Error::Database(error) if error.is_foreign_key_violation() => {
                warn!(%error, "username deleted while storing connection packages");
                PublishConnectionPackageError::UsernameNotFound
            }
            error => {
                error!(%error, "failed to store connection package");
                PublishConnectionPackageError::StorageError
            }
        }
    }
}

impl From<PublishConnectionPackageError> for Status {
    fn from(e: PublishConnectionPackageError) -> Self {
        let msg = e.to_string();
        match e {
            PublishConnectionPackageError::StorageError => Status::internal(msg),
            PublishConnectionPackageError::UsernameNotFound => Status::not_found(msg),
            PublishConnectionPackageError::InvalidKeyPackage
            | PublishConnectionPackageError::VerifyingKeyMismatch
            | PublishConnectionPackageError::UsernameHashMismatch => Status::invalid_argument(msg),
        }
    }
}

#[cfg(test)]
mod tests {
    use aircommon::crypto::signatures::signable::Verifiable;
    use aircommon::{
        credentials::keys::UsernameSigningKey, identifiers::UsernameHash, time::ExpirationData,
    };
    use airprotos::{
        auth_service::v1::SignedConnectionPackage,
        client::signed_connection_package::{
            SignedConnectionPackageIn, VerifiedSignedConnectionPackage,
        },
    };
    use chrono::Duration;
    use sqlx::PgPool;
    use tokio_util::sync::CancellationToken;

    use crate::air_service::BackendService;
    use crate::auth_service::{
        AuthService,
        connection_package::{
            StorableConnectionPackage,
            persistence::tests::{ConnectionPackageType, random_connection_package},
            signed::StorableSignedConnectionPackage,
        },
        usernames::UsernameRecord,
    };
    use crate::util::wait_for_lock_waiter;

    use super::PublishConnectionPackageError;

    async fn init_service(pool: &PgPool) -> anyhow::Result<AuthService> {
        Ok(AuthService::initialize(
            pool.clone(),
            "example.com".parse()?,
            Default::default(),
            0,
            CancellationToken::new(),
        )
        .await?)
    }

    async fn register_handle(
        pool: &PgPool,
        hash: UsernameHash,
        signing_key: &UsernameSigningKey,
    ) -> anyhow::Result<()> {
        UsernameRecord {
            username_hash: hash,
            verifying_key: signing_key.verifying_key().clone(),
            expiration_data: ExpirationData::new(Duration::days(1)),
        }
        .store(pool)
        .await?;
        Ok(())
    }

    #[sqlx::test]
    async fn publish_stores_valid_packages(pool: PgPool) -> anyhow::Result<()> {
        let service = init_service(&pool).await?;
        let hash = UsernameHash::new([1; 32]);
        let signing_key = UsernameSigningKey::generate()?;
        register_handle(&pool, hash, &signing_key).await?;

        let (_, signed_package, _) = SignedConnectionPackage::generate(hash, &signing_key, false)?;
        let signed_in = SignedConnectionPackageIn::try_from(signed_package)?;

        let (_, last_resort_package, _) =
            SignedConnectionPackage::generate(hash, &signing_key, true)?;
        let last_resort_in = SignedConnectionPackageIn::try_from(last_resort_package)?;

        service
            .as_publish_connection_packages_for_handle(
                &hash,
                signing_key.verifying_key(),
                vec![signed_in, last_resort_in],
            )
            .await?;

        let loaded = StorableSignedConnectionPackage::load_for_username(&pool, &hash).await?;
        assert!(loaded.is_some());

        Ok(())
    }

    fn signed_package_in(
        hash: UsernameHash,
        signing_key: &UsernameSigningKey,
        is_last_resort: bool,
    ) -> anyhow::Result<SignedConnectionPackageIn> {
        let (_, package, _) = SignedConnectionPackage::generate(hash, signing_key, is_last_resort)?;
        Ok(SignedConnectionPackageIn::try_from(package)?)
    }

    async fn stored_packages(pool: &PgPool, hash: &UsernameHash) -> anyhow::Result<Vec<Vec<u8>>> {
        Ok(sqlx::query_scalar(
            "SELECT payload FROM username_signed_connection_package WHERE hash = $1 ORDER BY id",
        )
        .bind(hash.as_bytes())
        .fetch_all(pool)
        .await?)
    }

    #[sqlx::test]
    async fn publish_replaces_the_pool(pool: PgPool) -> anyhow::Result<()> {
        let service = init_service(&pool).await?;
        let hash = UsernameHash::new([1; 32]);
        let signing_key = UsernameSigningKey::generate()?;
        register_handle(&pool, hash, &signing_key).await?;

        service
            .as_publish_connection_packages_for_handle(
                &hash,
                signing_key.verifying_key(),
                vec![
                    signed_package_in(hash, &signing_key, false)?,
                    signed_package_in(hash, &signing_key, false)?,
                    signed_package_in(hash, &signing_key, true)?,
                ],
            )
            .await?;
        assert_eq!(stored_packages(&pool, &hash).await?.len(), 3);

        let regular = signed_package_in(hash, &signing_key, false)?;
        let last_resort = signed_package_in(hash, &signing_key, true)?;
        let expected = vec![
            regular.payload_bytes().to_vec(),
            last_resort.payload_bytes().to_vec(),
        ];
        service
            .as_publish_connection_packages_for_handle(
                &hash,
                signing_key.verifying_key(),
                vec![regular, last_resort],
            )
            .await?;
        assert_eq!(stored_packages(&pool, &hash).await?, expected);

        Ok(())
    }

    #[sqlx::test]
    async fn concurrent_publishes_leave_one_pool(pool: PgPool) -> anyhow::Result<()> {
        let service = init_service(&pool).await?;
        let hash = UsernameHash::new([1; 32]);
        let signing_key = UsernameSigningKey::generate()?;
        register_handle(&pool, hash, &signing_key).await?;

        // The first pool is stored the way the handler does it, but not
        // committed yet.
        let first: VerifiedSignedConnectionPackage =
            signed_package_in(hash, &signing_key, false)?.verify(signing_key.verifying_key())?;
        let mut txn = pool.begin().await?;
        assert!(UsernameRecord::lock_for_write(txn.as_mut(), &hash).await?);
        StorableSignedConnectionPackage::store_multiple_for_username(&mut txn, &hash, vec![first])
            .await?;

        let second = signed_package_in(hash, &signing_key, false)?;
        let expected = vec![second.payload_bytes().to_vec()];
        let verifying_key = signing_key.verifying_key().clone();
        let publish = tokio::spawn(async move {
            service
                .as_publish_connection_packages_for_handle(&hash, &verifying_key, vec![second])
                .await
        });
        wait_for_lock_waiter(&pool).await?;
        txn.commit().await?;
        publish.await??;

        assert_eq!(stored_packages(&pool, &hash).await?, expected);
        Ok(())
    }

    #[sqlx::test]
    async fn publish_without_last_resort_keeps_the_stored_one(pool: PgPool) -> anyhow::Result<()> {
        let service = init_service(&pool).await?;
        let hash = UsernameHash::new([1; 32]);
        let signing_key = UsernameSigningKey::generate()?;
        register_handle(&pool, hash, &signing_key).await?;

        let last_resort = signed_package_in(hash, &signing_key, true)?;
        let last_resort_payload = last_resort.payload_bytes().to_vec();
        service
            .as_publish_connection_packages_for_handle(
                &hash,
                signing_key.verifying_key(),
                vec![signed_package_in(hash, &signing_key, false)?, last_resort],
            )
            .await?;

        let regular = signed_package_in(hash, &signing_key, false)?;
        let regular_payload = regular.payload_bytes().to_vec();
        service
            .as_publish_connection_packages_for_handle(
                &hash,
                signing_key.verifying_key(),
                vec![regular],
            )
            .await?;
        assert_eq!(
            stored_packages(&pool, &hash).await?,
            vec![last_resort_payload, regular_payload]
        );

        Ok(())
    }

    #[sqlx::test]
    async fn publish_for_unknown_handle_is_not_found(pool: PgPool) -> anyhow::Result<()> {
        let service = init_service(&pool).await?;
        let hash = UsernameHash::new([1; 32]);
        let signing_key = UsernameSigningKey::generate()?;

        let (_, signed_package, _) = SignedConnectionPackage::generate(hash, &signing_key, false)?;
        let signed_in = SignedConnectionPackageIn::try_from(signed_package)?;

        let result = service
            .as_publish_connection_packages_for_handle(
                &hash,
                signing_key.verifying_key(),
                vec![signed_in],
            )
            .await;
        assert!(matches!(
            result,
            Err(PublishConnectionPackageError::UsernameNotFound)
        ));

        let loaded = StorableSignedConnectionPackage::load_for_username(&pool, &hash).await?;
        assert!(loaded.is_none());

        Ok(())
    }

    #[sqlx::test]
    async fn publish_rejects_signed_package_for_other_handle(pool: PgPool) -> anyhow::Result<()> {
        let service = init_service(&pool).await?;
        let hash_a = UsernameHash::new([1; 32]);
        let hash_b = UsernameHash::new([2; 32]);
        let signing_key = UsernameSigningKey::generate()?;
        register_handle(&pool, hash_a, &signing_key).await?;

        let (_, signed_package, _) =
            SignedConnectionPackage::generate(hash_b, &signing_key, false)?;
        let signed_in = SignedConnectionPackageIn::try_from(signed_package)?;

        let result = service
            .as_publish_connection_packages_for_handle(
                &hash_a,
                signing_key.verifying_key(),
                vec![signed_in],
            )
            .await;
        assert!(matches!(
            result,
            Err(PublishConnectionPackageError::UsernameHashMismatch)
        ));

        let loaded = StorableSignedConnectionPackage::load_for_username(&pool, &hash_a).await?;
        assert!(loaded.is_none());

        Ok(())
    }

    #[sqlx::test]
    async fn publish_rejects_signed_package_signed_with_other_key(
        pool: PgPool,
    ) -> anyhow::Result<()> {
        let service = init_service(&pool).await?;
        let hash = UsernameHash::new([1; 32]);
        let signing_key_a = UsernameSigningKey::generate()?;
        let signing_key_b = UsernameSigningKey::generate()?;
        register_handle(&pool, hash, &signing_key_a).await?;

        let (_, signed_package, _) =
            SignedConnectionPackage::generate(hash, &signing_key_b, false)?;
        let signed_in = SignedConnectionPackageIn::try_from(signed_package)?;

        let result = service
            .as_publish_connection_packages_for_handle(
                &hash,
                signing_key_a.verifying_key(),
                vec![signed_in],
            )
            .await;
        assert!(matches!(
            result,
            Err(PublishConnectionPackageError::InvalidKeyPackage)
        ));

        let loaded = StorableSignedConnectionPackage::load_for_username(&pool, &hash).await?;
        assert!(loaded.is_none());

        Ok(())
    }
    async fn store_legacy_package(
        pool: &PgPool,
        hash: &UsernameHash,
        signing_key: &UsernameSigningKey,
    ) -> anyhow::Result<()> {
        let package = random_connection_package(
            signing_key.verifying_key().clone(),
            ConnectionPackageType::V2 {
                is_last_resort: true,
            },
        );
        StorableConnectionPackage::store_multiple_for_username(pool, [&package], hash).await?;
        Ok(())
    }

    async fn legacy_package_count(pool: &PgPool, hash: &UsernameHash) -> anyhow::Result<i64> {
        Ok(
            sqlx::query_scalar("SELECT count(*) FROM handle_connection_package WHERE hash = $1")
                .bind(hash.as_bytes())
                .fetch_one(pool)
                .await?,
        )
    }

    #[sqlx::test]
    async fn publish_with_signed_last_resort_deletes_legacy_packages(
        pool: PgPool,
    ) -> anyhow::Result<()> {
        let service = init_service(&pool).await?;
        let hash = UsernameHash::new([1; 32]);
        let other_hash = UsernameHash::new([2; 32]);
        let signing_key = UsernameSigningKey::generate()?;
        register_handle(&pool, hash, &signing_key).await?;
        register_handle(&pool, other_hash, &signing_key).await?;
        store_legacy_package(&pool, &hash, &signing_key).await?;
        store_legacy_package(&pool, &other_hash, &signing_key).await?;

        service
            .as_publish_connection_packages_for_handle(
                &hash,
                signing_key.verifying_key(),
                vec![
                    signed_package_in(hash, &signing_key, false)?,
                    signed_package_in(hash, &signing_key, true)?,
                ],
            )
            .await?;

        assert_eq!(legacy_package_count(&pool, &hash).await?, 0);
        assert_eq!(legacy_package_count(&pool, &other_hash).await?, 1);
        Ok(())
    }

    #[sqlx::test]
    async fn publish_without_signed_last_resort_keeps_legacy_packages(
        pool: PgPool,
    ) -> anyhow::Result<()> {
        let service = init_service(&pool).await?;
        let hash = UsernameHash::new([1; 32]);
        let signing_key = UsernameSigningKey::generate()?;
        register_handle(&pool, hash, &signing_key).await?;
        store_legacy_package(&pool, &hash, &signing_key).await?;

        service
            .as_publish_connection_packages_for_handle(
                &hash,
                signing_key.verifying_key(),
                vec![signed_package_in(hash, &signing_key, false)?],
            )
            .await?;

        assert_eq!(legacy_package_count(&pool, &hash).await?, 1);
        Ok(())
    }

    #[sqlx::test]
    async fn publish_with_stored_signed_last_resort_deletes_legacy_packages(
        pool: PgPool,
    ) -> anyhow::Result<()> {
        let service = init_service(&pool).await?;
        let hash = UsernameHash::new([1; 32]);
        let signing_key = UsernameSigningKey::generate()?;
        register_handle(&pool, hash, &signing_key).await?;
        service
            .as_publish_connection_packages_for_handle(
                &hash,
                signing_key.verifying_key(),
                vec![signed_package_in(hash, &signing_key, true)?],
            )
            .await?;
        store_legacy_package(&pool, &hash, &signing_key).await?;

        service
            .as_publish_connection_packages_for_handle(
                &hash,
                signing_key.verifying_key(),
                vec![signed_package_in(hash, &signing_key, false)?],
            )
            .await?;

        assert_eq!(legacy_package_count(&pool, &hash).await?, 0);
        Ok(())
    }
}
