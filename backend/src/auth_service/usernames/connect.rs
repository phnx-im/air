// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::{
    identifiers::UsernameHash, messages::connection_package::VersionedConnectionPackage,
    time::ExpirationData,
};
use airprotos::{
    auth_service::{
        convert::UsernameHashError,
        v1::{
            ConnectRequest, ConnectResponse, ConnectionOfferMessage,
            EnqueueConnectionOfferResponse, FetchConnectionPackageResponse, connect_request,
            connect_response, username_queue_message,
        },
    },
    validation::{MissingFieldError, MissingFieldExt},
};
use displaydoc::Display;
use futures_util::Stream;
use semver::VersionReq;
use sqlx::PgPool;
use thiserror::Error;
use tokio::sync::mpsc;
use tokio_stream::StreamExt;
use tonic::{Status, Streaming};
use tracing::{debug, error};

use crate::auth_service::{AuthService, connection_package::StorableConnectionPackage};

use super::{UsernameRecord, queue::UsernameQueueError};

/// The protocol for a user connecting to another user via their username
#[cfg_attr(test, mockall::automock)]
pub(crate) trait ConnectUsernameProtocol {
    /// Implements the Connect Username protocol
    async fn connect_username_protocol(
        self,
        incoming: Streaming<ConnectRequest>,
        outgoing: mpsc::Sender<Result<ConnectResponse, Status>>,
    ) where
        Self: Sized,
    {
        run_protocol(&self, incoming, &outgoing).await
    }

    async fn load_username_expiration_data(
        &self,
        hash: &UsernameHash,
    ) -> sqlx::Result<Option<ExpirationData>>;

    async fn get_connection_package_for_username(
        &self,
        hash: &UsernameHash,
    ) -> sqlx::Result<VersionedConnectionPackage>;

    async fn enqueue_connection_offer(
        &self,
        hash: &UsernameHash,
        connection_offer: ConnectionOfferMessage,
    ) -> Result<(), UsernameQueueError>;

    #[expect(clippy::needless_lifetimes)]
    fn client_version_req<'a>(&'a self) -> Option<&'a VersionReq>;
}

async fn run_protocol(
    protocol: &impl ConnectUsernameProtocol,
    incoming: impl Stream<Item = Result<ConnectRequest, Status>> + Unpin,
    outgoing: &mpsc::Sender<Result<ConnectResponse, Status>>,
) {
    if let Err(error) = run_protocol_impl(protocol, incoming, outgoing).await {
        error!(%error, "error in connect username protocol");
        let _ignore_closed_channel = outgoing.send(Err(error.into())).await;
    }
}

async fn run_protocol_impl(
    protocol: &impl ConnectUsernameProtocol,
    mut incoming: impl Stream<Item = Result<ConnectRequest, Status>> + Unpin,
    outgoing: &mpsc::Sender<Result<ConnectResponse, Status>>,
) -> Result<(), ConnectProtocolError> {
    // step 1: fetch connection package for a handle hash
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
            error!(%error, "error in connect username protocol");
            return Ok(());
        }
        None => return Ok(()),
    };

    crate::version::verify_client_version(
        protocol.client_version_req(),
        fetch_connection_package.client_metadata.as_ref(),
    )
    .map_err(ConnectProtocolError::UnsupportedVersion)?;

    let hash = fetch_connection_package
        .hash
        .ok_or_missing_field("hash")?
        .try_into()?;

    debug!("load username expiration data");
    let Some(expiration_data) = protocol.load_username_expiration_data(&hash).await? else {
        return Err(ConnectProtocolError::UsernameNotFound);
    };
    if !expiration_data.validate() {
        return Err(ConnectProtocolError::UsernameNotFound);
    }

    debug!("get connection package for username");
    let connection_package = protocol.get_connection_package_for_username(&hash).await?;
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
            error!(%error, "error in connect username protocol");
            return Ok(());
        }
        None => return Ok(()),
    };

    let connection_establishment_package = enqueue_offer
        .connection_offer
        .ok_or_missing_field("connection_offer")?;

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
    /// Username not found
    UsernameNotFound,
    /// Invalid hash: $0
    InvalidHash(#[from] UsernameHashError),
    /// Missing required field in request
    MissingField(#[from] MissingFieldError<&'static str>),
    /// Enqueue failed
    Enqueue(#[from] UsernameQueueError),
    /// Unsupported version
    UnsupportedVersion(Status),
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
            ConnectProtocolError::UsernameNotFound => Status::not_found(msg),
            ConnectProtocolError::MissingField(_) | ConnectProtocolError::InvalidHash(_) => {
                Status::invalid_argument(msg)
            }
            ConnectProtocolError::Enqueue(error) => {
                error!(%error, "enqueue failed");
                Status::internal(msg)
            }
            ConnectProtocolError::UnsupportedVersion(status) => status,
        }
    }
}

impl ConnectUsernameProtocol for AuthService {
    async fn load_username_expiration_data(
        &self,
        hash: &UsernameHash,
    ) -> sqlx::Result<Option<ExpirationData>> {
        Self::load_username_expiration_data_impl(&self.db_pool, hash).await
    }

    async fn get_connection_package_for_username(
        &self,
        hash: &UsernameHash,
    ) -> sqlx::Result<VersionedConnectionPackage> {
        StorableConnectionPackage::load_for_username(&self.db_pool, hash).await
    }

    async fn enqueue_connection_offer(
        &self,
        hash: &UsernameHash,
        connection_offer: ConnectionOfferMessage,
    ) -> Result<(), UsernameQueueError> {
        let payload = username_queue_message::Payload::ConnectionOffer(connection_offer);
        self.username_queues.enqueue(hash, payload).await?;
        Ok(())
    }

    fn client_version_req(&self) -> Option<&VersionReq> {
        self.client_version_req.as_ref()
    }
}

impl AuthService {
    async fn load_username_expiration_data_impl(
        pool: &PgPool,
        hash: &UsernameHash,
    ) -> sqlx::Result<Option<ExpirationData>> {
        let expiration_data = UsernameRecord::load_expiration_data(pool, hash).await?;
        let Some(expiration_data) = expiration_data else {
            return Ok(None);
        };

        // Delete the username if the expiration date has passed
        if !expiration_data.validate() {
            UsernameRecord::delete(pool, hash).await?;
            return Ok(None);
        }

        Ok(Some(expiration_data))
    }
}

#[cfg(test)]
mod tests {
    use std::{sync::LazyLock, time};

    use aircommon::{
        credentials::keys::{UsernameSigningKey, UsernameVerifyingKey},
        time::Duration,
    };
    use airprotos::{
        auth_service::v1::{
            self, ConnectionOfferMessage, EnqueueConnectionOfferResponse,
            EnqueueConnectionOfferStep, FetchConnectionPackageStep,
        },
        common::{self, v1::ClientMetadata},
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
        version: Some(common::v1::Version {
            major: 0,
            minor: 1,
            patch: 0,
            pre: "dev".to_owned(),
            build_number: 1,
            commit_hash: vec![0xa1, 0xb1, 0xc1, 0xd1],
        }),
    });

    #[expect(clippy::type_complexity, reason = "usage in tests is straightforward")]
    fn run_test_protocol(
        mock_protocol: MockConnectUsernameProtocol,
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
    async fn connect_username_protocol_success() -> anyhow::Result<()> {
        init_test_tracing();

        let signing_key = UsernameSigningKey::generate().unwrap();

        let hash = UsernameHash::new([1; 32]);
        let expiration_data = ExpirationData::new(Duration::days(1));
        let connection_package = random_connection_package(
            signing_key.verifying_key().clone(),
            ConnectionPackageType::V2 {
                is_last_resort: false,
            },
        );
        let connection_offer = ConnectionOfferMessage::default();

        let mut mock_protocol = MockConnectUsernameProtocol::new();

        mock_protocol
            .expect_load_username_expiration_data()
            .with(eq(hash))
            .returning(move |_| Ok(Some(expiration_data.clone())));

        let inner_connection_package = connection_package.clone();
        mock_protocol
            .expect_get_connection_package_for_username()
            .with(eq(hash))
            .returning(move |_| Ok(inner_connection_package.clone()));

        mock_protocol
            .expect_enqueue_connection_offer()
            .with(eq(hash), eq(connection_offer.clone()))
            .returning(|_, _| Ok(()));

        mock_protocol.expect_client_version_req().returning(|| None);

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
    async fn connect_username_protocol_username_not_found() -> anyhow::Result<()> {
        init_test_tracing();

        let hash = UsernameHash::new([1; 32]);

        let mut mock_protocol = MockConnectUsernameProtocol::new();

        mock_protocol
            .expect_load_username_expiration_data()
            .with(eq(hash))
            .returning(|_| Ok(None));

        mock_protocol.expect_client_version_req().returning(|| None);

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
    async fn connect_username_protocol_username_expired() -> anyhow::Result<()> {
        init_test_tracing();

        let hash = UsernameHash::new([1; 32]);

        let mut mock_protocol = MockConnectUsernameProtocol::new();

        mock_protocol
            .expect_load_username_expiration_data()
            .with(eq(hash))
            .returning(|_| Ok(Some(ExpirationData::new(Duration::milliseconds(1)))));

        mock_protocol.expect_client_version_req().returning(|| None);

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
    async fn connect_username_protocol_protocol_violation() -> anyhow::Result<()> {
        init_test_tracing();

        let mock_protocol = MockConnectUsernameProtocol::new();
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

        let mock_protocol = MockConnectUsernameProtocol::new();
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

        let signing_key = UsernameSigningKey::generate()?;

        let hash = UsernameHash::new([1; 32]);
        let expiration_data = ExpirationData::new(Duration::days(1));
        let connection_package = random_connection_package(
            signing_key.verifying_key().clone(),
            ConnectionPackageType::V2 {
                is_last_resort: false,
            },
        );

        let mut mock_protocol = MockConnectUsernameProtocol::new();

        mock_protocol
            .expect_load_username_expiration_data()
            .with(eq(hash))
            .returning(move |_| Ok(Some(expiration_data.clone())));

        let inner_connection_package = connection_package.clone();
        mock_protocol
            .expect_get_connection_package_for_username()
            .with(eq(hash))
            .returning(move |_| Ok(inner_connection_package.clone()));

        mock_protocol.expect_client_version_req().returning(|| None);

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
    async fn load_username_expiration_data_loads(pool: PgPool) -> anyhow::Result<()> {
        let hash = UsernameHash::new([1; 32]);
        let expiration_data = ExpirationData::new(Duration::days(1));

        let record = UsernameRecord {
            username_hash: hash,
            verifying_key: UsernameVerifyingKey::from_bytes(vec![1, 2, 3, 4, 5]),
            expiration_data: expiration_data.clone(),
        };
        let mut txn = pool.begin().await?;
        record.store(&mut txn).await?;
        txn.commit().await?;

        let expiration_data = AuthService::load_username_expiration_data_impl(&pool, &hash).await?;
        assert_eq!(expiration_data.as_ref(), Some(&record.expiration_data));

        Ok(())
    }

    #[sqlx::test]
    async fn load_username_expiration_data_deletes_expired_username(
        pool: PgPool,
    ) -> anyhow::Result<()> {
        let hash = UsernameHash::new([1; 32]);
        let expiration_data = ExpirationData::new(Duration::zero());

        let record = UsernameRecord {
            username_hash: hash,
            verifying_key: UsernameVerifyingKey::from_bytes(vec![1, 2, 3, 4, 5]),
            expiration_data: expiration_data.clone(),
        };
        let mut txn = pool.begin().await?;
        record.store(&mut txn).await?;
        txn.commit().await?;

        UsernameRecord::load_verifying_key(&pool, &hash)
            .await?
            .expect("username should exist");

        let expiration_data = AuthService::load_username_expiration_data_impl(&pool, &hash).await?;
        assert_eq!(expiration_data, None);

        // Check that the record is deleted
        let loaded = UsernameRecord::load_verifying_key(&pool, &hash).await?;
        assert_eq!(loaded, None);

        Ok(())
    }
}
