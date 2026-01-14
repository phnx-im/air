// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircoreclient::{EventMessage, Message, SystemMessage, store::Store};
use airserver_test_harness::utils::setup::TestBackend;

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
    setup.send_message(chat_alice_bob, &alice, vec![&bob]).await;
    setup.send_message(chat_alice_bob, &bob, vec![&alice]).await;

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
        group_chat.attributes().title,
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
