// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::{
    credentials::keys::HandleSigningKey,
    crypto::ConnectionDecryptionKey,
    identifiers::{UserHandle, UserHandleHash},
    messages::{client_as_out::UserHandleDeleteResponse, connection_package::ConnectionPackage},
};
use anyhow::Context;
pub use persistence::UserHandleRecord;
use tokio::task::spawn_blocking;
use tracing::error;

use crate::{
    clients::{CONNECTION_PACKAGES, CoreUser},
    store::StoreResult,
    user_handles::connection_packages::StorableConnectionPackage,
};

pub(crate) mod connection_packages;
mod persistence;

impl CoreUser {
    /// Registers a new user handle on the server and adds it locally.
    ///
    /// Returns a handle record on success, or `None` if the handle was already present.
    pub(crate) async fn add_user_handle(
        &self,
        handle: UserHandle,
    ) -> StoreResult<Option<UserHandleRecord>> {
        let signing_key = HandleSigningKey::generate()?;
        let handle_inner = handle.clone();
        let hash = spawn_blocking(move || handle_inner.calculate_hash()).await??;

        let api_client = self.api_client()?;
        let created = api_client
            .as_create_handle(&handle, hash, &signing_key)
            .await?;
        if !created {
            return Ok(None);
        }

        let record = UserHandleRecord::new(handle.clone(), hash, signing_key);

        let rollback = async |delete_locally: bool| {
            api_client
                .as_delete_handle(record.hash, &record.signing_key)
                .await
                .inspect_err(|error| {
                    error!(%error, "failed to delete user handle on the server in rollback");
                })
                .ok();
            if delete_locally {
                UserHandleRecord::delete(self.pool(), &record.handle)
                    .await
                    .inspect_err(|error| {
                        error!(%error, "failed to delete user handle locally in rollback");
                    })
                    .ok();
            }
        };

        let mut txn = self.pool().begin().await?;
        if let Err(error) = record.store(&mut *txn).await {
            error!(%error, "failed to store user handle; rollback");
            rollback(false).await;
            return Err(error.into());
        }

        // Publish connection packages
        let connection_package_bundles =
            generate_connection_packages(&record.signing_key, record.hash)?;

        // Store connection packages in the database
        let mut connection_packages = Vec::with_capacity(connection_package_bundles.len());
        for (decryption_key, connection_package) in connection_package_bundles {
            connection_package
                .store_for_handle(&mut txn, &handle, &decryption_key)
                .await?;
            connection_packages.push(connection_package);
        }
        txn.commit().await?;

        if let Err(error) = api_client
            .as_publish_connection_packages_for_handle(
                hash,
                connection_packages,
                &record.signing_key,
            )
            .await
        {
            error!(%error, "failed to publish connection packages; rollback");
            rollback(true).await;
            return Err(error.into());
        }

        Ok(Some(record))
    }

    /// Deletes the user handle on the server and removes it locally.
    pub(crate) async fn remove_user_handle(
        &self,
        handle: &UserHandle,
    ) -> StoreResult<UserHandleDeleteResponse> {
        let mut txn = self.pool().begin().await?;
        let record = UserHandleRecord::load(txn.as_mut(), handle)
            .await?
            .context("no user handle found")?;
        let api_client = self.api_client()?;
        let res = api_client
            .as_delete_handle(record.hash, &record.signing_key)
            .await?;

        self.remove_user_handle_locally(handle).await?;
        Ok(res)
    }

    pub(crate) async fn remove_user_handle_locally(&self, handle: &UserHandle) -> StoreResult<()> {
        let mut txn = self.pool().begin().await?;
        UserHandleRecord::delete(txn.as_mut(), handle).await?;
        txn.commit().await?;
        Ok(())
    }
}

fn generate_connection_packages(
    signing_key: &HandleSigningKey,
    hash: UserHandleHash,
) -> anyhow::Result<Vec<(ConnectionDecryptionKey, ConnectionPackage)>> {
    let mut connection_packages = Vec::with_capacity(CONNECTION_PACKAGES);
    for _ in 0..CONNECTION_PACKAGES - 1 {
        let connection_package = ConnectionPackage::new(hash, signing_key, false)?;
        connection_packages.push(connection_package);
    }
    // Last resort connection package
    let connection_package = ConnectionPackage::new(hash, signing_key, true)?;
    connection_packages.push(connection_package);
    Ok(connection_packages)
}
