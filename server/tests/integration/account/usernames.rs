// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Username validation and existence checks.

use std::assert_matches;

use aircommon::identifiers::Username;
use aircoreclient::AddUsernameContactError;
use airserver_test_harness::utils::setup::TestBackend;
use rand::RngExt;

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn username_sanity_checks() {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;

    let username_record = setup.add_username(&bob).await;
    let bob_username = username_record.username.clone();
    let bob_username_hash = bob_username.calculate_hash().unwrap();

    let apq_groups = setup.apq_groups;
    let username_record = setup.add_username(&alice).await;
    let alice_username = username_record.username.clone();
    let alice_username_hash = alice_username.calculate_hash().unwrap();
    let res = alice
        .add_contact(alice_username.clone(), alice_username_hash, apq_groups)
        .await
        .unwrap();
    assert_matches!(
        res,
        Err(AddUsernameContactError::OwnUsername),
        "Should not be able to add own username as contact"
    );

    // Try to add Bob twice
    let res = alice
        .add_contact(bob_username.clone(), bob_username_hash, apq_groups)
        .await
        .unwrap();
    assert_matches!(res, Ok(_), "Should be able to add Bob as contact");
    let res = alice
        .add_contact(bob_username.clone(), bob_username_hash, apq_groups)
        .await
        .unwrap();
    assert_matches!(
        res,
        Err(AddUsernameContactError::DuplicateRequest),
        "Should not be able to add Bob twice"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn check_username_exists() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;

    let random_number = rand::rng().random_range(100_000..1_000_000);
    let alice_username = Username::new(format!("alice-{}", random_number)).unwrap();

    let hash = alice
        .check_username_exists(alice_username.clone())
        .await
        .unwrap();
    assert!(hash.is_none(), "Alice's username should not exist yet");

    alice.add_username(alice_username.clone()).await.unwrap();

    let hash = alice
        .check_username_exists(alice_username.clone())
        .await
        .unwrap();
    assert!(hash.is_some(), "Alice's username should exist");

    alice.remove_username(&alice_username).await.unwrap();
    let hash = alice
        .check_username_exists(alice_username.clone())
        .await
        .unwrap();
    assert!(
        hash.is_none(),
        "Alice's username should not exist after removal"
    );
}
