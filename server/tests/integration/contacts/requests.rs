// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! How the contact requests between two users fold into one chat and end with
//! one connection.

use aircommon::identifiers::{UserId, Username};
use aircoreclient::{
    ChatId, ChatStatus, ChatType, SystemMessage, UsernameRecord, clients::CoreUser,
};
use airserver_test_harness::utils::{core_user_ext::CoreUserExt, setup::TestBackend};

/// The chats of pending contact requests from `sender`.
pub(crate) async fn pending_chats_from(device: &CoreUser, sender: &UserId) -> Vec<ChatId> {
    let mut chats = Vec::new();
    for chat_id in device.ordered_chat_ids().await.unwrap() {
        let chat = device.expect_chat(chat_id).await;
        if chat.chat_type() == &ChatType::PendingConnection(sender.clone()) {
            chats.push(chat_id);
        }
    }
    chats
}

/// Asserts that the chat of a sent request is closed, with the record of the
/// request going to `contact`, who is a contact already, as its last message.
async fn assert_closed_as_connected(device: &CoreUser, chat_id: ChatId, contact: &UserId) {
    let chat = device.expect_chat(chat_id).await;
    assert!(chat.status().is_inactive(), "the request chat stays open");
    let messages = device.system_messages(chat_id).await;
    let record = SystemMessage::SentConnectionRequestAlreadyConnected {
        contact: contact.clone(),
    };
    assert_eq!(messages.last(), Some(&record), "got {messages:?}");
    let records = messages
        .iter()
        .filter(|message| **message == record)
        .count();
    assert_eq!(records, 1, "got {messages:?}");
}

/// A username unique to `user` that differs from the one the harness
/// registers, with the tokens to register it.
pub(crate) async fn prepare_username(user: &CoreUser) -> Username {
    // Fills the token cache
    user.outbound_service().run_once().await;
    let suffix: String = user
        .user_id()
        .uuid()
        .simple()
        .to_string()
        .chars()
        .take(16)
        .collect();
    Username::new(format!("extra-{suffix}")).unwrap()
}

/// A username of `user` besides the one the harness registers.
pub(crate) async fn add_second_username(user: &CoreUser) -> UsernameRecord {
    let username = prepare_username(user).await;
    user.add_username(username).await.unwrap().unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn requests_of_one_sender_fold_into_newest_chat() {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    let first = setup.add_username(&bob).await;
    let second = add_second_username(&bob).await;
    let mut sent = Vec::new();
    for record in [&first, &second] {
        let chat_id = alice
            .add_contact(record.username.clone(), record.hash, setup.apq_groups)
            .await
            .unwrap()
            .unwrap();
        sent.push(chat_id);
    }

    bob.receive_connection_offer(&first).await;
    let chat_id = bob.receive_connection_offer(&second).await;
    assert_eq!(
        chat_id, sent[1],
        "the chat is the one of the newest request"
    );
    assert!(
        bob.chat(&sent[0]).await.is_none(),
        "the newer request took over the chat of the older one"
    );
    let messages = bob.system_messages(chat_id).await;
    assert!(
        messages.contains(&SystemMessage::ReceivedHandleConnectionRequest {
            sender: alice.user_id().clone(),
            user_handle: first.username.clone(),
        })
    );
    assert!(messages.contains(
        &SystemMessage::ReceivedAdditionalUsernameConnectionRequest {
            sender: alice.user_id().clone(),
            username: second.username.clone(),
        }
    ));

    bob.accept_contact_request(chat_id).await.unwrap().unwrap();
    bob.outbound_service().run_once().await;
    alice.drain().await;

    assert_eq!(
        alice.expect_chat(sent[1]).await.chat_type(),
        &ChatType::Connection(bob.user_id().clone())
    );
    assert!(
        pending_chats_from(&bob, alice.user_id()).await.is_empty(),
        "accepting settles the older request too"
    );
    assert_eq!(bob.contacts().await.unwrap().len(), 1);
    assert_closed_as_connected(&alice, sent[0], bob.user_id()).await;
    assert_eq!(bob.parked_request_closes().await.unwrap(), 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn stale_offer_after_accept_is_dropped() {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    let first = setup.add_username(&bob).await;
    let second = add_second_username(&bob).await;
    let mut sent = Vec::new();
    for record in [&first, &second] {
        let chat_id = alice
            .add_contact(record.username.clone(), record.hash, setup.apq_groups)
            .await
            .unwrap()
            .unwrap();
        sent.push(chat_id);
    }

    // The newer offer is accepted while the older one still waits in its
    // queue.
    let chat_id = bob.receive_connection_offer(&second).await;
    bob.accept_contact_request(chat_id).await.unwrap().unwrap();
    alice.drain().await;

    assert_eq!(bob.drain_username_queue(&first).await, None);
    assert!(
        pending_chats_from(&bob, alice.user_id()).await.is_empty(),
        "an offer from a connected contact is stale"
    );
    assert_eq!(
        bob.expect_chat(chat_id).await.chat_type(),
        &ChatType::Connection(alice.user_id().clone())
    );

    bob.outbound_service().run_once().await;
    alice.drain().await;
    assert_closed_as_connected(&alice, sent[0], bob.user_id()).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn crossed_requests_leave_one_connection() {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    let alice_record = setup.add_username(&alice).await;
    let bob_record = setup.add_username(&bob).await;

    let alice_request = alice
        .add_contact(
            bob_record.username.clone(),
            bob_record.hash,
            setup.apq_groups,
        )
        .await
        .unwrap()
        .unwrap();
    bob.add_contact(
        alice_record.username.clone(),
        alice_record.hash,
        setup.apq_groups,
    )
    .await
    .unwrap()
    .unwrap();

    // Alice accepts Bob's request. Bob's copy of Alice's request goes once he
    // learns of the connection.
    let bob_request_at_alice = alice.receive_connection_offer(&alice_record).await;
    let alice_request_at_bob = bob.receive_connection_offer(&bob_record).await;
    alice
        .accept_contact_request(bob_request_at_alice)
        .await
        .unwrap()
        .unwrap();
    bob.drain().await;

    assert!(
        bob.chat(&alice_request_at_bob).await.is_none(),
        "the request of a new contact is discarded"
    );
    assert_eq!(alice.contacts().await.unwrap().len(), 1);
    assert_eq!(
        bob.contacts().await.unwrap().len(),
        1,
        "the two stay connected through one chat"
    );

    bob.outbound_service().run_once().await;
    alice.drain().await;
    assert_closed_as_connected(&alice, alice_request, bob.user_id()).await;
    assert_eq!(alice.contacts().await.unwrap().len(), 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn asking_back_opens_pending_request() {
    let mut setup = TestBackend::single().await;
    let [alice, bob, charlie] = setup.add_users().await;
    // Alice and Bob share a group, through Charlie.
    let group_chat_id = setup.connected_group(&charlie, &[&alice, &bob]).await;

    let alice_chat_id = alice
        .add_contact_from_group(group_chat_id, bob.user_id().clone(), setup.apq_groups)
        .await
        .unwrap();
    let processed = bob.drain().await;
    let [bob_chat_id] = processed.new_connections.as_slice() else {
        panic!("expected one request, got {:?}", processed.new_connections);
    };

    let bob_again = bob
        .add_contact_from_group(group_chat_id, alice.user_id().clone(), setup.apq_groups)
        .await
        .unwrap();
    assert_eq!(bob_again, *bob_chat_id, "the pending request opens instead");

    bob.accept_contact_request(*bob_chat_id)
        .await
        .unwrap()
        .unwrap();
    alice.drain().await;
    assert_eq!(
        alice.expect_chat(alice_chat_id).await.chat_type(),
        &ChatType::Connection(bob.user_id().clone())
    );
    assert_eq!(alice.contacts().await.unwrap().len(), 2);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[ignore = "retracting an already accepted request leaves the peers disagreeing"]
async fn retracting_accepted_request_keeps_peers_consistent() {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    let bob_record = setup.add_username(&bob).await;
    let chat_id = alice
        .add_contact(
            bob_record.username.clone(),
            bob_record.hash,
            setup.apq_groups,
        )
        .await
        .unwrap()
        .unwrap();
    let bob_chat_id = bob.receive_connection_offer(&bob_record).await;

    // Bob accepts before alice learns about it, so her delete commit is stale.
    let bob_chat_id = bob
        .accept_contact_request(bob_chat_id)
        .await
        .unwrap()
        .unwrap();
    let _ = alice.retract_contact_request(chat_id).await;
    alice.outbound_service().run_once().await;
    alice.drain_unchecked().await;
    bob.drain_unchecked().await;

    let alice_connected = alice.chat(&chat_id).await.is_some_and(|chat| {
        chat.status() == &ChatStatus::Active
            && chat.chat_type() == &ChatType::Connection(bob.user_id().clone())
    });
    let bob_connected = bob.chat(&bob_chat_id).await.is_some_and(|chat| {
        chat.status() == &ChatStatus::Active
            && chat.chat_type() == &ChatType::Connection(alice.user_id().clone())
    });
    assert_eq!(
        alice_connected, bob_connected,
        "alice connected: {alice_connected}, bob connected: {bob_connected}"
    );
}

/// Sends a request from `alice` to a new username of `bob`, who is her contact
/// already.
async fn request_to_contact(
    setup: &TestBackend,
    alice: &CoreUser,
    bob: &CoreUser,
) -> (ChatId, UsernameRecord) {
    let record = setup.add_username(bob).await;
    let chat_id = alice
        .add_contact(record.username.clone(), record.hash, setup.apq_groups)
        .await
        .unwrap()
        .unwrap();
    (chat_id, record)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn request_to_a_contact_ends_with_a_record() {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    let connection = setup.connect_users(&alice, &bob).await;
    let (chat_id, record) = request_to_contact(&setup, &alice, &bob).await;

    assert_eq!(
        bob.drain_username_queue(&record).await,
        None,
        "the request of a contact is not shown"
    );
    assert_eq!(bob.parked_request_closes().await.unwrap(), 1);
    bob.outbound_service().run_once().await;
    assert_eq!(bob.parked_request_closes().await.unwrap(), 0);
    alice.drain().await;

    assert_closed_as_connected(&alice, chat_id, bob.user_id()).await;
    assert!(
        alice.username_contacts().await.unwrap().is_empty(),
        "the partial contact of the request is gone"
    );
    assert_eq!(
        alice.contact(bob.user_id()).await.unwrap().chat_id,
        connection,
        "the contact keeps its chat"
    );
    assert_eq!(
        bob.contact(alice.user_id()).await.unwrap().chat_id,
        connection
    );
    assert!(pending_chats_from(&bob, alice.user_id()).await.is_empty());

    // The username is free for a new request.
    let again = alice
        .add_contact(record.username.clone(), record.hash, setup.apq_groups)
        .await
        .unwrap();
    assert!(again.is_ok(), "got {again:?}");
}

/// Both of Bob's devices learn of the connection and close Alice's request.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn crossed_requests_close_once_with_a_linked_receiver() {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    let bob2 = bob.add_synced_device().await;
    let alice_record = setup.add_username(&alice).await;
    let bob_record = setup.add_username(&bob).await;

    let alice_request = alice
        .add_contact(
            bob_record.username.clone(),
            bob_record.hash,
            setup.apq_groups,
        )
        .await
        .unwrap()
        .unwrap();
    bob.add_contact(
        alice_record.username.clone(),
        alice_record.hash,
        setup.apq_groups,
    )
    .await
    .unwrap()
    .unwrap();
    bob2.drain().await;
    // Bob forwards Alice's request to his sibling.
    bob.receive_connection_offer(&bob_record).await;
    bob.outbound_service().run_once().await;
    bob2.drain().await;
    assert_eq!(pending_chats_from(&bob2, alice.user_id()).await.len(), 1);

    let bob_request_at_alice = alice.receive_connection_offer(&alice_record).await;
    alice
        .accept_contact_request(bob_request_at_alice)
        .await
        .unwrap()
        .unwrap();
    for device in [&bob, &bob2] {
        device.drain().await;
        assert_eq!(device.parked_request_closes().await.unwrap(), 1);
    }

    bob.outbound_service().run_once().await;
    bob2.outbound_service().run_once().await;
    for device in [&bob, &bob2] {
        assert_eq!(
            device.parked_request_closes().await.unwrap(),
            0,
            "the close that comes second finds the group closed"
        );
    }
    alice.drain().await;
    assert_closed_as_connected(&alice, alice_request, bob.user_id()).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn closed_request_ends_on_every_sender_device() {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    setup.connect_users(&alice, &bob).await;
    let alice2 = alice.add_synced_device().await;
    let (chat_id, record) = request_to_contact(&setup, &alice, &bob).await;
    alice2.drain().await;

    bob.drain_username_queue(&record).await;
    bob.outbound_service().run_once().await;
    for device in [&alice, &alice2] {
        device.drain().await;
        assert_closed_as_connected(device, chat_id, bob.user_id()).await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn missed_close_ends_with_a_record_through_resync() {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    setup.connect_users(&alice, &bob).await;
    let (chat_id, record) = request_to_contact(&setup, &alice, &bob).await;

    bob.drain_username_queue(&record).await;
    bob.outbound_service().run_once().await;
    alice.discard_queue().await;
    alice.enqueue_group_resync(chat_id).await.unwrap();
    alice.outbound_service().run_once().await;

    assert_closed_as_connected(&alice, chat_id, bob.user_id()).await;
    assert!(alice.username_contacts().await.unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn retracting_a_closed_request_keeps_the_record() {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    setup.connect_users(&alice, &bob).await;
    let (chat_id, record) = request_to_contact(&setup, &alice, &bob).await;

    bob.drain_username_queue(&record).await;
    bob.outbound_service().run_once().await;
    alice.retract_contact_request(chat_id).await.unwrap();
    alice.outbound_service().run_once().await;
    alice.drain_unchecked().await;

    assert_closed_as_connected(&alice, chat_id, bob.user_id()).await;
}

/// Alice sends Bob a request through a group, and then accepts his request to
/// her username. She closes her own request, and her sibling records that.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn crossed_targeted_request_is_closed_by_its_sender() {
    let mut setup = TestBackend::single().await;
    let [alice, bob, charlie] = setup.add_users().await;
    let group_chat_id = setup.connected_group(&charlie, &[&alice, &bob]).await;
    let alice2 = alice.add_synced_device().await;
    let alice_record = setup.add_username(&alice).await;

    let targeted = alice
        .add_contact_from_group(group_chat_id, bob.user_id().clone(), setup.apq_groups)
        .await
        .unwrap();
    alice2.drain().await;
    bob.drain().await;
    bob.add_contact(
        alice_record.username.clone(),
        alice_record.hash,
        setup.apq_groups,
    )
    .await
    .unwrap()
    .unwrap();

    let bob_request = alice.receive_connection_offer(&alice_record).await;
    let connection = alice
        .accept_contact_request(bob_request)
        .await
        .unwrap()
        .unwrap();
    assert_closed_as_connected(&alice, targeted, bob.user_id()).await;
    alice.outbound_service().run_once().await;
    alice.drain_unchecked().await;
    assert_closed_as_connected(&alice, targeted, bob.user_id()).await;

    alice2.drain().await;
    assert_closed_as_connected(&alice2, targeted, bob.user_id()).await;
    assert_eq!(
        alice2.contact(bob.user_id()).await.unwrap().chat_id,
        connection
    );
    bob.drain().await;
    assert_eq!(
        bob.contact(alice.user_id()).await.unwrap().chat_id,
        connection
    );
    assert!(pending_chats_from(&bob, alice.user_id()).await.is_empty());
}
