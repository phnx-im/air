// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::slice;

use aircoreclient::{
    DisplayName, UserProfile,
    clients::{
        process::process_qs::{QsProcessEventResult, QsStreamProcessor},
        queue_event,
    },
    store::Store,
};
use airserver_test_harness::utils::setup::TestBackend;
use chrono::{DateTime, Duration, Utc};
use mimi_content::MimiContent;
use tokio_stream::StreamExt;
use tracing::info;

use super::attachment::test_picture_bytes;

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[tracing::instrument(name = "Create group test", skip_all)]
async fn create_group() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    setup.create_group(&alice).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[tracing::instrument(name = "Invite to group test", skip_all)]
async fn invite_to_group() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let bob = setup.add_user().await;
    let charlie = setup.add_user().await;
    setup.connect_users(&alice, &bob).await;
    setup.connect_users(&alice, &charlie).await;
    let chat_id = setup.create_group(&alice).await;
    setup
        .invite_to_group(chat_id, &alice, vec![&bob, &charlie])
        .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[tracing::instrument(name = "Invite to group test", skip_all)]
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
        .invite_to_group(chat_id, &alice, vec![&bob, &charlie])
        .await;
    info!("Updating group");
    setup.update_group(chat_id, &bob).await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[tracing::instrument(name = "Remove from group test", skip_all)]
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
        .invite_to_group(chat_id, &alice, vec![&bob, &charlie, &dave])
        .await;
    // Check that Charlie has a user profile stored for bob, even though
    // he hasn't connected with them.
    let charlie_user = &setup.get_user(&charlie).user;
    let charlie_user_profile_bob = charlie_user.user_profile(&bob).await;
    assert!(charlie_user_profile_bob.user_id == bob);

    setup
        .remove_from_group(chat_id, &alice, vec![&bob])
        .await
        .unwrap();

    // Now that charlie is not in a group with Bob anymore, the user profile
    // should be the default one derived from the client id.
    let charlie_user = &setup.get_user(&charlie).user;
    let charlie_user_profile_bob = charlie_user.user_profile(&bob).await;
    assert_eq!(charlie_user_profile_bob, UserProfile::from_user_id(&bob));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[tracing::instrument(name = "Re-add to group test", skip_all)]
async fn re_add_client() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let bob = setup.add_user().await;
    setup.connect_users(&alice, &bob).await;
    let chat_id = setup.create_group(&alice).await;
    setup.invite_to_group(chat_id, &alice, vec![&bob]).await;
    for _ in 0..10 {
        setup
            .remove_from_group(chat_id, &alice, vec![&bob])
            .await
            .unwrap();
        setup.invite_to_group(chat_id, &alice, vec![&bob]).await;
    }
    setup.send_message(chat_id, &alice, vec![&bob], None).await;
    setup.send_message(chat_id, &bob, vec![&alice], None).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[tracing::instrument(name = "Invite to group test", skip_all)]
async fn leave_group() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let bob = setup.add_user().await;
    setup.connect_users(&alice, &bob).await;
    let chat_id = setup.create_group(&alice).await;
    setup.invite_to_group(chat_id, &alice, vec![&bob]).await;
    setup.leave_group(chat_id, &bob).await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[tracing::instrument(name = "Invite to group test", skip_all)]
async fn delete_group() {
    let mut setup = TestBackend::single().await;

    let alice = setup.add_user().await;
    let bob = setup.add_user().await;
    setup.connect_users(&alice, &bob).await;
    let chat_id = setup.create_group(&alice).await;
    setup.invite_to_group(chat_id, &alice, vec![&bob]).await;
    let delete_group = setup.delete_group(chat_id, &alice);
    delete_group.await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[tracing::instrument(name = "Room policy", skip_all)]
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

    setup.invite_to_group(chat_id, &alice, vec![&bob]).await;

    // Bob can invite charlie
    setup.invite_to_group(chat_id, &bob, vec![&charlie]).await;

    // Charlie can kick alice
    setup
        .remove_from_group(chat_id, &charlie, vec![&alice])
        .await
        .unwrap();

    // Charlie can kick bob
    setup
        .remove_from_group(chat_id, &charlie, vec![&bob])
        .await
        .unwrap();

    // TODO: This currently fails
    // Charlie can leave and an empty room remains
    // setup.leave_group(chat_id, &charlie).await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[tracing::instrument(name = "Update user profile on group join test", skip_all)]
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
        .get_user(&alice)
        .user
        .set_own_user_profile(alice_profile)
        .await
        .unwrap();

    // Bob doesn't fetch his queue, so he doesn't know about Alice's new profile.
    // He creates a group and invites Charlie.
    let chat_id = setup.create_group(&bob).await;

    let bob_user = &setup.get_user(&bob).user;
    bob_user
        .invite_users(chat_id, slice::from_ref(&charlie))
        .await
        .unwrap()
        .unwrap();

    // Charlie accepts the invitation.
    let charlie_user = &setup.get_user(&charlie).user;
    let charlie_qs_messages = charlie_user.qs_fetch_messages().await.unwrap();
    charlie_user
        .fully_process_qs_messages(charlie_qs_messages)
        .await;

    // Bob now invites Alice
    let bob_user = &setup.get_user(&bob).user;
    bob_user
        .invite_users(chat_id, slice::from_ref(&alice))
        .await
        .unwrap()
        .unwrap();

    // Charlie processes his messages again, fetching Alice's profile will fail because it tries to
    // download Alice's old profile.
    let charlie_user = &setup.get_user(&charlie).user;
    charlie_user.qs_fetch_messages().await.unwrap();
    charlie_user.outbound_service().run_once().await;

    let alice_profile = charlie_user.user_profile(&alice).await;
    assert_eq!(
        alice_profile,
        UserProfile::from_user_id(&alice),
        "Fetching Alice's profile should have failed"
    );

    // Alice accepts the invitation.
    let alice_user = &setup.get_user(&alice).user;
    let alice_qs_messages = alice_user.qs_fetch_messages().await.unwrap();
    alice_user
        .fully_process_qs_messages(alice_qs_messages)
        .await;

    // While processing her messages, Alice should have issued a profile update

    // Charlie picks up his messages.
    let charlie_user = &setup.get_user(&charlie).user;
    let charlie_qs_messages = charlie_user.qs_fetch_messages().await.unwrap();
    charlie_user
        .fully_process_qs_messages(charlie_qs_messages)
        .await;
    charlie_user.outbound_service().run_once().await;

    // Charlie should now have Alice's new profile.
    let charlie_user_profile = charlie_user.user_profile(&alice).await;
    assert_eq!(charlie_user_profile.display_name, alice_display_name);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[tracing::instrument(name = "Group with blocked contacts", skip_all)]
async fn group_with_blocked_contact() {
    let mut setup = TestBackend::single().await;

    let alice = setup.add_user().await;
    setup.get_user_mut(&alice).add_user_handle().await.unwrap();

    let bob = setup.add_user().await;
    let charlie = setup.add_user().await;

    setup.connect_users(&alice, &bob).await;
    setup.connect_users(&alice, &charlie).await;

    // Create a group with alice, bob and charlie
    let chat_id = setup.create_group(&alice).await;
    setup
        .invite_to_group(chat_id, &alice, vec![&bob, &charlie])
        .await;

    // Sending messages works before blocking
    setup
        .send_message(chat_id, &alice, vec![&bob, &charlie], None)
        .await;
    setup
        .send_message(chat_id, &bob, vec![&alice, &charlie], None)
        .await;

    // Block bob
    let alice_user = &setup.get_user(&alice).user;
    alice_user.block_contact(bob.clone()).await.unwrap();

    // Messages are still sent and received
    setup
        .send_message(chat_id, &bob, vec![&alice, &charlie], None)
        .await;
    setup
        .send_message(chat_id, &alice, vec![&bob, &charlie], None)
        .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[tracing::instrument(name = "Send message test", skip_all)]
async fn send_message() {
    info!("Setting up setup");
    let mut setup = TestBackend::single().await;
    info!("Creating users");
    let alice = setup.add_user().await;
    let bob = setup.add_user().await;
    let chat_id = setup.connect_users(&alice, &bob).await;
    setup.send_message(chat_id, &alice, vec![&bob], None).await;
    setup.send_message(chat_id, &bob, vec![&alice], None).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[tracing::instrument(name = "Update group data", skip_all)]
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
        .invite_to_group(chat_id, &alice, vec![&bob, &charlie])
        .await;

    // Alice updates the group picture
    let alice_user = &setup.get_user(&alice).user;
    let picture = test_picture_bytes();
    alice_user
        .set_chat_picture(chat_id, Some(picture.clone()))
        .await
        .unwrap();

    let expected_picture = alice_user
        .chat(&chat_id)
        .await
        .unwrap()
        .attributes
        .picture
        .unwrap()
        .clone();

    // Bob and Charlie should now have the updated group picture
    for user_id in [&bob, &charlie] {
        let user = &setup.get_user(user_id).user;
        // Fetch and process messages to get the update
        let qs_messages = user.qs_fetch_messages().await.unwrap();
        let result = user.fully_process_qs_messages(qs_messages).await;
        assert!(
            result.errors.is_empty(),
            "{:?} should process Alice's update without errors",
            user_id
        );
        let actual_picture = user
            .chat(&chat_id)
            .await
            .unwrap()
            .attributes
            .picture
            .unwrap()
            .clone();
        assert_eq!(actual_picture, expected_picture);
    }

    // Now Bob updates the group title
    let title = "New Group Title".to_string();
    let bob_user = &setup.get_user(&bob).user;
    bob_user
        .set_chat_title(chat_id, title.clone())
        .await
        .unwrap();

    for user_id in [&alice, &charlie] {
        let user = &setup.get_user_mut(user_id).user;
        // Fetch and process messages to get the update
        let qs_messages = user.qs_fetch_messages().await.unwrap();
        let result = user.fully_process_qs_messages(qs_messages).await;
        assert!(
            result.errors.is_empty(),
            "{:?} should process Bob's update without errors",
            user_id
        );
        let actual_title = user.chat(&chat_id).await.unwrap().attributes.title.clone();
        assert_eq!(actual_title, title);
    }
}

/// Tests that after being invited to a group, the invitee fetches the encrypted group profile from
/// object storage via the outbound service and sees the correct group attributes (title and
/// picture).
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[tracing::instrument(name = "Fetch group profile on invite", skip_all)]
async fn fetch_group_profile_on_invite() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let bob = setup.add_user().await;
    setup.connect_users(&alice, &bob).await;

    let chat_id = setup.create_group(&alice).await;

    // Record Alice's group attributes (title and resized picture set during create_group)
    let alice_user = &setup.get_user(&alice).user;
    let alice_chat = alice_user.chat(&chat_id).await.unwrap();
    let expected_title = alice_chat.attributes().title().to_owned();
    let expected_picture = alice_chat.attributes().picture.clone();

    // Alice invites Bob; the encrypted group profile is already uploaded from group creation
    alice_user
        .invite_users(chat_id, slice::from_ref(&bob))
        .await
        .unwrap()
        .unwrap();

    // Bob processes the invitation: this schedules a FetchGroupProfileOperation
    let bob_user = &setup.get_user(&bob).user;
    let qs_messages = bob_user.qs_fetch_messages().await.unwrap();
    let result = bob_user.fully_process_qs_messages(qs_messages).await;
    assert!(
        result.errors.is_empty(),
        "Bob should process invitation without errors: {:?}",
        result.errors
    );

    // Bob sees the group title immediately after processing the invitation, but not the picture
    let bob_chat = bob_user.chat(&chat_id).await.unwrap();
    assert_eq!(bob_chat.attributes().title(), &expected_title);

    // Bob runs the outbound service: this executes FetchGroupProfileOperation,
    // downloading and decrypting the encrypted group profile from object storage
    bob_user.outbound_service().run_once().await;

    // Bob should have the correct group attributes after fetching the encrypted profile
    let bob_chat = bob_user.chat(&chat_id).await.unwrap();
    assert_eq!(bob_chat.attributes().title(), &expected_title);
    assert_eq!(bob_chat.attributes().picture, expected_picture);
}

/// Tests that after a group title and picture update, other members fetch the new encrypted group
/// profile via the outbound service and see the updated attributes.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[tracing::instrument(name = "Fetch group profile on update", skip_all)]
async fn fetch_group_profile_on_update() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let bob = setup.add_user().await;
    setup.connect_users(&alice, &bob).await;

    let chat_id = setup.create_group(&alice).await;
    setup.invite_to_group(chat_id, &alice, vec![&bob]).await;

    let chat = setup.get_user(&alice).user.chat(&chat_id).await.unwrap();
    let title = chat.attributes().title().to_owned();

    // Bob sees the group title of group immediately
    let bob_user = &setup.get_user(&bob).user;
    let bob_chat = bob_user.chat(&chat_id).await.unwrap();
    assert_eq!(bob_chat.attributes().title(), &title);

    // Alice updates the group title and picture
    let new_title = "Updated Group Title".to_string();
    let alice_user = &setup.get_user(&alice).user;
    alice_user
        .set_chat_title(chat_id, new_title.clone())
        .await
        .unwrap();
    alice_user
        .set_chat_picture(chat_id, Some(test_picture_bytes()))
        .await
        .unwrap();

    // Record Alice's stored picture (may be resized relative to test_picture_bytes)
    let expected_picture = alice_user
        .chat(&chat_id)
        .await
        .unwrap()
        .attributes
        .picture
        .clone();

    // Bob fetches Alice's commits: this schedules FetchGroupProfileOperations
    let bob_user = &setup.get_user(&bob).user;
    let qs_messages = bob_user.qs_fetch_messages().await.unwrap();
    let result = bob_user.fully_process_qs_messages(qs_messages).await;
    assert!(
        result.errors.is_empty(),
        "Bob should process Alice's updates without errors: {:?}",
        result.errors
    );

    // Bob runs the outbound service: this executes FetchGroupProfileOperations,
    // downloading and decrypting the new encrypted group profile from object storage
    bob_user.outbound_service().run_once().await;

    // Bob should see the updated title and picture
    let bob_chat = bob_user.chat(&chat_id).await.unwrap();
    assert_eq!(bob_chat.attributes().title(), &new_title);
    assert_eq!(bob_chat.attributes().picture, expected_picture);
}

/// This test checks that bob can leave a group where he missed a commit. He leaves the group only
/// locally. Other user can remove the bob correctly (commit is merged), and add him again.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[tracing::instrument(name = "Missed commit", skip_all)]
async fn missed_commit() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let bob = setup.add_user().await;
    let charlie = setup.add_user().await;

    setup.connect_users(&alice, &bob).await;
    setup.connect_users(&alice, &charlie).await;

    // Alice creates a group and invites Bob
    let chat_id = setup.create_group(&alice).await;
    setup.invite_to_group(chat_id, &alice, vec![&bob]).await;

    // Alice invites Charlie
    let alice_user = &setup.get_user(&alice).user;
    alice_user
        .invite_users(chat_id, slice::from_ref(&charlie))
        .await
        .unwrap()
        .unwrap();

    let charlie_qs_messages = alice_user.qs_fetch_messages().await.unwrap();
    let result = alice_user
        .fully_process_qs_messages(charlie_qs_messages)
        .await;
    assert!(
        result.errors.is_empty(),
        "Alice should process Charlie's invitation without errors"
    );

    // Bob misses the invitation commit
    let bob_user = &setup.get_user(&bob).user;
    let (stream, responder) = bob_user.listen_queue().await.unwrap();
    let sequence_number = stream
        .map_while(|message| match message.event {
            Some(queue_event::Event::Message(queue_message)) => Some(queue_message.sequence_number),
            _ => None,
        })
        .fold(0, |_, sequence_number| sequence_number)
        .await;
    // Throw away all messages
    responder.ack(sequence_number + 1).await;

    // Bob tries to leave the group; this works but only locally
    bob_user.leave_chat(chat_id).await.unwrap();

    // ... and Alice still has 3 members
    let messages = alice_user.qs_fetch_messages().await.unwrap();
    alice_user.fully_process_qs_messages(messages).await;
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
    let charlie_user = &setup.get_user(&charlie).user;
    let messages = charlie_user.qs_fetch_messages().await.unwrap();
    charlie_user.fully_process_qs_messages(messages).await;
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
        .unwrap()
        .unwrap();
    let messages = alice_user.qs_fetch_messages().await.unwrap();
    alice_user.fully_process_qs_messages(messages).await;
    let messages = bob_user.qs_fetch_messages().await.unwrap();
    bob_user.fully_process_qs_messages(messages).await;
    let messages = charlie_user.qs_fetch_messages().await.unwrap();
    charlie_user.fully_process_qs_messages(messages).await;

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

    let alice_user = setup.get_user(&alice);
    let alice_core = &alice_user.user;

    // Make server drop connection instead of sending responses
    setup.listener_control_handle().set_drop_next_response();

    let _ = alice_core
        .invite_users(chat_id, &[bob])
        .await
        .expect_err("No error despite server dropping messages");

    // Bob should not be in the group.
    let number_of_members = alice_core
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
    let qs_messages = alice_core.qs_fetch_messages().await.unwrap();
    println!("Number of QS messages: {}", qs_messages.len());
    alice_core.fully_process_qs_messages(qs_messages).await;

    // Bob should now be in the group.
    let number_of_members = alice_core
        .mls_members(chat_id)
        .await
        .unwrap()
        .unwrap()
        .len();
    assert_eq!(number_of_members, 2);

    alice_core
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

    let alice_user = setup.get_user(&alice);
    let alice_core = &alice_user.user;

    // Initially, all chats are self-updated at creation time
    let mut self_updated_at = Vec::new();
    for chat_id in &chat_ids {
        let at = alice_core.self_updated_at(*chat_id).await.unwrap().unwrap();
        assert!(
            created_at <= at && at <= created_at + Duration::seconds(10),
            "Self update is not within 10 seconds of now",
        );
        self_updated_at.push(at);
    }

    // Set self_updated_at to the past and schedule a self update
    for chat_id in &chat_ids {
        alice_core
            .set_self_updated_at(*chat_id, DateTime::UNIX_EPOCH)
            .await
            .unwrap();
    }
    // Run the outbound service to update the chats
    alice_core
        .outbound_service()
        .schedule_self_update(DateTime::UNIX_EPOCH)
        .await
        .unwrap();
    alice_core.outbound_service().run_once().await;

    // The outbound service updates the chats in batches of 5
    for (idx, chat_id) in chat_ids.iter().take(UPDATE_BATCH_SIZE).enumerate() {
        let at = alice_core.self_updated_at(*chat_id).await.unwrap().unwrap();
        assert!(self_updated_at[idx] < at, "Self update not happened",);
    }
    assert_eq!(
        alice_core
            .self_updated_at(chat_ids[UPDATE_BATCH_SIZE])
            .await
            .unwrap()
            .unwrap(),
        DateTime::UNIX_EPOCH,
        "Last chat self-updated even though it should not have been"
    );

    // Another run should update the remaining chat
    alice_core
        .outbound_service()
        .schedule_self_update(DateTime::UNIX_EPOCH)
        .await
        .unwrap();
    alice_core.outbound_service().run_once().await;

    let at = alice_core
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
async fn self_update_skips_inactive_chats() {
    let mut setup = TestBackend::single().await;

    let alice = setup.add_user().await;
    let active_chat = setup.create_group(&alice).await;
    let inactive_chat = setup.create_group(&alice).await;

    let alice_user = setup.get_user(&alice);
    let alice_core = &alice_user.user;

    // Leave one chat to make it inactive
    alice_core.leave_chat(inactive_chat).await.unwrap();

    // Set self_updated_at to the past for both chats
    alice_core
        .set_self_updated_at(active_chat, DateTime::UNIX_EPOCH)
        .await
        .unwrap();
    alice_core
        .set_self_updated_at(inactive_chat, DateTime::UNIX_EPOCH)
        .await
        .unwrap();

    // Run the outbound service
    alice_core
        .outbound_service()
        .schedule_self_update(DateTime::UNIX_EPOCH)
        .await
        .unwrap();
    alice_core.outbound_service().run_once().await;

    // The active chat should have been self-updated
    let active_at = alice_core
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
        alice_core
            .self_updated_at(inactive_chat)
            .await
            .unwrap()
            .unwrap(),
        DateTime::UNIX_EPOCH,
        "Inactive chat was self-updated even though it should not have been"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[tracing::instrument(name = "Invite to group test", skip_all)]
async fn qs_stream_processor_partially_processes_messages() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let bob = setup.add_user().await;

    let connection_chat_id = setup.connect_users(&alice, &bob).await;
    let group_chat_id = setup.create_group(&alice).await;
    setup
        .invite_to_group(group_chat_id, &alice, vec![&bob])
        .await;

    // Remove bob chat only locally on their client
    setup
        .get_user(&bob)
        .user
        .erase_chat(group_chat_id)
        .await
        .unwrap();

    let alice_user = &setup.get_user(&alice).user;

    let content = MimiContent::simple_markdown_message("Hello from Alice!".to_owned(), [0; 16]);

    alice_user
        .send_message(connection_chat_id, content.clone(), None)
        .await
        .unwrap();
    alice_user
        .send_message(group_chat_id, content.clone(), None)
        .await
        .unwrap();
    alice_user
        .send_message(connection_chat_id, content, None)
        .await
        .unwrap();
    alice_user.outbound_service().run_once().await;

    let bob_user = &setup.get_user(&bob).user;
    // let batch = bob_user.qs_fetch_messages().await.unwrap();
    // bob_user.fully_process_qs_messages(batch).await;

    let (mut stream, responder) = bob_user.listen_queue().await.unwrap();
    let mut processor = QsStreamProcessor::new(Some(responder));

    while let Some(message) = stream.next().await {
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
