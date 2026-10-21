// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Own profile distribution to contacts, and dropping profiles of users that no
//! longer share a group.

use std::assert_matches;

use aircoreclient::{Asset, DisplayName, EventMessage, Message, SystemMessage, UserProfile};
use airserver_test_harness::utils::{core_user_ext::CoreUserExt, setup::TestBackend};

use crate::messaging::attachments::test_picture_bytes;

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn exchange_user_profiles() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;

    // Set a user profile for alice
    let alice_display_name: DisplayName = "4l1c3".parse().unwrap();

    let png_bytes = test_picture_bytes();

    let alice_profile_picture = Asset::Value(png_bytes.clone());

    let alice_profile = UserProfile {
        user_id: alice.user_id().clone(),
        display_name: alice_display_name.clone(),
        profile_picture: Some(alice_profile_picture.clone()),
    };
    alice.set_own_user_profile(alice_profile).await.unwrap();

    let bob = setup.add_user().await;

    // Set a user profile for
    let bob_display_name: DisplayName = "B0b".parse().unwrap();
    let bob_profile_picture = Asset::Value(png_bytes.clone());
    let bob_user_profile = UserProfile {
        user_id: bob.user_id().clone(),
        display_name: bob_display_name.clone(),
        profile_picture: Some(bob_profile_picture.clone()),
    };

    bob.set_own_user_profile(bob_user_profile).await.unwrap();
    let new_profile = bob.own_user_profile().await.unwrap();
    let Asset::Value(compressed_profile_picture) = new_profile.profile_picture.unwrap().clone();

    setup.connect_users(&alice, &bob).await;

    let bob_user_profile = alice.user_profile(bob.user_id()).await;

    let profile_picture = bob_user_profile
        .profile_picture
        .unwrap()
        .clone()
        .value()
        .unwrap()
        .to_vec();

    assert_eq!(profile_picture, compressed_profile_picture);

    assert!(bob_user_profile.display_name == bob_display_name);

    let alice_user_profile = alice.user_profile(alice.user_id()).await;

    assert_eq!(alice_user_profile.display_name, alice_display_name);

    let new_user_profile = UserProfile {
        user_id: alice.user_id().clone(),
        display_name: "New Alice".parse().unwrap(),
        profile_picture: None,
    };

    alice
        .set_own_user_profile(new_user_profile.clone())
        .await
        .unwrap();

    bob.drain().await;
    bob.outbound_service().run_once().await;

    let alice_user_profile = bob.user_profile(alice.user_id()).await;

    assert_eq!(alice_user_profile, new_user_profile);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn add_contact_and_change_profile() {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;

    let alice_username_record = setup.add_username(&alice).await;

    // Add Alice as a contact
    let bob_alice_chat_id = bob
        .add_contact(
            alice_username_record.username.clone(),
            alice_username_record.hash,
            setup.apq_groups,
        )
        .await
        .expect("fatal error")
        .expect("non-fatal error");

    // Change Bob's profile
    let bob_user_profile = UserProfile {
        user_id: bob.user_id().clone(),
        display_name: "B0b".parse().unwrap(),
        profile_picture: None,
    };

    bob.set_own_user_profile(bob_user_profile).await.unwrap();

    // Fetch invitation from Bob and accept it
    let mut messages = alice.fetch_username_messages().await.unwrap();
    assert_eq!(messages.len(), 1);
    let alice_bob_chat_id = alice
        .process_username_queue_message(alice_username_record.username, messages.pop().unwrap())
        .await
        .unwrap()
        .expect("the offer should create a pending chat")
        .chat_id;
    alice
        .accept_contact_request(alice_bob_chat_id)
        .await
        .unwrap()
        .unwrap();

    // Send message from Alice to Bob
    alice.send_text(alice_bob_chat_id, "hello").await;

    // Bob receives invitation acceptance and sees the message from Alice
    bob.drain().await;

    let messages = bob.messages(bob_alice_chat_id, 10).await.unwrap();
    assert_eq!(messages.len(), 3);
    assert_matches!(
        messages[0].message(),
        Message::Event(EventMessage::System(
            SystemMessage::NewHandleConnectionChat(_)
        ))
    );
    assert_matches!(
        messages[1].message(),
        Message::Event(EventMessage::System(
            SystemMessage::ReceivedConnectionConfirmation { .. }
        ))
    );
    assert_eq!(
        messages[2]
            .message()
            .mimi_content()
            .unwrap()
            .string_rendering()
            .unwrap(),
        "hello"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn user_deletion_triggers() {
    let mut setup = TestBackend::single().await;
    // Create alice and bob
    let [alice, bob, charlie] = setup.add_users().await;

    // Connect alice and bob
    setup.connect_users(&alice, &bob).await;
    // Connect alice and charlie
    setup.connect_users(&alice, &charlie).await;

    // Note that bob and charlie are not connected.

    // Alice creates a group and invites bob and charlie
    let chat_id = setup.create_group(&alice).await;
    setup
        .invite_to_group(chat_id, &alice, &[&bob, &charlie])
        .await;

    // Bob should have a user profile for charlie now, even though they
    // are not connected.
    let bob_user_profile_charlie = bob.user_profile(charlie.user_id()).await;
    assert!(bob_user_profile_charlie.user_id == *charlie.user_id());

    // Now charlie leaves the group
    setup.leave_group(chat_id, &charlie).await.unwrap();
    // Bob should not have a user profile for charlie anymore.

    let bob_user_profile_charlie = bob.user_profile(charlie.user_id()).await;
    assert_eq!(
        bob_user_profile_charlie,
        aircoreclient::UserProfile::from_user_id(charlie.user_id())
    );
}
