// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::slice;

use airapiclient::as_api::AsRequestError;
use aircoreclient::{DisplayName, UserProfile, clients::queue_event, store::Store};
use airserver_test_harness::utils::setup::TestBackend;
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
    setup.send_message(chat_id, &alice, vec![&bob]).await;
    setup.send_message(chat_id, &bob, vec![&alice]).await;
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
        .unwrap();

    // Charlie processes his messages again, this will fail, because he will
    // unsuccessfully try to download Alice's old profile.
    let charlie_user = &setup.get_user(&charlie).user;
    let charlie_qs_messages = charlie_user.qs_fetch_messages().await.unwrap();
    let result = charlie_user
        .fully_process_qs_messages(charlie_qs_messages)
        .await;

    assert!(result.changed_chats.is_empty());
    assert!(result.new_chats.is_empty());
    assert!(result.new_messages.is_empty());
    let err = &result.errors[0];
    let AsRequestError::Tonic(tonic_err) = err.downcast_ref().unwrap() else {
        panic!("Unexpected error type");
    };
    assert_eq!(tonic_err.code(), tonic::Code::InvalidArgument);
    assert_eq!(tonic_err.message(), "No ciphertext matching index");

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
        .send_message(chat_id, &alice, vec![&bob, &charlie])
        .await;
    setup
        .send_message(chat_id, &bob, vec![&alice, &charlie])
        .await;

    // Block bob
    let alice_user = &setup.get_user(&alice).user;
    alice_user.block_contact(bob.clone()).await.unwrap();

    // Messages are still sent and received
    setup
        .send_message(chat_id, &bob, vec![&alice, &charlie])
        .await;
    setup
        .send_message(chat_id, &alice, vec![&bob, &charlie])
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
    setup.send_message(chat_id, &alice, vec![&bob]).await;
    setup.send_message(chat_id, &bob, vec![&alice]).await;
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
