// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::{
    identifiers::QsClientId,
    messages::{
        FriendshipToken,
        client_qs::{
            EncryptionKeyResponse, KeyPackageParams, KeyPackageResponse, PublishKeyPackagesParams,
        },
    },
    virtual_client::KeyPackageBatchId,
};
use airprotos::queue_service;
use apqmls::messages::{ApqKeyPackage, ApqKeyPackageIn};
use mls_assist::{
    openmls::{
        key_packages::KeyPackageIn,
        prelude::{KeyPackage, OpenMlsProvider, ProtocolVersion},
    },
    openmls_rust_crypto::OpenMlsRustCrypto,
};

use crate::{
    errors::qs::{
        QsEncryptionKeyError, QsKeyPackageError, QsPublishKeyPackagesError, QsStageKeyPackagesError,
    },
    qs::{
        Qs, client_id_decryption_key::StorableClientIdDecryptionKey, client_record::QsClientRecord,
        key_package::StorableKeyPackage, staged_key_package::StagedKeyPackages,
    },
};

impl Qs {
    /// Clients publish key packages to the server.
    #[tracing::instrument(skip_all, err)]
    pub(crate) async fn qs_publish_key_packages(
        &self,
        params: PublishKeyPackagesParams,
    ) -> Result<(), QsPublishKeyPackagesError> {
        let PublishKeyPackagesParams {
            sender,
            key_packages,
        } = params;

        let mut verified_key_packages = Vec::with_capacity(key_packages.len());
        let mut last_resort_key_package = None;
        for key_package in key_packages {
            let verified_key_package: KeyPackage = key_package
                .validate(
                    OpenMlsRustCrypto::default().crypto(),
                    ProtocolVersion::default(),
                )
                .map_err(|_| QsPublishKeyPackagesError::InvalidKeyPackage)?;

            let is_last_resort = verified_key_package.last_resort();

            if is_last_resort {
                last_resort_key_package = Some(verified_key_package);
            } else {
                verified_key_packages.push(verified_key_package);
            }
        }

        let mut txn = self.db_pool.begin().await?;
        if let Some(last_resort_key_package) = last_resort_key_package {
            last_resort_key_package
                .replace_last_resort(&mut txn, &sender)
                .await?;
        }
        KeyPackage::replace_multiple(&mut txn, &sender, &verified_key_packages).await?;
        txn.commit().await?;

        Ok(())
    }

    /// Clients publish APQ key packages to the server.
    #[tracing::instrument(skip_all, err)]
    pub(crate) async fn qs_publish_apq_key_packages(
        &self,
        sender: QsClientId,
        key_packages: Vec<ApqKeyPackageIn>,
    ) -> Result<(), QsPublishKeyPackagesError> {
        let mut verified_key_packages = Vec::with_capacity(key_packages.len());
        let mut last_resort_key_package = None;
        for key_package in key_packages {
            let verified_key_package: ApqKeyPackage = key_package
                .validate(OpenMlsRustCrypto::default().crypto())
                .map_err(|_| QsPublishKeyPackagesError::InvalidKeyPackage)?;

            let is_last_resort = verified_key_package.t_key_package().last_resort()
                && verified_key_package.pq_key_package().last_resort();

            if is_last_resort {
                last_resort_key_package = Some(verified_key_package);
            } else {
                verified_key_packages.push(verified_key_package);
            }
        }

        let mut txn = self.db_pool.begin().await?;
        if let Some(last_resort_key_package) = last_resort_key_package {
            last_resort_key_package
                .replace_last_resort(&mut txn, &sender)
                .await?;
        }
        ApqKeyPackage::replace_multiple(&mut txn, &sender, &verified_key_packages).await?;
        txn.commit().await?;

        Ok(())
    }

    /// Stage key packages for a given client.
    #[tracing::instrument(skip_all, err)]
    pub(crate) async fn qs_stage_key_packages(
        &self,
        client_id: QsClientId,
        batch_id: KeyPackageBatchId,
        key_packages_proto: Vec<queue_service::v1::KeyPackage>,
        apq_key_packages_proto: Vec<queue_service::v1::ApqKeyPackage>,
    ) -> Result<(), QsStageKeyPackagesError> {
        let crypto = OpenMlsRustCrypto::default();
        let protocol_version = ProtocolVersion::default();

        // Validated T key packages with their TLS bytes
        let key_packages: Vec<(KeyPackage, Vec<u8>)> = key_packages_proto
            .into_iter()
            .map(|proto| {
                let key_package_in: KeyPackageIn = (&proto)
                    .try_into()
                    .map_err(|_| QsStageKeyPackagesError::InvalidKeyPackageTls)?;
                let key_package: KeyPackage = key_package_in
                    .validate(crypto.crypto(), protocol_version)
                    .map_err(|_| QsStageKeyPackagesError::InvalidKeyPackage)?;
                Ok((key_package, proto.tls))
            })
            .collect::<Result<_, QsStageKeyPackagesError>>()?;
        // Validated APQ key packages with their TLS bytes
        let apq_key_packages: Vec<(ApqKeyPackage, Vec<u8>)> = apq_key_packages_proto
            .into_iter()
            .map(|proto| {
                let key_package_in: ApqKeyPackageIn = (&proto)
                    .try_into()
                    .map_err(|_| QsStageKeyPackagesError::InvalidKeyPackageTls)?;
                let key_package: ApqKeyPackage = key_package_in
                    .validate(crypto.crypto())
                    .map_err(|_| QsStageKeyPackagesError::InvalidKeyPackage)?;
                let mut tls = proto.t_key_package_tls;
                tls.extend_from_slice(&proto.pq_key_package_tls);
                Ok((key_package, tls))
            })
            .collect::<Result<_, QsStageKeyPackagesError>>()?;

        if key_packages.is_empty() && apq_key_packages.is_empty() {
            return Err(QsStageKeyPackagesError::EmptyBatch);
        }

        let user_id = QsClientRecord::load_user_id(&self.db_pool, &client_id)
            .await?
            .ok_or(QsStageKeyPackagesError::UnknownClient)?;

        let batch = StagedKeyPackages {
            user_id,
            batch_id,
            key_packages,
            apq_key_packages,
        };

        let mut txn = self.db_pool.begin().await?;
        batch.stage(&mut txn).await?;
        txn.commit().await?;

        Ok(())
    }

    /// Retrieve a key package for a given client.
    #[tracing::instrument(skip_all, err)]
    pub(crate) async fn qs_key_package(
        &self,
        params: KeyPackageParams,
    ) -> Result<KeyPackageResponse, QsKeyPackageError> {
        let KeyPackageParams { sender } = params;

        let mut connection = self.db_pool.acquire().await.map_err(|e| {
            tracing::warn!("Failed to acquire connection: {:?}", e);
            QsKeyPackageError::StorageError
        })?;

        let key_package = KeyPackage::load_user_key_package(&mut connection, &sender)
            .await
            .map_err(|e| {
                tracing::warn!("Storage provider error: {:?}", e);
                QsKeyPackageError::StorageError
            })?;

        let response = KeyPackageResponse { key_package };
        Ok(response)
    }

    /// Retrieve an APQ key package for a given client.
    #[tracing::instrument(skip_all, err)]
    pub(crate) async fn qs_apq_key_package(
        &self,
        sender: FriendshipToken,
    ) -> Result<ApqKeyPackage, QsKeyPackageError> {
        let mut connection = self.db_pool.acquire().await.map_err(|e| {
            tracing::warn!("Failed to acquire connection: {:?}", e);
            QsKeyPackageError::StorageError
        })?;

        ApqKeyPackage::load_user_key_package(&mut connection, &sender)
            .await
            .map_err(|e| {
                tracing::warn!("Storage provider error: {:?}", e);
                QsKeyPackageError::StorageError
            })
    }

    /// Retrieve the client id encryption key of this QS
    #[tracing::instrument(skip_all, err)]
    pub(crate) async fn qs_encryption_key(
        &self,
    ) -> Result<EncryptionKeyResponse, QsEncryptionKeyError> {
        StorableClientIdDecryptionKey::load(&self.db_pool)
            .await
            .map_err(|e| {
                tracing::warn!("Failed to load client id decryption key: {:?}", e);
                QsEncryptionKeyError::StorageError
            })?
            .map(|decryption_key| {
                let encryption_key = decryption_key.encryption_key().clone();
                EncryptionKeyResponse { encryption_key }
            })
            .ok_or(QsEncryptionKeyError::LibraryError)
    }
}
