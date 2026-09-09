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
