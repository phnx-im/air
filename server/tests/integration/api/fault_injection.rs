// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! The harness's fault injection aimed at a single gRPC method.

use aircommon::identifiers::Username;
use airserver_test_harness::utils::setup::TestBackend;

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn targeted_fault_hits_only_that_method() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let username = Username::new("non-existent".to_owned()).unwrap();

    setup
        .listener_control_handle()
        .set_reject_next_request_to("CheckUsernameExists");

    // Other calls pass and leave the fault armed.
    alice.qs_fetch_messages().await.unwrap();

    alice
        .check_username_exists(username.clone())
        .await
        .unwrap_err();
    assert_eq!(alice.check_username_exists(username).await.unwrap(), None);
}
