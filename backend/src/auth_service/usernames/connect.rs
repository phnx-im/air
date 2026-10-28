// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::{identifiers::UsernameHash, time::ExpirationData};
use airprotos::{
    auth_service::{
        convert::UsernameHashError,
        v1::{
            ConnectUsernameRequest, ConnectUsernameResponse, ConnectionOfferMessage,
            EnqueueConnectionOfferResponse, FetchSignedConnectionPackageResponse, OperationType,
            SignedConnectionPackage, connect_username_request::Step, connect_username_response,
            username_queue_message,
        },
    },
    validation::{MissingFieldError, MissingFieldExt},
};
use chrono::Utc;
use displaydoc::Display;
use futures_util::Stream;
use privacypass::{amortized_tokens::AmortizedToken, private_tokens::Ristretto255};
use sqlx::PgPool;
use thiserror::Error;
use tls_codec::Deserialize;
use tokio::sync::mpsc;
use tokio_stream::StreamExt;
use tonic::{Status, Streaming};
use tracing::{debug, error};

use crate::{
    auth_service::{AuthService, connection_package::StorableSignedConnectionPackage},
    errors::auth_service::RedeemTokenError,
    version::VersionPolicy,
};

use super::{UsernameRecord, queue::UsernameQueueError};

/// The protocol for a user connecting to another user via their username
#[cfg_attr(test, mockall::automock)]
pub(crate) trait ConnectUsernameProtocol {
    /// Implements the Connect Username protocol
    async fn connect_username_protocol(
        self,
        incoming: Streaming<ConnectUsernameRequest>,
        outgoing: mpsc::Sender<Result<ConnectUsernameResponse, Status>>,
    ) where
        Self: Sized,
    {
        run_protocol(&self, incoming, &outgoing).await
    }

    /// Returns `None` if the username does not exist or is expired.
    async fn load_username_expiration_data(
        &self,
        hash: &UsernameHash,
    ) -> sqlx::Result<Option<ExpirationData>>;

    /// Hands out a connection package of the username, redeeming `token`
    /// first.
    async fn fetch_connection_package(
        &self,
        hash: &UsernameHash,
        token: AmortizedToken<Ristretto255>,
    ) -> Result<SignedConnectionPackage, ConnectProtocolError>;

    async fn enqueue_connection_offer(
        &self,
        hash: &UsernameHash,
        connection_offer: ConnectionOfferMessage,
    ) -> Result<(), UsernameQueueError>;

    fn version_policy(&self) -> &VersionPolicy;
}

async fn run_protocol(
    protocol: &impl ConnectUsernameProtocol,
    incoming: impl Stream<Item = Result<ConnectUsernameRequest, Status>> + Unpin,
    outgoing: &mpsc::Sender<Result<ConnectUsernameResponse, Status>>,
) {
    if let Err(error) = run_protocol_impl(protocol, incoming, outgoing).await {
        error!(%error, "error in connect username protocol");
        let _ignore_closed_channel = outgoing.send(Err(error.into())).await;
    }
}

async fn run_protocol_impl(
    protocol: &impl ConnectUsernameProtocol,
    mut incoming: impl Stream<Item = Result<ConnectUsernameRequest, Status>> + Unpin,
    outgoing: &mpsc::Sender<Result<ConnectUsernameResponse, Status>>,
) -> Result<(), ConnectProtocolError> {
    // step 1: fetch connection package for a handle hash
    debug!("step 1: waiting for fetch connection package step");
    let step = incoming.next().await;
    let fetch = match step {
        Some(Ok(ConnectUsernameRequest {
            step: Some(Step::FetchSigned(fetch)),
        })) => fetch,
        Some(Ok(_)) => {
            return Err(ConnectProtocolError::ProtocolViolation(
                "expected fetch signed",
            ));
        }
        Some(Err(error)) => {
            error!(%error, "error in connect username protocol");
            return Ok(());
        }
        None => return Ok(()),
    };

    // TODO: Communicate verification result to the client
    let _verified = protocol
        .version_policy()
        .verify_client_version(fetch.client_metadata.as_ref(), Utc::now())
        .map_err(ConnectProtocolError::UnsupportedVersion)?;

    let hash = fetch.hash.ok_or_missing_field("hash")?.try_into()?;

    debug!("load username expiration data");
    if protocol
        .load_username_expiration_data(&hash)
        .await?
        .is_none()
    {
        return Err(ConnectProtocolError::UsernameNotFound);
    }

    let token = fetch.token.ok_or(ConnectProtocolError::MissingToken)?;
    let token = AmortizedToken::<Ristretto255>::tls_deserialize_exact(&token)
        .map_err(|_| ConnectProtocolError::MalformedToken)?;

    debug!("get connection package for username");
    let connection_package = protocol.fetch_connection_package(&hash, token).await?;
    let step = connect_username_response::Step::FetchSignedResponse(
        FetchSignedConnectionPackageResponse {
            connection_package: Some(connection_package),
        },
    );
    if outgoing
        .send(Ok(ConnectUsernameResponse { step: Some(step) }))
        .await
        .is_err()
    {
        return Ok(()); // protocol aborted
    }

    // step 2: enqueue encrypted connection establishment package
    debug!("step 2: waiting for enqueue package step");
    let step = incoming.next().await;
    let enqueue_offer = match step {
        Some(Ok(ConnectUsernameRequest {
            step: Some(Step::Enqueue(enqueue_package)),
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
        .send(Ok(ConnectUsernameResponse {
            step: Some(connect_username_response::Step::EnqueueResponse(
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
    /// No connection package available
    NoConnectionPackage,
    /// Invalid hash: $0
    InvalidHash(#[from] UsernameHashError),
    /// Missing required field in request
    MissingField(#[from] MissingFieldError<&'static str>),
    /// Enqueue failed
    Enqueue(#[from] UsernameQueueError),
    /// Unsupported version
    UnsupportedVersion(Status),
    /// Missing token
    MissingToken,
    /// Malformed token
    MalformedToken,
    /// Token redemption failed
    TokenRedemption(#[from] RedeemTokenError),
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
            ConnectProtocolError::UsernameNotFound | ConnectProtocolError::NoConnectionPackage => {
                Status::not_found(msg)
            }
            ConnectProtocolError::MissingField(_)
            | ConnectProtocolError::InvalidHash(_)
            | ConnectProtocolError::MalformedToken => Status::invalid_argument(msg),
            ConnectProtocolError::MissingToken => Status::unauthenticated(msg),
            ConnectProtocolError::TokenRedemption(error) => error.into(),
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

    async fn fetch_connection_package(
        &self,
        hash: &UsernameHash,
        token: AmortizedToken<Ristretto255>,
    ) -> Result<SignedConnectionPackage, ConnectProtocolError> {
        let mut txn = self.db_pool.begin().await?;
        debug!("redeem connection request token");
        self.as_redeem_token(&mut txn, token, OperationType::ConnectUsername)
            .await?;
        let package = StorableSignedConnectionPackage::load_for_username(txn.as_mut(), hash)
            .await?
            .ok_or(ConnectProtocolError::NoConnectionPackage)?;
        txn.commit().await?;
        Ok(package)
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

    fn version_policy(&self) -> &VersionPolicy {
        &self.version_policy
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
            ConnectionOfferMessage, EnqueueConnectionOfferResponse, EnqueueConnectionOfferStep,
            FetchSignedConnectionPackageStep, connect_username_request,
        },
        common::{self, v1::ClientMetadata},
    };
    use mockall::predicate::*;
    use privacypass::TokenType;
    use tls_codec::Serialize;
    use tokio::{sync::mpsc, task::JoinHandle, time::timeout};
    use tokio_stream::wrappers::ReceiverStream;
    use tokio_util::sync::CancellationToken;

    use crate::{air_service::BackendService, auth_service::usernames::tests::issue_token};

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
        mpsc::Sender<Result<ConnectUsernameRequest, Status>>,
        mpsc::Receiver<Result<ConnectUsernameResponse, Status>>,
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
    async fn connect_username_protocol_username_not_found() -> anyhow::Result<()> {
        init_test_tracing();

        let hash = UsernameHash::new([1; 32]);

        // No fetch is expected, so the token is not redeemed. The username
        // check comes first.
        let mut mock_protocol = MockConnectUsernameProtocol::new();

        mock_protocol
            .expect_load_username_expiration_data()
            .with(eq(hash))
            .returning(|_| Ok(None));

        mock_protocol
            .expect_version_policy()
            .return_const(Default::default());

        let (requests, mut responses, run_handle) = run_test_protocol(mock_protocol);

        requests
            .send(Ok(fetch_signed_request(hash, Some(dummy_token()))))
            .await
            .unwrap();

        let response = responses.recv().await.unwrap();
        assert_eq!(response.unwrap_err().code(), tonic::Code::NotFound);

        run_handle.await.expect("protocol panicked");

        Ok(())
    }

    #[tokio::test]
    async fn connect_username_protocol_fetch_signed_success() -> anyhow::Result<()> {
        init_test_tracing();

        let signing_key = UsernameSigningKey::generate()?;

        let hash = UsernameHash::new([1; 32]);
        let expiration_data = ExpirationData::new(Duration::days(1));
        let (_decryption_key, connection_package, _metadata) =
            SignedConnectionPackage::generate(hash, &signing_key, false)?;
        let connection_offer = ConnectionOfferMessage::default();

        let mut mock_protocol = MockConnectUsernameProtocol::new();

        mock_protocol
            .expect_load_username_expiration_data()
            .with(eq(hash))
            .returning(move |_| Ok(Some(expiration_data.clone())));

        let inner_connection_package = connection_package.clone();
        mock_protocol
            .expect_fetch_connection_package()
            .withf(move |fetched_hash, _token| *fetched_hash == hash)
            .returning(move |_, _| Ok(inner_connection_package.clone()));

        mock_protocol
            .expect_enqueue_connection_offer()
            .with(eq(hash), eq(connection_offer.clone()))
            .returning(|_, _| Ok(()));

        mock_protocol
            .expect_version_policy()
            .return_const(Default::default());

        let (requests, mut responses, run_handle) = run_test_protocol(mock_protocol);

        // step 1
        requests
            .send(Ok(fetch_signed_request(hash, Some(dummy_token()))))
            .await
            .unwrap();
        match responses.recv().await.unwrap() {
            Ok(ConnectUsernameResponse {
                step:
                    Some(connect_username_response::Step::FetchSignedResponse(
                        FetchSignedConnectionPackageResponse {
                            connection_package: Some(received_connection_package),
                        },
                    )),
            }) => {
                assert_eq!(connection_package, received_connection_package);
            }
            _ => panic!("unexpected response type"),
        }

        // step 2
        let request_enqueue = ConnectUsernameRequest {
            step: Some(connect_username_request::Step::Enqueue(
                EnqueueConnectionOfferStep {
                    connection_offer: Some(connection_offer.clone()),
                },
            )),
        };
        requests.send(Ok(request_enqueue)).await.unwrap();
        match responses.recv().await.unwrap() {
            Ok(ConnectUsernameResponse {
                step:
                    Some(connect_username_response::Step::EnqueueResponse(
                        EnqueueConnectionOfferResponse {},
                    )),
            }) => {}
            _ => panic!("unexpected response type"),
        }

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
            .send(Ok(ConnectUsernameRequest { step: None }))
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
            .send(Ok(ConnectUsernameRequest {
                step: Some(connect_username_request::Step::Enqueue(
                    EnqueueConnectionOfferStep {
                        connection_offer: None,
                    },
                )),
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
        let (_decryption_key, connection_package, _metadata) =
            SignedConnectionPackage::generate(hash, &signing_key, false)?;

        let mut mock_protocol = MockConnectUsernameProtocol::new();

        mock_protocol
            .expect_load_username_expiration_data()
            .with(eq(hash))
            .returning(move |_| Ok(Some(expiration_data.clone())));

        let inner_connection_package = connection_package.clone();
        mock_protocol
            .expect_fetch_connection_package()
            .withf(move |fetched_hash, _token| *fetched_hash == hash)
            .returning(move |_, _| Ok(inner_connection_package.clone()));

        mock_protocol
            .expect_version_policy()
            .return_const(Default::default());

        let (requests, mut responses, run_handle) = run_test_protocol(mock_protocol);

        requests
            .send(Ok(fetch_signed_request(hash, Some(dummy_token()))))
            .await
            .unwrap();
        let response = responses.recv().await.unwrap();
        assert!(response.is_ok());

        requests
            .send(Ok(fetch_signed_request(hash, Some(dummy_token()))))
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
        record.store(&pool).await?;

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
        record.store(&pool).await?;

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

    /// A token that deserializes. The mocked protocol never verifies it.
    fn dummy_token() -> Vec<u8> {
        AmortizedToken::<Ristretto255>::new(
            TokenType::PrivateRistretto255,
            [1; 32],
            [2; 32],
            [3; 32],
            Default::default(),
        )
        .tls_serialize_detached()
        .unwrap()
    }

    fn fetch_signed_request(hash: UsernameHash, token: Option<Vec<u8>>) -> ConnectUsernameRequest {
        ConnectUsernameRequest {
            step: Some(connect_username_request::Step::FetchSigned(
                FetchSignedConnectionPackageStep {
                    client_metadata: Some(CLIENT_METADATA.clone()),
                    hash: Some(hash.into()),
                    token,
                },
            )),
        }
    }

    /// A protocol whose username exists.
    fn protocol_with_username(hash: UsernameHash) -> MockConnectUsernameProtocol {
        let expiration_data = ExpirationData::new(Duration::days(1));
        let mut mock_protocol = MockConnectUsernameProtocol::new();
        mock_protocol
            .expect_load_username_expiration_data()
            .with(eq(hash))
            .returning(move |_| Ok(Some(expiration_data.clone())));
        mock_protocol
            .expect_version_policy()
            .return_const(Default::default());
        mock_protocol
    }

    #[tokio::test]
    async fn connect_username_protocol_rejects_a_spent_token() -> anyhow::Result<()> {
        init_test_tracing();

        let hash = UsernameHash::new([1; 32]);
        let mut mock_protocol = protocol_with_username(hash);
        mock_protocol
            .expect_fetch_connection_package()
            .times(1)
            .returning(|_, _| Err(RedeemTokenError::InvalidToken.into()));

        let (requests, mut responses, run_handle) = run_test_protocol(mock_protocol);
        requests
            .send(Ok(fetch_signed_request(hash, Some(dummy_token()))))
            .await?;
        let response = responses.recv().await.unwrap();
        assert_eq!(response.unwrap_err().code(), tonic::Code::Unauthenticated);

        run_handle.await.expect("protocol panicked");
        Ok(())
    }

    #[tokio::test]
    async fn connect_username_protocol_rejects_a_missing_token() -> anyhow::Result<()> {
        init_test_tracing();

        let hash = UsernameHash::new([1; 32]);
        // No fetch is expected: nothing is handed out for free.
        let mock_protocol = protocol_with_username(hash);

        let (requests, mut responses, run_handle) = run_test_protocol(mock_protocol);
        requests.send(Ok(fetch_signed_request(hash, None))).await?;
        let response = responses.recv().await.unwrap();
        assert_eq!(response.unwrap_err().code(), tonic::Code::Unauthenticated);

        run_handle.await.expect("protocol panicked");
        Ok(())
    }

    #[tokio::test]
    async fn connect_username_protocol_rejects_a_malformed_token() -> anyhow::Result<()> {
        init_test_tracing();

        let hash = UsernameHash::new([1; 32]);
        // No fetch is expected: the token never makes it that far.
        let mock_protocol = protocol_with_username(hash);

        let (requests, mut responses, run_handle) = run_test_protocol(mock_protocol);
        requests
            .send(Ok(fetch_signed_request(hash, Some(vec![0xff; 3]))))
            .await?;
        let response = responses.recv().await.unwrap();
        assert_eq!(response.unwrap_err().code(), tonic::Code::InvalidArgument);

        run_handle.await.expect("protocol panicked");
        Ok(())
    }

    async fn setup(pool: &PgPool) -> anyhow::Result<AuthService> {
        Ok(AuthService::initialize(
            pool.clone(),
            "example.com".parse()?,
            Default::default(),
            0,
            CancellationToken::new(),
        )
        .await?)
    }

    async fn store_username(
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

    /// The fallback used to live in the protocol, where the mock covered it.
    #[sqlx::test]
    async fn fetch_connection_package_without_packages_is_not_found(
        pool: PgPool,
    ) -> anyhow::Result<()> {
        let service = setup(&pool).await?;
        let signing_key = UsernameSigningKey::generate()?;
        let hash = UsernameHash::new([1; 32]);
        store_username(&pool, hash, &signing_key).await?;

        let token = issue_token(&service, &pool, OperationType::ConnectUsername).await?;
        let result = service.fetch_connection_package(&hash, token).await;
        assert!(matches!(
            result,
            Err(ConnectProtocolError::NoConnectionPackage)
        ));
        Ok(())
    }
}
