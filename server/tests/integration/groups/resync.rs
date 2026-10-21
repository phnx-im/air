// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Resyncing groups and connection groups via external commit.

use std::{collections::HashSet, slice};

use aircoreclient::{DisplayName, SystemMessage, UserProfile, clients::MarkChatAsRead};
use airserver_test_harness::utils::{core_user_ext::CoreUserExt, setup::TestBackend};
use mimi_content::MimiContent;

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn resync() {
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
async fn resync_group_not_found_cleans_up_local_state() {
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
async fn resync_valid_group_succeeds() {
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
async fn resync_with_blank_leaf_succeeds() {
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
async fn resync_connection_group() {
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
