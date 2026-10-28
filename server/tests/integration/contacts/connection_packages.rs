// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Signed connection packages and the task uploading them.

use airapiclient::ApiClient;
use aircommon::{identifiers::UsernameHash, messages::client_as::SerializedToken};
use aircoreclient::clients::CoreUser;
use airprotos::{
    auth_service::v1::OperationType,
    client::signed_connection_package::{AnyConnectionPackage, AnyConnectionPackageIn},
};
use airserver_test_harness::utils::setup::TestBackend;
use chrono::Utc;

/// The user's cached contact request tokens, so that a bare client can pay
/// for fetches. The user must not send contact requests afterwards.
async fn tokens_of(user: &CoreUser) -> Vec<SerializedToken> {
    user.cached_privacy_pass_tokens(OperationType::ConnectUsername)
        .await
        .unwrap()
        .into_iter()
        .map(SerializedToken::new)
        .collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn username_connect_serves_signed_package() {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;

    let record = setup.add_username(&bob).await;
    let mut tokens = tokens_of(&bob).await;

    // A new client asks for a signed package and gets one, carrying the
    // owner's features.
    let client = ApiClient::with_endpoint(&setup.server_url()).unwrap();
    let (package, _responder) = client
        .as_connect_username(record.hash, tokens.pop().unwrap())
        .await
        .unwrap();
    assert!(matches!(package, AnyConnectionPackageIn::Signed(_)));
    let package = package.verify(&record.hash).unwrap();
    let AnyConnectionPackage::Signed(package) = package else {
        panic!("expected signed connection package");
    };
    assert_eq!(package.username_hash(), &record.hash);
    assert!(package.air_features().pq_groups);

    setup.connect_users(&alice, &bob).await;
}

/// Fetches and verifies one connection package for the username, paying
/// with one of `tokens`.
async fn fetch_connection_package(
    client: &ApiClient,
    hash: UsernameHash,
    tokens: &mut Vec<SerializedToken>,
) -> anyhow::Result<AnyConnectionPackage> {
    let token = tokens.pop().expect("out of tokens");
    let (package, _responder) = client.as_connect_username(hash, token).await?;
    Ok(package.verify(&hash)?)
}

/// Consumes connection packages until the server hands out the last resort one.
async fn drain_connection_packages(
    client: &ApiClient,
    hash: UsernameHash,
    tokens: &mut Vec<SerializedToken>,
) -> anyhow::Result<()> {
    for _ in 0..100 {
        if fetch_connection_package(client, hash, tokens)
            .await?
            .is_last_resort()
        {
            return Ok(());
        }
    }
    anyhow::bail!("connection packages not drained after 100 fetches");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn upload_task_replenishes_packages() -> anyhow::Result<()> {
    let mut setup = TestBackend::single().await;
    let bob = setup.add_user().await;
    let record = setup.add_username(&bob).await;
    let client = ApiClient::with_endpoint(&setup.server_url())?;
    let mut tokens = tokens_of(&bob).await;

    // Use up the packages published on creation, so that new ones are observable.
    drain_connection_packages(&client, record.hash, &mut tokens).await?;
    assert!(
        fetch_connection_package(&client, record.hash, &mut tokens)
            .await?
            .is_last_resort()
    );

    // Run the one-shot task as if the username predated signed connection packages.
    bob.outbound_service()
        .schedule_signed_connection_package_upload(vec![record.hash], Utc::now())
        .await?;
    bob.outbound_service().run_once().await;

    let state = bob
        .outbound_service()
        .signed_connection_package_upload_state()
        .await?
        .expect("task should exist");
    assert!(state.parked, "task should be parked after success");
    assert!(state.pending_usernames.is_empty());

    let package = fetch_connection_package(&client, record.hash, &mut tokens).await?;
    assert!(matches!(package, AnyConnectionPackage::Signed(_)));
    assert!(
        !package.is_last_resort(),
        "the task should have published fresh packages"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn upload_task_retries_after_network_error() -> anyhow::Result<()> {
    let mut setup = TestBackend::single().await;
    let bob = setup.add_user().await;
    let record = setup.add_username(&bob).await;

    // The upload fails with a network error.
    setup.listener_control_handle().set_drop_all();
    bob.outbound_service()
        .schedule_signed_connection_package_upload(vec![record.hash], Utc::now())
        .await?;
    bob.outbound_service().run_once().await;
    setup.listener_control_handle().set_normal();

    let state = bob
        .outbound_service()
        .signed_connection_package_upload_state()
        .await?
        .expect("task should exist");
    assert!(!state.parked, "failed task must be retried");
    assert_eq!(state.pending_usernames, vec![record.hash]);

    // The retry succeeds.
    bob.outbound_service()
        .schedule_signed_connection_package_upload(state.pending_usernames, Utc::now())
        .await?;
    bob.outbound_service().run_once().await;

    let state = bob
        .outbound_service()
        .signed_connection_package_upload_state()
        .await?
        .expect("task should exist");
    assert!(state.parked);
    assert!(state.pending_usernames.is_empty());

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn upload_task_drops_unknown_username() -> anyhow::Result<()> {
    let mut setup = TestBackend::single().await;
    let bob = setup.add_user().await;

    // A username which does not exist locally is treated as done.
    bob.outbound_service()
        .schedule_signed_connection_package_upload(vec![UsernameHash::new([7; 32])], Utc::now())
        .await?;
    bob.outbound_service().run_once().await;

    let state = bob
        .outbound_service()
        .signed_connection_package_upload_state()
        .await?
        .expect("task should exist");
    assert!(state.parked);
    assert!(state.pending_usernames.is_empty());

    Ok(())
}
