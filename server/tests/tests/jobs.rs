// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

// A PCO is created when executing a CO. The PCO remains after the CO execution
// only if the CO could not be completed successfully due to a network error or
// a wrong epoch error. Wrong epoch errors occur if another participant has also
// created a commit and the server has processed their commit first. Network
// errors can be simulated in a test by calling `set_drop_next_response` on the
// test setup's listener control handle (see groups.rs for an example of this).
// A client can be made to fetch and process messages from the queue by calling
// `qs_fetch_messages` followed by `fully_process_qs_messages` on the resulting
// messages. See the other tests for examples. A client will retry PCOs when
// running the outbound service, via `run_once`. Again, see the other tests for
// examples.

// Things to test
// - When executing a ChatOperation (CO) and there is a PendingChatOperation
//   (PCO) for the same chat, the PCO is executed first and then the CO.
// - When executing a CO and there is a PCO for the same chat, but the PCO
//   execution fails, the CO also fails and the PCO is not deleted.
// - When a PCO exists for a chat in state "waiting for queue response" and
//   we're getting a matching queue response, the pending commit should be
//   merged.
// - When a PCO exists for a chat in state "waiting for queue response" and
//   we're getting another commit for the same group, the following should
//   happen:
//   - If it's a leave operation, it should be deleted iff the incoming commit
//     covers that leave operation
//  - If it's not a leave operation, the incoming commit should be applied and
//    the existing pending commit should be discarded and the PCO should be
//    deleted.
// - When executing a PCO either as part of executing a CO or as part of the
//   retry mechanism, the following should happen:
//   - If the PCO is in the state "waiting for queue response", execution should
//     fail immediately.
//   - If the epoch is wrong because another participant has already committed
//     in the meantime, the PCO should be put into status "waiting for queue
//     response".
//   - If it's a network error, or this is a retry after an earlier network
//     error, check if the maximum retry count (5) was reached if it was, delete
//     the PCO.
//   - If the PCO is a leave operation, it should take immediate local effect
//     regardless of any "wrong epoch" or network errors. If there was such an
//     error, it should be retried, though.
//   - If the PCO was successfully executed, it should be deleted.

use std::time::Duration;

use aircommon::identifiers::UserId;
use aircoreclient::{ChatId, ChatStatus, store::Store};
use airserver_test_harness::utils::setup::TestBackend;
use tokio::time::sleep;

async fn setup_group_with_contacts() -> (TestBackend, UserId, UserId, UserId, ChatId) {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let bob = setup.add_user().await;
    let charlie = setup.add_user().await;

    setup.connect_users(&alice, &bob).await;
    setup.connect_users(&alice, &charlie).await;

    let chat_id = setup.create_group(&alice).await;
    setup.invite_to_group(chat_id, &alice, vec![&bob]).await;

    (setup, alice, bob, charlie, chat_id)
}

async fn setup_group_with_charlie_member() -> (TestBackend, UserId, UserId, UserId, ChatId) {
    let (mut setup, alice, bob, charlie, chat_id) = setup_group_with_contacts().await;
    setup.invite_to_group(chat_id, &alice, vec![&charlie]).await;
    (setup, alice, bob, charlie, chat_id)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn pending_chat_operation_is_executed_before_chat_operation() {
    let (setup, alice, bob, charlie, chat_id) = setup_group_with_charlie_member().await;
    let alice_user = &setup.get_user(&alice).user;

    setup.listener_control_handle().set_drop_next_request();
    let _ = alice_user
        .remove_users(chat_id, vec![charlie.clone()])
        .await
        .expect_err("expected remove to fail due to network error");

    // Re-inviting Charlie should succeed only if the pending remove is executed first.
    alice_user
        .invite_users(chat_id, std::slice::from_ref(&charlie))
        .await
        .expect("invite should succeed after pending remove is executed");

    let members = alice_user.chat_participants(chat_id).await.unwrap();
    assert!(members.contains(&alice));
    assert!(members.contains(&bob));
    assert!(members.contains(&charlie));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn chat_operation_fails_if_pending_operation_fails() {
    let (setup, alice, _bob, charlie, chat_id) = setup_group_with_charlie_member().await;
    let alice_user = &setup.get_user(&alice).user;

    setup.listener_control_handle().set_drop_next_response();
    let _ = alice_user
        .remove_users(chat_id, vec![charlie.clone()])
        .await
        .expect_err("expected remove to fail due to network error");

    let pending = alice_user
        .pending_chat_operation_info(chat_id)
        .await
        .unwrap()
        .expect("pending operation should exist");
    assert_eq!(pending.operation_type, "other");
    assert_eq!(pending.request_status, "ready_to_retry");

    setup.listener_control_handle().set_drop_next_response();
    let _ = alice_user
        .update_key(chat_id)
        .await
        .expect_err("expected update to fail because pending operation failed");

    let pending_after = alice_user
        .pending_chat_operation_info(chat_id)
        .await
        .unwrap()
        .expect("pending operation should still exist");
    assert_eq!(pending_after.operation_type, "other");
    assert_eq!(pending_after.request_status, "ready_to_retry");
    assert!(pending_after.number_of_attempts >= pending.number_of_attempts);

    let members = alice_user.chat_participants(chat_id).await.unwrap();
    assert!(members.contains(&charlie));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn commit_response_merges_pending_commit() {
    let (setup, alice, _bob, charlie, chat_id) = setup_group_with_charlie_member().await;
    let alice_user = &setup.get_user(&alice).user;

    setup.listener_control_handle().set_drop_next_response();
    let _ = alice_user
        .remove_users(chat_id, vec![charlie.clone()])
        .await
        .expect_err("expected remove to fail due to network error");

    let qs_messages = alice_user.qs_fetch_messages().await.unwrap();
    let result = alice_user.fully_process_qs_messages(qs_messages).await;
    assert!(
        result.errors.is_empty(),
        "processing commit response should succeed"
    );

    let members = alice_user.chat_participants(chat_id).await.unwrap();
    assert!(!members.contains(&charlie));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn wrong_epoch_marks_pending_waiting_and_commit_clears_it() {
    let (setup, alice, bob, charlie, chat_id) = setup_group_with_charlie_member().await;
    let alice_user = &setup.get_user(&alice).user;
    let bob_user = &setup.get_user(&bob).user;

    setup.listener_control_handle().set_drop_next_request();
    let _ = alice_user
        .remove_users(chat_id, vec![charlie.clone()])
        .await
        .expect_err("expected remove to fail due to network error");

    // Bob commits first so Alice's pending commit is out of date.
    bob_user.update_key(chat_id).await.unwrap();

    let _ = alice_user
        .update_key(chat_id)
        .await
        .expect_err("expected wrong epoch when retrying pending commit");

    let pending = alice_user
        .pending_chat_operation_info(chat_id)
        .await
        .unwrap()
        .expect("pending operation should exist");
    assert_eq!(pending.request_status, "waiting_for_queue_response");

    let _ = alice_user
        .update_key(chat_id)
        .await
        .expect_err("pending operation waiting for queue response should fail immediately");

    let qs_messages = alice_user.qs_fetch_messages().await.unwrap();
    let result = alice_user.fully_process_qs_messages(qs_messages).await;
    assert!(
        result.errors.is_empty(),
        "processing incoming commit should succeed"
    );

    let members = alice_user.chat_participants(chat_id).await.unwrap();
    assert!(members.contains(&charlie));

    let pending_after = alice_user
        .pending_chat_operation_info(chat_id)
        .await
        .unwrap();
    assert!(
        pending_after.is_none(),
        "pending operation should be deleted"
    );

    alice_user.update_key(chat_id).await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn network_errors_eventually_delete_pending_operation() {
    let (setup, alice, bob, _charlie, chat_id) = setup_group_with_contacts().await;
    let alice_user = &setup.get_user(&alice).user;

    setup.listener_control_handle().set_drop_next_response();
    let _ = alice_user
        .update_key(chat_id)
        .await
        .expect_err("expected update to fail due to network error");

    let mut pending = alice_user
        .pending_chat_operation_info(chat_id)
        .await
        .unwrap()
        .expect("pending operation should exist");
    assert_eq!(pending.operation_type, "other");

    while pending.number_of_attempts < 5 {
        setup.listener_control_handle().set_drop_next_response();
        let _ = alice_user
            .update_key(chat_id)
            .await
            .expect_err("expected retry to fail due to network error");

        match alice_user
            .pending_chat_operation_info(chat_id)
            .await
            .unwrap()
        {
            Some(info) => pending = info,
            None => break,
        }
    }

    let pending_after = alice_user
        .pending_chat_operation_info(chat_id)
        .await
        .unwrap();
    assert!(
        pending_after.is_none(),
        "pending operation should be deleted"
    );

    let members = alice_user.chat_participants(chat_id).await.unwrap();
    assert!(members.contains(&alice));
    assert!(members.contains(&bob));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn leave_with_wrong_epoch_applies_locally_and_keeps_pending() {
    let (setup, alice, bob, _charlie, chat_id) = setup_group_with_contacts().await;
    let alice_user = &setup.get_user(&alice).user;
    let bob_user = &setup.get_user(&bob).user;

    // Bob advances the epoch; Alice does not process the commit.
    bob_user.update_key(chat_id).await.unwrap();

    alice_user.leave_chat(chat_id).await.unwrap();

    let chat = alice_user.chat(&chat_id).await.unwrap();
    assert!(matches!(chat.status(), ChatStatus::Inactive(_)));

    let pending = alice_user
        .pending_chat_operation_info(chat_id)
        .await
        .unwrap()
        .expect("pending leave should remain");
    assert_eq!(pending.operation_type, "leave");

    // We have to sleep here until we can input `now` into the operation
    sleep(Duration::from_secs(2)).await;

    // Running the outbound service after fetching and processing messages
    // should retry the leave and eventually delete the pending operation.
    let qs_messages = alice_user.qs_fetch_messages().await.unwrap();
    let result = alice_user.fully_process_qs_messages(qs_messages).await;
    assert!(
        result.errors.is_empty(),
        "processing incoming commit should succeed"
    );
    alice_user.outbound_service().run_once().await;

    // Have Bob fetch and process messages to make sure that if Alice's leave
    // was retried and succeeded.
    let qs_messages = bob_user.qs_fetch_messages().await.unwrap();
    let result = bob_user.fully_process_qs_messages(qs_messages).await;
    assert!(
        result.errors.is_empty(),
        "Bob should be able to process messages after Alice's leave is retried and succeeds"
    );

    let chat_participants = bob_user.chat_participants(chat_id).await.unwrap();
    assert!(chat_participants.len() == 1);
    assert!(!chat_participants.contains(&alice));

    let pending_after = alice_user
        .pending_chat_operation_info(chat_id)
        .await
        .unwrap();
    assert!(
        pending_after.is_none(),
        "pending operation should be deleted after successful retry"
    );
}
