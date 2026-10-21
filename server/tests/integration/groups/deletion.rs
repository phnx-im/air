// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Deleting T and APQ groups, with and without tombstone, and local state after
//! deletion.

use aircoreclient::{ChatStatus, MessageDraft, SystemMessage};
use airserver_test_harness::utils::{core_user_ext::CoreUserExt, setup::TestBackend};

use super::count;

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn delete_non_apq_group() {
    let mut setup = TestBackend::single().await;

    let [alice, bob, charlie] = setup.add_users().await;
    setup.connect_users(&alice, &bob).await;
    setup.connect_users(&alice, &charlie).await;
    let chat_id = setup.create_non_apq_group(&alice).await;
    setup
        .invite_to_group(chat_id, &alice, &[&bob, &charlie])
        .await;
    setup.delete_group(chat_id, &alice).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn delete_apq_group() {
    let mut setup = TestBackend::single().await;

    let [alice, bob, charlie] = setup.add_users().await;
    let chat_id = setup.connected_apq_group(&alice, &[&bob, &charlie]).await;
    setup.delete_group(chat_id, &alice).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn delete_apq_group_by_invitee() {
    let mut setup = TestBackend::single().await;

    let [alice, bob, charlie] = setup.add_users().await;
    let chat_id = setup.connected_apq_group(&alice, &[&bob, &charlie]).await;
    setup.delete_group(chat_id, &bob).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn delete_group_as_sole_member() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let chat_id = setup.create_group(&alice).await;

    assert!(alice.ds_has_group(chat_id).await.unwrap());

    alice.delete_chat(chat_id).await.unwrap();
    let messages = alice.system_messages(chat_id).await;
    assert_eq!(
        count(
            &messages,
            &SystemMessage::DeleteGroup(alice.user_id().clone())
        ),
        1,
        "got {messages:?}"
    );
    assert!(alice.expect_chat(chat_id).await.status().is_inactive());
    assert!(
        !alice.ds_has_group(chat_id).await.unwrap(),
        "the DS deletes the group of a sole member"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn delete_group_without_tombstone() {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    let chat_id = setup.connected_group(&alice, &[&bob]).await;

    let bobs_removal = SystemMessage::Remove(Some(alice.user_id().clone()), bob.user_id().clone());
    let deletions = |messages: &[SystemMessage]| {
        count(
            messages,
            &SystemMessage::DeleteGroup(alice.user_id().clone()),
        )
    };

    // Alice deletes the group as clients before the tombstone do.
    alice.delete_chat_without_tombstone(chat_id).await.unwrap();
    assert!(alice.expect_chat(chat_id).await.status().is_inactive());
    let messages = alice.system_messages(chat_id).await;
    assert_eq!(count(&messages, &bobs_removal), 1, "got {messages:?}");
    assert_eq!(deletions(&messages), 0, "got {messages:?}");

    bob.drain().await;
    assert!(bob.expect_chat(chat_id).await.status().is_inactive());
    let messages = bob.system_messages(chat_id).await;
    assert_eq!(count(&messages, &bobs_removal), 1, "got {messages:?}");
    assert_eq!(deletions(&messages), 0, "got {messages:?}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn store_draft_after_delete() {
    let mut setup = TestBackend::single().await;

    let alice = setup.add_user().await;
    let chat_id = setup.create_apq_group(&alice).await;

    let _ = alice.delete_chat(chat_id).await;
    alice
        .erase_chat(chat_id)
        .await
        .expect("erasing an APQ chat must not fail");

    // The compose field's draft is persisted after the chat has been erased.
    // This must not trip the `message_draft -> chat` foreign key.
    alice
        .store_message_draft(chat_id, Some(&MessageDraft::empty()))
        .await
        .expect("storing a draft for a deleted chat must not fail");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn apq_delete_removes_member_from_both_t_and_pq() {
    let mut setup = TestBackend::single().await;

    let [alice, bob, charlie] = setup.add_users().await;

    setup.connect_users(&alice, &bob).await;
    setup.connect_users(&alice, &charlie).await;

    let chat_id = setup.create_apq_group(&alice).await;
    setup.invite_to_group(chat_id, &alice, &[&bob]).await;
    setup.invite_to_group(chat_id, &alice, &[&charlie]).await;

    // Snapshot the PQ epoch before the delete so we can assert it advanced.
    let pq_epoch_before = alice
        .chat_debug_info(chat_id)
        .await
        .unwrap()
        .pq
        .expect("APQ group must have a PQ group")
        .epoch;

    // Alice deletes the APQ group. This commits a removal of every other member
    // on both the T and the PQ group.
    alice.delete_chat(chat_id).await.unwrap();

    // Alice must have committed the removal on both groups: only her own leaf is
    // left, nothing stays pending, and the PQ epoch advanced.
    let debug_info = alice.chat_debug_info(chat_id).await.unwrap();
    let pq = debug_info.pq.expect("APQ group must have a PQ group");

    assert_eq!(
        debug_info.members.len(),
        1,
        "deleter's T group must have every other member removed"
    );
    assert_eq!(
        debug_info.pending_proposals, 0,
        "deleter's T group must have no pending proposals after the delete commit"
    );
    assert!(
        !debug_info.has_pending_commit,
        "deleter's T group must have merged the delete commit"
    );
    assert_eq!(
        pq.pending_proposals, 0,
        "deleter's PQ group must have no pending proposals after the delete commit"
    );
    assert!(
        !pq.has_pending_commit,
        "deleter's PQ group must have merged the delete commit"
    );
    assert!(
        pq.epoch > pq_epoch_before,
        "deleter's PQ group must advance its epoch by committing the delete"
    );

    // Alice's chat is now inactive.
    let alice_chat = alice.expect_chat(chat_id).await;
    assert!(
        matches!(alice_chat.status(), ChatStatus::Inactive(_)),
        "deleter's chat must be inactive after deleting the APQ group"
    );

    // Bob and Charlie pick up the delete commit and their chats go inactive.
    for (label, user) in [("Bob", &bob), ("Charlie", &charlie)] {
        user.drain().await;
        let chat = user.expect_chat(chat_id).await;
        assert!(
            matches!(chat.status(), ChatStatus::Inactive(_)),
            "{label}'s chat must be inactive after the APQ group was deleted"
        );
    }
}
