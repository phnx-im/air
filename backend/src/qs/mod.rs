// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! This module contains the APIs of the queuing service (QS). It only performs
//! a limited amount of rate-limiting, so it should only be deployed behind a
//! rate-limiting module.
//!
//! NOTE: This document and the API stubs in this module represent a work in
//! progress and will likely change in their details. However, barring the
//! discovery of a major flaw in the current design, the general design of the
//! QS should remain the same.
//!
//! # Overview
//!
//! The QS maintains the queues of clients of the homeserver and provides the
//! following functionalities:
//!
//! * queue creation by clients (although each client can only create a single
//!   queue)
//! * enqueuing of messages by delivery services (either local or remote) that
//!   are authorized to enqueue in a given queue
//! * dequeuing of messages by the owner of a given queue
//! * updating of queue information by the owner of a given queue
//! * notification of the queue owner upon message enqueuing, either via a
//!   Websocket or via a push token
//! * queue deletion either by the queue owner or by another authorized client
//!
//! # Encryption-at-rest
//!
//! To protect the metadata visible in MLS PublicMessages, the QS encrypts
//! messages in queues to the owning client. This is done using a simple
//! construction, where the owning client provides an HPKE public key to which
//! the QS can encrypt the symmetric key it uses to encrypt messages. This key
//! is occasionally updated by sampling a fresh key and using an HKDF to combine
//! it with the existing key. The fresh key is then encrypted to the HPKE key
//! and enqueued. Additionally, with each encryption, the key is ratcheted
//! forward using the same HKDF (but without fresh key material).
//!
//! # Queue creation
//!
//! Clients can create queues that are not associated with them and are
//! therefore pseudonymous.
//!
//! # Message enqueuing
//!
//! Delivery services that want to enqueue a message in a queue with a given
//! QueueID have to prove that they are authorized by the owner of the queue by
//! providing a signature over the enqueuing request.
//!
//! The QS then encrypts the messages (see above on how messages are encrypted
//! at rest), marks them with a sequence number and enqueues them.
//!
//! # Message dequeuing
//!
//! Clients that want to dequeue messages first have to authenticate themselves
//! as the owner of the given queue.
//!
//! They can then request messages with a given range of sequence numbers. When
//! receiving such a request, the QS deletes any messages with sequence numbers
//! smaller than the smalles requested one and responds with the requested
//! messages.

use aircommon::{
    identifiers::{Fqdn, QsClientId},
    messages::{QueueMessage, client_ds::DsEventMessage, push_token::PushToken},
};
use client_id_decryption_key::StorableClientIdDecryptionKey;

use metrics::describe_gauge;
use sqlx::PgPool;

use crate::{
    air_service::{BackendService, ServiceCreationError},
    messages::intra_backend::DsFanOutMessage,
    qs::queue::Queues,
};

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
}

pub(crate) const METRIC_AIR_QS_TOTAL_USERS: &str = "air_qs_total_users";
pub(crate) const METRIC_AIR_QS_MAU_USERS: &str = "air_qs_mau_users";
pub(crate) const METRIC_AIR_QS_DAU_USERS: &str = "air_qs_dau_users";
pub(crate) const METRIC_AIR_ACTIVE_USERS: &str = "air_qs_active_users";

impl BackendService for Qs {
    async fn initialize(db_pool: PgPool, domain: Fqdn) -> Result<Self, ServiceCreationError> {
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
        })
    }

    fn describe_metrics() {
        describe_gauge!(METRIC_AIR_QS_TOTAL_USERS, "Number of total users");
        describe_gauge!(
            METRIC_AIR_QS_DAU_USERS,
            "Number of rolling DAU (daily active users)"
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
