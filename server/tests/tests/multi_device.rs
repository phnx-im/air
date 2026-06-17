// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use airapiclient::ApiClient;
use aircoreclient::clients::{CoreUser, multi_device::MultiDeviceProvisionStep};
use airprotos::relay_service::v1::LinkingSessionId;
use airserver_test_harness::utils::setup::TestBackend;

/// Receives the session ID from the first provisioning step. The receiver must
/// stay alive afterwards: the new device later sends a `Linking` step, and
/// dropping the receiver would make that send fail and abort provisioning.
async fn recv_session_id(
    rx: &mut tokio::sync::mpsc::Receiver<MultiDeviceProvisionStep>,
) -> LinkingSessionId {
    match rx
        .recv()
        .await
        .expect("provision channel closed before session id")
    {
        MultiDeviceProvisionStep::SessionId(session_id) => session_id,
        MultiDeviceProvisionStep::Linking => panic!("unexpected Linking step before session id"),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[tracing::instrument(name = "Test multi-device linking session", skip_all)]
async fn multi_device_linking_session() {
    let mut setup = TestBackend::single().await;
    let server_url = setup.server_url();
    let alice = setup.add_user().await;

    let (session_tx, mut session_rx) = tokio::sync::mpsc::channel(1);

    let new_device_task = tokio::spawn(async move {
        let api_client = ApiClient::with_endpoint(&server_url).unwrap();
        let old_device_message =
            CoreUser::multi_device_provision_client_with_api(&api_client, session_tx)
                .await
                .unwrap();

        assert_eq!(old_device_message, "pong!");
    });

    let session_id = recv_session_id(&mut session_rx).await;

    // The old device scans/types the session ID.
    let new_device_message = setup
        .get_user(&alice)
        .user()
        .multi_device_link_client(session_id)
        .await
        .unwrap();

    assert_eq!(new_device_message, "ping!");

    new_device_task.await.unwrap();
}

// Linking with a session ID that was never registered returns an error.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[tracing::instrument(name = "Test link with nonexistent session ID", skip_all)]
async fn multi_device_link_with_nonexistent_session_id() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;

    let fake_digest =
        hex::decode("68924f1f6f60d5fdb8463881a5945e58c3f1402c65681b1270f5aeccbed17bd1")
            .unwrap()
            .try_into()
            .unwrap();
    let fake_session_id = LinkingSessionId::from_digest(&fake_digest, 8).unwrap();
    let result = setup
        .get_user(&alice)
        .user()
        .multi_device_link_client(fake_session_id)
        .await;

    assert!(result.is_err());
}

// A session can only be claimed once; a second link attempt on the same session ID
// must fail even when called by the same user.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[tracing::instrument(name = "Test second link attempt returns error", skip_all)]
async fn multi_device_second_link_attempt_returns_error() {
    let mut setup = TestBackend::single().await;
    let server_url = setup.server_url();
    let alice = setup.add_user().await;

    let (session_tx, mut session_rx) = tokio::sync::mpsc::channel(1);

    let new_device_task = tokio::spawn(async move {
        let api_client = ApiClient::with_endpoint(&server_url).unwrap();
        CoreUser::multi_device_provision_client_with_api(&api_client, session_tx)
            .await
            .unwrap()
    });

    let session_id = recv_session_id(&mut session_rx).await;

    let result = setup
        .get_user(&alice)
        .user()
        .multi_device_link_client(session_id.clone())
        .await
        .unwrap();
    assert_eq!(result, "ping!");

    new_device_task.await.unwrap();

    // Session was already consumed — a second attempt must fail.
    let second_result = setup
        .get_user(&alice)
        .user()
        .multi_device_link_client(session_id)
        .await;

    assert!(second_result.is_err());
}

// Two concurrent linking sessions must not interfere with each other.
// Each new device must be linked to the correct existing device.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[tracing::instrument(name = "Test concurrent linking sessions don't interfere", skip_all)]
async fn multi_device_concurrent_linking_sessions_dont_interfere() {
    let mut setup = TestBackend::single().await;
    let server_url = setup.server_url();
    let alice = setup.add_user().await;
    let bob = setup.add_user().await;

    let (alice_session_tx, mut alice_session_rx) = tokio::sync::mpsc::channel(1);
    let (bob_session_tx, mut bob_session_rx) = tokio::sync::mpsc::channel(1);

    let alice_server_url = server_url.clone();
    let alice_new_device = tokio::spawn(async move {
        let api_client = ApiClient::with_endpoint(&alice_server_url).unwrap();
        CoreUser::multi_device_provision_client_with_api(&api_client, alice_session_tx)
            .await
            .unwrap()
    });

    let bob_new_device = tokio::spawn(async move {
        let api_client = ApiClient::with_endpoint(&server_url).unwrap();
        CoreUser::multi_device_provision_client_with_api(&api_client, bob_session_tx)
            .await
            .unwrap()
    });

    let alice_session_id = recv_session_id(&mut alice_session_rx).await;
    let bob_session_id = recv_session_id(&mut bob_session_rx).await;

    // Session IDs derived from different key packages must be distinct.
    assert_ne!(alice_session_id, bob_session_id);

    let alice_result = setup
        .get_user(&alice)
        .user()
        .multi_device_link_client(alice_session_id)
        .await
        .unwrap();
    assert_eq!(alice_result, "ping!");

    let bob_result = setup
        .get_user(&bob)
        .user()
        .multi_device_link_client(bob_session_id)
        .await
        .unwrap();
    assert_eq!(bob_result, "ping!");

    assert_eq!(alice_new_device.await.unwrap(), "pong!");
    assert_eq!(bob_new_device.await.unwrap(), "pong!");
}
