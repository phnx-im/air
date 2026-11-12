// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Group creation without a stream: the creator credential must be the one the
//! AS registered.

use airapiclient::{ApiClient, ds_api::DsRequestError};
use aircommon::{credentials::UserCredential, identifiers::UserId};
use aircoreclient::clients::CoreUser;
use airserver_test_harness::utils::setup::TestBackend;
use tonic::Code;

/// Creates a self-group of `creator` without a stream. The out-of-band
/// creator credential names `claimed_user_id` but keeps the creator's key.
async fn create_self_group_claiming(
    setup: &TestBackend,
    creator: &CoreUser,
    claimed_user_id: &UserId,
) -> Result<(), DsRequestError> {
    let api_client = ApiClient::with_endpoint(&setup.server_url()).unwrap();
    let (group_id, pq_group_id, _provisioning) =
        api_client.ds_request_group_id(None, true).await.unwrap();
    let (mut params, ear_key) = creator
        .self_group_creation_params_for_test(group_id, pq_group_id.unwrap())
        .await
        .unwrap();
    let (mut payload, signature) = creator
        .signing_key_for_test()
        .credential()
        .clone()
        .into_parts();
    payload.csr.user_id = claimed_user_id.clone();
    params.creator_user_credential = Some(UserCredential::new(payload, signature));
    api_client
        .ds_create_group(params, creator.signing_key_for_test(), &ear_key)
        .await
}

#[track_caller]
fn assert_permission_denied(result: Result<(), DsRequestError>, message: &str) {
    match result {
        Err(DsRequestError::Tonic(status)) => {
            assert_eq!(status.code(), Code::PermissionDenied, "{status:?}");
            assert!(status.message().contains(message), "{status:?}");
        }
        result => panic!("expected permission denied, got {result:?}"),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn self_group_with_the_registered_credential_is_created() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;

    create_self_group_claiming(&setup, &alice, alice.user_id())
        .await
        .unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn self_group_claiming_another_user_is_rejected() {
    let mut setup = TestBackend::single().await;
    let [mallory, alice] = setup.add_users().await;

    let result = create_self_group_claiming(&setup, &mallory, alice.user_id()).await;
    assert_permission_denied(result, "does not match the registered user");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn self_group_claiming_an_unknown_user_is_rejected() {
    let mut setup = TestBackend::single().await;
    let mallory = setup.add_user().await;
    let unknown = UserId::random(mallory.user_id().domain().clone());

    let result = create_self_group_claiming(&setup, &mallory, &unknown).await;
    assert_permission_denied(result, "unknown user");
}
