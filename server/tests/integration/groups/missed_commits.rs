// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Members that missed a commit, removal or deletion, and recover by processing
//! or resync.

use std::slice;

use aircoreclient::{ChatStatus, SystemMessage};
use airserver_test_harness::utils::{core_user_ext::CoreUserExt, setup::TestBackend};

use super::{count, removals};

/// This test checks that bob can leave a group where he missed a commit. He leaves the group only
/// locally. Other user can remove the bob correctly (commit is merged), and add him again.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn missed_commit() {
    let mut setup = TestBackend::single().await;
    let [alice, bob, charlie] = setup.add_users().await;

    setup.connect_users(&alice, &bob).await;
    setup.connect_users(&alice, &charlie).await;

    // Alice creates a group and invites Bob
    let chat_id = setup.create_group(&alice).await;
    setup.invite_to_group(chat_id, &alice, &[&bob]).await;

    // Alice invites Charlie
    alice
        .invite_users(chat_id, slice::from_ref(charlie.user_id()))
        .await
        .unwrap();

    alice.drain().await;

    // Bob misses the invitation commit
    bob.discard_queue().await;

    // Bob tries to leave the group; this works but only locally
    bob.leave_chat(chat_id).await.unwrap();

    // ... and Alice still has 3 members
    alice.drain().await;
    assert_eq!(
        alice.mls_members(chat_id).await.unwrap().unwrap().len(),
        3,
        "Alice still has 3 members"
    );

    // ... and Charlie still has 3 members
    charlie.drain().await;
    assert_eq!(
        charlie.mls_members(chat_id).await.unwrap().unwrap().len(),
        3,
        "Charlie still has 3 members"
    );

    // Alice removes Bob and adds him again
    alice
        .remove_users(chat_id, vec![bob.user_id().clone()])
        .await
        .unwrap();
    alice
        .invite_users(chat_id, slice::from_ref(bob.user_id()))
        .await
        .unwrap();
    alice.drain().await;
    bob.drain().await;
    charlie.drain().await;

    // Now everyone sees 3 members
    assert_eq!(
        alice.mls_members(chat_id).await.unwrap().unwrap().len(),
        3,
        "Alice has 3 members"
    );
    assert_eq!(
        bob.mls_members(chat_id).await.unwrap().unwrap().len(),
        3,
        "Bob has 3 members"
    );
    assert_eq!(
        charlie.mls_members(chat_id).await.unwrap().unwrap().len(),
        3,
        "Charlie has 3 members"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn missed_delete_commit_then_resync() {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    let chat_id = setup.connected_group(&alice, &[&bob]).await;

    alice.delete_chat(chat_id).await.unwrap();

    // Bob misses the delete commit and only gets to it after the resync.
    let late_messages = bob.qs_fetch_messages().await.unwrap();
    assert_eq!(bob.expect_chat(chat_id).await.status(), &ChatStatus::Active);
    let participants = bob.chat_participants(chat_id).await.unwrap();

    bob.enqueue_group_resync(chat_id).await.unwrap();
    bob.outbound_service().run_once().await;

    assert!(bob.expect_chat(chat_id).await.status().is_inactive());
    let messages = bob.system_messages(chat_id).await;
    assert_eq!(
        count(
            &messages,
            &SystemMessage::DeleteGroup(alice.user_id().clone())
        ),
        1,
        "got {messages:?}"
    );
    assert_eq!(removals(&messages), 0, "got {messages:?}");
    assert!(
        bob.resync_status(chat_id).await.unwrap().is_none(),
        "the resync entry is gone"
    );
    assert!(
        bob.mls_members(chat_id).await.unwrap().is_none(),
        "the MLS group is gone"
    );
    assert_eq!(
        bob.chat_participants(chat_id).await,
        Some(participants),
        "the chat still lists its members"
    );

    bob.fully_process_qs_messages(late_messages).await;
    let messages = bob.system_messages(chat_id).await;
    assert_eq!(
        count(
            &messages,
            &SystemMessage::DeleteGroup(alice.user_id().clone())
        ),
        1,
        "the late delete commit adds nothing: {messages:?}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn missed_removal_then_resync() {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    let chat_id = setup.connected_group(&alice, &[&bob]).await;

    alice
        .remove_users(chat_id, vec![bob.user_id().clone()])
        .await
        .unwrap();

    bob.discard_queue().await;
    let participants = bob.chat_participants(chat_id).await.unwrap();
    bob.enqueue_group_resync(chat_id).await.unwrap();
    bob.outbound_service().run_once().await;

    assert!(bob.expect_chat(chat_id).await.status().is_inactive());
    assert!(
        bob.resync_status(chat_id).await.unwrap().is_none(),
        "the resync entry is gone"
    );
    assert_eq!(
        bob.chat_participants(chat_id).await,
        Some(participants),
        "the chat still lists its members"
    );
    let messages = bob.system_messages(chat_id).await;
    assert_eq!(
        count(
            &messages,
            &SystemMessage::DeleteGroup(alice.user_id().clone())
        ),
        0,
        "got {messages:?}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn missed_deletion_without_tombstone_then_resync() {
    let mut setup = TestBackend::single().await;
    let [alice, bob, charlie] = setup.add_users().await;
    let chat_id = setup.connected_group(&alice, &[&bob, &charlie]).await;

    alice.delete_chat_without_tombstone(chat_id).await.unwrap();

    bob.discard_queue().await;
    bob.enqueue_group_resync(chat_id).await.unwrap();
    bob.outbound_service().run_once().await;

    assert!(bob.expect_chat(chat_id).await.status().is_inactive());
    assert!(
        bob.resync_status(chat_id).await.unwrap().is_none(),
        "the resync entry is gone"
    );
}
