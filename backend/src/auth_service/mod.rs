// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::sync::Arc;

use aircommon::{
    credentials::keys::ClientVerifyingKey,
    crypto::signatures::DEFAULT_SIGNATURE_SCHEME,
    identifiers::{Fqdn, UserId},
};
use credentials::{
    CredentialGenerationError, intermediate_signing_key::IntermediateSigningKey,
    signing_key::StorableSigningKey,
};
use sqlx::PgPool;
use thiserror::Error;
use tokio_util::sync::CancellationToken;
use usernames::UsernameQueues;

use crate::{
    air_service::{BackendService, ServiceCreationError},
    auth_service::{
        admission::{ChallengeSender, load_or_generate_endpoint_bucket_key},
        client_record::ClientRecord,
        registration_gate::{RegistrationGate, load_or_generate_ip_bucket_key},
    },
    bucket_key::BucketKey,
    errors::StorageError,
    settings::RegistrationSettings,
    version::VersionPolicy,
};

pub mod admission;
pub mod cli;
pub mod client_api;
mod client_record;
mod connection_package;
mod credentials;
pub mod grpc;
mod invitation_code_record;
pub mod privacy_pass;
pub(crate) mod registration_challenge;
pub(crate) mod registration_gate;
pub mod user_record;
mod usernames;

#[derive(Debug, Clone)]
pub struct AuthService {
    db_pool: PgPool,
    pub(crate) username_queues: UsernameQueues,
    version_policy: VersionPolicy,
    registration_gate: RegistrationGate,
    endpoint_bucket_key: BucketKey,
    challenge_sender: Option<Arc<dyn ChallengeSender>>,
    unredeemable_code: Option<Arc<str>>,
    stop: CancellationToken,
}

impl AuthService {
    pub fn database_pool(&self) -> &PgPool {
        &self.db_pool
    }

    pub fn set_registration_settings(&mut self, settings: RegistrationSettings) {
        self.registration_gate.set_settings(settings);
    }

    /// Drops registration rows that no counter window can still see.
    pub async fn prune_registration_records(&self) -> sqlx::Result<u64> {
        self.registration_gate.prune(&self.db_pool).await
    }

    /// Republishes the deployment-scoped registration gate gauge.
    pub async fn refresh_registration_gauge(&self) {
        self.registration_gate.refresh_gauge(&self.db_pool).await;
    }

    /// Drops admission rows that are no longer needed.
    pub async fn prune_admission_records(&self) -> sqlx::Result<u64> {
        admission::prune_admission_records(
            &self.db_pool,
            &self.registration_gate.settings().admission,
        )
        .await
    }

    /// Sets the sender, so that the service can send admission challenges to
    /// push endpoints.
    pub fn set_challenge_sender(&mut self, sender: Arc<dyn ChallengeSender>) {
        self.challenge_sender = Some(sender);
    }

    pub(crate) fn registration_gate(&self) -> &RegistrationGate {
        &self.registration_gate
    }

    pub fn set_unredeemable_code(&mut self, code: String) {
        self.unredeemable_code = Some(code.into());
    }

    pub fn is_unredeemable_code(&self, code: &str) -> bool {
        self.unredeemable_code.as_deref() == Some(code)
    }

    pub async fn load_client_verifying_key(
        &self,
        user_id: &UserId,
    ) -> Result<Option<ClientVerifyingKey>, StorageError> {
        let client_verifying_key = ClientRecord::load(&self.db_pool, user_id)
            .await?
            .map(|record| record.credential.verifying_key().clone());
        Ok(client_verifying_key)
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
        version_policy: VersionPolicy,
        stop: CancellationToken,
    ) -> Result<Self, ServiceCreationError> {
        let username_queues = UsernameQueues::new(db_pool.clone(), stop.clone()).await?;
        let bucket_key = load_or_generate_ip_bucket_key(&db_pool).await?;
        let endpoint_bucket_key = load_or_generate_endpoint_bucket_key(&db_pool).await?;
        let auth_service = Self {
            db_pool,
            username_queues,
            version_policy,
            registration_gate: RegistrationGate::new(RegistrationSettings::default(), bucket_key),
            endpoint_bucket_key,
            challenge_sender: None,
            unredeemable_code: None,
            stop,
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

        // Ensure a VOPRF key exists for Privacy Pass (creates one if missing
        // or rotates if the current key is stale).
        privacy_pass::rotate_keys_if_needed(&auth_service.db_pool)
            .await
            .map_err(ServiceCreationError::init_error)?;

        Ok(auth_service)
    }
}

impl AuthService {
    /// Returns a reference to the database pool for spawning background tasks.
    pub fn db_pool(&self) -> &PgPool {
        &self.db_pool
    }
}

pub trait AsConnector: Sync + Send + std::fmt::Debug + 'static {
    type Error: Send + std::error::Error;

    fn client_verifying_key(
        &self,
        user_id: &UserId,
    ) -> impl Future<Output = Result<Option<ClientVerifyingKey>, Self::Error>> + Send;
}
