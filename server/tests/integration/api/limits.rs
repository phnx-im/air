// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Package upload limits over the real gRPC stack.

use aircommon::limits::{
    MAX_CONNECTION_PACKAGE_BYTES, MAX_CONNECTION_PACKAGES_PER_REQUEST, MAX_KEY_PACKAGE_BYTES,
    MAX_KEY_PACKAGES_PER_REQUEST,
};
use airprotos::{
    auth_service::v1::{
        PublishConnectionPackagesPayload, PublishConnectionPackagesRequest,
        SignedConnectionPackage, UsernameHash, auth_service_client::AuthServiceClient,
    },
    queue_service::v1::{
        ApqKeyPackage, KeyPackage, PublishApqKeyPackagesPayload, PublishApqKeyPackagesRequest,
        PublishKeyPackagesPayload, PublishKeyPackagesRequest, StageKeyPackagesPayload,
        StageKeyPackagesRequest, queue_service_client::QueueServiceClient,
    },
};
use airserver_test_harness::utils::setup::TestBackend;
use tonic::{Code, Status, transport::Channel};

fn garbage_key_packages(count: usize) -> Vec<KeyPackage> {
    (0..count).map(|_| KeyPackage { tls: vec![0; 8] }).collect()
}

fn assert_invalid_argument(status: Status, message: &str) {
    assert_eq!(status.code(), Code::InvalidArgument, "{status:?}");
    assert_eq!(status.message(), message);
}

// The packages are garbage and the user does not exist, so a limit message
// proves that the check ran before any lookup or parsing.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn oversized_key_package_uploads_are_rejected() {
    let setup = TestBackend::single().await;
    let channel = Channel::from_shared(String::from(setup.server_url()))
        .unwrap()
        .connect()
        .await
        .unwrap();
    let mut qs = QueueServiceClient::new(channel);

    let status = qs
        .publish_key_packages(PublishKeyPackagesRequest {
            payload: Some(PublishKeyPackagesPayload {
                client_metadata: None,
                client_id: None,
                key_packages: garbage_key_packages(MAX_KEY_PACKAGES_PER_REQUEST + 1),
            }),
            signature: None,
        })
        .await
        .unwrap_err();
    assert_invalid_argument(status, "too many key packages");

    let status = qs
        .stage_key_packages(StageKeyPackagesRequest {
            payload: Some(StageKeyPackagesPayload {
                client_metadata: None,
                client_id: None,
                epoch_id: vec![0; 32],
                leaf_index: 0,
                generation: 0,
                key_packages: vec![KeyPackage {
                    tls: vec![0; MAX_KEY_PACKAGE_BYTES + 1],
                }],
                apq_key_packages: Vec::new(),
            }),
            signature: None,
        })
        .await
        .unwrap_err();
    assert_invalid_argument(status, "key package too large");

    let status = qs
        .publish_apq_key_packages(PublishApqKeyPackagesRequest {
            payload: Some(PublishApqKeyPackagesPayload {
                client_metadata: None,
                client_id: None,
                apq_key_packages: vec![
                    ApqKeyPackage {
                        t_key_package_tls: vec![0; 8],
                        pq_key_package_tls: vec![0; 8],
                    };
                    MAX_KEY_PACKAGES_PER_REQUEST + 1
                ],
            }),
            signature: None,
        })
        .await
        .unwrap_err();
    assert_invalid_argument(status, "too many key packages");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn oversized_connection_package_uploads_are_rejected() {
    let setup = TestBackend::single().await;
    let channel = Channel::from_shared(String::from(setup.server_url()))
        .unwrap()
        .connect()
        .await
        .unwrap();
    let mut auth = AuthServiceClient::new(channel);

    let status = auth
        .publish_connection_packages(PublishConnectionPackagesRequest {
            payload: Some(PublishConnectionPackagesPayload {
                client_metadata: None,
                hash: Some(UsernameHash { bytes: vec![1; 32] }),
                connection_packages: Vec::new(),
                signed_connection_packages: vec![
                    SignedConnectionPackage {
                        payload: vec![0; MAX_CONNECTION_PACKAGE_BYTES + 1],
                        signature: None,
                    };
                    MAX_CONNECTION_PACKAGES_PER_REQUEST + 1
                ],
            }),
            signature: None,
        })
        .await
        .unwrap_err();
    assert_invalid_argument(status, "too many connection packages");
}
