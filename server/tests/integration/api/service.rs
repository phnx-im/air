// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! gRPC method aliases, health check and rejection of unsupported client
//! versions.

use std::assert_matches;

use airapiclient::{ApiClient, as_api::AsRequestError, qs_api::QsRequestError};
use airbackend::{settings::VersionExpiration, version::VersionPolicy};
use aircommon::{
    credentials::keys::UsernameSigningKey,
    crypto::signatures::keys::QsClientSigningKey,
    identifiers::{QsClientId, Username},
};
use airprotos::{
    auth_service::v1::auth_service_server,
    common::v1::{StatusDetails, StatusDetailsCode},
    delivery_service::v1::delivery_service_server,
    queue_service::v1::queue_service_server,
};
use airserver_test_harness::utils::setup::{TestBackend, TestBackendParams};
use chrono::DateTime;
use semver::Version;
use tonic::{Code, codegen::http, transport::Channel};
use tonic_health::pb::{
    HealthCheckRequest, health_check_response::ServingStatus, health_client::HealthClient,
};

/// Old method paths must not return UNIMPLEMENTED: proves the alias layer is wired up.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn old_grpc_paths_are_routed() {
    use tower::ServiceExt as _;

    const OLD_PATHS: &[&str] = &[
        "/auth_service.v1.AuthService/CheckHandleExists",
        "/auth_service.v1.AuthService/CreateHandle",
        "/auth_service.v1.AuthService/DeleteHandle",
        "/auth_service.v1.AuthService/RefreshHandle",
        "/auth_service.v1.AuthService/ConnectHandle",
        "/auth_service.v1.AuthService/ListenHandle",
    ];

    let setup = TestBackend::single().await;
    let channel = Channel::from_shared(String::from(setup.server_url()))
        .unwrap()
        .connect()
        .await
        .unwrap();

    for path in OLD_PATHS {
        let request = http::Request::builder()
            .method(http::Method::POST)
            .uri(*path)
            .header("content-type", "application/grpc")
            .header("te", "trailers")
            .body(tonic::body::Body::empty())
            .unwrap();

        let response = channel.clone().oneshot(request).await.unwrap();

        // Expect either a code != UNIMPLEMENTED (non-streaming RPC) or an abstent grpc-status which
        // is a 200 HTTP/2 request (streaming RPC). Both cases mean that the redirect worked.
        let code = response
            .headers()
            .get("grpc-status")
            .map(|v| Code::from_bytes(v.as_bytes()));
        match code {
            Some(code) => {
                // Non-streaming RPC: returns grpc-status as header
                assert_ne!(code, Code::Unknown, "grpc status failed to parse");
                assert_ne!(
                    code,
                    Code::Unimplemented,
                    "path {path} is unknown to the server",
                );
            }
            None => {
                // Streaming RPC: returns HTTP/2 200 OK and no grpc-status header
                assert!(
                    response.status().is_success(),
                    "path {path} is unknown to the server"
                );
            }
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn health_check_reports_services_serving() {
    let setup = TestBackend::single().await;
    let channel = Channel::from_shared(setup.server_url().to_string())
        .unwrap()
        .connect()
        .await
        .unwrap();
    let mut client = HealthClient::new(channel);

    let names = [
        auth_service_server::SERVICE_NAME,
        delivery_service_server::SERVICE_NAME,
        queue_service_server::SERVICE_NAME,
    ];

    for name in names {
        let response = client
            .check(HealthCheckRequest {
                service: name.to_string(),
            })
            .await;
        if let Err(error) = response {
            panic!("Health check failed for service {name}: {error}");
        }
        let response = response.unwrap().into_inner();
        assert_eq!(
            ServingStatus::try_from(response.status).unwrap(),
            ServingStatus::Serving
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn expired_client_version_is_rejected() {
    let setup = TestBackend::single_with_params(TestBackendParams {
        version_policy: VersionPolicy::new(vec![VersionExpiration {
            older_than: Version::new(999, 0, 0),
            expires_on: DateTime::UNIX_EPOCH,
        }]),
        ..Default::default()
    })
    .await;

    let client = ApiClient::with_endpoint(&setup.server_url()).unwrap();

    let username = Username::new("test-username".to_string()).unwrap();
    let signing_key = UsernameSigningKey::generate().unwrap();
    let hash = username.calculate_hash().unwrap();

    let res = client.as_listen_username(hash, &signing_key).await;
    let status = match res {
        Err(AsRequestError::Tonic(status)) => status,
        Err(error) => panic!("Unexpected error type: {error:?}"),
        Ok(_) => panic!("Expected error"),
    };
    assert_eq!(status.code(), tonic::Code::FailedPrecondition);

    let details = StatusDetails::from_status(&status).unwrap();
    assert_matches!(details.code(), StatusDetailsCode::VersionUnsupported);

    let client_id = QsClientId::random(&mut rand::rng());
    let signing_key = QsClientSigningKey::generate().unwrap();
    // Signing key does not have to be valid, because the client version is checked first.
    let res = client.qs_listen_queue(client_id, 0, &signing_key).await;
    match res {
        Err(QsRequestError::Tonic(status)) => status,
        Err(error) => panic!("Unexpected error type: {error:?}"),
        Ok(_) => panic!("Expected error"),
    };
    assert_eq!(status.code(), tonic::Code::FailedPrecondition);

    let details = StatusDetails::from_status(&status).unwrap();
    assert_matches!(details.code(), StatusDetailsCode::VersionUnsupported);
}
