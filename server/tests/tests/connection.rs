// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::time::Duration;

use aircommon::component::{AirComponent, AirFeatures};
use aircommon::time::TimeStamp;
use aircoreclient::{
    ChatId, EventMessage, Message, SystemMessage, clients::CoreUser, store::Store,
};
use airserver_test_harness::utils::setup::TestBackend;
use chrono::{DateTime, TimeZone};
use tokio::task::spawn_blocking;
use tokio_stream::StreamExt;

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[tracing::instrument(name = "Connect users test", skip_all)]
async fn connect_users_via_user_handle() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let bob = setup.add_user().await;
    setup.connect_users(&alice, &bob).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[tracing::instrument(name = "Communication and persistence", skip_all)]
async fn communication_and_persistence() {
    let mut setup = TestBackend::single().await;

    // Create alice and bob
    let alice = setup.add_user().await;
    let bob = setup.add_user().await;

    // Connect them
    let chat_alice_bob = setup.connect_users(&alice, &bob).await;

    // Test the connection chat by sending messages back and forth.
    setup
        .send_message(chat_alice_bob, &alice, vec![&bob], None)
        .await;
    setup
        .send_message(chat_alice_bob, &bob, vec![&alice], None)
        .await;

    let count_18 = setup
        .scan_database("\x18", true, vec![&alice, &bob])
        .await
        .len();
    let count_19 = setup
        .scan_database("\x19", true, vec![&alice, &bob])
        .await
        .len();

    let good = count_18 < count_19 * 3 / 2;

    // TODO: Remove the ! in front of !good when we have fixed our code.
    assert!(
        !good,
        "Having too many 0x18 is an indicator for using Vec<u8> instead of ByteBuf"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[tracing::instrument(name = "Connect users via targeted message", skip_all)]
async fn connect_users_via_targeted_message() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let bob = setup.add_user().await;
    let charlie = setup.add_user().await;

    // Alice is connected to Bob and Charlie, but Bob and Charlie are not connected.
    setup.connect_users(&alice, &bob).await;
    setup.connect_users(&alice, &charlie).await;

    // Alice creates a group and invites Bob and Charlie
    let group_chat_id = setup.create_group(&alice).await;
    setup
        .invite_to_group(group_chat_id, &alice, vec![&bob, &charlie])
        .await;
    let alice_user = &setup.get_user(&alice).user;
    let group_chat = alice_user.chat(&group_chat_id).await.unwrap();

    // Bob now connects to Charlie via a targeted message sent through the
    // shared group.
    let bob_user = &setup.get_user(&bob).user;
    let bob_chat_id = bob_user
        .add_contact_from_group(group_chat_id, charlie.clone())
        .await
        .unwrap();

    // Bob should have the right system message in the chat
    let chat_message = bob_user
        .messages(bob_chat_id, 1)
        .await
        .unwrap()
        .pop()
        .unwrap();
    let Message::Event(EventMessage::System(SystemMessage::NewDirectConnectionChat(user_id))) =
        chat_message.message()
    else {
        panic!("Expected NewDirectConnectionChat system message");
    };
    assert!(
        *user_id == charlie,
        "System message should indicate connection to Charlie"
    );

    // Charlie picks up his messages
    let charlie_user = &setup.get_user(&charlie).user;
    let qs_messages = charlie_user.qs_fetch_messages().await.unwrap();
    let mut result = charlie_user.fully_process_qs_messages(qs_messages).await;
    assert!(
        result.errors.is_empty(),
        "Charlie should process Bob's targeted message without errors"
    );

    // Charlie accepts the connection request
    charlie_user
        .accept_contact_request(bob_chat_id)
        .await
        .unwrap()
        .unwrap();

    // Charlie should have two messages in the new chat
    let charlie_chat_id = result.new_connections.pop().unwrap();
    let messages = charlie_user.messages(charlie_chat_id, 2).await.unwrap();
    let Message::Event(EventMessage::System(SystemMessage::ReceivedDirectConnectionRequest {
        sender,
        chat_name,
    })) = messages[0].message()
    else {
        panic!("Expected NewDirectConnectionChat system message");
    };
    assert_eq!(
        *sender, bob,
        "System message should indicate connection from Bob"
    );
    assert_eq!(
        *chat_name,
        group_chat.attributes().unwrap().title,
        "System message should have the correct chat title"
    );
    let Message::Event(EventMessage::System(SystemMessage::AcceptedConnectionRequest {
        contact,
        user_handle: None,
    })) = messages[1].message()
    else {
        panic!("Expected AcceptedConnectionRequest system message");
    };
    assert_eq!(
        *contact, bob,
        "System message should indicate acceptance of connection from Bob"
    );

    // Now Bob picks up his messages
    let bob_user = &setup.get_user(&bob).user;
    let qs_messages = bob_user.qs_fetch_messages().await.unwrap();
    let result = bob_user.fully_process_qs_messages(qs_messages).await;
    assert!(
        result.errors.is_empty(),
        "Bob should process Charlie's response without errors"
    );
    // Bob should have a system message indicating that Charlie accepted the connection
    let messages = bob_user.messages(bob_chat_id, 1).await.unwrap();
    let Message::Event(EventMessage::System(SystemMessage::ReceivedConnectionConfirmation {
        sender,
        user_handle: None,
    })) = messages[0].message()
    else {
        panic!("Expected ReceivedConnectionConfirmation system message");
    };
    assert!(
        *sender == charlie,
        "System message should indicate acceptance of connection from Charlie"
    );

    // Bob and Charlie should now be connected
    let bob_contact = bob_user.contact(&charlie).await;
    assert!(
        bob_contact.is_some(),
        "Bob should have Charlie as a contact"
    );
    let charlie_user = &setup.get_user(&charlie).user;
    let charlie_contact = charlie_user.contact(&bob).await;
    assert!(
        charlie_contact.is_some(),
        "Charlie should have Bob as a contact"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[tracing::instrument(name = "Sanity checks for targeted message connections", skip_all)]
async fn sanity_checks_for_targeted_message_connections() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let bob = setup.add_user().await;
    let charlie = setup.add_user().await;

    // Alice is connected to Bob and Charlie, but Bob and Charlie are not connected.
    setup.connect_users(&alice, &bob).await;
    setup.connect_users(&alice, &charlie).await;

    // Alice creates a group and invites Bob and Charlie
    let group_chat_id = setup.create_group(&alice).await;
    setup
        .invite_to_group(group_chat_id, &alice, vec![&bob, &charlie])
        .await;

    // Alice shouldn't be able to add Bob as a contact from the group, since they are already connected.
    let alice = setup.get_user(&alice);
    let alice_user = &alice.user;
    let res = alice_user
        .add_contact_from_group(group_chat_id, bob.clone())
        .await;
    assert!(
        res.is_err(),
        "Alice should not be able to add Bob as a contact from the group since they are already connected"
    );

    // Bob now connects to Charlie via a targeted message sent through the
    // shared group.
    let bob = setup.get_user(&bob);
    let bob_user = &bob.user;
    bob_user
        .add_contact_from_group(group_chat_id, charlie.clone())
        .await
        .unwrap();

    // Bob shouldn't be able to add Charlie again.
    let res = bob_user
        .add_contact_from_group(group_chat_id, charlie.clone())
        .await;
    assert!(
        res.is_err(),
        "Bob should not be able to add Charlie again as a contact from the group"
    );
}

/// Test that the timestamp on a received connection request reflects when the
/// request was sent (server's enqueue time), not when the recipient processed it.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[tracing::instrument(name = "Connection request timestamp test", skip_all)]
async fn connection_request_has_server_timestamp() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let bob = setup.add_user().await;

    // Bob adds a username
    let test_bob = setup.get_user_mut(&bob);
    let bob_username_record = test_bob.add_username().await.unwrap();
    let bob_username = bob_username_record.username.clone();

    // Alice sends a connection request to Bob
    let test_alice = setup.get_user_mut(&alice);
    let alice_user = &mut test_alice.user;
    let username_hash = spawn_blocking({
        let username = bob_username.clone();
        move || username.calculate_hash().unwrap()
    })
    .await
    .unwrap();

    alice_user
        .add_contact(bob_username.clone(), username_hash)
        .await
        .expect("fatal error")
        .expect("non-fatal error");

    // Bob fetches and processes the connection request
    let test_bob = setup.get_user_mut(&bob);
    let bob_user = &mut test_bob.user;
    let (mut stream, responder) = bob_user
        .listen_username(&bob_username_record)
        .await
        .unwrap();

    // Process handle queue messages, extracting the server timestamp before processing
    let mut bob_chat_id = None;
    let mut server_timestamp = None;
    while let Some(Some(message)) = tokio::time::timeout(Duration::from_millis(500), stream.next())
        .await
        .unwrap()
    {
        let message_id = message.message_id.unwrap();

        // Extract the server's created_at timestamp from the message
        let created_at = message
            .created_at
            .as_ref()
            .expect("Message should have created_at timestamp");
        server_timestamp = Some(TimeStamp::from(
            chrono::Utc
                .timestamp_opt(created_at.seconds, created_at.nanos as u32)
                .single()
                .expect("Valid timestamp"),
        ));

        let chat_id = bob_user
            .process_username_queue_message(bob_username_record.username.clone(), message)
            .await
            .unwrap();
        bob_chat_id = Some(chat_id);
        responder.ack(message_id.into()).await;
    }

    let bob_chat_id = bob_chat_id.expect("Bob should have processed at least one message");
    let server_timestamp = server_timestamp.expect("Should have captured server timestamp");

    // Get the system message and its timestamp
    let messages = bob_user.messages(bob_chat_id, 1).await.unwrap();
    let received_request_message = messages.first().expect("Should have at least one message");

    let Message::Event(EventMessage::System(SystemMessage::ReceivedHandleConnectionRequest {
        sender,
        user_handle,
    })) = received_request_message.message()
    else {
        panic!("Expected ReceivedHandleConnectionRequest system message");
    };

    assert_eq!(
        *sender, alice,
        "System message should indicate connection from Alice"
    );
    assert_eq!(
        *user_handle, bob_username,
        "System message should have the correct username"
    );

    // The system message timestamp should exactly match the server's created_at timestamp
    let message_timestamp = received_request_message.timestamp();
    let server_timestamp_chrono: chrono::DateTime<chrono::Utc> = server_timestamp.into();

    assert_eq!(
        message_timestamp, server_timestamp_chrono,
        "System message timestamp should match the server's created_at timestamp exactly"
    );
}

/// Helper: trigger the self-update timed task for the given chat.
async fn run_self_update(user: &CoreUser, chat_id: ChatId) {
    user.set_self_updated_at(chat_id, DateTime::UNIX_EPOCH)
        .await
        .unwrap();
    user.outbound_service()
        .schedule_self_update(DateTime::UNIX_EPOCH)
        .await
        .unwrap();
    user.outbound_service().run_once().await;
}

/// New connection chats start with a non-empty group data extension (an empty legacy_title for
/// backward compatibility with old clients). After self-update, a client supporting
/// `empty_connection_group_attributes` erases that data. The other side sees the erasure after
/// fetching the commit. No system messages are produced.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[tracing::instrument(name = "Erase connection group data on self-update", skip_all)]
async fn erase_connection_group_data_on_self_update() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let bob = setup.add_user().await;

    let chat_id = setup.connect_users(&alice, &bob).await;

    let alice_user = &setup.get_user(&alice).user;
    let bob_user = &setup.get_user(&bob).user;

    // New connection chats have legacy_title: Some("") set for backward compat => not fully empty.
    let group_data = alice_user.group_data(chat_id).await.unwrap().unwrap();
    assert!(
        !group_data.is_empty(),
        "New connection group data should not be empty before migration"
    );
    assert_eq!(
        group_data.legacy_title.as_deref(),
        Some(""),
        "New connection group should have an empty legacy_title"
    );

    let alice_messages_before = alice_user.messages(chat_id, 100).await.unwrap();
    let bob_messages_before = bob_user.messages(chat_id, 100).await.unwrap();

    // Alice self-updates: the migration erases the group data extension.
    run_self_update(alice_user, chat_id).await;

    let group_data = alice_user.group_data(chat_id).await.unwrap().unwrap();
    assert!(
        group_data.is_empty(),
        "Alice's connection group data should be fully erased after self-update"
    );

    // Bob fetches and processes Alice's commit.
    let qs_messages = bob_user.qs_fetch_messages().await.unwrap();
    let result = bob_user.fully_process_qs_messages(qs_messages).await;
    assert!(
        result.errors.is_empty(),
        "Bob should process Alice's commit without errors: {:?}",
        result.errors
    );

    let group_data = bob_user.group_data(chat_id).await.unwrap().unwrap();
    assert!(
        group_data.is_empty(),
        "Bob's connection group data should be erased after processing Alice's commit"
    );

    // Connection chats carry no title or picture => attributes is None.
    let alice_chat = alice_user.chat(&chat_id).await.unwrap();
    assert_eq!(alice_chat.attributes().map(|a| a.title()), None);
    assert_eq!(alice_chat.attributes().and_then(|a| a.picture()), None);
    let bob_chat = bob_user.chat(&chat_id).await.unwrap();
    assert_eq!(bob_chat.attributes().map(|a| a.title()), None);
    assert_eq!(bob_chat.attributes().and_then(|a| a.picture()), None);

    // Erasing group data must not produce any system messages on either side.
    let alice_messages_after = alice_user.messages(chat_id, 100).await.unwrap();
    let bob_messages_after = bob_user.messages(chat_id, 100).await.unwrap();
    assert_eq!(
        alice_messages_before, alice_messages_after,
        "Erasing connection group data should not produce messages for Alice"
    );
    assert_eq!(
        bob_messages_before, bob_messages_after,
        "Erasing connection group data should not produce messages for Bob"
    );

    // Running self-update again does not produce another commit — the data is already empty.
    run_self_update(alice_user, chat_id).await;
    let group_data = alice_user.group_data(chat_id).await.unwrap().unwrap();
    assert!(
        group_data.is_empty(),
        "Connection group data should remain empty after a redundant self-update"
    );
}

/// Legacy connection group data (title + picture written in the old plaintext format) is erased on
/// self-update for clients supporting `empty_connection_group_attributes`, rather than being
/// migrated to the new encrypted format as would happen for regular group chats. No system
/// messages are produced.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[tracing::instrument(name = "Erase legacy connection group data on self-update", skip_all)]
async fn erase_legacy_connection_group_data_on_self_update() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let bob = setup.add_user().await;

    let chat_id = setup.connect_users(&alice, &bob).await;

    let alice_user = &setup.get_user(&alice).user;
    let bob_user = &setup.get_user(&bob).user;

    // Simulate an old client that stored a plaintext title and picture in the group extension.
    alice_user
        .set_legacy_group_data(chat_id, "Alice & Bob".to_owned(), Some(vec![1, 2, 3]))
        .await
        .unwrap();

    // Bob picks up the commit containing the legacy data.
    let qs_messages = bob_user.qs_fetch_messages().await.unwrap();
    let result = bob_user.fully_process_qs_messages(qs_messages).await;
    assert!(
        result.errors.is_empty(),
        "Bob should process the legacy group data commit without errors: {:?}",
        result.errors
    );

    // Both sides can read the legacy title before migration.
    let group_data = alice_user.group_data(chat_id).await.unwrap().unwrap();
    assert_eq!(group_data.legacy_title.as_deref(), Some("Alice & Bob"));
    assert!(group_data.legacy_picture.is_some());
    let group_data = bob_user.group_data(chat_id).await.unwrap().unwrap();
    assert_eq!(group_data.legacy_title.as_deref(), Some("Alice & Bob"));
    assert!(group_data.legacy_picture.is_some());

    let alice_messages_before = alice_user.messages(chat_id, 100).await.unwrap();
    let bob_messages_before = bob_user.messages(chat_id, 100).await.unwrap();

    // Alice self-updates: connection chat data is erased, NOT migrated to the encrypted format.
    run_self_update(alice_user, chat_id).await;

    let group_data = alice_user.group_data(chat_id).await.unwrap().unwrap();
    assert!(
        group_data.is_empty(),
        "Legacy connection group data should be erased (not migrated) after self-update"
    );
    assert!(
        group_data.encrypted_title.is_none(),
        "Connection group data should not be migrated to the encrypted format"
    );

    // Bob fetches and processes Alice's erasure commit.
    let qs_messages = bob_user.qs_fetch_messages().await.unwrap();
    let result = bob_user.fully_process_qs_messages(qs_messages).await;
    assert!(
        result.errors.is_empty(),
        "Bob should process Alice's erasure commit without errors: {:?}",
        result.errors
    );

    let group_data = bob_user.group_data(chat_id).await.unwrap().unwrap();
    assert!(
        group_data.is_empty(),
        "Bob's connection group data should also be erased after processing Alice's commit"
    );

    // Connection chats carry no title or picture => attributes is None.
    let alice_chat = alice_user.chat(&chat_id).await.unwrap();
    assert_eq!(alice_chat.attributes().map(|a| a.title()), None);
    assert_eq!(alice_chat.attributes().and_then(|a| a.picture()), None);
    let bob_chat = bob_user.chat(&chat_id).await.unwrap();
    assert_eq!(bob_chat.attributes().map(|a| a.title()), None);
    assert_eq!(bob_chat.attributes().and_then(|a| a.picture()), None);

    // Erasing group data must not produce any system messages on either side.
    let alice_messages_after = alice_user.messages(chat_id, 100).await.unwrap();
    let bob_messages_after = bob_user.messages(chat_id, 100).await.unwrap();
    assert_eq!(
        alice_messages_before, alice_messages_after,
        "Erasing legacy connection group data should not produce messages for Alice"
    );
    assert_eq!(
        bob_messages_before, bob_messages_after,
        "Erasing legacy connection group data should not produce messages for Bob"
    );
}

/// When Bob has `empty_connection_group_attributes = false` (old client) and Alice has it `true`
/// (new client), neither side should erase the group data: erasure requires all members to support
/// the flag. Once Bob "upgrades" and sets his flag to `true`, Alice's next self-update does erase
/// the data and Bob sees the result.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[tracing::instrument(name = "Erase connection group data mixed feature support", skip_all)]
async fn erase_connection_group_data_mixed_feature_support() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let bob = setup.add_user().await;

    let chat_id = setup.connect_users(&alice, &bob).await;

    let alice_user = &setup.get_user(&alice).user;
    let bob_user = &setup.get_user(&bob).user;

    // Simulate Bob being an old client: downgrade his leaf node to advertise
    // empty_connection_group_attributes = false.
    let old_air_component = AirComponent {
        features: AirFeatures {
            encrypted_group_profiles: true,
            empty_connection_group_attributes: false,
            pq_groups: setup.apq_groups,
        },
    };
    bob_user
        .set_group_air_component(chat_id, old_air_component)
        .await
        .unwrap();

    // Alice processes Bob's leaf-node downgrade commit.
    let qs_messages = alice_user.qs_fetch_messages().await.unwrap();
    let result = alice_user.fully_process_qs_messages(qs_messages).await;
    assert!(
        result.errors.is_empty(),
        "Alice should process Bob's downgrade commit without errors: {:?}",
        result.errors
    );

    let alice_messages_before = alice_user.messages(chat_id, 100).await.unwrap();
    let bob_messages_before = bob_user.messages(chat_id, 100).await.unwrap();

    // Alice self-updates: Bob lacks the flag, so no erasure should happen even though Alice
    // supports it. Erasure requires ALL members to have the flag.
    run_self_update(alice_user, chat_id).await;

    let group_data = alice_user.group_data(chat_id).await.unwrap().unwrap();
    assert!(
        !group_data.is_empty(),
        "Alice should not erase connection group data while Bob lacks the feature flag"
    );

    // Bob self-updates: his own flag is false, so no erasure either.
    run_self_update(bob_user, chat_id).await;

    // Alice processes Bob's self-update.
    let qs_messages = alice_user.qs_fetch_messages().await.unwrap();
    let result = alice_user.fully_process_qs_messages(qs_messages).await;
    assert!(
        result.errors.is_empty(),
        "Alice should process Bob's self-update without errors: {:?}",
        result.errors
    );

    // Bob processes Alice's self-update.
    let qs_messages = bob_user.qs_fetch_messages().await.unwrap();
    let result = bob_user.fully_process_qs_messages(qs_messages).await;
    assert!(
        result.errors.is_empty(),
        "Bob should process Alice's self-update without errors: {:?}",
        result.errors
    );

    // Neither side erased the group data, and no system messages appeared.
    let group_data = bob_user.group_data(chat_id).await.unwrap().unwrap();
    assert!(
        !group_data.is_empty(),
        "Group data should still be present while Bob lacks the feature flag"
    );
    assert_eq!(
        alice_messages_before,
        alice_user.messages(chat_id, 100).await.unwrap(),
        "No messages should have been produced while Bob lacks the feature flag"
    );
    assert_eq!(
        bob_messages_before,
        bob_user.messages(chat_id, 100).await.unwrap(),
        "No messages should have been produced while Bob lacks the feature flag"
    );

    // Bob "upgrades": commit a leaf node that sets empty_connection_group_attributes = true.
    let new_air_component = AirComponent {
        features: AirFeatures {
            encrypted_group_profiles: true,
            empty_connection_group_attributes: true,
            pq_groups: setup.apq_groups,
        },
    };
    bob_user
        .set_group_air_component(chat_id, new_air_component)
        .await
        .unwrap();

    // Alice processes Bob's upgrade commit.
    let qs_messages = alice_user.qs_fetch_messages().await.unwrap();
    let result = alice_user.fully_process_qs_messages(qs_messages).await;
    assert!(
        result.errors.is_empty(),
        "Alice should process Bob's upgrade commit without errors: {:?}",
        result.errors
    );

    // Now all members support the flag. Alice's next self-update should erase the data.
    run_self_update(alice_user, chat_id).await;

    let group_data = alice_user.group_data(chat_id).await.unwrap().unwrap();
    assert!(
        group_data.is_empty(),
        "Alice should erase connection group data once all members support the feature flag"
    );

    // Bob processes Alice's erasure commit.
    let qs_messages = bob_user.qs_fetch_messages().await.unwrap();
    let result = bob_user.fully_process_qs_messages(qs_messages).await;
    assert!(
        result.errors.is_empty(),
        "Bob should process Alice's erasure commit without errors: {:?}",
        result.errors
    );

    let group_data = bob_user.group_data(chat_id).await.unwrap().unwrap();
    assert!(
        group_data.is_empty(),
        "Bob should see empty group data after processing Alice's erasure"
    );

    // Attributes are None for both sides.
    let alice_chat = alice_user.chat(&chat_id).await.unwrap();
    assert_eq!(alice_chat.attributes().map(|a| a.title()), None);
    assert_eq!(alice_chat.attributes().and_then(|a| a.picture()), None);
    let bob_chat = bob_user.chat(&chat_id).await.unwrap();
    assert_eq!(bob_chat.attributes().map(|a| a.title()), None);
    assert_eq!(bob_chat.attributes().and_then(|a| a.picture()), None);

    // The entire sequence must have produced no system messages.
    assert_eq!(
        alice_messages_before,
        alice_user.messages(chat_id, 100).await.unwrap(),
        "Mixed feature support test should not produce messages for Alice"
    );
    assert_eq!(
        bob_messages_before,
        bob_user.messages(chat_id, 100).await.unwrap(),
        "Mixed feature support test should not produce messages for Bob"
    );
}
