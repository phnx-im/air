// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! This module contains the implementation of the queue service.

use aircommon::{
    identifiers::{Fqdn, QsClientId},
    messages::{QueueMessage, client_ds::DsEventMessage, push_token::PushToken},
};
use client_id_decryption_key::StorableClientIdDecryptionKey;

use metrics::describe_gauge;
use semver::VersionReq;
use sqlx::PgPool;

use crate::{
    air_service::{BackendService, ServiceCreationError},
    messages::intra_backend::DsFanOutMessage,
    qs::queue::Queues,
};

mod auth;
pub mod client_api;
mod client_id_decryption_key;
mod client_record;
pub mod ds_api;
pub mod errors;
pub mod grpc;
mod key_package;
pub mod network_provider;
pub mod qs_api;
mod queue;
mod user_record;

#[derive(Debug, Clone)]
pub struct Qs {
    domain: Fqdn,
    db_pool: PgPool,
    queues: Queues,
    client_version_req: Option<VersionReq>,
}

pub(crate) const METRIC_AIR_QS_TOTAL_USERS: &str = "air_qs_total_users";
pub(crate) const METRIC_AIR_QS_MAU_USERS: &str = "air_qs_mau_users";
pub(crate) const METRIC_AIR_QS_WAU_USERS: &str = "air_qs_wau_users";
pub(crate) const METRIC_AIR_QS_DAU_USERS: &str = "air_qs_dau_users";
pub(crate) const METRIC_AIR_ACTIVE_USERS: &str = "air_qs_active_users";

impl BackendService for Qs {
    async fn initialize(
        db_pool: PgPool,
        domain: Fqdn,
        client_version_req: Option<VersionReq>,
    ) -> Result<Self, ServiceCreationError> {
        // Check if the requisite key material exists and if it doesn't, generate it.

        let decryption_key_exists = StorableClientIdDecryptionKey::load(&db_pool)
            .await?
            .is_some();
        if !decryption_key_exists {
            StorableClientIdDecryptionKey::generate_and_store(&db_pool)
                .await
                .map_err(|e| ServiceCreationError::InitializationFailed(Box::new(e)))?;
        }

        let queues = Queues::new(db_pool.clone()).await?;

        Ok(Self {
            domain,
            db_pool,
            queues,
            client_version_req,
        })
    }

    fn describe_metrics() {
        describe_gauge!(METRIC_AIR_QS_TOTAL_USERS, "Number of total users");
        describe_gauge!(
            METRIC_AIR_QS_DAU_USERS,
            "Number of rolling DAU (daily active users)"
        );
        describe_gauge!(
            METRIC_AIR_QS_WAU_USERS,
            "Number of rolling WAU (weekly active users)"
        );
        describe_gauge!(
            METRIC_AIR_QS_MAU_USERS,
            "Number of rolling MAU (monthly active users)"
        );
        describe_gauge!(
            METRIC_AIR_ACTIVE_USERS,
            "Number of currently connetected users"
        );
    }
}

impl Qs {
    pub(crate) fn queues(&self) -> &Queues {
        &self.queues
    }
}

pub enum Notification {
    Event(DsEventMessage),
    QueueUpdate(QueueMessage),
}

#[derive(Debug)]
pub enum NotifierError {
    ClientNotFound,
}

/// Notifies connected and listening clients about events.
///
/// TODO: This should be unified with push notifications later
#[expect(async_fn_in_trait)]
pub trait Notifier {
    async fn notify(
        &self,
        client_id: &QsClientId,
        notification: Notification,
    ) -> Result<(), NotifierError>;
}

#[derive(Debug)]
pub enum PushNotificationError {
    /// Just for logging.
    Other(String),
    /// The push token is invalid.
    InvalidToken(String),
    /// The authorization header is invalid.
    InvalidBearer,
    /// Network error.
    NetworkError(String),
    /// Unsupported type of push token.
    UnsupportedType,
    /// The JWT token for APNS could not be created.
    JwtCreationError(String),
    /// OAuth error.
    OAuthError(String),
    /// Configuration error.
    InvalidConfiguration(String),
}

pub trait PushNotificationProvider: std::fmt::Debug + Send + Sync + 'static {
    fn push(
        &self,
        push_token: PushToken,
    ) -> impl Future<Output = Result<(), PushNotificationError>> + Send;
}

pub trait QsConnector: Sync + Send + std::fmt::Debug + 'static {
    type EnqueueError: Send + std::error::Error;

    fn dispatch(
        &self,
        message: DsFanOutMessage,
    ) -> impl Future<Output = Result<(), Self::EnqueueError>> + Send + 'static;
}
