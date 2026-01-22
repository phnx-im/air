// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{collections::HashMap, fs};

use airapiclient::as_api::AsRequestError;
use aircommon::{assert_matches, identifiers::UserHandle};
use aircoreclient::{
    AddHandleContactError, AddHandleContactResult, Asset, BlockedContactError, DisplayName,
    EventMessage, Message, SystemMessage, UserProfile, clients::CoreUser, store::Store,
};
use airserver_test_harness::utils::setup::{TestBackend, TestUser};
use mimi_content::MimiContent;
use rand::Rng;
use tracing::info;

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[tracing::instrument(name = "Create user", skip_all)]
async fn create_user() {
    let mut setup = TestBackend::single().await;
    setup.add_user().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[tracing::instrument(name = "User profile exchange test", skip_all)]
async fn exchange_user_profiles() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;

    // Set a user profile for alice
    let alice_display_name: DisplayName = "4l1c3".parse().unwrap();

    let png_bytes = super::attachment::test_picture_bytes();

    let alice_profile_picture = Asset::Value(png_bytes.clone());

    let alice_profile = UserProfile {
        user_id: alice.clone(),
        display_name: alice_display_name.clone(),
        profile_picture: Some(alice_profile_picture.clone()),
    };
    let alice_user = &setup.get_user(&alice).user;
    alice_user
        .set_own_user_profile(alice_profile)
        .await
        .unwrap();

    let bob = setup.add_user().await;

    // Set a user profile for
    let bob_display_name: DisplayName = "B0b".parse().unwrap();
    let bob_profile_picture = Asset::Value(png_bytes.clone());
    let bob_user_profile = UserProfile {
        user_id: bob.clone(),
        display_name: bob_display_name.clone(),
        profile_picture: Some(bob_profile_picture.clone()),
    };

    let bob_user = &setup.get_user(&bob).user;
    bob_user
        .set_own_user_profile(bob_user_profile)
        .await
        .unwrap();
    let new_profile = bob_user.own_user_profile().await.unwrap();
    let Asset::Value(compressed_profile_picture) = new_profile.profile_picture.unwrap().clone();

    setup.connect_users(&alice, &bob).await;

    let bob_user_profile = setup.get_user(&alice).user.user_profile(&bob).await;

    let profile_picture = bob_user_profile
        .profile_picture
        .unwrap()
        .clone()
        .value()
        .unwrap()
        .to_vec();

    assert_eq!(profile_picture, compressed_profile_picture);

    assert!(bob_user_profile.display_name == bob_display_name);

    let alice_user = &setup.get_user(&alice).user;

    let alice_user_profile = alice_user.user_profile(&alice).await;

    assert_eq!(alice_user_profile.display_name, alice_display_name);

    let new_user_profile = UserProfile {
        user_id: alice.clone(),
        display_name: "New Alice".parse().unwrap(),
        profile_picture: None,
    };

    alice_user
        .set_own_user_profile(new_user_profile.clone())
        .await
        .unwrap();

    let bob_user = &setup.get_user(&bob).user;
    let qs_messages = bob_user.qs_fetch_messages().await.unwrap();
    bob_user.fully_process_qs_messages(qs_messages).await;
    let alice_user_profile = bob_user.user_profile(&alice).await;

    assert_eq!(alice_user_profile, new_user_profile);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[tracing::instrument(name = "User persistence test", skip_all)]
async fn client_persistence() {
    // Create and persist the user.
    let mut setup = TestBackend::single().await;
    let alice = setup.add_persisted_user().await;

    let db_path = setup.temp_dir().to_owned();

    // Try to load the user from the database.
    CoreUser::load_with_server_url(&alice, db_path.to_str().unwrap(), Some(setup.server_url()))
        .await
        .unwrap();

    let client_db_path = db_path.join(format!("{}@{}.db", alice.uuid(), alice.domain()));
    assert!(client_db_path.exists());

    setup.delete_user(&alice).await;

    assert!(!client_db_path.exists());
    assert!(
        CoreUser::load_with_server_url(&alice, db_path.to_str().unwrap(), Some(setup.server_url()))
            .await
            .is_err()
    );

    // `CoreUser::load` opened the client DB, and so it was re-created.
    fs::remove_file(client_db_path).unwrap();
    fs::remove_file(db_path.join("air.db")).unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[tracing::instrument(name = "Test server error if unknown user", skip_all)]
async fn error_if_user_doesnt_exist() {
    let mut setup = TestBackend::single().await;

    let alice = setup.add_user().await;
    let alice_user = &setup.get_user(&alice).user;

    let handle = UserHandle::new("non-existent".to_owned()).unwrap();
    let hash = handle.calculate_hash().unwrap();

    let res = alice_user.add_contact(handle, hash).await.unwrap();

    assert!(matches!(
        res,
        AddHandleContactResult::Err(AddHandleContactError::HandleNotFound)
    ));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[tracing::instrument(name = "Delete user test", skip_all)]
async fn delete_user() {
    let mut setup = TestBackend::single().await;

    let alice = setup.add_user().await;
    // Adding another user with the same id should fail.
    match TestUser::try_new(&alice, setup.server_url(), "DUMMY007").await {
        Ok(_) => panic!("Should not be able to create a user with the same id"),
        Err(e) => match e.downcast_ref::<AsRequestError>().unwrap() {
            AsRequestError::Tonic(status) => {
                assert_eq!(status.code(), tonic::Code::AlreadyExists);
            }
            _ => panic!("Unexpected error type: {e}"),
        },
    }

    setup.delete_user(&alice).await;
    // After deletion, adding the user again should work.
    // Note: Since the user is ephemeral, there is nothing to test on the client side.
    TestUser::try_new(&alice, setup.server_url(), "DUMMY007")
        .await
        .unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[tracing::instrument(name = "Blocked contact", skip_all)]
async fn blocked_contact() {
    info!("Setting up setup");
    let mut setup = TestBackend::single().await;
    info!("Creating users");
    let alice = setup.add_user().await;
    setup.get_user_mut(&alice).add_user_handle().await.unwrap();
    info!("Created alice");
    let bob = setup.add_user().await;

    let chat_id = setup.connect_users(&alice, &bob).await;
    setup.send_message(chat_id, &alice, vec![&bob]).await;
    setup.send_message(chat_id, &bob, vec![&alice]).await;

    let alice_test_user = setup.get_user(&alice);
    let alice_user = &alice_test_user.user;
    let bob_test_user = setup.get_user(&bob);
    let bob_user = &bob_test_user.user;

    alice_user.block_contact(bob.clone()).await.unwrap();

    alice_test_user.fetch_and_process_qs_messages().await;
    bob_test_user.fetch_and_process_qs_messages().await;

    // Not possible to send a message to Bob
    let msg = MimiContent::simple_markdown_message("Hello".into(), [0; 16]);
    let res = alice_user.send_message(chat_id, msg.clone(), None).await;
    res.unwrap_err().downcast::<BlockedContactError>().unwrap();

    assert_eq!(bob_test_user.fetch_and_process_qs_messages().await, 0);

    // Updating Alice's profile is not communicated to Bob
    alice_user
        .update_user_profile(UserProfile {
            user_id: alice.clone(),
            display_name: "Alice in Wonderland".parse().unwrap(),
            profile_picture: None,
        })
        .await
        .unwrap();
    assert_eq!(bob_test_user.fetch_and_process_qs_messages().await, 0);

    // Updating Bob's profile is not communicated to Alice
    bob_user
        .update_user_profile(UserProfile {
            user_id: bob.clone(),
            display_name: "Annoying Bob".parse().unwrap(),
            profile_picture: None,
        })
        .await
        .unwrap();
    // We get the message but it is dropped
    let messages = alice_test_user.user.qs_fetch_messages().await.unwrap();
    assert_eq!(messages.len(), 1);
    let res = alice_user.fully_process_qs_messages(messages).await;
    assert!(res.is_empty(), "message is dropped");

    // Messages from bob are dropped
    bob_user.send_message(chat_id, msg, None).await.unwrap();
    bob_test_user.user.outbound_service().run_once().await;
    // We get the message but it is dropped
    let messages = alice_test_user.user.qs_fetch_messages().await.unwrap();
    assert_eq!(messages.len(), 1);
    let res = alice_test_user
        .user
        .fully_process_qs_messages(messages)
        .await;
    assert!(res.is_empty(), "message is dropped");

    // Bob cannot establish a new connection with Alice
    let alice_handle = alice_test_user
        .user_handle_record
        .as_ref()
        .unwrap()
        .handle
        .clone();
    let alice_handle_hash = alice_handle.calculate_hash().unwrap();
    bob_test_user
        .user
        .add_contact(alice_handle.clone(), alice_handle_hash)
        .await
        .unwrap();
    let mut messages = alice_test_user.user.fetch_handle_messages().await.unwrap();
    assert_eq!(messages.len(), 1);

    let res = alice_test_user
        .user
        .process_handle_queue_message(alice_handle, messages.pop().unwrap())
        .await;
    res.unwrap_err().downcast::<BlockedContactError>().unwrap();

    // Unblock Bob
    alice_test_user
        .user
        .unblock_contact(bob.clone())
        .await
        .unwrap();

    // Sending messages works again
    setup.send_message(chat_id, &alice, vec![&bob]).await;
    setup.send_message(chat_id, &bob, vec![&alice]).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[tracing::instrument(name = "Delete account", skip_all)]
async fn delete_account() {
    let mut setup = TestBackend::single().await;

    let alice = setup.add_user().await;
    setup.get_user_mut(&alice).add_user_handle().await.unwrap();

    let bob = setup.add_user().await;

    let contact_chat_id = setup.connect_users(&alice, &bob).await;

    // Create a group with Alice and Bob
    let chat_id = setup.create_group(&alice).await;
    setup.invite_to_group(chat_id, &alice, vec![&bob]).await;

    // Delete the account
    let db_path = None;
    setup
        .get_user(&alice)
        .user
        .delete_account(db_path)
        .await
        .unwrap();

    // Check that Alice left the group
    let bob_user = &setup.get_user(&bob).user;
    let qs_messages = bob_user.qs_fetch_messages().await.unwrap();
    let result = bob_user.fully_process_qs_messages(qs_messages).await;
    assert!(result.errors.is_empty());

    let participants = setup
        .get_user(&bob)
        .user
        .chat_participants(contact_chat_id)
        .await
        .unwrap();
    assert_eq!(participants, [bob.clone()].into_iter().collect());

    let participants = setup
        .get_user(&bob)
        .user
        .chat_participants(chat_id)
        .await
        .unwrap();
    assert_eq!(participants, [bob.clone()].into_iter().collect());

    // After deletion, adding the user again should work.
    // Note: Since the user is ephemeral, there is nothing to test on the client side.
    let mut new_alice = TestUser::try_new(&alice, setup.server_url(), "DUMMY007")
        .await
        .unwrap();
    // Adding a user handle to the new user should work, because the previous user handle was
    // deleted.
    new_alice.add_user_handle().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[tracing::instrument(name = "Handle sanity checks test", skip_all)]
async fn handle_sanity_checks() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let bob = setup.add_user().await;

    let bob = setup.get_user_mut(&bob);
    let handle_record = bob.add_user_handle().await.unwrap();
    let bob_handle = handle_record.handle.clone();
    let bob_handle_hash = bob_handle.calculate_hash().unwrap();

    let alice = setup.get_user_mut(&alice);
    let handle_record = alice.add_user_handle().await.unwrap();
    let alice_handle = handle_record.handle.clone();
    let alice_handle_hash = alice_handle.calculate_hash().unwrap();
    let alice_user = &alice.user;
    let res = alice_user
        .add_contact(alice_handle.clone(), alice_handle_hash)
        .await
        .unwrap();
    assert!(
        matches!(
            res,
            AddHandleContactResult::Err(AddHandleContactError::OwnHandle)
        ),
        "Should not be able to add own handle as contact"
    );

    // Try to add Bob twice
    let res = alice_user
        .add_contact(bob_handle.clone(), bob_handle_hash)
        .await
        .unwrap();
    assert!(
        matches!(res, AddHandleContactResult::Ok(_)),
        "Should be able to add Bob as contact"
    );
    let res = alice_user
        .add_contact(bob_handle.clone(), bob_handle_hash)
        .await
        .unwrap();
    assert!(
        matches!(
            res,
            AddHandleContactResult::Err(AddHandleContactError::DuplicateRequest)
        ),
        "Should not be able to add Bob twice"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[tracing::instrument(name = "Check handle exists", skip_all)]
async fn check_handle_exists() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let alice_user = &setup.get_user(&alice).user;

    let random_number = rand::thread_rng().gen_range(100_000..1_000_000);
    let alice_handle = UserHandle::new(format!("alice-{}", random_number)).unwrap();

    let hash = alice_user
        .check_handle_exists(alice_handle.clone())
        .await
        .unwrap();
    assert!(hash.is_none(), "Alice's handle should not exist yet");

    alice_user
        .add_user_handle(alice_handle.clone())
        .await
        .unwrap();

    let hash = alice_user
        .check_handle_exists(alice_handle.clone())
        .await
        .unwrap();
    assert!(hash.is_some(), "Alice's handle should exist");

    alice_user.remove_user_handle(&alice_handle).await.unwrap();
    let hash = alice_user
        .check_handle_exists(alice_handle.clone())
        .await
        .unwrap();
    assert!(
        hash.is_none(),
        "Alice's handle should not exist after removal"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[tracing::instrument(name = "Safety codes", skip_all)]
async fn safety_codes() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let bob = setup.add_user().await;
    let charlie = setup.add_user().await;

    let _alice_bob_id = setup.connect_users(&alice, &bob).await;
    let _alice_charlie_id = setup.connect_users(&alice, &charlie).await;

    let group_id = setup.create_group(&alice).await;
    setup
        .invite_to_group(group_id, &alice, vec![&bob, &charlie])
        .await;

    // Have everyone compute everyones safety code and verify that they match
    let users = [&alice, &bob, &charlie];
    let mut codes = HashMap::new();
    for computing_user in &users {
        for user in users {
            let user_code = setup
                .get_user(computing_user)
                .user
                .safety_code(user)
                .await
                .unwrap();

            // If this is the first time we see a code for this user, store it
            // and continue.
            let Some(expected_code) = codes.get(user) else {
                codes.insert(user.clone(), user_code);
                continue;
            };

            assert_eq!(
                &user_code, expected_code,
                "Safety code for {:?} computed by {:?} does not match",
                user, computing_user
            );
            let expected_code_chunks = expected_code.to_chunks();
            let user_code_chunks = user_code.to_chunks();
            for chunk in &user_code_chunks {
                assert!(
                    *chunk < 100_000,
                    "Safety code chunk should be less than 100,000"
                );
            }
            assert_eq!(
                expected_code_chunks, user_code_chunks,
                "Safety code chunks for {:?} computed by {:?} do not match",
                user, computing_user
            );
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[tracing::instrument(name = "Adding a contact and changing user profile", skip_all)]
async fn add_contact_and_change_profile() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;

    let bob = setup.add_user().await;

    let alice_test_user = setup.get_user_mut(&alice);
    let alice_handle = alice_test_user.add_user_handle().await.unwrap();

    // Add Alice as a contact
    let bob_user = setup.get_user(&bob).user.clone();
    let res = bob_user
        .add_contact(alice_handle.handle.clone(), alice_handle.hash)
        .await
        .unwrap();
    let bob_alice_chat_id = match res {
        AddHandleContactResult::Ok(chat_id) => chat_id,
        AddHandleContactResult::Err(error) => {
            panic!("Unexpected error: {error:?}");
        }
    };

    // Change Bob's profile
    let bob_user_profile = UserProfile {
        user_id: bob.clone(),
        display_name: "B0b".parse().unwrap(),
        profile_picture: None,
    };

    bob_user
        .set_own_user_profile(bob_user_profile)
        .await
        .unwrap();

    // Fetch invitation from Bob and accept it
    let alice_user = &setup.get_user(&alice).user;
    let mut messages = alice_user.fetch_handle_messages().await.unwrap();
    assert_eq!(messages.len(), 1);
    let alice_bob_chat_id = alice_user
        .process_handle_queue_message(alice_handle.handle, messages.pop().unwrap())
        .await
        .unwrap();
    alice_user
        .accept_contact_request(alice_bob_chat_id)
        .await
        .unwrap();

    // Send message from Alice to Bob
    alice_user
        .send_message(
            alice_bob_chat_id,
            MimiContent::simple_markdown_message("hello".to_owned(), [0; 16]),
            None,
        )
        .await
        .unwrap();
    alice_user.outbound_service().run_once().await;

    // Bob receives invitation acceptance and sees the message from Alice
    let bob_user = &setup.get_user(&bob).user;
    let qs_messages = bob_user.qs_fetch_messages().await.unwrap();
    let res = bob_user.fully_process_qs_messages(qs_messages).await;
    assert!(res.errors.is_empty());

    let messages = bob_user.messages(bob_alice_chat_id, 10).await.unwrap();
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
