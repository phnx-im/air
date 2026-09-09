// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::{
    credentials::keys::UsernameVerifyingKey,
    crypto::signatures::signable::Verifiable,
    identifiers::UsernameHash,
    messages::connection_package::{VersionedConnectionPackage, VersionedConnectionPackageIn},
};
use airprotos::client::signed_connection_package::{
    SignedConnectionPackageIn, VerifiedSignedConnectionPackage,
};
use thiserror::Error;
use tonic::Status;

use crate::auth_service::{
    AuthService,
    connection_package::{StorableConnectionPackage, signed::StorableSignedConnectionPackage},
};

impl AuthService {
    pub(crate) async fn as_publish_connection_packages_for_handle(
        &self,
        hash: &UsernameHash,
        verifying_key: &UsernameVerifyingKey,
        connection_packages: Vec<VersionedConnectionPackageIn>,
        signed_connection_packages: Vec<SignedConnectionPackageIn>,
    ) -> Result<(), PublishConnectionPackageError> {
        let connection_packages = connection_packages
            .into_iter()
            .map(|cp| {
                let verified: VersionedConnectionPackage = cp
                    .verify()
                    .map_err(|_| PublishConnectionPackageError::InvalidKeyPackage)?;
                if verified.username_hash() != hash {
                    return Err(PublishConnectionPackageError::UsernameHashMismatch);
                }
                Ok(verified)
            })
            .collect::<Result<Vec<VersionedConnectionPackage>, PublishConnectionPackageError>>()?;

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

        StorableConnectionPackage::store_multiple_for_username(
            &self.db_pool,
            &connection_packages,
            hash,
        )
        .await
        .map_err(|_| PublishConnectionPackageError::StorageError)?;

        StorableSignedConnectionPackage::store_multiple_for_username(
            &self.db_pool,
            hash,
            signed_connection_packages,
        )
        .await
        .map_err(|_| PublishConnectionPackageError::StorageError)?;

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
}

impl From<PublishConnectionPackageError> for Status {
    fn from(e: PublishConnectionPackageError) -> Self {
        let msg = e.to_string();
        match e {
            PublishConnectionPackageError::StorageError => Status::internal(msg),
            PublishConnectionPackageError::InvalidKeyPackage
            | PublishConnectionPackageError::VerifyingKeyMismatch
            | PublishConnectionPackageError::UsernameHashMismatch => Status::invalid_argument(msg),
        }
    }
}

#[cfg(test)]
mod tests {
    use aircommon::{
        credentials::keys::UsernameSigningKey,
        identifiers::UsernameHash,
        messages::connection_package::{ConnectionPackage, VersionedConnectionPackageIn},
        time::ExpirationData,
    };
    use airprotos::{
        auth_service::v1::SignedConnectionPackage,
        client::signed_connection_package::SignedConnectionPackageIn,
    };
    use chrono::Duration;
    use sqlx::PgPool;
    use tokio_util::sync::CancellationToken;

    use crate::air_service::BackendService;
    use crate::auth_service::{
        AuthService, connection_package::signed::StorableSignedConnectionPackage,
        usernames::UsernameRecord,
    };

    use super::PublishConnectionPackageError;

    async fn init_service(pool: &PgPool) -> anyhow::Result<AuthService> {
        Ok(AuthService::initialize(
            pool.clone(),
            "example.com".parse()?,
            Default::default(),
            CancellationToken::new(),
        )
        .await?)
    }

    /// Registers a handle so connection packages can be published for it.
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

        let (_, legacy_package, _) = ConnectionPackage::generate(hash, &signing_key, false)?;
        let legacy_proto = airprotos::auth_service::v1::ConnectionPackage::from(legacy_package);
        let legacy_in = VersionedConnectionPackageIn::try_from(legacy_proto)?;

        let (_, signed_package, _) = SignedConnectionPackage::generate(hash, &signing_key, false)?;
        let signed_in = SignedConnectionPackageIn::try_from(signed_package)?;

        let (_, last_resort_package, _) =
            SignedConnectionPackage::generate(hash, &signing_key, true)?;
        let last_resort_in = SignedConnectionPackageIn::try_from(last_resort_package)?;

        service
            .as_publish_connection_packages_for_handle(
                &hash,
                signing_key.verifying_key(),
                vec![legacy_in],
                vec![signed_in, last_resort_in],
            )
            .await?;

        let loaded = StorableSignedConnectionPackage::load_for_username(&pool, &hash).await?;
        assert!(loaded.is_some());

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
                Vec::new(),
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
                Vec::new(),
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

    #[sqlx::test]
    async fn publish_rejects_legacy_package_for_other_handle(pool: PgPool) -> anyhow::Result<()> {
        let service = init_service(&pool).await?;
        let hash_a = UsernameHash::new([1; 32]);
        let hash_b = UsernameHash::new([2; 32]);
        let signing_key = UsernameSigningKey::generate()?;
        register_handle(&pool, hash_a, &signing_key).await?;

        let (_, legacy_package, _) = ConnectionPackage::generate(hash_b, &signing_key, false)?;
        let legacy_proto = airprotos::auth_service::v1::ConnectionPackage::from(legacy_package);
        let legacy_in = VersionedConnectionPackageIn::try_from(legacy_proto)?;

        let result = service
            .as_publish_connection_packages_for_handle(
                &hash_a,
                signing_key.verifying_key(),
                vec![legacy_in],
                Vec::new(),
            )
            .await;
        assert!(matches!(
            result,
            Err(PublishConnectionPackageError::UsernameHashMismatch)
        ));

        Ok(())
    }
}
