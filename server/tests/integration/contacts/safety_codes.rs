// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Safety codes between connected users.

use std::collections::HashMap;

use airserver_test_harness::utils::setup::TestBackend;

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn safety_codes() {
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
