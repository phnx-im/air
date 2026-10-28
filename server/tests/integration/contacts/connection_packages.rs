// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Signed connection packages.

use airapiclient::ApiClient;
use aircommon::messages::client_as::SerializedToken;
use aircoreclient::clients::CoreUser;
use airprotos::auth_service::v1::OperationType;
use airserver_test_harness::utils::setup::TestBackend;

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

    // The package carries the owner's features.
    let client = ApiClient::with_endpoint(&setup.server_url()).unwrap();
    let (package, _responder) = client
        .as_connect_username(record.hash, tokens.pop().unwrap())
        .await
        .unwrap();
    let package = package.verify_for_username(&record.hash).unwrap();
    assert_eq!(package.username_hash(), &record.hash);
    assert!(package.air_features().pq_groups);

    setup.connect_users(&alice, &bob).await;
}
