// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Connecting users from a shared group via targeted messages.

use aircoreclient::{ChatId, EventMessage, Message, SystemMessage, clients::CoreUser};
use airserver_test_harness::utils::{core_user_ext::CoreUserExt, setup::TestBackend};

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn connect_from_group_creates_contacts() {
    let mut setup = TestBackend::single().await;
    let [alice, bob, charlie] = setup.add_users().await;

    // Alice is connected to Bob and Charlie, but Bob and Charlie are not connected.
    setup.connect_users(&alice, &bob).await;
    setup.connect_users(&alice, &charlie).await;

    // Alice creates a group and invites Bob and Charlie
    let group_chat_id = setup.create_group(&alice).await;
    setup
        .invite_to_group(group_chat_id, &alice, &[&bob, &charlie])
        .await;

    // Bob now connects to Charlie via a targeted message sent through the
    // shared group.
    let bob_chat_id = bob
        .add_contact_from_group(group_chat_id, charlie.user_id().clone(), setup.apq_groups)
        .await
        .unwrap();

    // Bob should have the right system message in the chat
    let chat_message = bob.messages(bob_chat_id, 1).await.unwrap().pop().unwrap();
    let Message::Event(EventMessage::System(SystemMessage::SentGroupConnectionRequest {
        recipient,
        origin_chat_id,
    })) = chat_message.message()
    else {
        panic!("Expected SentGroupConnectionRequest system message");
    };
    assert!(
        *recipient == *charlie.user_id(),
        "System message should indicate connection to Charlie"
    );
    assert_eq!(*origin_chat_id, group_chat_id);

    // Charlie picks up his messages
    let mut result = charlie.drain().await;

    // Charlie accepts the connection request
    charlie
        .accept_contact_request(bob_chat_id)
        .await
        .unwrap()
        .unwrap();

    // Charlie should have two messages in the new chat
    let charlie_chat_id = result.new_connections.pop().unwrap();
    let messages = charlie.messages(charlie_chat_id, 2).await.unwrap();
    let Message::Event(EventMessage::System(SystemMessage::ReceivedGroupConnectionRequest {
        sender,
        origin_chat_id,
    })) = messages[0].message()
    else {
        panic!("Expected ReceivedGroupConnectionRequest system message");
    };
    assert_eq!(
        *sender,
        *bob.user_id(),
        "System message should indicate connection from Bob"
    );
    assert_eq!(
        *origin_chat_id, group_chat_id,
        "System message should reference the group chat"
    );
    let Message::Event(EventMessage::System(SystemMessage::AcceptedConnectionRequest {
        contact,
        user_handle: None,
    })) = messages[1].message()
    else {
        panic!("Expected AcceptedConnectionRequest system message");
    };
    assert_eq!(
        *contact,
        *bob.user_id(),
        "System message should indicate acceptance of connection from Bob"
    );

    // Now Bob picks up his messages
    bob.drain().await;
    // Bob should have a system message indicating that Charlie accepted the connection
    let messages = bob.messages(bob_chat_id, 1).await.unwrap();
    let Message::Event(EventMessage::System(SystemMessage::ReceivedConnectionConfirmation {
        sender,
        user_handle: None,
    })) = messages[0].message()
    else {
        panic!("Expected ReceivedConnectionConfirmation system message");
    };
    assert!(
        *sender == *charlie.user_id(),
        "System message should indicate acceptance of connection from Charlie"
    );

    // Bob and Charlie should now be connected
    let bob_contact = bob.contact(charlie.user_id()).await;
    assert!(
        bob_contact.is_some(),
        "Bob should have Charlie as a contact"
    );
    let charlie_contact = charlie.contact(bob.user_id()).await;
    assert!(
        charlie_contact.is_some(),
        "Charlie should have Bob as a contact"
    );

    // The connection group is APQ iff requested, and both sides agree on it.
    assert_eq!(bob.chat_is_apq(bob_chat_id).await, Some(setup.apq_groups));
    assert_eq!(
        charlie.chat_is_apq(charlie_chat_id).await,
        Some(setup.apq_groups)
    );
}

/// Connects `initiator` to `peer` through the shared group and returns the connection chat as
/// seen by the initiator and by the peer.
async fn connect_from_group(
    group_chat_id: ChatId,
    initiator: &CoreUser,
    peer: &CoreUser,
    prefer_apq: bool,
) -> (ChatId, ChatId) {
    let initiator_chat_id = initiator
        .add_contact_from_group(group_chat_id, peer.user_id().clone(), prefer_apq)
        .await
        .unwrap();

    let mut result = peer.drain().await;
    let peer_chat_id = result.new_connections.pop().unwrap();
    peer.accept_contact_request(peer_chat_id)
        .await
        .unwrap()
        .unwrap();

    initiator.drain().await;

    assert!(initiator.contact(peer.user_id()).await.is_some());
    assert!(peer.contact(initiator.user_id()).await.is_some());
    (initiator_chat_id, peer_chat_id)
}

/// The connection request rides the APQ origin group as a T targeted message, and the connection
/// group itself is APQ.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn connect_from_group_creates_contacts_apq() {
    let mut setup = TestBackend::single().await;
    let [alice, bob, charlie] = setup.add_users().await;

    let group_chat_id = setup.connected_apq_group(&alice, &[&bob, &charlie]).await;

    let (bob_chat_id, charlie_chat_id) =
        connect_from_group(group_chat_id, &bob, &charlie, true).await;

    assert_eq!(bob.chat_is_apq(bob_chat_id).await, Some(true));
    assert_eq!(charlie.chat_is_apq(charlie_chat_id).await, Some(true));

    setup.send_message(bob_chat_id, &bob, &[&charlie]).await;
    setup.send_message(bob_chat_id, &charlie, &[&bob]).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn connect_from_group_to_contact_or_pending_is_rejected() {
    let mut setup = TestBackend::single().await;
    let [alice, bob, charlie] = setup.add_users().await;

    // Alice is connected to Bob and Charlie, but Bob and Charlie are not connected.
    setup.connect_users(&alice, &bob).await;
    setup.connect_users(&alice, &charlie).await;

    // Alice creates a group and invites Bob and Charlie
    let group_chat_id = setup.create_group(&alice).await;
    setup
        .invite_to_group(group_chat_id, &alice, &[&bob, &charlie])
        .await;

    // Alice shouldn't be able to add Bob as a contact from the group, since they are already connected.
    let res = alice
        .add_contact_from_group(group_chat_id, bob.user_id().clone(), setup.apq_groups)
        .await;
    assert!(
        res.is_err(),
        "Alice should not be able to add Bob as a contact from the group since they are already connected"
    );

    // Bob now connects to Charlie via a targeted message sent through the
    // shared group.
    bob.add_contact_from_group(group_chat_id, charlie.user_id().clone(), setup.apq_groups)
        .await
        .unwrap();

    // Bob shouldn't be able to add Charlie again.
    let res = bob
        .add_contact_from_group(group_chat_id, charlie.user_id().clone(), setup.apq_groups)
        .await;
    assert!(
        res.is_err(),
        "Bob should not be able to add Charlie again as a contact from the group"
    );
}
