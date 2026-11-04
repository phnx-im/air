// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Registering a username while requests to the AS fail.

use aircommon::identifiers::Username;
use aircoreclient::{UsernameRecord, clients::CoreUser};
use airserver_test_harness::utils::{core_user_ext::CoreUserExt, setup::TestBackend};

use crate::contacts::requests::prepare_username;

async fn assert_connectable(
    alice: &CoreUser,
    bob: &CoreUser,
    record: &UsernameRecord,
    apq_groups: bool,
) {
    alice
        .add_contact(record.username.clone(), record.hash, apq_groups)
        .await
        .unwrap()
        .unwrap();
    bob.receive_connection_offer(record).await;
}

/// A username of `bob` that alice registers after the create request of bob
/// was dropped.
async fn username_taken_after_dropped_create(
    setup: &TestBackend,
    alice: &CoreUser,
    bob: &CoreUser,
) -> Username {
    let username = prepare_username(bob).await;
    alice.outbound_service().run_once().await;

    setup
        .listener_control_handle()
        .set_drop_next_request_to("CreateUsername");
    bob.add_username(username.clone()).await.unwrap_err();
    alice
        .add_username(username.clone())
        .await
        .unwrap()
        .expect("the username is free");
    username
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn lost_create_response_keeps_username() {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    let username = prepare_username(&bob).await;

    // The AS registers the username, but bob never sees the response.
    setup
        .listener_control_handle()
        .set_drop_next_response_to("CreateUsername");
    bob.add_username(username.clone()).await.unwrap_err();

    let record = bob
        .add_username(username)
        .await
        .unwrap()
        .expect("the username was lost");
    assert_connectable(&alice, &bob, &record, setup.apq_groups).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn failed_publish_keeps_username() {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    let username = prepare_username(&bob).await;

    setup
        .listener_control_handle()
        .set_drop_next_request_to("PublishConnectionPackages");
    let record = bob
        .add_username(username.clone())
        .await
        .unwrap()
        .expect("the username was lost");
    assert_eq!(bob.usernames().await.unwrap(), [username]);

    bob.outbound_service().run_once().await;
    assert_connectable(&alice, &bob, &record, setup.apq_groups).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn removed_unpublished_username_registers_under_a_new_key() {
    let mut setup = TestBackend::single().await;
    let bob = setup.add_user().await;
    let username = prepare_username(&bob).await;

    setup
        .listener_control_handle()
        .set_drop_next_request_to("PublishConnectionPackages");
    let removed = bob
        .add_username(username.clone())
        .await
        .unwrap()
        .expect("the username was lost");
    bob.remove_username(&username).await.unwrap();

    let record = bob
        .add_username(username)
        .await
        .unwrap()
        .expect("the username was lost");
    assert_ne!(
        record.signing_key.verifying_key(),
        removed.signing_key.verifying_key()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn lost_initial_publish_response_keeps_package_keys() {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    let username = prepare_username(&bob).await;

    // The AS stores the packages, but bob never sees the response.
    setup
        .listener_control_handle()
        .set_drop_next_response_to("PublishConnectionPackages");
    let record = bob
        .add_username(username)
        .await
        .unwrap()
        .expect("the username was lost");

    assert_connectable(&alice, &bob, &record, setup.apq_groups).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn refused_publish_drops_package_keys() {
    let mut setup = TestBackend::single().await;
    let bob = setup.add_user().await;
    let username = prepare_username(&bob).await;

    setup
        .listener_control_handle()
        .set_reject_next_request_to("PublishConnectionPackages");
    bob.add_username(username.clone())
        .await
        .unwrap()
        .expect("the username was lost");
    let hashes = bob.connection_package_hashes(&username).await.unwrap();
    assert!(hashes.is_empty(), "{} keys kept", hashes.len());

    setup
        .listener_control_handle()
        .set_reject_next_request_to("PublishConnectionPackages");
    bob.outbound_service().run_once().await;
    let hashes = bob.connection_package_hashes(&username).await.unwrap();
    assert!(hashes.is_empty(), "{} keys kept", hashes.len());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn retry_finds_username_taken_meanwhile() {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    let username = username_taken_after_dropped_create(&setup, &alice, &bob).await;

    assert!(bob.add_username(username).await.unwrap().is_none());
    assert!(bob.usernames().await.unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn unpublished_username_taken_meanwhile_is_removed() {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    let username = username_taken_after_dropped_create(&setup, &alice, &bob).await;

    // Without the publication, bob cannot tell that alice holds the username.
    setup
        .listener_control_handle()
        .set_drop_next_request_to("PublishConnectionPackages");
    bob.add_username(username).await.unwrap();

    bob.outbound_service().run_once().await;
    assert!(bob.usernames().await.unwrap().is_empty());
}
