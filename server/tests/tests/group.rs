// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::slice;

use aircoreclient::{
    ChatStatus, DisplayName, EventMessage, Message, MessageDraft, SystemMessage, UserProfile,
    clients::{
        MarkChatAsRead,
        process::qs_stream::{QsProcessEventResult, QsStreamProcessor},
    },
};
use airserver_test_harness::utils::{
    core_user_ext::CoreUserExt,
    setup::{TestBackend, expected_display_name},
};
use chrono::{DateTime, Duration, Utc};
use mimi_content::MimiContent;
use tokio_stream::StreamExt;
use tracing::info;

use super::attachment::test_picture_bytes;

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn create_group() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    setup.create_group(&alice).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn create_empty_group() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let chat_id = setup.create_group(&alice).await;

    let alice_user = setup.user(&alice);

    // Inviting an empty user list should be a no-op
    let result = alice_user
        .invite_users(chat_id, &[])
        .await
        .expect("inviting an empty user list should succeed");
    assert!(result.messages.is_empty());
    assert!(result.users_not_added.is_empty());

    let participants = alice_user.chat_participants(chat_id).await.unwrap();
    assert_eq!(participants.len(), 1);
    assert!(participants.contains(&alice));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn invite_to_group() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let bob = setup.add_user().await;
    let charlie = setup.add_user().await;
    setup.connect_users(&alice, &bob).await;
    setup.connect_users(&alice, &charlie).await;
    let chat_id = setup.create_group(&alice).await;
    setup
        .invite_to_group(chat_id, &alice, &[&bob, &charlie])
        .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn update_group() {
    let mut setup = TestBackend::single().await;
    info!("Adding users");
    let alice = setup.add_user().await;
    let bob = setup.add_user().await;
    let charlie = setup.add_user().await;
    info!("Connecting users");
    setup.connect_users(&alice, &bob).await;
    setup.connect_users(&alice, &charlie).await;
    let chat_id = setup.create_group(&alice).await;
    info!("Inviting to group");
    setup
        .invite_to_group(chat_id, &alice, &[&bob, &charlie])
        .await;
    info!("Updating group");
    setup.update_group(chat_id, &bob).await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn remove_from_group() {
    let mut setup = TestBackend::single().await;

    let alice = setup.add_user().await;
    let bob = setup.add_user().await;
    let charlie = setup.add_user().await;
    let dave = setup.add_user().await;

    setup.connect_users(&alice, &bob).await;
    setup.connect_users(&alice, &charlie).await;
    setup.connect_users(&alice, &dave).await;
    let chat_id = setup.create_group(&alice).await;
    setup
        .invite_to_group(chat_id, &alice, &[&bob, &charlie, &dave])
        .await;
    // Check that Charlie has a user profile stored for bob, even though
    // he hasn't connected with them.
    let charlie_user = setup.user(&charlie);
    let charlie_user_profile_bob = charlie_user.user_profile(&bob).await;
    assert!(charlie_user_profile_bob.user_id == bob);

    setup
        .remove_from_group(chat_id, &alice, &[&bob])
        .await
        .unwrap();

    // Now that charlie is not in a group with Bob anymore, the user profile
    // should be the default one derived from the client id.
    let charlie_user_profile_bob = charlie_user.user_profile(&bob).await;
    assert_eq!(charlie_user_profile_bob, UserProfile::from_user_id(&bob));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn re_add_client() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let bob = setup.add_user().await;
    setup.connect_users(&alice, &bob).await;
    let chat_id = setup.create_group(&alice).await;
    setup.invite_to_group(chat_id, &alice, &[&bob]).await;
    for _ in 0..10 {
        setup
            .remove_from_group(chat_id, &alice, &[&bob])
            .await
            .unwrap();
        setup.invite_to_group(chat_id, &alice, &[&bob]).await;
    }
    setup.send_message(chat_id, &alice, &[&bob]).await;
    setup.send_message(chat_id, &bob, &[&alice]).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn leave_group() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let bob = setup.add_user().await;
    setup.connect_users(&alice, &bob).await;
    let chat_id = setup.create_group(&alice).await;
    setup.invite_to_group(chat_id, &alice, &[&bob]).await;
    setup.leave_group(chat_id, &bob).await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn delete_non_apq_group() {
    let mut setup = TestBackend::single().await;

    let alice = setup.add_user().await;
    let bob = setup.add_user().await;
    let charlie = setup.add_user().await;
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

    let alice = setup.add_user().await;
    let bob = setup.add_user().await;
    let charlie = setup.add_user().await;
    setup.connect_users(&alice, &bob).await;
    setup.connect_users(&alice, &charlie).await;
    let chat_id = setup.create_apq_group(&alice).await;
    setup
        .invite_to_group(chat_id, &alice, &[&bob, &charlie])
        .await;
    setup.delete_group(chat_id, &alice).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn delete_apq_group_by_invitee() {
    let mut setup = TestBackend::single().await;

    let alice = setup.add_user().await;
    let bob = setup.add_user().await;
    let charlie = setup.add_user().await;
    setup.connect_users(&alice, &bob).await;
    setup.connect_users(&alice, &charlie).await;
    let chat_id = setup.create_apq_group(&alice).await;
    setup
        .invite_to_group(chat_id, &alice, &[&bob, &charlie])
        .await;
    setup.delete_group(chat_id, &bob).await;
}

fn removals(messages: &[SystemMessage]) -> usize {
    messages
        .iter()
        .filter(|message| matches!(message, SystemMessage::Remove(_remover, _removed)))
        .count()
}

fn count(messages: &[SystemMessage], expected: &SystemMessage) -> usize {
    messages
        .iter()
        .filter(|message| *message == expected)
        .count()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn delete_group_as_sole_member() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let chat_id = setup.create_group(&alice).await;

    let alice_user = setup.user(&alice);
    assert!(alice_user.ds_has_group(chat_id).await.unwrap());

    alice_user.delete_chat(chat_id).await.unwrap();
    let messages = alice_user.system_messages(chat_id).await;
    assert_eq!(
        count(&messages, &SystemMessage::DeleteGroup(alice.clone())),
        1,
        "got {messages:?}"
    );
    assert!(alice_user.expect_chat(chat_id).await.status().is_inactive());
    assert!(
        !alice_user.ds_has_group(chat_id).await.unwrap(),
        "the DS deletes the group of a sole member"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn missed_delete_commit_then_resync() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let bob = setup.add_user().await;
    setup.connect_users(&alice, &bob).await;
    let chat_id = setup.create_group(&alice).await;
    setup.invite_to_group(chat_id, &alice, &[&bob]).await;

    let alice_user = setup.user(&alice);
    alice_user.delete_chat(chat_id).await.unwrap();

    // Bob misses the delete commit and only gets to it after the resync.
    let bob_user = setup.user(&bob);
    let late_messages = bob_user.qs_fetch_messages().await.unwrap();
    assert_eq!(
        bob_user.expect_chat(chat_id).await.status(),
        &ChatStatus::Active
    );
    let participants = bob_user.chat_participants(chat_id).await.unwrap();

    bob_user.enqueue_group_resync(chat_id).await.unwrap();
    bob_user.outbound_service().run_once().await;

    assert!(bob_user.expect_chat(chat_id).await.status().is_inactive());
    let messages = bob_user.system_messages(chat_id).await;
    assert_eq!(
        count(&messages, &SystemMessage::DeleteGroup(alice.clone())),
        1,
        "got {messages:?}"
    );
    assert_eq!(removals(&messages), 0, "got {messages:?}");
    assert!(
        bob_user.resync_status(chat_id).await.unwrap().is_none(),
        "the resync entry is gone"
    );
    assert!(
        bob_user.mls_members(chat_id).await.unwrap().is_none(),
        "the MLS group is gone"
    );
    assert_eq!(
        bob_user.chat_participants(chat_id).await,
        Some(participants),
        "the chat still lists its members"
    );

    bob_user.fully_process_qs_messages(late_messages).await;
    let messages = bob_user.system_messages(chat_id).await;
    assert_eq!(
        count(&messages, &SystemMessage::DeleteGroup(alice.clone())),
        1,
        "the late delete commit adds nothing: {messages:?}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn missed_removal_then_resync() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let bob = setup.add_user().await;
    setup.connect_users(&alice, &bob).await;
    let chat_id = setup.create_group(&alice).await;
    setup.invite_to_group(chat_id, &alice, &[&bob]).await;

    let alice_user = setup.user(&alice);
    alice_user
        .remove_users(chat_id, vec![bob.clone()])
        .await
        .unwrap();

    let bob_user = setup.user(&bob);
    bob_user.discard_queue().await;
    let participants = bob_user.chat_participants(chat_id).await.unwrap();
    bob_user.enqueue_group_resync(chat_id).await.unwrap();
    bob_user.outbound_service().run_once().await;

    assert!(bob_user.expect_chat(chat_id).await.status().is_inactive());
    assert!(
        bob_user.resync_status(chat_id).await.unwrap().is_none(),
        "the resync entry is gone"
    );
    assert_eq!(
        bob_user.chat_participants(chat_id).await,
        Some(participants),
        "the chat still lists its members"
    );
    let messages = bob_user.system_messages(chat_id).await;
    assert_eq!(
        count(&messages, &SystemMessage::DeleteGroup(alice.clone())),
        0,
        "got {messages:?}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn missed_deletion_without_tombstone_then_resync() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let bob = setup.add_user().await;
    let charlie = setup.add_user().await;
    setup.connect_users(&alice, &bob).await;
    setup.connect_users(&alice, &charlie).await;
    let chat_id = setup.create_group(&alice).await;
    setup
        .invite_to_group(chat_id, &alice, &[&bob, &charlie])
        .await;

    let alice_user = setup.user(&alice);
    alice_user
        .delete_chat_without_tombstone(chat_id)
        .await
        .unwrap();

    let bob_user = setup.user(&bob);
    bob_user.discard_queue().await;
    bob_user.enqueue_group_resync(chat_id).await.unwrap();
    bob_user.outbound_service().run_once().await;

    assert!(bob_user.expect_chat(chat_id).await.status().is_inactive());
    assert!(
        bob_user.resync_status(chat_id).await.unwrap().is_none(),
        "the resync entry is gone"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn delete_group_without_tombstone() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let bob = setup.add_user().await;
    setup.connect_users(&alice, &bob).await;
    let chat_id = setup.create_group(&alice).await;
    setup.invite_to_group(chat_id, &alice, &[&bob]).await;

    let bobs_removal = SystemMessage::Remove(Some(alice.clone()), bob.clone());
    let deletions =
        |messages: &[SystemMessage]| count(messages, &SystemMessage::DeleteGroup(alice.clone()));

    // Alice deletes the group as clients before the tombstone do.
    let alice_user = setup.user(&alice);
    alice_user
        .delete_chat_without_tombstone(chat_id)
        .await
        .unwrap();
    assert!(alice_user.expect_chat(chat_id).await.status().is_inactive());
    let messages = alice_user.system_messages(chat_id).await;
    assert_eq!(count(&messages, &bobs_removal), 1, "got {messages:?}");
    assert_eq!(deletions(&messages), 0, "got {messages:?}");

    let bob_user = setup.user(&bob);
    bob_user.drain().await;
    assert!(bob_user.expect_chat(chat_id).await.status().is_inactive());
    let messages = bob_user.system_messages(chat_id).await;
    assert_eq!(count(&messages, &bobs_removal), 1, "got {messages:?}");
    assert_eq!(deletions(&messages), 0, "got {messages:?}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn store_draft_after_delete() {
    let mut setup = TestBackend::single().await;

    let alice = setup.add_user().await;
    let chat_id = setup.create_apq_group(&alice).await;

    let alice_user = setup.user(&alice);
    let _ = alice_user.delete_chat(chat_id).await;
    alice_user
        .erase_chat(chat_id)
        .await
        .expect("erasing an APQ chat must not fail");

    // The compose field's draft is persisted after the chat has been erased.
    // This must not trip the `message_draft -> chat` foreign key.
    alice_user
        .store_message_draft(chat_id, Some(&MessageDraft::empty()))
        .await
        .expect("storing a draft for a deleted chat must not fail");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn room_policy() {
    let mut setup = TestBackend::single().await;

    // Create alice and bob
    let alice = setup.add_user().await;
    let bob = setup.add_user().await;
    let charlie = setup.add_user().await;

    // Connect them
    let _chat_alice_bob = setup.connect_users(&alice, &bob).await;
    let _chat_alice_charlie = setup.connect_users(&alice, &charlie).await;
    let _chat_bob_charlie = setup.connect_users(&bob, &charlie).await;

    // Create an independent group and invite bob.
    let chat_id = setup.create_group(&alice).await;

    setup.invite_to_group(chat_id, &alice, &[&bob]).await;

    // Bob can invite charlie
    setup.invite_to_group(chat_id, &bob, &[&charlie]).await;

    // Charlie can kick alice
    setup
        .remove_from_group(chat_id, &charlie, &[&alice])
        .await
        .unwrap();

    // Charlie can kick bob
    setup
        .remove_from_group(chat_id, &charlie, &[&bob])
        .await
        .unwrap();

    // TODO: This currently fails
    // Charlie can leave and an empty room remains
    // setup.leave_group(chat_id, &charlie).await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn update_user_profile_on_group_join() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let bob = setup.add_user().await;
    let charlie = setup.add_user().await;

    // Alice and Bob are connected.
    let _alice_bob_chat = setup.connect_users(&alice, &bob).await;
    // Bob and Charlie are connected.
    let _bob_charlie_chat = setup.connect_users(&bob, &charlie).await;

    // Alice updates her profile.
    let alice_display_name: DisplayName = "4l1c3".parse().unwrap();
    let alice_profile = UserProfile {
        user_id: alice.clone(),
        display_name: alice_display_name.clone(),
        profile_picture: None,
    };
    setup
        .user(&alice)
        .set_own_user_profile(alice_profile)
        .await
        .unwrap();

    // Bob doesn't fetch his queue, so he doesn't know about Alice's new profile.
    // He creates a group and invites Charlie.
    let chat_id = setup.create_group(&bob).await;

    let bob_user = setup.user(&bob);
    bob_user
        .invite_users(chat_id, slice::from_ref(&charlie))
        .await
        .unwrap();

    // Charlie accepts the invitation.
    let charlie_user = setup.user(&charlie);
    charlie_user.drain().await;

    // Bob now invites Alice
    bob_user
        .invite_users(chat_id, slice::from_ref(&alice))
        .await
        .unwrap();

    // Charlie processes his messages again, fetching Alice's profile will fail because it tries to
    // download Alice's old profile.
    charlie_user.qs_fetch_messages().await.unwrap();
    charlie_user.outbound_service().run_once().await;

    let alice_profile = charlie_user.user_profile(&alice).await;
    assert_eq!(
        alice_profile,
        UserProfile::from_user_id(&alice),
        "Fetching Alice's profile should have failed"
    );

    // Alice accepts the invitation.
    let alice_user = setup.user(&alice);
    alice_user.drain().await;

    // While processing her messages, Alice should have issued a profile update

    // Charlie picks up his messages.
    charlie_user.drain().await;
    charlie_user.outbound_service().run_once().await;

    // Charlie should now have Alice's new profile.
    let charlie_user_profile = charlie_user.user_profile(&alice).await;
    assert_eq!(charlie_user_profile.display_name, alice_display_name);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn group_with_blocked_contact() {
    let mut setup = TestBackend::single().await;

    let alice = setup.add_user().await;
    setup.add_username(&alice).await;

    let bob = setup.add_user().await;
    let charlie = setup.add_user().await;

    setup.connect_users(&alice, &bob).await;
    setup.connect_users(&alice, &charlie).await;

    // Create a group with alice, bob and charlie
    let chat_id = setup.create_group(&alice).await;
    setup
        .invite_to_group(chat_id, &alice, &[&bob, &charlie])
        .await;

    // Sending messages works before blocking
    setup.send_message(chat_id, &alice, &[&bob, &charlie]).await;
    setup.send_message(chat_id, &bob, &[&alice, &charlie]).await;

    // Block bob
    let alice_user = setup.user(&alice);
    alice_user.block_contact(bob.clone()).await.unwrap();

    // Messages are still sent and received
    setup.send_message(chat_id, &bob, &[&alice, &charlie]).await;
    setup.send_message(chat_id, &alice, &[&bob, &charlie]).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn send_message() {
    info!("Setting up setup");
    let mut setup = TestBackend::single().await;
    info!("Creating users");
    let alice = setup.add_user().await;
    let bob = setup.add_user().await;
    let chat_id = setup.connect_users(&alice, &bob).await;
    setup.send_message(chat_id, &alice, &[&bob]).await;
    setup.send_message(chat_id, &bob, &[&alice]).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn update_group_data() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let bob = setup.add_user().await;
    let charlie = setup.add_user().await;

    let _alice_bob_chat = setup.connect_users(&alice, &bob).await;
    let _alice_charlie_chat = setup.connect_users(&alice, &charlie).await;

    // Alice creates a group and invites Bob and Charlie
    let chat_id = setup.create_group(&alice).await;
    setup
        .invite_to_group(chat_id, &alice, &[&bob, &charlie])
        .await;

    // Alice updates the group picture
    let alice_user = setup.user(&alice);
    let picture = test_picture_bytes();
    alice_user
        .set_chat_picture(chat_id, Some(picture.clone()))
        .await
        .unwrap();

    let expected_picture = alice_user
        .expect_chat(chat_id)
        .await
        .attributes()
        .unwrap()
        .picture()
        .unwrap()
        .to_owned();

    // Bob and Charlie should now have the updated group picture
    for user_id in [&bob, &charlie] {
        let user = setup.user(user_id);
        // Fetch and process messages to get the update
        user.drain().await;
        user.outbound_service().run_once().await;
        let actual_picture = user
            .expect_chat(chat_id)
            .await
            .attributes()
            .unwrap()
            .picture()
            .unwrap()
            .to_owned();
        assert_eq!(actual_picture, expected_picture);
    }

    // Now Bob updates the group title
    let title = "New Group Title".to_string();
    let bob_user = setup.user(&bob);
    bob_user
        .set_chat_title(chat_id, title.clone())
        .await
        .unwrap();

    for user_id in [&alice, &charlie] {
        let user = setup.user(user_id);
        // Fetch and process messages to get the update
        user.drain().await;
        let actual_title = user
            .expect_chat(chat_id)
            .await
            .attributes()
            .unwrap()
            .title
            .clone();
        assert_eq!(actual_title, title);
    }
}

/// Regression test for the `OwnPendingCommit` merge path.
///
/// When we commit (here: a title change), the DS both sends a `DsCommitResponse`
/// and echoes the commit back to us via fanout. If the echo is processed before
/// the inline merge (a real race, guarded by `is_pending_for_chat`), it hits the
/// `OwnPendingCommit` arm instead of `DsCommitResponse`. That arm must apply the
/// same side effects: update the chat title carried by the commit and delete the
/// pending chat operation. This test drives that arm directly by staging the
/// commit without merging and feeding the echo back through the QS path.
///
/// This is what would happen in the scenario of virtual-clients.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn own_pending_commit_merges_group_data_and_clears_pending_op() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let bob = setup.add_user().await;

    let _alice_bob_chat = setup.connect_users(&alice, &bob).await;
    let chat_id = setup.create_group(&alice).await;
    setup.invite_to_group(chat_id, &alice, &[&bob]).await;

    let alice_user = setup.user(&alice);

    // Stage Alice's title-change commit WITHOUT merging it. This leaves a
    // pending commit and a stored pending chat operation, and hands back the
    // commit bytes the DS would echo back to Alice.
    let title = "Raced Title".to_string();
    let echo = alice_user
        .stage_group_title_commit(chat_id, title.clone())
        .await
        .unwrap();

    // Preconditions: pending op exists, title not yet applied.
    assert!(
        alice_user
            .pending_chat_operation_info(chat_id)
            .await
            .unwrap()
            .is_some(),
        "a pending chat operation should be stored before the merge",
    );
    assert_ne!(
        alice_user
            .expect_chat(chat_id)
            .await
            .attributes()
            .unwrap()
            .title,
        title,
        "title must not be applied before the commit is merged",
    );

    // Feed Alice her own commit back through the QS path while the commit is
    // still pending -> hits the `OwnPendingCommit` arm.
    alice_user
        .process_incoming_mls_message(&echo)
        .await
        .unwrap();

    // The arm must apply the group data (title) ...
    assert_eq!(
        alice_user
            .expect_chat(chat_id)
            .await
            .attributes()
            .unwrap()
            .title,
        title,
        "OwnPendingCommit must apply the group data carried by the commit",
    );
    // ... and delete the pending chat operation.
    assert!(
        alice_user
            .pending_chat_operation_info(chat_id)
            .await
            .unwrap()
            .is_none(),
        "OwnPendingCommit must delete the pending chat operation",
    );
}

/// Tests that after being invited to a group, the invitee fetches the encrypted group profile from
/// object storage via the outbound service and sees the correct group attributes (title and
/// picture).
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn fetch_group_profile_on_invite() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let bob = setup.add_user().await;
    setup.connect_users(&alice, &bob).await;

    let chat_id = setup.create_group(&alice).await;

    let alice_user = setup.user(&alice);
    alice_user
        .set_chat_picture(chat_id, Some(test_picture_bytes()))
        .await
        .unwrap();

    // Record Alice's group attributes (title and resized picture)
    let alice_chat = alice_user.expect_chat(chat_id).await;
    let attributes = alice_chat.attributes().unwrap().clone();
    let expected_title = attributes.title;
    let expected_picture = attributes.picture;

    // Alice invites Bob; the encrypted group profile is already uploaded from group creation
    alice_user
        .invite_users(chat_id, slice::from_ref(&bob))
        .await
        .unwrap();

    // Bob processes the invitation: this schedules a FetchGroupProfileOperation
    let bob_user = setup.user(&bob);
    bob_user.drain().await;

    // Bob sees the group title immediately after processing the invitation, but not the picture
    let bob_chat = bob_user.expect_chat(chat_id).await;
    assert_eq!(bob_chat.attributes().unwrap().title(), &expected_title);

    // Bob runs the outbound service: this executes FetchGroupProfileOperation,
    // downloading and decrypting the encrypted group profile from object storage
    bob_user.outbound_service().run_once().await;

    // Bob should have the correct group attributes after fetching the encrypted profile
    let bob_chat = bob_user.expect_chat(chat_id).await;
    assert_eq!(bob_chat.attributes().unwrap().title(), &expected_title);
    assert_eq!(bob_chat.attributes().unwrap().picture, expected_picture);

    // Regression: joining the group should not surface spurious "title changed" or "picture
    // changed" system messages: the title is unchanged and the picture is just the initial
    // download.
    let mut bob_messages = bob_user.messages(chat_id, 1024).await.unwrap();
    // Only keep system messages about the title or picture change
    bob_messages.retain(|message| {
        matches!(
            message.message(),
            Message::Event(EventMessage::System(
                SystemMessage::ChangeTitle { .. } | SystemMessage::ChangePicture(_),
            ))
        )
    });
    assert!(
        bob_messages.is_empty(),
        "unexpected system messages on initial join: {bob_messages:#?}"
    );
}

/// Tests that after a group title and picture update, other members fetch the new encrypted group
/// profile via the outbound service and see the updated attributes.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn fetch_group_profile_on_update() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let bob = setup.add_user().await;
    setup.connect_users(&alice, &bob).await;

    let chat_id = setup.create_group(&alice).await;
    setup.invite_to_group(chat_id, &alice, &[&bob]).await;

    let chat = setup.user(&alice).expect_chat(chat_id).await;
    let title = chat.attributes().unwrap().title().to_owned();

    // Bob sees the group title of group immediately
    let bob_user = setup.user(&bob);
    let bob_chat = bob_user.expect_chat(chat_id).await;
    assert_eq!(bob_chat.attributes().unwrap().title(), &title);

    // Alice updates the group title and picture
    let new_title = "Updated Group Title".to_string();
    let alice_user = setup.user(&alice);
    alice_user
        .set_chat_title(chat_id, new_title.clone())
        .await
        .unwrap();
    alice_user
        .set_chat_picture(chat_id, Some(test_picture_bytes()))
        .await
        .unwrap();

    // Record Alice's stored picture (may be resized relative to test_picture_bytes)
    let chat = alice_user.expect_chat(chat_id).await;
    let expected_picture = chat.attributes().unwrap().picture.clone();

    // Bob fetches Alice's commits: this schedules FetchGroupProfileOperations
    bob_user.drain().await;

    // Bob runs the outbound service: this executes FetchGroupProfileOperations,
    // downloading and decrypting the new encrypted group profile from object storage
    bob_user.outbound_service().run_once().await;

    // Bob should see the updated title and picture
    let bob_chat = bob_user.expect_chat(chat_id).await;
    assert_eq!(bob_chat.attributes().unwrap().title(), &new_title);
    assert_eq!(
        bob_chat.attributes().unwrap().picture(),
        expected_picture.as_deref()
    );
}

/// This test checks that bob can leave a group where he missed a commit. He leaves the group only
/// locally. Other user can remove the bob correctly (commit is merged), and add him again.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn missed_commit() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let bob = setup.add_user().await;
    let charlie = setup.add_user().await;

    setup.connect_users(&alice, &bob).await;
    setup.connect_users(&alice, &charlie).await;

    // Alice creates a group and invites Bob
    let chat_id = setup.create_group(&alice).await;
    setup.invite_to_group(chat_id, &alice, &[&bob]).await;

    // Alice invites Charlie
    let alice_user = setup.user(&alice);
    alice_user
        .invite_users(chat_id, slice::from_ref(&charlie))
        .await
        .unwrap();

    alice_user.drain().await;

    // Bob misses the invitation commit
    let bob_user = setup.user(&bob);
    bob_user.discard_queue().await;

    // Bob tries to leave the group; this works but only locally
    bob_user.leave_chat(chat_id).await.unwrap();

    // ... and Alice still has 3 members
    alice_user.drain().await;
    assert_eq!(
        alice_user
            .mls_members(chat_id)
            .await
            .unwrap()
            .unwrap()
            .len(),
        3,
        "Alice still has 3 members"
    );

    // ... and Charlie still has 3 members
    let charlie_user = setup.user(&charlie);
    charlie_user.drain().await;
    assert_eq!(
        charlie_user
            .mls_members(chat_id)
            .await
            .unwrap()
            .unwrap()
            .len(),
        3,
        "Charlie still has 3 members"
    );

    // Alice removes Bob and adds him again
    alice_user
        .remove_users(chat_id, vec![bob.clone()])
        .await
        .unwrap();
    alice_user
        .invite_users(chat_id, slice::from_ref(&bob))
        .await
        .unwrap();
    alice_user.drain().await;
    bob_user.drain().await;
    charlie_user.drain().await;

    // Now everyone sees 3 members
    assert_eq!(
        alice_user
            .mls_members(chat_id)
            .await
            .unwrap()
            .unwrap()
            .len(),
        3,
        "Alice has 3 members"
    );
    assert_eq!(
        bob_user.mls_members(chat_id).await.unwrap().unwrap().len(),
        3,
        "Bob has 3 members"
    );
    assert_eq!(
        charlie_user
            .mls_members(chat_id)
            .await
            .unwrap()
            .unwrap()
            .len(),
        3,
        "Charlie has 3 members"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn confirmation_via_queue() {
    let mut setup = TestBackend::single().await;

    let alice = setup.add_user().await;
    let bob = setup.add_user().await;

    setup.connect_users(&alice, &bob).await;
    let chat_id = setup.create_group(&alice).await;

    let alice_user = setup.user(&alice);

    // Make server drop connection instead of sending responses
    setup.listener_control_handle().set_drop_next_response();

    let _ = alice_user
        .invite_users(chat_id, &[bob])
        .await
        .expect_err("No error despite server dropping messages");

    // Bob should not be in the group.
    let number_of_members = alice_user
        .mls_members(chat_id)
        .await
        .unwrap()
        .unwrap()
        .len();
    assert_eq!(number_of_members, 1);

    // Set server to normal networking mode
    setup.listener_control_handle().set_normal();

    // At this point, Alice has a pending commit. Once she receives the
    // confirmation from the queue, she should be able to create another commit.
    let qs_messages = alice_user.qs_fetch_messages().await.unwrap();
    println!("Number of QS messages: {}", qs_messages.len());
    alice_user.fully_process_qs_messages(qs_messages).await;

    // Bob should now be in the group.
    let number_of_members = alice_user
        .mls_members(chat_id)
        .await
        .unwrap()
        .unwrap()
        .len();
    assert_eq!(number_of_members, 2);

    alice_user
        .update_key(chat_id)
        .await
        .expect("No error despite server dropping messages");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn self_update() {
    let mut setup = TestBackend::single().await;

    let alice = setup.add_user().await;

    let created_at = Utc::now();

    // Outbound service updates chats in batches of 5
    const UPDATE_BATCH_SIZE: usize = 5;

    let mut chat_ids = Vec::new();
    for _ in 0..UPDATE_BATCH_SIZE + 1 {
        chat_ids.push(setup.create_group(&alice).await);
    }

    let alice_user = setup.user(&alice);

    // Initially, all chats are self-updated at creation time
    let mut self_updated_at = Vec::new();
    for chat_id in &chat_ids {
        let at = alice_user.self_updated_at(*chat_id).await.unwrap().unwrap();
        assert!(
            created_at <= at && at <= created_at + Duration::seconds(10),
            "Self update is not within 10 seconds of now",
        );
        self_updated_at.push(at);
    }

    // Set self_updated_at to the past and schedule a self update
    for chat_id in &chat_ids {
        alice_user
            .set_self_updated_at(*chat_id, DateTime::UNIX_EPOCH)
            .await
            .unwrap();
    }
    // Run the outbound service to update the chats
    alice_user
        .outbound_service()
        .schedule_self_update(DateTime::UNIX_EPOCH)
        .await
        .unwrap();
    alice_user.outbound_service().run_once().await;

    // The outbound service updates the chats in batches of 5
    for (idx, chat_id) in chat_ids.iter().take(UPDATE_BATCH_SIZE).enumerate() {
        let at = alice_user.self_updated_at(*chat_id).await.unwrap().unwrap();
        assert!(self_updated_at[idx] < at, "Self update not happened",);
    }
    assert_eq!(
        alice_user
            .self_updated_at(chat_ids[UPDATE_BATCH_SIZE])
            .await
            .unwrap()
            .unwrap(),
        DateTime::UNIX_EPOCH,
        "Last chat self-updated even though it should not have been"
    );

    // Another run should update the remaining chat
    alice_user
        .outbound_service()
        .schedule_self_update(DateTime::UNIX_EPOCH)
        .await
        .unwrap();
    alice_user.outbound_service().run_once().await;

    let at = alice_user
        .self_updated_at(chat_ids[UPDATE_BATCH_SIZE])
        .await
        .unwrap()
        .unwrap();
    assert!(
        self_updated_at[UPDATE_BATCH_SIZE] < at,
        "Self update not happened"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn self_update_continues_after_a_broken_chat() {
    let mut setup = TestBackend::single().await;

    let alice = setup.add_user().await;
    let broken_chat = setup.create_group(&alice).await;
    let healthy_chat = setup.create_group(&alice).await;

    let alice_user = setup.user(&alice);

    // Chats are updated ordered by their self-update time, so the broken chat comes first.
    alice_user
        .set_self_updated_at(broken_chat, DateTime::UNIX_EPOCH)
        .await
        .unwrap();
    let healthy_updated_at = DateTime::UNIX_EPOCH + Duration::seconds(1);
    alice_user
        .set_self_updated_at(healthy_chat, healthy_updated_at)
        .await
        .unwrap();

    alice_user
        .corrupt_mls_group_state(broken_chat)
        .await
        .unwrap();

    alice_user
        .outbound_service()
        .schedule_self_update(DateTime::UNIX_EPOCH)
        .await
        .unwrap();
    alice_user.outbound_service().run_once().await;

    let at = alice_user
        .self_updated_at(healthy_chat)
        .await
        .unwrap()
        .unwrap();
    assert!(
        healthy_updated_at < at,
        "Chat after the broken one was not self-updated"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn self_update_skips_inactive_chats() {
    let mut setup = TestBackend::single().await;

    let alice = setup.add_user().await;
    let active_chat = setup.create_group(&alice).await;
    let inactive_chat = setup.create_group(&alice).await;

    let alice_user = setup.user(&alice);

    // Leave one chat to make it inactive
    alice_user.leave_chat(inactive_chat).await.unwrap();

    // Set self_updated_at to the past for both chats
    alice_user
        .set_self_updated_at(active_chat, DateTime::UNIX_EPOCH)
        .await
        .unwrap();
    alice_user
        .set_self_updated_at(inactive_chat, DateTime::UNIX_EPOCH)
        .await
        .unwrap();

    // Run the outbound service
    alice_user
        .outbound_service()
        .schedule_self_update(DateTime::UNIX_EPOCH)
        .await
        .unwrap();
    alice_user.outbound_service().run_once().await;

    // The active chat should have been self-updated
    let active_at = alice_user
        .self_updated_at(active_chat)
        .await
        .unwrap()
        .unwrap();
    assert!(
        DateTime::UNIX_EPOCH < active_at,
        "Active chat was not self-updated"
    );

    // The inactive chat should NOT have been self-updated
    assert_eq!(
        alice_user
            .self_updated_at(inactive_chat)
            .await
            .unwrap()
            .unwrap(),
        DateTime::UNIX_EPOCH,
        "Inactive chat was self-updated even though it should not have been"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn qs_stream_processor_partially_processes_messages() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let bob = setup.add_user().await;

    let connection_chat_id = setup.connect_users(&alice, &bob).await;
    let group_chat_id = setup.create_group(&alice).await;
    setup.invite_to_group(group_chat_id, &alice, &[&bob]).await;

    // Remove bob chat only locally on their client
    setup.user(&bob).erase_chat(group_chat_id).await.unwrap();

    let alice_user = setup.user(&alice);

    let content = MimiContent::simple_markdown_message("Hello from Alice!".to_owned(), [0; 16]);

    alice_user
        .send_message(
            connection_chat_id,
            content.clone(),
            None,
            MarkChatAsRead::Yes,
        )
        .await
        .unwrap();
    alice_user
        .send_message(group_chat_id, content.clone(), None, MarkChatAsRead::Yes)
        .await
        .unwrap();
    alice_user
        .send_message(connection_chat_id, content, None, MarkChatAsRead::Yes)
        .await
        .unwrap();
    alice_user.outbound_service().run_once().await;

    let bob_user = setup.user(&bob);

    let (mut stream, responder) = bob_user.listen_queue().await.unwrap();
    let mut processor = QsStreamProcessor::new(Some(responder));

    while let Some(Ok(message)) = stream.next().await {
        match processor.process_event(bob_user, message).await {
            QsProcessEventResult::Accumulated => (),
            QsProcessEventResult::Ignored => (),
            QsProcessEventResult::FullyProcessed { processed } => {
                assert_eq!(processed.processed, 3);
                assert_eq!(processed.errors.len(), 1);
                return;
            }
            QsProcessEventResult::PartiallyProcessed { .. } => unreachable!(),
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn update_group_profile_in_group_with_profile_component() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let bob = setup.add_user().await;
    setup.connect_users(&alice, &bob).await;

    let chat_id = setup
        .create_group_with_profile_component(&alice, setup.apq_groups)
        .await;
    setup.invite_to_group(chat_id, &alice, &[&bob]).await;

    let alice_user = setup.user(&alice);
    let bob_user = setup.user(&bob);
    for user in [alice_user, bob_user] {
        assert!(user.has_group_profile_component(chat_id).await.unwrap());
        // The profile is read from the component, not from the group data extension.
        let group_data = user.group_data(chat_id).await.unwrap().unwrap();
        assert!(group_data.encrypted_title.is_some());
    }

    // Alice updates the title
    let title = "Component Title".to_string();
    alice_user
        .set_chat_title(chat_id, title.clone())
        .await
        .unwrap();
    assert_eq!(
        alice_user
            .expect_chat(chat_id)
            .await
            .attributes()
            .unwrap()
            .title(),
        &title
    );
    assert!(
        alice_user
            .has_group_profile_component(chat_id)
            .await
            .unwrap()
    );

    // Bob sees the new title
    bob_user.drain().await;
    assert_eq!(
        bob_user
            .expect_chat(chat_id)
            .await
            .attributes()
            .unwrap()
            .title(),
        &title
    );

    // Bob updates the picture
    let picture = test_picture_bytes();
    bob_user
        .set_chat_picture(chat_id, Some(picture))
        .await
        .unwrap();
    let expected_picture = bob_user
        .expect_chat(chat_id)
        .await
        .attributes()
        .unwrap()
        .picture()
        .unwrap()
        .to_owned();

    // Alice fetches the new external group profile
    alice_user.drain().await;
    alice_user.outbound_service().run_once().await;
    let alice_chat = alice_user.expect_chat(chat_id).await;
    assert_eq!(alice_chat.attributes().unwrap().title(), &title);
    assert_eq!(
        alice_chat.attributes().unwrap().picture(),
        Some(expected_picture.as_slice())
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn create_apq_group() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    setup.create_apq_group(&alice).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn apq_self_update_updates_both_timestamps() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let chat_id = setup.create_apq_group(&alice).await;

    let alice_user = setup.user(&alice);

    let now = Utc::now();

    // Force both T and PQ self_updated_at to the past so the joint APQ
    // self-update is triggered.
    alice_user
        .set_self_updated_at(chat_id, DateTime::UNIX_EPOCH)
        .await
        .unwrap();
    alice_user
        .set_pq_self_updated_at(chat_id, DateTime::UNIX_EPOCH)
        .await
        .unwrap();

    // Run the outbound service to perform the self-update.
    alice_user
        .outbound_service()
        .schedule_self_update(DateTime::UNIX_EPOCH)
        .await
        .unwrap();
    alice_user.outbound_service().run_once().await;

    // Both T and PQ self_updated_at should now be advanced past now.
    let after_t = alice_user.self_updated_at(chat_id).await.unwrap().unwrap();
    let after_pq = alice_user
        .pq_self_updated_at(chat_id)
        .await
        .unwrap()
        .unwrap();

    assert!(now < after_t, "T self_updated_at was not updated");
    assert!(now < after_pq, "PQ self_updated_at was not updated");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn invite_to_apq_group() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let bob = setup.add_user().await;
    let charlie = setup.add_user().await;
    setup.connect_users(&alice, &bob).await;
    setup.connect_users(&alice, &charlie).await;
    let chat_id = setup.create_apq_group(&alice).await;
    setup
        .invite_to_group(chat_id, &alice, &[&bob, &charlie])
        .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn send_message_in_apq_group() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let bob = setup.add_user().await;
    let charlie = setup.add_user().await;
    setup.connect_users(&alice, &bob).await;
    setup.connect_users(&alice, &charlie).await;
    let chat_id = setup.create_apq_group(&alice).await;
    setup
        .invite_to_group(chat_id, &alice, &[&bob, &charlie])
        .await;
    setup.send_message(chat_id, &alice, &[&bob, &charlie]).await;
    setup.send_message(chat_id, &bob, &[&alice, &charlie]).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn apq_leave_removes_member_from_both_t_and_pq() {
    let mut setup = TestBackend::single().await;

    let alice = setup.add_user().await;
    let bob = setup.add_user().await;
    let charlie = setup.add_user().await;

    setup.connect_users(&alice, &bob).await;
    setup.connect_users(&alice, &charlie).await;

    let chat_id = setup.create_apq_group(&alice).await;
    setup.invite_to_group(chat_id, &alice, &[&bob]).await;
    setup.invite_to_group(chat_id, &alice, &[&charlie]).await;

    let alice_user = setup.user(&alice);
    let bob_user = setup.user(&bob);
    let charlie_user = setup.user(&charlie);

    // Alice leaves the APQ group.
    alice_user.leave_chat(chat_id).await.unwrap();

    // Bob picks up Alice's self-remove proposal.
    bob_user.drain().await;

    let debug_info = bob_user.chat_debug_info(chat_id).await.unwrap();
    let pq = debug_info
        .pq
        .clone()
        .expect("APQ group must have a PQ group");

    assert_eq!(
        debug_info.pending_proposals, 1,
        "T group must have Alice's self-remove proposal staged"
    );
    assert_eq!(
        pq.pending_proposals, 1,
        "PQ group must also have Alice's self-remove proposal staged"
    );
    let pq_epoch_before = pq.epoch;

    // A non-leaver (Bob) commits the self-remove via an APQ FULL commit, which
    // sweeps the pending proposal on both the T and PQ groups.
    bob_user.update_apq_key(chat_id).await.unwrap();

    // Charlie picks up the commit ...
    charlie_user.drain().await;

    // ... and Alice learns she was removed.
    alice_user.drain().await;

    // Both Bob and Charlie must have committed the removal on the T and PQ
    // groups: Alice is gone, nothing is left pending, and the PQ epoch advanced.
    for (label, user) in [("Bob", bob_user), ("Charlie", charlie_user)] {
        let debug_info = user.chat_debug_info(chat_id).await.unwrap();
        let pq = debug_info.pq.expect("APQ group must have a PQ group");

        assert_eq!(
            debug_info.members.len(),
            2,
            "{label}'s T group must have Alice removed (only Bob and Charlie left)"
        );
        assert_eq!(
            debug_info.pending_proposals, 0,
            "{label}'s T group must have no pending proposals after the commit"
        );
        assert_eq!(
            pq.pending_proposals, 0,
            "{label}'s PQ group must have no pending proposals after the commit"
        );
        assert!(
            pq.epoch > pq_epoch_before,
            "{label}'s PQ group must advance its epoch by committing the removal"
        );
    }

    // Alice's chat is now inactive.
    let alice_chat = alice_user.expect_chat(chat_id).await;
    assert!(
        matches!(alice_chat.status(), ChatStatus::Inactive(_)),
        "Alice's chat must be inactive after being removed from the APQ group"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn apq_delete_removes_member_from_both_t_and_pq() {
    let mut setup = TestBackend::single().await;

    let alice = setup.add_user().await;
    let bob = setup.add_user().await;
    let charlie = setup.add_user().await;

    setup.connect_users(&alice, &bob).await;
    setup.connect_users(&alice, &charlie).await;

    let chat_id = setup.create_apq_group(&alice).await;
    setup.invite_to_group(chat_id, &alice, &[&bob]).await;
    setup.invite_to_group(chat_id, &alice, &[&charlie]).await;

    let alice_user = setup.user(&alice);
    let bob_user = setup.user(&bob);
    let charlie_user = setup.user(&charlie);

    // Snapshot the PQ epoch before the delete so we can assert it advanced.
    let pq_epoch_before = alice_user
        .chat_debug_info(chat_id)
        .await
        .unwrap()
        .pq
        .expect("APQ group must have a PQ group")
        .epoch;

    // Alice deletes the APQ group. This commits a removal of every other member
    // on both the T and the PQ group.
    alice_user.delete_chat(chat_id).await.unwrap();

    // Alice must have committed the removal on both groups: only her own leaf is
    // left, nothing stays pending, and the PQ epoch advanced.
    let debug_info = alice_user.chat_debug_info(chat_id).await.unwrap();
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
    let alice_chat = alice_user.expect_chat(chat_id).await;
    assert!(
        matches!(alice_chat.status(), ChatStatus::Inactive(_)),
        "deleter's chat must be inactive after deleting the APQ group"
    );

    // Bob and Charlie pick up the delete commit and their chats go inactive.
    for (label, user) in [("Bob", bob_user), ("Charlie", charlie_user)] {
        user.drain().await;
        let chat = user.expect_chat(chat_id).await;
        assert!(
            matches!(chat.status(), ChatStatus::Inactive(_)),
            "{label}'s chat must be inactive after the APQ group was deleted"
        );
    }
}

/// A member joining from a Welcome that is several epochs stale.
///
/// `ds_welcome_info` answers for the epoch asked for, so the ratchet tree it
/// returns is historical. The profile keys shipped with it have to describe
/// that same epoch. Serving the group's current keys instead means any leaf
/// that changed hands in between resolves to the wrong occupant, and the
/// ciphertext is bound to a user id, so it does not decrypt.
///
/// The joiner is deliberately stopped after its Welcome and before it catches
/// up: later commits carry added members' profile keys in their AAD, which
/// would re-deliver the keys and hide the problem.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn stale_welcome_epoch_preserves_profile_keys() {
    let mut setup = TestBackend::single().await;

    let alice = setup.add_user().await; // committer
    let bob = setup.add_user().await; // joins from a stale Welcome
    let charlie = setup.add_user().await; // leaves the group, freeing his leaf
    let dave = setup.add_user().await; // stays throughout
    let eve = setup.add_user().await; // takes over Charlie's leaf

    for member in [&bob, &charlie, &dave, &eve] {
        setup.connect_users(&alice, member).await;
    }
    for member in [&alice, &bob, &charlie, &dave, &eve] {
        let profile = UserProfile {
            user_id: member.clone(),
            display_name: expected_display_name(member),
            profile_picture: None,
        };
        setup
            .user(member)
            .set_own_user_profile(profile)
            .await
            .unwrap();
    }

    let chat_id = setup.create_group(&alice).await;
    setup
        .invite_and_settle(&alice, chat_id, &[&charlie, &dave])
        .await;
    let charlie_index = setup.own_leaf_index(&charlie, chat_id).await;

    // Alice invites Bob. Bob's Welcome describes this epoch, with Charlie
    // still in the group. Bob does not touch his queue yet.
    setup
        .user(&alice)
        .invite_users(chat_id, slice::from_ref(&bob))
        .await
        .unwrap();
    setup.settle(&[&alice, &charlie, &dave]).await;

    // The group moves on: Charlie's leaf is vacated and handed to Eve.
    setup
        .user(&alice)
        .remove_users(chat_id, vec![charlie.clone()])
        .await
        .unwrap();
    setup.settle(&[&alice, &dave]).await;
    setup.invite_and_settle(&alice, chat_id, &[&eve]).await;
    let eve_index = setup.own_leaf_index(&eve, chat_id).await;
    assert_eq!(
        eve_index, charlie_index,
        "the test needs Eve to take over Charlie's leaf, otherwise nothing \
         changed hands between Bob's Welcome and now"
    );

    // Bob processes his Welcome and nothing else, so his view is the epoch the
    // Welcome describes rather than the group's current one.
    let queued = setup.user(&bob).qs_fetch_messages().await.unwrap();
    let mut joined = false;
    let mut consumed = 0usize;
    for message in queued {
        let bob_user = setup.user(&bob);
        bob_user.fully_process_qs_messages(vec![message]).await;
        consumed += 1;
        if bob_user.chat_participants(chat_id).await.is_some() {
            joined = true;
            break;
        }
    }
    assert!(
        joined,
        "Bob should have joined after processing his Welcome"
    );
    tracing::info!(consumed, "messages Bob processed before joining");
    // Everything still queued is deliberately left unprocessed.
    for _ in 0..3 {
        setup.user(&bob).outbound_service().run_once().await;
    }

    // Bob is at the Welcome's epoch, where Charlie is still a member.
    let bob_view = setup.user(&bob).chat_participants(chat_id).await.unwrap();
    // Bob's two views of the same group must agree. The ratchet tree is the
    // epoch his Welcome describes; the participant list is derived from the
    // room state that came with it. If the DS answers with a current room
    // state, the list names whoever holds a leaf now rather than who held it
    // at that epoch.
    let mls_view = setup.user(&bob).group_members(chat_id).await.unwrap();
    assert_eq!(
        bob_view, mls_view,
        "Bob's participant list disagrees with his own ratchet tree: the two \
         halves of his welcome info describe different epochs"
    );
    assert!(
        bob_view.contains(&charlie),
        "Bob should still see Charlie, who was a member at his Welcome's epoch"
    );
    assert!(
        !bob_view.contains(&eve),
        "Bob should not see Eve, who joined after his Welcome's epoch"
    );

    setup
        .assert_profiles_resolve(&bob, &[&alice, &charlie, &dave], "stale welcome")
        .await;
}
