// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Requesting invitation codes and marking them as copied.

use airserver_test_harness::utils::setup::TestBackend;

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn requested_code_is_stored_and_consumes_token() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;

    // No codes in the local store yet.
    let codes_before = alice.load_invitation_codes().await.unwrap();
    assert!(codes_before.is_empty());

    let token_ids = alice.load_invitation_token_ids().await.unwrap();
    assert!(
        !token_ids.is_empty(),
        "should have tokens after replenishment"
    );
    let token_count_before = token_ids.len();
    let token_id = token_ids.into_iter().next().unwrap();

    // Request the code
    let code = alice
        .request_invitation_code(token_id)
        .await
        .unwrap()
        .unwrap();

    assert!(!code.code.is_empty(), "invitation code should be non-empty");
    assert!(
        !code.copied,
        "newly issued code should not be marked copied"
    );

    // Code is persisted in the local store
    let codes_after = alice.load_invitation_codes().await.unwrap();
    assert_eq!(codes_after.len(), 1);
    assert_eq!(codes_after[0].code, code.code);
    assert!(!codes_after[0].copied);

    // The consumed token is gone
    let token_ids_after = alice.load_invitation_token_ids().await.unwrap();
    assert_eq!(
        token_ids_after.len(),
        token_count_before - 1,
        "one token should have been consumed"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn marked_code_is_stored_as_copied() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;

    let token_id = alice
        .load_invitation_token_ids()
        .await
        .unwrap()
        .into_iter()
        .next()
        .unwrap();

    let code = alice
        .request_invitation_code(token_id)
        .await
        .unwrap()
        .unwrap();

    alice
        .mark_invitation_code_as_copied(&code.code)
        .await
        .unwrap();

    let stored = alice.load_invitation_codes().await.unwrap();
    assert_eq!(stored.len(), 1);
    assert!(stored[0].copied, "code should be marked as copied");
}
