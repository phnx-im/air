// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Pending chat operations: ordering, wrong-epoch recovery, retries and lost
//! responses.

use std::time::Duration;

use aircoreclient::{ChatId, ChatStatus, clients::CoreUser};
use airserver_test_harness::utils::{core_user_ext::CoreUserExt, setup::TestBackend};
use tokio::time::sleep;

// A PCO is created when executing a CO. The PCO remains after the CO execution
// only if the CO could not be completed successfully due to a network error or
// a wrong epoch error. Wrong epoch errors occur if another participant has also
// created a commit and the server has processed their commit first. Network
// errors can be simulated in a test by calling `set_drop_next_response` on the
// test setup's listener control handle (see send.rs for an example of this).
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
//   - Network errors are retried without limit and never spend an attempt.
//     Only recoverable server errors spend the budget of 5 attempts, after
//     which the PCO is deleted.
//   - If the PCO is a leave operation, it should take immediate local effect
//     regardless of any "wrong epoch" or network errors. If there was such an
//     error, it should be retried, though.
//   - If the PCO was successfully executed, it should be deleted.
async fn setup_group_with_contacts() -> (TestBackend, CoreUser, CoreUser, CoreUser, ChatId) {
    let mut setup = TestBackend::single().await;
    let [alice, bob, charlie] = setup.add_users().await;

    setup.connect_users(&alice, &bob).await;
    setup.connect_users(&alice, &charlie).await;

    let chat_id = setup.create_group(&alice).await;
    setup.invite_to_group(chat_id, &alice, &[&bob]).await;

    (setup, alice, bob, charlie, chat_id)
}

async fn setup_group_with_charlie_member() -> (TestBackend, CoreUser, CoreUser, CoreUser, ChatId) {
    let (setup, alice, bob, charlie, chat_id) = setup_group_with_contacts().await;
    setup.invite_to_group(chat_id, &alice, &[&charlie]).await;
    (setup, alice, bob, charlie, chat_id)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn pending_operation_runs_before_new_one() {
    let (setup, alice, bob, charlie, chat_id) = setup_group_with_charlie_member().await;

    setup.listener_control_handle().set_drop_next_request();
    let _ = alice
        .remove_users(chat_id, vec![charlie.user_id().clone()])
        .await
        .expect_err("expected remove to fail due to network error");

    // Re-inviting Charlie should succeed only if the pending remove is executed first.
    alice
        .invite_users(chat_id, std::slice::from_ref(charlie.user_id()))
        .await
        .expect("invite should succeed after pending remove is executed");

    let members = alice.chat_participants(chat_id).await.unwrap();
    assert!(members.contains(alice.user_id()));
    assert!(members.contains(bob.user_id()));
    assert!(members.contains(charlie.user_id()));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn operation_fails_if_pending_one_fails() {
    let (setup, alice, _bob, charlie, chat_id) = setup_group_with_charlie_member().await;

    setup.listener_control_handle().set_drop_next_response();
    let _ = alice
        .remove_users(chat_id, vec![charlie.user_id().clone()])
        .await
        .expect_err("expected remove to fail due to network error");

    let pending = alice
        .pending_chat_operation_info(chat_id)
        .await
        .unwrap()
        .expect("pending operation should exist");
    assert!(pending.operation_type == "other" || pending.operation_type == "apq_other");
    assert_eq!(pending.request_status, "ready_to_retry");

    setup.listener_control_handle().set_drop_next_response();
    let _ = alice
        .update_key(chat_id)
        .await
        .expect_err("expected update to fail because pending operation failed");

    let pending_after = alice
        .pending_chat_operation_info(chat_id)
        .await
        .unwrap()
        .expect("pending operation should still exist");
    assert!(pending_after.operation_type == "other" || pending_after.operation_type == "apq_other");
    assert_eq!(pending_after.request_status, "ready_to_retry");
    assert!(pending_after.number_of_attempts >= pending.number_of_attempts);

    let members = alice.chat_participants(chat_id).await.unwrap();
    assert!(members.contains(charlie.user_id()));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn commit_response_merges_pending_commit() {
    let (setup, alice, _bob, charlie, chat_id) = setup_group_with_charlie_member().await;

    setup.listener_control_handle().set_drop_next_response();
    let _ = alice
        .remove_users(chat_id, vec![charlie.user_id().clone()])
        .await
        .expect_err("expected remove to fail due to network error");

    alice.drain().await;

    let members = alice.chat_participants(chat_id).await.unwrap();
    assert!(!members.contains(charlie.user_id()));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn wrong_epoch_marks_pending_waiting_and_commit_clears_it() {
    let (setup, alice, bob, charlie, chat_id) = setup_group_with_charlie_member().await;

    setup.listener_control_handle().set_drop_next_request();
    let _ = alice
        .remove_users(chat_id, vec![charlie.user_id().clone()])
        .await
        .expect_err("expected remove to fail due to network error");

    // Bob commits first so Alice's pending commit is out of date.
    bob.update_key(chat_id).await.unwrap();

    let _ = alice
        .update_key(chat_id)
        .await
        .expect_err("expected wrong epoch when retrying pending commit");

    let pending = alice
        .pending_chat_operation_info(chat_id)
        .await
        .unwrap()
        .expect("pending operation should exist");
    assert_eq!(pending.request_status, "waiting_for_queue_response");

    let _ = alice
        .update_key(chat_id)
        .await
        .expect_err("pending operation waiting for queue response should fail immediately");

    alice.drain().await;

    let members = alice.chat_participants(chat_id).await.unwrap();
    assert!(members.contains(charlie.user_id()));

    let pending_after = alice.pending_chat_operation_info(chat_id).await.unwrap();
    assert!(
        pending_after.is_none(),
        "pending operation should be deleted"
    );

    alice.update_key(chat_id).await.unwrap();
}

async fn setup_apq_group_with_charlie_member() -> (TestBackend, CoreUser, CoreUser, CoreUser, ChatId)
{
    let mut setup = TestBackend::single().await;
    let [alice, bob, charlie] = setup.add_users().await;

    setup.connect_users(&alice, &bob).await;
    setup.connect_users(&alice, &charlie).await;

    let chat_id = setup.create_apq_group(&alice).await;
    setup.invite_to_group(chat_id, &alice, &[&bob]).await;
    setup.invite_to_group(chat_id, &alice, &[&charlie]).await;

    (setup, alice, bob, charlie, chat_id)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn apq_self_remove_is_committed_on_both_legs() {
    let (_setup, alice, bob, charlie, chat_id) = setup_apq_group_with_charlie_member().await;

    charlie.leave_chat(chat_id).await.unwrap();

    bob.drain().await;

    let before = bob.chat_debug_info(chat_id).await.unwrap();
    let pq_before = before.pq.clone().expect("APQ group must have a PQ group");
    assert_eq!(
        before.pending_proposals, 1,
        "T leg should hold Charlie's self-remove"
    );
    assert_eq!(
        pq_before.pending_proposals, 1,
        "PQ leg should hold Charlie's self-remove"
    );

    bob.update_key(chat_id).await.unwrap();

    let after = bob.chat_debug_info(chat_id).await.unwrap();
    let pq_after = after.pq.clone().expect("APQ group must have a PQ group");
    assert_eq!(after.pending_proposals, 0, "T self-remove not committed");
    assert_eq!(
        pq_after.pending_proposals, 0,
        "PQ self-remove not committed"
    );
    // A T-only update would have left the PQ epoch where it was.
    assert!(
        pq_after.epoch > pq_before.epoch,
        "PQ leg must advance with the self-remove, got {} -> {}",
        pq_before.epoch,
        pq_after.epoch
    );

    let participants = bob.chat_participants(chat_id).await.unwrap();
    assert!(!participants.contains(charlie.user_id()));
    assert!(participants.contains(alice.user_id()));
}

// Make sure that the rosters of T and PQ groups in an APQ group remain
// identical in the context of pending commits.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn apq_wrong_epoch_recovery_keeps_rosters_consistent() {
    let (setup, alice, bob, charlie, chat_id) = setup_apq_group_with_charlie_member().await;

    setup.listener_control_handle().set_drop_next_request();
    let _ = alice
        .remove_users(chat_id, vec![charlie.user_id().clone()])
        .await
        .expect_err("expected remove to fail due to network error");

    // Bob commits first so Alice's pending commit is out of date.
    bob.update_key(chat_id).await.unwrap();

    let _ = alice
        .update_key(chat_id)
        .await
        .expect_err("expected wrong epoch when retrying pending commit");

    let pending = alice
        .pending_chat_operation_info(chat_id)
        .await
        .unwrap()
        .expect("pending operation should exist");
    assert_eq!(pending.request_status, "waiting_for_queue_response");

    // Alice processes Bob's winning commit, which must discard her stale pending
    // commit on both the T and the PQ group.
    alice.drain().await;

    let pending_after = alice.pending_chat_operation_info(chat_id).await.unwrap();
    assert!(
        pending_after.is_none(),
        "pending operation should be deleted"
    );

    // The discard must clear the pending commit on both groups, including the
    // PQ group whose pending commit otherwise leaks.
    let debug_info = alice.chat_debug_info(chat_id).await.unwrap();
    let pq = debug_info
        .pq
        .clone()
        .expect("APQ group must have a PQ group");
    assert!(
        !debug_info.has_pending_commit,
        "T pending commit not cleared"
    );
    assert!(!pq.has_pending_commit, "PQ pending commit not cleared");
    // Bob's winning commit was a T-only update, so it advances the T epoch but
    // not the PQ epoch.
    assert!(
        pq.epoch < debug_info.epoch,
        "PQ epoch must stay behind T after a T-only update, got T={} PQ={}",
        debug_info.epoch,
        pq.epoch
    );

    // Charlie is still a member, so removing him must succeed.
    let participants = alice.chat_participants(chat_id).await.unwrap();
    assert!(
        participants.contains(charlie.user_id()),
        "Charlie should still be a member"
    );
    alice
        .remove_users(chat_id, vec![charlie.user_id().clone()])
        .await
        .expect("follow-up joint APQ remove should succeed");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn network_errors_do_not_spend_attempts() {
    let (setup, alice, bob, _charlie, chat_id) = setup_group_with_contacts().await;

    setup.listener_control_handle().set_drop_next_response();
    let _ = alice
        .update_key(chat_id)
        .await
        .expect_err("expected update to fail due to network error");

    // More retries than the budget of recoverable errors allows
    for _ in 0..6 {
        setup.listener_control_handle().set_drop_next_response();
        let _ = alice
            .update_key(chat_id)
            .await
            .expect_err("expected retry to fail due to network error");
    }

    let pending = alice
        .pending_chat_operation_info(chat_id)
        .await
        .unwrap()
        .expect("pending operation should survive network errors");
    assert_eq!(pending.operation_type, "other");
    assert_eq!(pending.request_status, "ready_to_retry");
    assert_eq!(pending.number_of_attempts, 0);

    // The first commit reached the DS, its queue response resolves the op
    alice.drain().await;

    let pending_after = alice.pending_chat_operation_info(chat_id).await.unwrap();
    assert!(
        pending_after.is_none(),
        "pending operation should be deleted"
    );

    let members = alice.chat_participants(chat_id).await.unwrap();
    assert!(members.contains(alice.user_id()));
    assert!(members.contains(bob.user_id()));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn leave_with_wrong_epoch_applies_locally_and_keeps_pending() {
    let (_setup, alice, bob, _charlie, chat_id) = setup_group_with_contacts().await;

    // Bob advances the epoch; Alice does not process the commit.
    bob.update_key(chat_id).await.unwrap();

    alice.leave_chat(chat_id).await.unwrap();

    let chat = alice.expect_chat(chat_id).await;
    assert!(matches!(chat.status(), ChatStatus::Inactive(_)));

    let pending = alice
        .pending_chat_operation_info(chat_id)
        .await
        .unwrap()
        .expect("pending leave should remain");
    assert_eq!(pending.operation_type, "leave");

    // We have to sleep here until we can input `now` into the operation
    sleep(Duration::from_secs(2)).await;

    // Running the outbound service after fetching and processing messages
    // should retry the leave and eventually delete the pending operation.
    alice.drain().await;
    alice.outbound_service().run_once().await;

    // Have Bob fetch and process messages to make sure that if Alice's leave
    // was retried and succeeded.
    bob.drain().await;

    let chat_participants = bob.chat_participants(chat_id).await.unwrap();
    assert!(chat_participants.len() == 1);
    assert!(!chat_participants.contains(alice.user_id()));

    let pending_after = alice.pending_chat_operation_info(chat_id).await.unwrap();
    assert!(
        pending_after.is_none(),
        "pending operation should be deleted after successful retry"
    );
    assert!(
        alice.mls_chat_participants(chat_id).await.is_none(),
        "the group is deleted once the DS holds the leave"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn pending_leave_is_dropped_when_other_commits_it() {
    let (setup, alice, bob, _charlie, chat_id) = setup_group_with_contacts().await;

    setup.listener_control_handle().set_drop_next_response();
    alice.leave_chat(chat_id).await.unwrap();

    let pending = alice
        .pending_chat_operation_info(chat_id)
        .await
        .unwrap()
        .expect("pending leave should remain after dropped response");
    assert_eq!(pending.operation_type, "leave");

    bob.drain().await;

    bob.update_key(chat_id).await.unwrap();

    alice.drain().await;

    let pending_after = alice.pending_chat_operation_info(chat_id).await.unwrap();
    assert!(
        pending_after.is_none(),
        "pending leave should be deleted once another member commits our self-remove"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn commit_ignoring_pending_self_remove_is_rejected() {
    let (_setup, alice, bob, _charlie, chat_id) = setup_group_with_contacts().await;

    alice.leave_chat(chat_id).await.unwrap();

    // Bob leaves the proposal sitting in his queue, so his commit cannot carry
    // it.
    bob.update_key(chat_id)
        .await
        .expect_err("DS should reject a commit that ignores the pending self-remove");

    let pending_after = bob.pending_chat_operation_info(chat_id).await.unwrap();
    assert!(
        pending_after.is_none(),
        "rejected operation should be cleaned up, not parked"
    );

    bob.drain().await;

    bob.update_key(chat_id)
        .await
        .expect("commit carrying the self-remove should be accepted");

    let chat_participants = bob.chat_participants(chat_id).await.unwrap();
    assert_eq!(chat_participants.len(), 1);
    assert!(!chat_participants.contains(alice.user_id()));
}

/// Connects Alice and Bob and has Alice delete their chat while the DS loses
/// either her request or its response. Returns what the retry needs.
async fn delete_chat_with_a_network_error(
    lose_response: bool,
) -> (TestBackend, CoreUser, CoreUser, ChatId) {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    let chat_id = setup.connect_users(&alice, &bob).await;

    if lose_response {
        setup.listener_control_handle().set_drop_next_response();
    } else {
        setup.listener_control_handle().set_drop_next_request();
    }
    alice
        .delete_and_erase_chat(chat_id)
        .await
        .expect_err("expected the delete to fail due to a network error");

    // The chat stays, and no delete is parked that erasing it would drop.
    assert!(alice.chat(&chat_id).await.is_some());
    assert!(
        alice
            .pending_chat_operation_info(chat_id)
            .await
            .unwrap()
            .is_none()
    );
    (setup, alice, bob, chat_id)
}

/// Retries the delete and checks that it reached Bob.
async fn assert_retried_delete_reaches_the_contact(
    alice: &CoreUser,
    bob: &CoreUser,
    chat_id: ChatId,
) {
    alice.delete_and_erase_chat(chat_id).await.unwrap();
    assert!(alice.chat(&chat_id).await.is_none());

    bob.drain().await;
    let chat = bob.expect_chat(chat_id).await;
    assert!(matches!(chat.status(), ChatStatus::Inactive(_)));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn offline_delete_keeps_chat_until_retry() {
    let (_setup, alice, bob, chat_id) = delete_chat_with_a_network_error(false).await;
    assert_retried_delete_reaches_the_contact(&alice, &bob, chat_id).await;
}

/// The DS applied the first attempt, so the retry finds the group gone.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn delete_after_lost_response_finishes_on_retry() {
    let (_setup, alice, bob, chat_id) = delete_chat_with_a_network_error(true).await;
    assert_retried_delete_reaches_the_contact(&alice, &bob, chat_id).await;
}
