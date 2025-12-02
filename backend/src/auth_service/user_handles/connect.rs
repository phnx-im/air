// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::{
    identifiers::UserHandleHash, messages::connection_package::VersionedConnectionPackage,
    time::ExpirationData,
};
use airprotos::{
    auth_service::{
        convert::UserHandleHashError,
        v1::{
            ConnectRequest, ConnectResponse, ConnectionOfferMessage,
            EnqueueConnectionOfferResponse, FetchConnectionPackageResponse, connect_request,
            connect_response, handle_queue_message,
        },
    },
    validation::{MissingFieldError, MissingFieldExt},
};
use displaydoc::Display;
use futures_util::Stream;
use sqlx::PgPool;
use thiserror::Error;
use tokio::sync::mpsc;
use tokio_stream::StreamExt;
use tonic::{Status, Streaming};
use tracing::{debug, error};

use crate::auth_service::{AuthService, connection_package::StorableConnectionPackage};

use super::{UserHandleRecord, queue::HandleQueueError};

/// The protocol for a user connecting to another user via their handle
#[cfg_attr(test, mockall::automock)]
pub(crate) trait ConnectHandleProtocol {
    /// Implements the Connect Handle protocol
    async fn connect_handle_protocol(
        self,
        incoming: Streaming<ConnectRequest>,
        outgoing: mpsc::Sender<Result<ConnectResponse, Status>>,
    ) where
        Self: Sized,
    {
        run_protocol(&self, incoming, &outgoing).await
    }

    async fn load_user_handle_expiration_data(
        &self,
        hash: &UserHandleHash,
    ) -> sqlx::Result<Option<ExpirationData>>;

    async fn get_connection_package_for_handle(
        &self,
        hash: &UserHandleHash,
    ) -> sqlx::Result<VersionedConnectionPackage>;

    async fn enqueue_connection_offer(
        &self,
        hash: &UserHandleHash,
        connection_offer: ConnectionOfferMessage,
    ) -> Result<(), HandleQueueError>;
}

async fn run_protocol(
    protocol: &impl ConnectHandleProtocol,
    incoming: impl Stream<Item = Result<ConnectRequest, Status>> + Unpin,
    outgoing: &mpsc::Sender<Result<ConnectResponse, Status>>,
) {
    if let Err(error) = run_protocol_impl(protocol, incoming, outgoing).await {
        error!(%error, "error in connect handle protocol");
        let _ignore_closed_channel = outgoing.send(Err(error.into())).await;
    }
}

async fn run_protocol_impl(
    protocol: &impl ConnectHandleProtocol,
    mut incoming: impl Stream<Item = Result<ConnectRequest, Status>> + Unpin,
    outgoing: &mpsc::Sender<Result<ConnectResponse, Status>>,
) -> Result<(), ConnectProtocolError> {
    // step 1: fetch connetion package for a handle hash
    debug!("step 1: waiting for fetch connection package step");
    let step = incoming.next().await;
    let fetch_connection_package = match step {
        Some(Ok(ConnectRequest {
            step: Some(connect_request::Step::Fetch(fetch)),
        })) => fetch,
        Some(Ok(_)) => {
            return Err(ConnectProtocolError::ProtocolViolation("expected fetch"));
        }
        Some(Err(error)) => {
            error!(%error, "error in connect handle protocol");
            return Ok(());
        }
        None => return Ok(()),
    };

    let hash = fetch_connection_package
        .hash
        .ok_or_missing_field("hash")?
        .try_into()?;

    debug!("load user handle expiration data");
    let Some(expiration_data) = protocol.load_user_handle_expiration_data(&hash).await? else {
        return Err(ConnectProtocolError::HandleNotFound);
    };
    if !expiration_data.validate() {
        return Err(ConnectProtocolError::HandleNotFound);
    }

    debug!("get connection package for handle");
    let connection_package = protocol.get_connection_package_for_handle(&hash).await?;
    if outgoing
        .send(Ok(ConnectResponse {
            step: Some(connect_response::Step::FetchResponse(
                FetchConnectionPackageResponse {
                    connection_package: Some(connection_package.into()),
                },
            )),
        }))
        .await
        .is_err()
    {
        return Ok(()); // protocol aborted
    }

    // step 2: enqueue encrypted connection establishment package
    debug!("step 2: waiting for enqueue package step");
    let step = incoming.next().await;
    let enqueue_offer = match step {
        Some(Ok(ConnectRequest {
            step: Some(connect_request::Step::Enqueue(enqueue_package)),
        })) => enqueue_package,
        Some(Ok(_)) => {
            return Err(ConnectProtocolError::ProtocolViolation("expected enqueue"));
        }
        Some(Err(error)) => {
            error!(%error, "error in connect handle protocol");
            return Ok(());
        }
        None => return Ok(()),
    };

    let connection_establishment_package = enqueue_offer
        .connection_offer
        .ok_or_missing_field("connecton_offer")?;

    debug!("enqueue connection offer");
    protocol
        .enqueue_connection_offer(&hash, connection_establishment_package)
        .await?;

    // acknowledge
    debug!("acknowledge protocol finished");
    if outgoing
        .send(Ok(ConnectResponse {
            step: Some(connect_response::Step::EnqueueResponse(
                EnqueueConnectionOfferResponse {},
            )),
        }))
        .await
        .is_err()
    {
        return Ok(()); // protocol aborted
    }

    debug!("protocol finished");
    Ok(())
}

#[derive(Debug, Error, Display)]
pub(crate) enum ConnectProtocolError {
    /// Protocol violation: $0
    ProtocolViolation(&'static str),
    /// Database provider error
    Database(#[from] sqlx::Error),
    /// Handle not found
    HandleNotFound,
    /// Invalid hash: $0
    InvalidHash(#[from] UserHandleHashError),
    /// Missing required field in request
    MissingField(#[from] MissingFieldError<&'static str>),
    /// Enqueue failed
    Enqueue(#[from] HandleQueueError),
}

impl From<ConnectProtocolError> for Status {
    fn from(error: ConnectProtocolError) -> Self {
        let msg = error.to_string();
        match error {
            ConnectProtocolError::ProtocolViolation(_) => Status::failed_precondition(msg),
            ConnectProtocolError::Database(error) => {
                error!(%error, "database error");
                Status::internal(msg)
            }
            ConnectProtocolError::HandleNotFound => Status::not_found(msg),
            ConnectProtocolError::MissingField(_) | ConnectProtocolError::InvalidHash(_) => {
                Status::invalid_argument(msg)
            }
            ConnectProtocolError::Enqueue(error) => {
                error!(%error, "enqueue failed");
                Status::internal(msg)
            }
        }
    }
}

impl ConnectHandleProtocol for AuthService {
    async fn load_user_handle_expiration_data(
        &self,
        hash: &UserHandleHash,
    ) -> sqlx::Result<Option<ExpirationData>> {
        Self::load_user_handle_expiration_data_impl(&self.db_pool, hash).await
    }

    async fn get_connection_package_for_handle(
        &self,
        hash: &UserHandleHash,
    ) -> sqlx::Result<VersionedConnectionPackage> {
        StorableConnectionPackage::load_for_handle(&self.db_pool, hash).await
    }

    async fn enqueue_connection_offer(
        &self,
        hash: &UserHandleHash,
        connection_offer: ConnectionOfferMessage,
    ) -> Result<(), HandleQueueError> {
        let payload = handle_queue_message::Payload::ConnectionOffer(connection_offer);
        self.handle_queues.enqueue(hash, payload).await?;
        Ok(())
    }
}

impl AuthService {
    async fn load_user_handle_expiration_data_impl(
        pool: &PgPool,
        hash: &UserHandleHash,
    ) -> sqlx::Result<Option<ExpirationData>> {
        let expiration_data = UserHandleRecord::load_expiration_data(pool, hash).await?;
        let Some(expiration_data) = expiration_data else {
            return Ok(None);
        };

        // Delete the handle if the expiration date has passed
        if !expiration_data.validate() {
            UserHandleRecord::delete(pool, hash).await?;
            return Ok(None);
        }

        Ok(Some(expiration_data))
    }
}

#[cfg(test)]
mod tests {
    use std::{sync::LazyLock, time};

    use aircommon::{
        credentials::keys::{HandleSigningKey, HandleVerifyingKey},
        time::Duration,
    };
    use airprotos::{
        auth_service::v1::{
            self, ConnectionOfferMessage, EnqueueConnectionOfferResponse,
            EnqueueConnectionOfferStep, FetchConnectionPackageStep,
        },
        common::v1::{ClientMetadata, Platform, ReleaseChannel},
    };
    use mockall::predicate::*;
    use tokio::{sync::mpsc, task::JoinHandle, time::timeout};
    use tokio_stream::wrappers::ReceiverStream;

    use crate::auth_service::connection_package::persistence::tests::{
        ConnectionPackageType, random_connection_package,
    };

    use super::*;

    fn init_test_tracing() {
        let _ = tracing_subscriber::fmt::fmt()
            .with_test_writer()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .try_init();
    }

    const PROTOCOL_TIMEOUT: time::Duration = time::Duration::from_secs(1);

    static CLIENT_METADATA: LazyLock<ClientMetadata> = LazyLock::new(|| ClientMetadata {
        version: "0.1.0".to_owned(),
        platform: Platform::Linux.into(),
        channel: ReleaseChannel::Dev.into(),
    });

    #[expect(clippy::type_complexity, reason = "usage in tests is straightforward")]
    fn run_test_protocol(
        mock_protocol: MockConnectHandleProtocol,
    ) -> (
        mpsc::Sender<Result<ConnectRequest, Status>>,
        mpsc::Receiver<Result<ConnectResponse, Status>>,
        JoinHandle<()>,
    ) {
        let (requests_tx, requests_rx) = mpsc::channel(10);
        let (responses_tx, responses_rx) = mpsc::channel(10);

        // run the protocol
        let run_handle = tokio::spawn(async move {
            timeout(
                PROTOCOL_TIMEOUT,
                run_protocol(
                    &mock_protocol,
                    ReceiverStream::new(requests_rx),
                    &responses_tx,
                ),
            )
            .await
            .expect("protocol handler timed out")
        });

        (requests_tx, responses_rx, run_handle)
    }

    #[tokio::test]
    async fn connect_handle_protocol_success() -> anyhow::Result<()> {
        init_test_tracing();

        let signing_key = HandleSigningKey::generate().unwrap();

        let hash = UserHandleHash::new([1; 32]);
        let expiration_data = ExpirationData::new(Duration::days(1));
        let connection_package = random_connection_package(
            signing_key.verifying_key().clone(),
            ConnectionPackageType::V2 {
                is_last_resort: false,
            },
        );
        let connection_offer = ConnectionOfferMessage::default();

        let mut mock_protocol = MockConnectHandleProtocol::new();

        mock_protocol
            .expect_load_user_handle_expiration_data()
            .with(eq(hash))
            .returning(move |_| Ok(Some(expiration_data.clone())));

        let inner_connection_package = connection_package.clone();
        mock_protocol
            .expect_get_connection_package_for_handle()
            .with(eq(hash))
            .returning(move |_| Ok(inner_connection_package.clone()));

        mock_protocol
            .expect_enqueue_connection_offer()
            .with(eq(hash), eq(connection_offer.clone()))
            .returning(|_, _| Ok(()));

        let (requests, mut responses, run_handle) = run_test_protocol(mock_protocol);

        let request_fetch = ConnectRequest {
            step: Some(connect_request::Step::Fetch(FetchConnectionPackageStep {
                client_metadata: Some(CLIENT_METADATA.clone()),
                hash: Some(hash.into()),
            })),
        };

        // step 1
        requests.send(Ok(request_fetch)).await.unwrap();
        match responses.recv().await.unwrap() {
            Ok(ConnectResponse {
                step:
                    Some(connect_response::Step::FetchResponse(FetchConnectionPackageResponse {
                        connection_package: Some(received_connection_package),
                    })),
            }) => {
                let connection_package_proto: v1::ConnectionPackage = connection_package.into();
                assert_eq!(connection_package_proto, received_connection_package);
            }
            _ => panic!("unexpected response type"),
        }

        // step 2
        let request_enqueue = ConnectRequest {
            step: Some(connect_request::Step::Enqueue(EnqueueConnectionOfferStep {
                connection_offer: Some(connection_offer.clone()),
            })),
        };
        requests.send(Ok(request_enqueue)).await.unwrap();
        match responses.recv().await.unwrap() {
            Ok(ConnectResponse {
                step:
                    Some(connect_response::Step::EnqueueResponse(EnqueueConnectionOfferResponse {})),
            }) => {}
            _ => panic!("unexpected response type"),
        }

        run_handle.await.expect("protocol panicked");

        Ok(())
    }

    #[tokio::test]
    async fn connect_handle_protocol_handle_not_found() -> anyhow::Result<()> {
        init_test_tracing();

        let hash = UserHandleHash::new([1; 32]);

        let mut mock_protocol = MockConnectHandleProtocol::new();

        mock_protocol
            .expect_load_user_handle_expiration_data()
            .with(eq(hash))
            .returning(|_| Ok(None));

        let (requests, mut responses, run_handle) = run_test_protocol(mock_protocol);

        let request_fetch = ConnectRequest {
            step: Some(connect_request::Step::Fetch(FetchConnectionPackageStep {
                client_metadata: Some(CLIENT_METADATA.clone()),
                hash: Some(hash.into()),
            })),
        };

        requests.send(Ok(request_fetch)).await.unwrap();

        let response = responses.recv().await.unwrap();
        assert_eq!(response.unwrap_err().code(), tonic::Code::NotFound);

        run_handle.await.expect("protocol panicked");

        Ok(())
    }

    #[tokio::test]
    async fn connect_handle_protocol_handle_expired() -> anyhow::Result<()> {
        init_test_tracing();

        let hash = UserHandleHash::new([1; 32]);

        let mut mock_protocol = MockConnectHandleProtocol::new();

        mock_protocol
            .expect_load_user_handle_expiration_data()
            .with(eq(hash))
            .returning(|_| Ok(Some(ExpirationData::new(Duration::milliseconds(1)))));

        let (requests, mut responses, run_handle) = run_test_protocol(mock_protocol);

        let request_fetch = ConnectRequest {
            step: Some(connect_request::Step::Fetch(FetchConnectionPackageStep {
                client_metadata: Some(CLIENT_METADATA.clone()),
                hash: Some(hash.into()),
            })),
        };

        requests.send(Ok(request_fetch)).await.unwrap();

        let response = responses.recv().await.unwrap();
        assert_eq!(response.unwrap_err().code(), tonic::Code::NotFound);

        run_handle.await.expect("protocol panicked");

        Ok(())
    }

    #[tokio::test]
    async fn connect_handle_protocol_protocol_violation() -> anyhow::Result<()> {
        init_test_tracing();

        let mock_protocol = MockConnectHandleProtocol::new();
        let (requests, mut responses, run_handle) = run_test_protocol(mock_protocol);

        // empty requests in step 1

        requests
            .send(Ok(ConnectRequest { step: None }))
            .await
            .unwrap();
        let response = responses.recv().await.unwrap();
        assert_eq!(
            response.unwrap_err().code(),
            tonic::Code::FailedPrecondition
        );

        run_handle.await.expect("protocol panicked");

        let mock_protocol = MockConnectHandleProtocol::new();
        let (requests, mut responses, run_handle) = run_test_protocol(mock_protocol);

        // enqueue in step 1

        requests
            .send(Ok(ConnectRequest {
                step: Some(connect_request::Step::Enqueue(EnqueueConnectionOfferStep {
                    connection_offer: None,
                })),
            }))
            .await
            .unwrap();
        let response = responses.recv().await.unwrap();
        assert_eq!(
            response.unwrap_err().code(),
            tonic::Code::FailedPrecondition
        );

        run_handle.await.expect("protocol panicked");

        // fetch in step 2

        let signing_key = HandleSigningKey::generate()?;

        let hash = UserHandleHash::new([1; 32]);
        let expiration_data = ExpirationData::new(Duration::days(1));
        let connection_package = random_connection_package(
            signing_key.verifying_key().clone(),
            ConnectionPackageType::V2 {
                is_last_resort: false,
            },
        );

        let mut mock_protocol = MockConnectHandleProtocol::new();

        mock_protocol
            .expect_load_user_handle_expiration_data()
            .with(eq(hash))
            .returning(move |_| Ok(Some(expiration_data.clone())));

        let inner_connection_package = connection_package.clone();
        mock_protocol
            .expect_get_connection_package_for_handle()
            .with(eq(hash))
            .returning(move |_| Ok(inner_connection_package.clone()));

        let (requests, mut responses, run_handle) = run_test_protocol(mock_protocol);

        requests
            .send(Ok(ConnectRequest {
                step: Some(connect_request::Step::Fetch(FetchConnectionPackageStep {
                    client_metadata: Some(CLIENT_METADATA.clone()),
                    hash: Some(hash.into()),
                })),
            }))
            .await
            .unwrap();
        let response = responses.recv().await.unwrap();
        assert!(response.is_ok());

        requests
            .send(Ok(ConnectRequest {
                step: Some(connect_request::Step::Fetch(FetchConnectionPackageStep {
                    client_metadata: Some(CLIENT_METADATA.clone()),
                    hash: Some(hash.into()),
                })),
            }))
            .await
            .unwrap();
        let response = responses.recv().await.unwrap();
        assert_eq!(
            response.unwrap_err().code(),
            tonic::Code::FailedPrecondition
        );

        run_handle.await.expect("protocol panicked");

        Ok(())
    }

    #[sqlx::test]
    async fn load_user_handle_expiration_data_loads(pool: PgPool) -> anyhow::Result<()> {
        let hash = UserHandleHash::new([1; 32]);
        let expiration_data = ExpirationData::new(Duration::days(1));

        let record = UserHandleRecord {
            user_handle_hash: hash,
            verifying_key: HandleVerifyingKey::from_bytes(vec![1, 2, 3, 4, 5]),
            expiration_data: expiration_data.clone(),
        };
        record.store(&pool).await?;

        let expiration_data =
            AuthService::load_user_handle_expiration_data_impl(&pool, &hash).await?;
        assert_eq!(expiration_data.as_ref(), Some(&record.expiration_data));

        Ok(())
    }

    #[sqlx::test]
    async fn load_user_handle_expiration_data_deletes_expired_handle(
        pool: PgPool,
    ) -> anyhow::Result<()> {
        let hash = UserHandleHash::new([1; 32]);
        let expiration_data = ExpirationData::new(Duration::zero());

        let record = UserHandleRecord {
            user_handle_hash: hash,
            verifying_key: HandleVerifyingKey::from_bytes(vec![1, 2, 3, 4, 5]),
            expiration_data: expiration_data.clone(),
        };
        record.store(&pool).await?;

        UserHandleRecord::load_verifying_key(&pool, &hash)
            .await?
            .expect("handle should exist");

        let expiration_data =
            AuthService::load_user_handle_expiration_data_impl(&pool, &hash).await?;
        assert_eq!(expiration_data, None);

        // Check that the record is deleted
        let loaded = UserHandleRecord::load_verifying_key(&pool, &hash).await?;
        assert_eq!(loaded, None);

        Ok(())
    }
}
