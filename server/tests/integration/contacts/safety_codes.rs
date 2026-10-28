// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Safety codes between connected users.

use std::collections::HashMap;

use airserver_test_harness::utils::{
    core_user_ext::CoreUserExt,
    setup::{TestBackend, TestUser},
};

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn safety_codes_match_across_users() {
    let mut setup = TestBackend::single().await;
    let [alice, bob, charlie] = setup.add_users().await;

    setup.connected_group(&alice, &[&bob, &charlie]).await;

    // Have everyone compute everyones safety code and verify that they match
    let users = [&alice, &bob, &charlie];
    let mut codes = HashMap::new();
    for computing_user in &users {
        for user in users {
            let user_code = computing_user.safety_code(user.user_id()).await.unwrap();

            // If this is the first time we see a code for this user, store it
            // and continue.
            let Some(expected_code) = codes.get(user.user_id()) else {
                codes.insert(user.user_id().clone(), user_code);
                continue;
            };

            assert_eq!(
                &user_code,
                expected_code,
                "Safety code for {:?} computed by {:?} does not match",
                user.user_id(),
                computing_user.user_id()
            );
            let expected_code_chunks = expected_code.to_chunks();
            let user_code_chunks = user_code.to_chunks();
            for chunk in &user_code_chunks {
                assert!(
                    *chunk < 100_000,
                    "Safety code chunk should be less than 100,000"
                );
            }
            assert_eq!(
                expected_code_chunks,
                user_code_chunks,
                "Safety code chunks for {:?} computed by {:?} do not match",
                user.user_id(),
                computing_user.user_id()
            );
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[ignore = "the safety code uses a stale credential of a re-registered user"]
async fn safety_code_follows_reregistered_user() {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    setup.connect_users(&alice, &bob).await;
    let alice_id = alice.user_id().clone();
    let old_code = bob.safety_code(&alice_id).await.unwrap();

    alice.outbound_service().run_once().await;
    alice.drain().await;
    alice.outbound_service().start_and_wait().await;
    alice.delete_account().await.unwrap();
    bob.drain_unchecked().await;

    // The user id registers again with a new credential and connects to bob.
    let new_alice = TestUser::try_new(&alice_id, setup.server_url(), Some("DUMMY007"))
        .await
        .unwrap();
    let record = new_alice.add_username().await;
    let new_alice = new_alice.user();
    bob.add_contact(record.username.clone(), record.hash, setup.apq_groups)
        .await
        .unwrap()
        .unwrap();
    let chat_id = new_alice.receive_connection_offer(&record).await;
    new_alice
        .accept_contact_request(chat_id)
        .await
        .unwrap()
        .unwrap();
    bob.drain().await;

    let own_code = new_alice.safety_code(&alice_id).await.unwrap();
    assert_ne!(own_code, old_code);
    assert_eq!(bob.safety_code(&alice_id).await.unwrap(), own_code);
}
