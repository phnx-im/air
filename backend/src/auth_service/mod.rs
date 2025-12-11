// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::sync::Arc;

use aircommon::{crypto::signatures::DEFAULT_SIGNATURE_SCHEME, identifiers::Fqdn};
use credentials::{
    CredentialGenerationError, intermediate_signing_key::IntermediateSigningKey,
    signing_key::StorableSigningKey,
};
use semver::VersionReq;
use sqlx::PgPool;
use thiserror::Error;
use user_handles::UserHandleQueues;

use crate::{
    air_service::{BackendService, ServiceCreationError},
    errors::StorageError,
};

pub mod cli;
pub mod client_api;
mod client_record;
mod connection_package;
mod credentials;
pub mod grpc;
mod invitation_code_record;
mod privacy_pass;
mod user_handles;
pub mod user_record;

#[derive(Debug, Clone)]
pub struct AuthService {
    db_pool: PgPool,
    pub(crate) handle_queues: UserHandleQueues,
    client_version_req: Option<VersionReq>,
    invitation_only: bool,
    unredeemable_code: Option<Arc<str>>,
}

impl AuthService {
    pub fn disable_invitation_only(&mut self) {
        self.invitation_only = false;
    }

    pub fn set_unredeemable_code(&mut self, code: String) {
        self.unredeemable_code = Some(code.into());
    }

    pub fn is_unredeemable_code(&self, code: &str) -> bool {
        self.unredeemable_code.as_deref() == Some(code)
    }
}

#[derive(Debug, Error)]
pub enum AuthServiceCreationError {
    #[error(transparent)]
    Storage(#[from] StorageError),
    #[error("Error generating initial credentials")]
    Credential(#[from] CredentialGenerationError),
}

impl<T: Into<sqlx::Error>> From<T> for AuthServiceCreationError {
    fn from(e: T) -> Self {
        Self::Storage(StorageError::from(e.into()))
    }
}

impl BackendService for AuthService {
    async fn initialize(
        db_pool: PgPool,
        domain: Fqdn,
        client_version_req: Option<VersionReq>,
    ) -> Result<Self, ServiceCreationError> {
        let handle_queues = UserHandleQueues::new(db_pool.clone()).await?;
        let auth_service = Self {
            db_pool,
            handle_queues,
            client_version_req,
            invitation_only: true,
            unredeemable_code: None,
        };

        // Check if there is an active AS signing key
        let mut transaction = auth_service.db_pool.begin().await?;
        let active_signing_key_exists =
            StorableSigningKey::load(&mut *transaction).await?.is_some();

        if !active_signing_key_exists {
            let signature_scheme = DEFAULT_SIGNATURE_SCHEME;
            // Generate a new AS signing key
            StorableSigningKey::generate_store_and_activate(
                &mut transaction,
                domain.clone(),
                signature_scheme,
            )
            .await
            .map_err(ServiceCreationError::init_error)?;
            // Generate and sign an intermediate signing key
            IntermediateSigningKey::generate_sign_and_activate(
                &mut transaction,
                domain,
                signature_scheme,
            )
            .await
            .map_err(ServiceCreationError::init_error)?;
        }
        transaction.commit().await?;

        Ok(auth_service)
    }
}
