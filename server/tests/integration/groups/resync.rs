// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Resyncing groups and connection groups via external commit.

use std::{collections::HashSet, slice};

use aircommon::identifiers::QualifiedGroupId;
use aircoreclient::{
    ChatId, ChatStatus, DisplayName, ResyncStatus, SystemMessage, UserProfile,
    clients::{CoreUser, MarkChatAsRead},
};
use airserver_test_harness::utils::{core_user_ext::CoreUserExt, setup::TestBackend};
use mimi_content::MimiContent;
use uuid::Uuid;

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn missed_commit_triggers_resync_and_rejoin() {
    let mut setup = TestBackend::single().await;

    let alice = setup.add_user().await;
    setup.add_username(&alice).await;

    let [bob, charlie] = setup.add_users().await;

    setup.connect_users(&alice, &bob).await;
    setup.connect_users(&alice, &charlie).await;

    let chat_id = setup.create_group(&alice).await;
    setup.invite_to_group(chat_id, &alice, &[&bob]).await;

    // To trigger resync, we have Alice add Charlie to the group and Bob
    // fetching, but not processing the commit.

    // Alice creates a invites charlie and sends the commit to the DS
    alice
        .invite_users(chat_id, slice::from_ref(charlie.user_id()))
        .await
        .unwrap();

    // Bob fetches the invite and acks it s.t. it's removed from the queue,
    // but does not process it. This is to simulate Bob missing the commit.
    let qs_messages = bob.qs_fetch_messages().await.unwrap();
    let [message] = qs_messages.as_slice() else {
        panic!("Bob should have one message in the queue");
    };
    let (mut stream, responder) = bob.listen_queue().await.unwrap();
    responder.ack(message.sequence_number + 1).await;
    responder.close(&mut stream).await;

    // Alice performs an update, which bob fetches and processes, triggering a
    // resync.
    alice.update_key(chat_id).await.unwrap();

    // Bob fetches and processes the update
    bob.drain().await;
    // Run outbound service to complete the rejoin process
    bob.outbound_service().run_once().await;
    // Instead of throwing an error, Bob should have re-synced as part of processing the update.

    // The resync diff should surface Charlie's addition without an actor.
    let messages = bob.system_messages(chat_id).await;
    let actorless_charlie_adds = messages
        .iter()
        .filter(|message| matches!(message, SystemMessage::Add(None, added) if added == charlie.user_id()))
        .count();
    assert_eq!(
        actorless_charlie_adds, 1,
        "Bob should store exactly one actor-less add of Charlie"
    );
    let charlie_adds_with_actor = messages
        .iter()
        .filter(
            |message| matches!(message, SystemMessage::Add(Some(_), added) if added == charlie.user_id()),
        )
        .count();
    assert_eq!(
        charlie_adds_with_actor, 0,
        "Bob never processed the invite commit, so no add with an actor"
    );

    // Alice processes Bob's rejoin
    alice.drain().await;

    // Bob should have rejoined the group and should be able to send a message.
    setup.send_message(chat_id, &bob, &[&alice, &charlie]).await;

    // When Alice sends another message, Bob should be able to process it without errors.
    alice
        .send_message(
            chat_id,
            MimiContent::simple_markdown_message("message".to_owned(), [0; 16]),
            None,
            MarkChatAsRead::Yes,
        )
        .await
        .unwrap();

    bob.drain().await;

    // Now Alice leaves the group, which means that if Bob resyncs again, he
    // should commit the SelfRemove proposal in the process.

    // Alice sends an update, which Bob misses again.
    alice.update_key(chat_id).await.unwrap();

    // Bob fetches the update and acks it s.t. it's removed from the queue,
    // but does not process it. This is to simulate Bob missing the commit.
    let qs_messages = bob.qs_fetch_messages().await.unwrap();
    let [message] = qs_messages.as_slice() else {
        panic!("Bob should have one message in the queue");
    };
    let (mut stream, responder) = bob.listen_queue().await.unwrap();
    responder.ack(message.sequence_number + 1).await;
    responder.close(&mut stream).await;

    // Now Alice leaves the group, which means that if Bob resyncs again, he
    // should commit the SelfRemove proposal in the process.
    alice.leave_chat(chat_id).await.unwrap();

    // Bob fetches and processes his messages, which should trigger a resync.

    // Alice is still part of the group.
    let participants = bob.group_members(chat_id).await.unwrap();
    assert_eq!(
        participants,
        [
            alice.user_id().clone(),
            bob.user_id().clone(),
            charlie.user_id().clone()
        ]
        .into_iter()
        .collect()
    );

    bob.drain().await;
    bob.outbound_service().run_once().await;
    // Instead of throwing an error, Bob should have re-synced as part of processing the update.

    // The resync committed Alice's SelfRemove, which Bob cannot attribute.
    let messages = bob.system_messages(chat_id).await;
    let actorless_alice_removes = messages
        .iter()
        .filter(
            |message| matches!(message, SystemMessage::Remove(None, removed) if removed == alice.user_id()),
        )
        .count();
    assert_eq!(
        actorless_alice_removes, 1,
        "Bob should store exactly one actor-less removal of Alice"
    );
    let alice_removes_with_actor = messages
        .iter()
        .filter(|message| {
            matches!(message, SystemMessage::Remove(Some(_), removed) if removed == alice.user_id())
        })
        .count();
    assert_eq!(
        alice_removes_with_actor, 0,
        "Bob never processed the leave commit, so no removal with an actor"
    );

    // No spurious membership notices for Bob or Charlie.
    let actorless: Vec<_> = messages
        .iter()
        .filter(|message| {
            matches!(
                message,
                SystemMessage::Add(None, _) | SystemMessage::Remove(None, _)
            )
        })
        .collect();
    assert_eq!(
        actorless.len(),
        2,
        "resync should only emit the Charlie add and the Alice remove, got {actorless:?}"
    );

    // Alice not in the group anymore.
    let participants = bob.group_members(chat_id).await.unwrap();
    assert_eq!(
        participants,
        [bob.user_id().clone(), charlie.user_id().clone()]
            .into_iter()
            .collect()
    );

    // Messages should reach Charlie.
    setup.send_message(chat_id, &bob, &[&charlie]).await;

    // Charlie should also only see Bob in the group.

    let participants = charlie.group_members(chat_id).await.unwrap();
    assert_eq!(
        participants,
        [bob.user_id().clone(), charlie.user_id().clone()]
            .into_iter()
            .collect()
    );
}

/// A wrong-epoch answer to our own commit parks it and nothing more. Only a message from an epoch
/// ahead of ours schedules the resync.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn wrong_epoch_on_own_commit_does_not_resync() {
    let mut setup = TestBackend::single().await;

    let [alice, bob] = setup.add_users().await;
    let chat_id = setup.connected_group(&alice, &[&bob]).await;

    // Alice commits. Bob acks the commit without processing it, so he is one epoch behind.
    alice.update_key(chat_id).await.unwrap();

    let qs_messages = bob.qs_fetch_messages().await.unwrap();
    let [message] = qs_messages.as_slice() else {
        panic!("Bob should have one message in the queue");
    };
    let (mut stream, responder) = bob.listen_queue().await.unwrap();
    responder.ack(message.sequence_number + 1).await;
    responder.close(&mut stream).await;

    let group_id = bob.expect_chat(chat_id).await.group_id;

    // Bob's own commit is rejected with a wrong epoch: parked, nothing else.
    bob.update_key(chat_id)
        .await
        .expect_err("commit at a stale epoch must be rejected");
    let pending = bob
        .pending_chat_operation_info(chat_id)
        .await
        .unwrap()
        .expect("rejected commit should be parked");
    assert_eq!(pending.request_status, "waiting_for_queue_response");
    assert!(bob.resync_status(chat_id).await.unwrap().is_none());
    assert!(!bob.chat_is_pending(&group_id).await.unwrap());

    // An outbound run changes nothing either.
    bob.outbound_service().run_once().await;
    assert!(bob.resync_status(chat_id).await.unwrap().is_none());
    assert!(!bob.chat_is_pending(&group_id).await.unwrap());

    // The group setup leaves a message from Bob behind. Alice reads it first, or her send would
    // add a read receipt to Bob's queue.
    let newest = alice.expect_last_message(chat_id).await;
    alice.mark_chat_as_read(chat_id, newest.id()).await.unwrap();

    // Alice sends a message from the epoch Bob missed. Processing it proves the desync and
    // schedules the resync.
    alice.send_text(chat_id, "message").await;

    let qs_messages = bob.qs_fetch_messages().await.unwrap();
    assert_eq!(qs_messages.len(), 1, "Bob should receive Alice's message");
    let result = bob.fully_process_qs_messages(qs_messages).await;
    assert!(
        result.errors.is_empty(),
        "a message from a future epoch must schedule a resync, not fail"
    );
    assert!(bob.resync_status(chat_id).await.unwrap().is_some());
    assert!(bob.chat_is_pending(&group_id).await.unwrap());

    // The resync replaces the group state, which also drops the parked commit.
    bob.outbound_service().run_once().await;
    assert!(bob.resync_status(chat_id).await.unwrap().is_none());
    assert!(!bob.chat_is_pending(&group_id).await.unwrap());
    assert!(
        bob.pending_chat_operation_info(chat_id)
            .await
            .unwrap()
            .is_none()
    );

    // Alice processes Bob's rejoin and Bob can send again.
    alice.drain().await;

    setup.send_message(chat_id, &bob, &[&alice]).await;
}

/// When the DS returns "group not found" for a resync, the client must stop
/// retrying and remove the resync from the queue.
///
/// Note: in production the group_id in the resync queue matches the local group,
/// so the full cleanup (chat marked inactive, MLS group deleted) also runs. This
/// test uses a fabricated group_id that only exists in the resync queue, so only
/// the resync removal can be verified here.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn group_not_found_removes_resync_entry() {
    let mut setup = TestBackend::single().await;

    let [alice, bob] = setup.add_users().await;
    let chat_id = setup.connected_group(&alice, &[&bob]).await;

    // Enqueue a resync with a fabricated group_id that doesn't exist on the
    // server. This simulates a group that was deleted or garbage-collected
    // server-side while the client still had a stale resync entry.
    bob.enqueue_resync_for_nonexistent_group(chat_id, "localhost")
        .await
        .unwrap();
    assert!(
        bob.resync_status(chat_id).await.unwrap().is_some(),
        "resync should be queued"
    );

    // Bob runs the outbound service. The resync hits "group not found"
    // and must not loop forever.
    bob.outbound_service().run_once().await;

    assert!(
        bob.resync_status(chat_id).await.unwrap().is_none(),
        "resync should have been removed after group not found"
    );
}

/// Baseline test: resync for an existing group succeeds and the user can
/// continue communicating normally afterwards.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn resync_restores_messaging() {
    let mut setup = TestBackend::single().await;

    let [alice, bob] = setup.add_users().await;
    let chat_id = setup.connected_group(&alice, &[&bob]).await;

    // Bob enqueues a resync for the group.
    bob.enqueue_group_resync(chat_id).await.unwrap();
    assert!(
        bob.resync_status(chat_id).await.unwrap().is_some(),
        "resync should be queued"
    );

    // Run outbound service — resync should succeed.
    bob.outbound_service().run_once().await;

    assert!(
        bob.resync_status(chat_id).await.unwrap().is_none(),
        "resync should have completed"
    );

    // Alice processes Bob's rejoin commit.
    alice.drain().await;

    // Bob should be able to send messages normally.
    setup.send_message(chat_id, &bob, &[&alice]).await;

    // Alice performs a further commit-based operation (for APQ groups, one that touches both legs).
    // Bob must process it cleanly, which for APQ groups also verifies that the resync left the T
    // and PQ legs at compatible epochs (PSK continuity).
    if setup.apq_groups {
        alice.update_apq_key(chat_id).await.unwrap();
    } else {
        alice.update_key(chat_id).await.unwrap();
    }

    bob.drain().await;
}

/// Resync with holes in the leaf nodes: Alice creates a group with Bob,
/// Charlie and Dave (leaf indices 0, 1, 2, 3), and then removes Bob. This
/// blanks Bob's leaf, leaving a hole in the leaf nodes, so that leaf indices
/// no longer match positions in flattened member lists. Charlie then leaves
/// the chat, which puts a pending SelfRemove proposal (for leaf index 2) on
/// the DS. Alice resyncs first, then Dave; both resyncs must succeed.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn resync_with_blank_leaves_succeeds() {
    let mut setup = TestBackend::single().await;

    let [alice, bob, charlie, dave] = setup.add_users().await;
    let chat_id = setup
        .connected_group(&alice, &[&bob, &charlie, &dave])
        .await;

    // Removing Bob blanks his leaf, creating a hole in the leaf nodes.
    setup
        .remove_from_group(chat_id, &alice, &[&bob])
        .await
        .unwrap();

    // Charlie leaves the chat. This puts a pending SelfRemove proposal for leaf index 2 on the DS,
    // which is committed by the next resync's external commit.
    charlie.leave_chat(chat_id).await.unwrap();

    // Alice and Dave observe Charlie's proposal.
    for user in [&alice, &dave] {
        user.drain().await;
    }

    // Dave changes his profile. This rotates his user profile key, so Alice's post-resync profile
    // fetch must decrypt it with the key from the external commit info.
    let dave_display_name: DisplayName = "D4v3".parse().unwrap();
    dave.set_own_user_profile(UserProfile {
        user_id: dave.user_id().clone(),
        display_name: dave_display_name.clone(),
        profile_picture: None,
    })
    .await
    .unwrap();

    // Alice resyncs first.
    alice.enqueue_group_resync(chat_id).await.unwrap();
    assert!(
        alice.resync_status(chat_id).await.unwrap().is_some(),
        "resync should be queued"
    );

    // Run outbound service — resync should succeed despite the blank leaves.
    alice.outbound_service().run_once().await;

    assert!(
        alice.resync_status(chat_id).await.unwrap().is_none(),
        "resync should have completed"
    );

    // Run the outbound service again to process the user profile fetch operations enqueued by the
    // resync.
    alice.outbound_service().run_once().await;

    // Alice should see Dave's updated profile after the resync.
    let dave_profile = alice.user_profile(dave.user_id()).await;
    assert_eq!(dave_profile.display_name, dave_display_name);

    // Alice must not have received her own external commit from the DS.
    alice.drain().await;

    // Dave processes Alice's rejoin commit.
    dave.drain().await;

    // Now Dave resyncs as well.
    dave.enqueue_group_resync(chat_id).await.unwrap();
    assert!(
        dave.resync_status(chat_id).await.unwrap().is_some(),
        "resync should be queued"
    );

    dave.outbound_service().run_once().await;

    assert!(
        dave.resync_status(chat_id).await.unwrap().is_none(),
        "resync should have completed"
    );

    // Run the outbound service again to process the user profile fetch operations enqueued by the
    // resync.
    dave.outbound_service().run_once().await;

    // Dave must not have received his own external commit from the DS.
    dave.drain().await;

    // Alice processes Dave's rejoin commit.
    alice.drain().await;

    // Both group members should agree on the group membership.
    let expected_members: HashSet<_> = [alice.user_id().clone(), dave.user_id().clone()]
        .into_iter()
        .collect();
    let participants = alice.group_members(chat_id).await.unwrap();
    assert_eq!(participants, expected_members);
    let participants = dave.group_members(chat_id).await.unwrap();
    assert_eq!(participants, expected_members);

    // Dave sends a message.
    //
    // Note: the external commit re-added Dave at the leftmost blank leaf (Bob's old position), i.e.
    // his leaf index changed.
    dave.send_text(chat_id, "message").await;

    // Alice receives the message.
    alice.drain().await;

    // The DS must not fan out Dave's own message back to him.
    dave.drain().await;
}

/// A connection group is resynced with the same machinery as any other group.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn connection_group_resync_restores_messaging() {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    let chat_id = setup.connect_users(&alice, &bob).await;

    bob.enqueue_group_resync(chat_id).await.unwrap();
    bob.outbound_service().run_once().await;
    assert!(
        bob.resync_status(chat_id).await.unwrap().is_none(),
        "resync should have completed"
    );

    alice.drain().await;

    setup.send_message(chat_id, &bob, &[&alice]).await;
    setup.send_message(chat_id, &alice, &[&bob]).await;

    // A commit touching both legs after the resync shows that they are at compatible epochs.
    if setup.apq_groups {
        alice.update_apq_key(chat_id).await.unwrap();
    } else {
        alice.update_key(chat_id).await.unwrap();
    }
    bob.drain().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[ignore = "a message sent during a pending resync is dropped from the queue unsent"]
async fn message_sent_during_resync_is_delivered_after_it() {
    let mut setup = TestBackend::single().await;
    let [alice, bob, charlie] = setup.add_users().await;
    setup.connect_users(&alice, &bob).await;
    setup.connect_users(&alice, &charlie).await;
    let chat_id = setup.create_group(&alice).await;
    setup.invite_to_group(chat_id, &alice, &[&bob]).await;

    // Bob misses a commit and processes the next one, which schedules a
    // resync.
    alice
        .invite_users(chat_id, slice::from_ref(charlie.user_id()))
        .await
        .unwrap();
    let qs_messages = bob.qs_fetch_messages().await.unwrap();
    let [message] = qs_messages.as_slice() else {
        panic!("Bob should have one message in the queue");
    };
    let (mut stream, responder) = bob.listen_queue().await.unwrap();
    responder.ack(message.sequence_number + 1).await;
    responder.close(&mut stream).await;
    alice.update_key(chat_id).await.unwrap();
    bob.drain().await;

    // Bob sends before the resync ran.
    let content = MimiContent::simple_markdown_message("during resync".to_owned(), [3; 16]);
    bob.send_message(chat_id, content, None, MarkChatAsRead::Yes)
        .await
        .unwrap();
    bob.outbound_service()
        .send_queued_messages_once()
        .await
        .unwrap();

    bob.outbound_service().run_once().await;
    bob.outbound_service().run_once().await;
    alice.drain_unchecked().await;

    assert!(
        alice
            .text_messages(chat_id)
            .await
            .iter()
            .any(|text| text.contains("during resync")),
        "alice never received the message"
    );
}

async fn group_uuid(user: &CoreUser, chat_id: ChatId) -> Uuid {
    let group_id = user.chat(&chat_id).await.unwrap().group_id;
    QualifiedGroupId::try_from(group_id).unwrap().group_uuid()
}

async fn epoch(user: &CoreUser, chat_id: ChatId) -> u64 {
    let (epoch, _) = user
        .group_epoch_and_own_index(chat_id)
        .await
        .unwrap()
        .expect("the group should exist");
    epoch
}

async fn own_index(user: &CoreUser, chat_id: ChatId) -> u32 {
    let (_, own_index) = user
        .group_epoch_and_own_index(chat_id)
        .await
        .unwrap()
        .expect("the group should exist");
    own_index
}

async fn epoch_authenticator(user: &CoreUser, chat_id: ChatId) -> Vec<u8> {
    user.epoch_authenticator(chat_id)
        .await
        .unwrap()
        .expect("the group should exist")
}

async fn resync_status(user: &CoreUser, chat_id: ChatId) -> Option<ResyncStatus> {
    user.resync_status(chat_id).await.unwrap()
}

/// Acks all queued messages of `user` without processing them.
async fn miss_queued_messages(user: &CoreUser) {
    let qs_messages = user.qs_fetch_messages().await.unwrap();
    let last = qs_messages.last().expect("a queued message to miss");
    let (mut stream, responder) = user.listen_queue().await.unwrap();
    responder.ack(last.sequence_number + 1).await;
    responder.close(&mut stream).await;
}

/// Has `alice` move the group on by two commits, of which `bob` misses the
/// first, so that the second makes him queue a resync.
async fn make_miss_a_commit(alice: &CoreUser, bob: &CoreUser, chat_id: ChatId) {
    alice.update_key(chat_id).await.unwrap();
    miss_queued_messages(bob).await;
    alice.update_key(chat_id).await.unwrap();
    bob.drain().await;
    assert_eq!(
        resync_status(bob, chat_id).await,
        Some(ResyncStatus::Pending)
    );
}

/// Has `alice` remove `charlie` in a commit `bob` misses, then move the group
/// on so that he queues a resync.
async fn make_miss_a_removal(
    alice: &CoreUser,
    bob: &CoreUser,
    charlie: &CoreUser,
    chat_id: ChatId,
) {
    alice
        .remove_users(chat_id, vec![charlie.user_id().clone()])
        .await
        .unwrap();
    miss_queued_messages(bob).await;
    alice.update_key(chat_id).await.unwrap();
    bob.drain().await;
    assert_eq!(
        resync_status(bob, chat_id).await,
        Some(ResyncStatus::Pending)
    );
}

/// How many system messages of `user` say that `removed` was removed, without
/// naming who removed them.
async fn removal_messages(user: &CoreUser, chat_id: ChatId, removed: &CoreUser) -> usize {
    user.system_messages(chat_id)
        .await
        .into_iter()
        .filter(|message| {
            matches!(message, SystemMessage::Remove(None, user_id) if user_id == removed.user_id())
        })
        .count()
}

/// The gRPC method that sends a resync commit for the test's groups.
fn resync_method(setup: &TestBackend) -> &'static str {
    if setup.apq_groups {
        "ApqResync"
    } else {
        "Resync"
    }
}

/// Runs a resync of `bob` whose commit never reaches the DS. Returns the
/// epoch authenticator of the state the stored commit produces.
async fn drop_resync_request(setup: &TestBackend, bob: &CoreUser, chat_id: ChatId) -> Vec<u8> {
    setup
        .listener_control_handle()
        .set_drop_next_request_to(resync_method(setup));
    bob.outbound_service().run_once().await;
    assert_eq!(
        resync_status(bob, chat_id).await,
        Some(ResyncStatus::Pending)
    );
    epoch_authenticator(bob, chat_id).await
}

async fn assert_messaging_works(
    setup: &TestBackend,
    alice: &CoreUser,
    bob: &CoreUser,
    chat_id: ChatId,
) {
    alice.drain().await;
    setup.send_message(chat_id, bob, &[alice]).await;
    setup.send_message(chat_id, alice, &[bob]).await;
}

/// A DS replaying the group state from before a member's removal can't make
/// another member rejoin it.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn resync_rejects_stale_snapshot() {
    let mut setup = TestBackend::single().await;
    let [alice, bob, charlie] = setup.add_users().await;
    setup.connect_users(&alice, &bob).await;
    setup.connect_users(&alice, &charlie).await;
    let chat_id = setup.create_group(&alice).await;
    setup
        .invite_to_group(chat_id, &alice, &[&bob, &charlie])
        .await;

    let group_uuid = group_uuid(&bob, chat_id).await;
    let stale_row = setup.ds_group_row(group_uuid).await;

    alice
        .remove_users(chat_id, vec![charlie.user_id().clone()])
        .await
        .unwrap();
    alice.update_key(chat_id).await.unwrap();
    bob.drain().await;
    let bob_epoch = epoch(&bob, chat_id).await;

    let current_row = setup.ds_group_row(group_uuid).await;
    setup.set_ds_group_row(group_uuid, &stale_row).await;
    bob.enqueue_group_resync(chat_id).await.unwrap();
    bob.outbound_service().run_once().await;

    assert_eq!(
        resync_status(&bob, chat_id).await,
        Some(ResyncStatus::Failed)
    );
    assert_eq!(epoch(&bob, chat_id).await, bob_epoch);
    assert!(
        !bob.group_members(chat_id)
            .await
            .unwrap()
            .contains(charlie.user_id())
    );

    // The next real commit clears the failed resync.
    setup.set_ds_group_row(group_uuid, &current_row).await;
    alice.update_key(chat_id).await.unwrap();
    bob.drain().await;
    assert_eq!(resync_status(&bob, chat_id).await, None);
    setup.send_message(chat_id, &bob, &[&alice]).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn resync_keeps_group_when_snapshot_predates_join() {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    setup.connect_users(&alice, &bob).await;
    let chat_id = setup.create_group(&alice).await;

    let group_uuid = group_uuid(&alice, chat_id).await;
    let stale_row = setup.ds_group_row(group_uuid).await;

    setup.invite_to_group(chat_id, &alice, &[&bob]).await;
    let bob_epoch = epoch(&bob, chat_id).await;

    let current_row = setup.ds_group_row(group_uuid).await;
    setup.set_ds_group_row(group_uuid, &stale_row).await;
    bob.enqueue_group_resync(chat_id).await.unwrap();
    bob.outbound_service().run_once().await;

    assert_eq!(
        resync_status(&bob, chat_id).await,
        Some(ResyncStatus::Failed)
    );
    assert_eq!(epoch(&bob, chat_id).await, bob_epoch);
    assert!(!bob.chat(&chat_id).await.unwrap().status().is_inactive());

    // The next real commit clears the failed resync.
    setup.set_ds_group_row(group_uuid, &current_row).await;
    alice.update_key(chat_id).await.unwrap();
    bob.drain().await;
    assert_eq!(resync_status(&bob, chat_id).await, None);
    setup.send_message(chat_id, &bob, &[&alice]).await;
}

/// A resync triggered by a missed commit refuses a state at the member's own
/// epoch, which a DS could serve after forging the trigger. A manual resync
/// into the real state recovers.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn future_epoch_resync_rejects_current_epoch() {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    let chat_id = setup.connected_group(&alice, &[&bob]).await;

    let group_uuid = group_uuid(&bob, chat_id).await;
    let bob_epoch = epoch(&bob, chat_id).await;
    let row_at_bob_epoch = setup.ds_group_row(group_uuid).await;

    make_miss_a_commit(&alice, &bob, chat_id).await;

    let current_row = setup.ds_group_row(group_uuid).await;
    setup.set_ds_group_row(group_uuid, &row_at_bob_epoch).await;
    bob.outbound_service().run_once().await;

    assert_eq!(
        resync_status(&bob, chat_id).await,
        Some(ResyncStatus::Failed)
    );
    assert_eq!(epoch(&bob, chat_id).await, bob_epoch);

    setup.set_ds_group_row(group_uuid, &current_row).await;
    bob.enqueue_group_resync(chat_id).await.unwrap();
    bob.outbound_service().run_once().await;

    assert_eq!(resync_status(&bob, chat_id).await, None);
    assert_eq!(epoch(&bob, chat_id).await, bob_epoch + 3);
    assert_messaging_works(&setup, &alice, &bob, chat_id).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn resync_resends_stored_commit_after_dropped_request() {
    let mut setup = TestBackend::single().await;
    let [alice, bob, charlie] = setup.add_users().await;
    let chat_id = setup.connected_group(&alice, &[&bob, &charlie]).await;

    make_miss_a_removal(&alice, &bob, &charlie, chat_id).await;
    let attempted = drop_resync_request(&setup, &bob, chat_id).await;

    bob.outbound_service().run_once().await;
    assert_eq!(resync_status(&bob, chat_id).await, None);
    assert_eq!(epoch_authenticator(&bob, chat_id).await, attempted);
    assert_eq!(removal_messages(&bob, chat_id, &charlie).await, 1);
    assert_messaging_works(&setup, &alice, &bob, chat_id).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn resync_finishes_when_commit_response_is_lost() {
    let mut setup = TestBackend::single().await;
    let [alice, bob, charlie] = setup.add_users().await;
    let chat_id = setup.connected_group(&alice, &[&bob, &charlie]).await;

    make_miss_a_removal(&alice, &bob, &charlie, chat_id).await;
    setup
        .listener_control_handle()
        .set_drop_next_response_to(resync_method(&setup));
    bob.outbound_service().run_once().await;
    assert_eq!(
        resync_status(&bob, chat_id).await,
        Some(ResyncStatus::Pending)
    );
    let accepted = epoch_authenticator(&bob, chat_id).await;

    bob.outbound_service().run_once().await;
    assert_eq!(resync_status(&bob, chat_id).await, None);
    assert_eq!(epoch_authenticator(&bob, chat_id).await, accepted);
    assert_eq!(removal_messages(&bob, chat_id, &charlie).await, 1);
    assert_messaging_works(&setup, &alice, &bob, chat_id).await;
}

/// A lost response is recognized from the fetched state also when the resend
/// fails the DS's sender check rather than its epoch check, because the commit
/// moved Bob's leaf.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn resync_finishes_lost_response_when_leaf_moved() {
    let mut setup = TestBackend::single().await;
    let [alice, charlie, bob] = setup.add_users().await;
    let chat_id = setup.connected_group(&alice, &[&charlie, &bob]).await;
    setup
        .remove_from_group(chat_id, &alice, &[&charlie])
        .await
        .unwrap();
    let queued_index = own_index(&bob, chat_id).await;

    make_miss_a_commit(&alice, &bob, chat_id).await;
    setup
        .listener_control_handle()
        .set_drop_next_response_to(resync_method(&setup));
    bob.outbound_service().run_once().await;
    assert_eq!(
        resync_status(&bob, chat_id).await,
        Some(ResyncStatus::Pending)
    );
    assert!(own_index(&bob, chat_id).await < queued_index);
    let accepted = epoch_authenticator(&bob, chat_id).await;

    bob.outbound_service().run_once().await;
    assert_eq!(resync_status(&bob, chat_id).await, None);
    assert_eq!(epoch_authenticator(&bob, chat_id).await, accepted);
    assert_messaging_works(&setup, &alice, &bob, chat_id).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn resync_rebuilds_when_stored_commit_misses_self_remove() {
    let mut setup = TestBackend::single().await;
    let [alice, bob, charlie, dave] = setup.add_users().await;
    let chat_id = setup
        .connected_group(&alice, &[&bob, &charlie, &dave])
        .await;

    make_miss_a_removal(&alice, &bob, &dave, chat_id).await;
    charlie.drain().await;
    let attempted = drop_resync_request(&setup, &bob, chat_id).await;

    charlie.leave_chat(chat_id).await.unwrap();
    miss_queued_messages(&bob).await;

    bob.outbound_service().run_once().await;
    assert_eq!(resync_status(&bob, chat_id).await, None);
    assert_ne!(epoch_authenticator(&bob, chat_id).await, attempted);
    assert!(
        !bob.group_members(chat_id)
            .await
            .unwrap()
            .contains(charlie.user_id())
    );
    // The rebuild reports the changes since the state before the first
    // attempt, which still had Dave.
    assert_eq!(removal_messages(&bob, chat_id, &dave).await, 1);
    assert_eq!(removal_messages(&bob, chat_id, &charlie).await, 1);
    assert_messaging_works(&setup, &alice, &bob, chat_id).await;
}

/// Alice's commit takes the epoch the stored commit would produce. Only the
/// transcript hash tells the two states apart, so the resync rebuilds instead
/// of finishing.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn resync_rebuilds_when_another_commit_won() {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    let chat_id = setup.connected_group(&alice, &[&bob]).await;

    make_miss_a_commit(&alice, &bob, chat_id).await;
    let attempted = drop_resync_request(&setup, &bob, chat_id).await;

    alice.update_key(chat_id).await.unwrap();
    miss_queued_messages(&bob).await;

    bob.outbound_service().run_once().await;
    assert_eq!(resync_status(&bob, chat_id).await, None);
    assert_ne!(epoch_authenticator(&bob, chat_id).await, attempted);
    assert_messaging_works(&setup, &alice, &bob, chat_id).await;
}

/// A rebuild is checked against the state the first attempt joined, not
/// against the older state the DS last accepted from this member.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn resync_rebuild_rejects_snapshot_older_than_attempt() {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    let chat_id = setup.connected_group(&alice, &[&bob]).await;
    let group_uuid = group_uuid(&bob, chat_id).await;

    alice.update_key(chat_id).await.unwrap();
    miss_queued_messages(&bob).await;
    // Newer than Bob's state, but older than the one his first attempt joins.
    let older_row = setup.ds_group_row(group_uuid).await;
    alice.update_key(chat_id).await.unwrap();
    bob.drain().await;
    assert_eq!(
        resync_status(&bob, chat_id).await,
        Some(ResyncStatus::Pending)
    );

    let attempted = drop_resync_request(&setup, &bob, chat_id).await;

    let current_row = setup.ds_group_row(group_uuid).await;
    setup.set_ds_group_row(group_uuid, &older_row).await;
    bob.outbound_service().run_once().await;
    assert_eq!(
        resync_status(&bob, chat_id).await,
        Some(ResyncStatus::Failed)
    );
    assert_eq!(epoch_authenticator(&bob, chat_id).await, attempted);

    setup.set_ds_group_row(group_uuid, &current_row).await;
    bob.enqueue_group_resync(chat_id).await.unwrap();
    bob.outbound_service().run_once().await;
    assert_eq!(resync_status(&bob, chat_id).await, None);
    assert_eq!(epoch_authenticator(&bob, chat_id).await, attempted);
    assert_messaging_works(&setup, &alice, &bob, chat_id).await;
}

/// With a stored commit, a verified state without Bob's leaf only tears his
/// group down if it is newer than the state the commit was built on.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn resync_with_stored_commit_tears_down_only_on_newer_state() {
    let mut setup = TestBackend::single().await;
    let [alice, bob, charlie] = setup.add_users().await;
    setup.connect_users(&alice, &bob).await;
    setup.connect_users(&alice, &charlie).await;
    let chat_id = setup.create_group(&alice).await;
    setup
        .invite_to_group(chat_id, &alice, &[&bob, &charlie])
        .await;
    let group_uuid = group_uuid(&bob, chat_id).await;

    let row_before_fork = setup.ds_group_row(group_uuid).await;
    charlie.update_key(chat_id).await.unwrap();
    miss_queued_messages(&alice).await;
    bob.drain().await;

    bob.enqueue_group_resync(chat_id).await.unwrap();
    let attempted = drop_resync_request(&setup, &bob, chat_id).await;

    // Alice forks the group at that epoch by removing Bob from the state before
    // it.
    setup.set_ds_group_row(group_uuid, &row_before_fork).await;
    alice
        .remove_users(chat_id, vec![bob.user_id().clone()])
        .await
        .unwrap();

    bob.outbound_service().run_once().await;
    assert_eq!(
        resync_status(&bob, chat_id).await,
        Some(ResyncStatus::Failed)
    );
    assert_eq!(epoch_authenticator(&bob, chat_id).await, attempted);
    assert!(!bob.chat(&chat_id).await.unwrap().status().is_inactive());

    alice.update_key(chat_id).await.unwrap();
    bob.enqueue_group_resync(chat_id).await.unwrap();
    bob.outbound_service().run_once().await;
    assert_eq!(resync_status(&bob, chat_id).await, None);
    assert!(bob.chat(&chat_id).await.unwrap().status().is_inactive());
}

/// A group closed reply to a resend is not verified, so it doesn't tear the
/// group down. The fetched state decides, which here shows the group open.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn group_closed_reply_to_resend_does_not_tear_down() {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    let chat_id = setup.connected_group(&alice, &[&bob]).await;

    make_miss_a_commit(&alice, &bob, chat_id).await;
    drop_resync_request(&setup, &bob, chat_id).await;

    setup
        .listener_control_handle()
        .set_close_next_request_to(resync_method(&setup));
    bob.outbound_service().run_once().await;
    assert_eq!(resync_status(&bob, chat_id).await, None);
    assert!(!bob.chat(&chat_id).await.unwrap().status().is_inactive());
    assert_messaging_works(&setup, &alice, &bob, chat_id).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn manual_resync_resends_after_rejected_commit() {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    let chat_id = setup.connected_group(&alice, &[&bob]).await;

    make_miss_a_commit(&alice, &bob, chat_id).await;
    setup
        .listener_control_handle()
        .set_reject_next_request_to(resync_method(&setup));
    bob.outbound_service().run_once().await;
    assert_eq!(
        resync_status(&bob, chat_id).await,
        Some(ResyncStatus::Failed)
    );
    let attempted = epoch_authenticator(&bob, chat_id).await;

    bob.enqueue_group_resync(chat_id).await.unwrap();
    bob.outbound_service().run_once().await;
    assert_eq!(resync_status(&bob, chat_id).await, None);
    assert_eq!(epoch_authenticator(&bob, chat_id).await, attempted);
    assert_messaging_works(&setup, &alice, &bob, chat_id).await;
}

/// With a stored commit, a verified state that closes the group doesn't tear it
/// down at the epoch the commit was built on, also when it keeps our leaf.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn resync_with_stored_commit_refuses_closed_state_at_same_epoch() {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    let chat_id = setup.connected_group(&alice, &[&bob]).await;
    let alice2 = alice.add_synced_device().await;
    bob.drain().await;
    let group_uuid = group_uuid(&alice2, chat_id).await;

    let row_before_fork = setup.ds_group_row(group_uuid).await;
    bob.update_key(chat_id).await.unwrap();
    miss_queued_messages(&alice).await;
    alice2.drain().await;

    alice2.enqueue_group_resync(chat_id).await.unwrap();
    let attempted = drop_resync_request(&setup, &alice2, chat_id).await;

    // Alice's first device forks the group at that epoch by deleting it from the
    // state before it. The deletion keeps the leaf both devices share.
    setup.set_ds_group_row(group_uuid, &row_before_fork).await;
    alice.delete_chat(chat_id).await.unwrap();

    alice2.outbound_service().run_once().await;
    assert_eq!(
        resync_status(&alice2, chat_id).await,
        Some(ResyncStatus::Failed)
    );
    assert_eq!(epoch_authenticator(&alice2, chat_id).await, attempted);
    assert!(!alice2.chat(&chat_id).await.unwrap().status().is_inactive());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn resync_with_stored_commit_closes_group_on_newer_state() {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    let chat_id = setup.connected_group(&alice, &[&bob]).await;
    let alice2 = alice.add_synced_device().await;

    alice2.enqueue_group_resync(chat_id).await.unwrap();
    drop_resync_request(&setup, &alice2, chat_id).await;
    alice.delete_chat(chat_id).await.unwrap();

    alice2.outbound_service().run_once().await;
    assert_eq!(resync_status(&alice2, chat_id).await, None);
    assert!(alice2.chat(&chat_id).await.unwrap().status().is_inactive());
    let deletions = alice2
        .system_messages(chat_id)
        .await
        .into_iter()
        .filter(|message| *message == SystemMessage::DeleteGroup(alice.user_id().clone()))
        .count();
    assert_eq!(deletions, 1);
}

async fn chat_title(user: &CoreUser, chat_id: ChatId) -> String {
    user.chat(&chat_id)
        .await
        .unwrap()
        .attributes()
        .unwrap()
        .title()
        .to_owned()
}

/// While the resync of a chat is pending, the operations a user starts in it
/// fail without leaving anything behind, and work again once it is done.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn group_operations_fail_while_resync_is_pending() {
    let mut setup = TestBackend::single().await;
    let [alice, bob, charlie] = setup.add_users().await;
    let chat_id = setup.connected_group(&alice, &[&bob]).await;
    setup.connect_users(&bob, &charlie).await;

    make_miss_a_commit(&alice, &bob, chat_id).await;
    drop_resync_request(&setup, &bob, chat_id).await;
    let title = chat_title(&bob, chat_id).await;

    assert!(
        bob.set_chat_title(chat_id, "renamed".to_owned())
            .await
            .is_err()
    );
    assert!(
        bob.invite_users(chat_id, slice::from_ref(charlie.user_id()))
            .await
            .is_err()
    );
    assert!(bob.leave_chat(chat_id).await.is_err());
    assert!(bob.delete_and_erase_chat(chat_id).await.is_err());
    assert!(
        bob.pending_chat_operation_info(chat_id)
            .await
            .unwrap()
            .is_none()
    );
    let chat = bob.chat(&chat_id).await.expect("the chat should be kept");
    assert_eq!(chat.status(), &ChatStatus::Active);
    assert_eq!(chat_title(&bob, chat_id).await, title);

    bob.outbound_service().run_once().await;
    assert_eq!(resync_status(&bob, chat_id).await, None);
    bob.set_chat_title(chat_id, "renamed".to_owned())
        .await
        .unwrap();
    bob.invite_users(chat_id, slice::from_ref(charlie.user_id()))
        .await
        .unwrap();
    alice.drain().await;
    assert_eq!(chat_title(&alice, chat_id).await, "renamed");
    assert!(
        alice
            .group_members(chat_id)
            .await
            .unwrap()
            .contains(charlie.user_id())
    );
}
