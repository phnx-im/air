// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! How the contact requests between two users fold into one chat and end with
//! one connection.

use aircommon::identifiers::{UserId, Username};
use aircoreclient::{
    ChatId, ChatType, EventMessage, Message, SystemMessage, UsernameRecord, clients::CoreUser,
};
use airserver_test_harness::utils::setup::TestBackend;

use super::group_bootstrap::{
    add_username, drain_expecting_success, drain_username_queue, receive_connection_offer,
};

async fn system_messages(device: &CoreUser, chat_id: ChatId) -> Vec<SystemMessage> {
    device
        .messages(chat_id, 100)
        .await
        .unwrap()
        .into_iter()
        .filter_map(|message| match message.message() {
            Message::Event(EventMessage::System(system)) => Some(system.clone()),
            _ => None,
        })
        .collect()
}

/// The chats of pending contact requests from `sender`.
pub(crate) async fn pending_chats_from(device: &CoreUser, sender: &UserId) -> Vec<ChatId> {
    let mut chats = Vec::new();
    for chat_id in device.ordered_chat_ids().await.unwrap() {
        let chat = device.chat(&chat_id).await.unwrap();
        if chat.chat_type() == &ChatType::PendingConnection(sender.clone()) {
            chats.push(chat_id);
        }
    }
    chats
}

/// A username of `user_id` besides the one the harness registers.
pub(crate) async fn add_second_username(setup: &TestBackend, user_id: &UserId) -> UsernameRecord {
    let user = setup.get_user(user_id).user().clone();
    user.outbound_service().run_once().await;
    let suffix: String = user_id
        .uuid()
        .simple()
        .to_string()
        .chars()
        .take(16)
        .collect();
    let username = Username::new(format!("extra-{suffix}")).unwrap();
    user.add_username(username).await.unwrap().unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[tracing::instrument(name = "Requests of one sender fold", skip_all)]
async fn requests_of_one_sender_fold_into_the_chat_of_the_newest() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let bob = setup.add_user().await;
    let first = add_username(&mut setup, &bob).await;
    let second = add_second_username(&setup, &bob).await;

    let alice_user = setup.get_user(&alice).user().clone();
    let bob_user = setup.get_user(&bob).user().clone();
    let mut sent = Vec::new();
    for record in [&first, &second] {
        let chat_id = alice_user
            .add_contact(record.username.clone(), record.hash, setup.apq_groups)
            .await
            .unwrap()
            .unwrap();
        sent.push(chat_id);
    }

    receive_connection_offer(&bob_user, &first).await;
    let chat_id = receive_connection_offer(&bob_user, &second).await;
    assert_eq!(
        chat_id, sent[1],
        "the chat is the one of the newest request"
    );
    assert!(
        bob_user.chat(&sent[0]).await.is_none(),
        "the newer request took over the chat of the older one"
    );
    let messages = system_messages(&bob_user, chat_id).await;
    assert!(
        messages.contains(&SystemMessage::ReceivedHandleConnectionRequest {
            sender: alice.clone(),
            user_handle: first.username.clone(),
        })
    );
    assert!(messages.contains(
        &SystemMessage::ReceivedAdditionalUsernameConnectionRequest {
            sender: alice.clone(),
            username: second.username.clone(),
        }
    ));

    bob_user
        .accept_contact_request(chat_id)
        .await
        .unwrap()
        .unwrap();
    drain_expecting_success(&alice_user, "alice failed to follow the accept").await;

    assert_eq!(
        alice_user.chat(&sent[1]).await.unwrap().chat_type(),
        &ChatType::Connection(bob.clone())
    );
    assert!(
        pending_chats_from(&bob_user, &alice).await.is_empty(),
        "accepting settles the older request too"
    );
    assert_eq!(bob_user.contacts().await.unwrap().len(), 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[tracing::instrument(name = "Older offer fetched after accepting", skip_all)]
async fn an_offer_fetched_after_accepting_a_newer_one_is_dropped() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let bob = setup.add_user().await;
    let first = add_username(&mut setup, &bob).await;
    let second = add_second_username(&setup, &bob).await;

    let alice_user = setup.get_user(&alice).user().clone();
    let bob_user = setup.get_user(&bob).user().clone();
    for record in [&first, &second] {
        alice_user
            .add_contact(record.username.clone(), record.hash, setup.apq_groups)
            .await
            .unwrap()
            .unwrap();
    }

    // The newer offer is accepted while the older one still waits in its
    // queue.
    let chat_id = receive_connection_offer(&bob_user, &second).await;
    bob_user
        .accept_contact_request(chat_id)
        .await
        .unwrap()
        .unwrap();
    drain_expecting_success(&alice_user, "alice failed to follow the accept").await;

    assert_eq!(drain_username_queue(&bob_user, &first).await, None);
    assert!(
        pending_chats_from(&bob_user, &alice).await.is_empty(),
        "an offer from a connected contact is stale"
    );
    assert_eq!(
        bob_user.chat(&chat_id).await.unwrap().chat_type(),
        &ChatType::Connection(alice.clone())
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[tracing::instrument(name = "Mutual requests", skip_all)]
async fn crossed_requests_leave_one_connection() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let bob = setup.add_user().await;
    let alice_record = add_username(&mut setup, &alice).await;
    let bob_record = add_username(&mut setup, &bob).await;
    let alice_user = setup.get_user(&alice).user().clone();
    let bob_user = setup.get_user(&bob).user().clone();

    alice_user
        .add_contact(
            bob_record.username.clone(),
            bob_record.hash,
            setup.apq_groups,
        )
        .await
        .unwrap()
        .unwrap();
    bob_user
        .add_contact(
            alice_record.username.clone(),
            alice_record.hash,
            setup.apq_groups,
        )
        .await
        .unwrap()
        .unwrap();

    // Alice accepts Bob's request. Bob's copy of Alice's request goes once he
    // learns of the connection.
    let bob_request_at_alice = receive_connection_offer(&alice_user, &alice_record).await;
    let alice_request_at_bob = receive_connection_offer(&bob_user, &bob_record).await;
    alice_user
        .accept_contact_request(bob_request_at_alice)
        .await
        .unwrap()
        .unwrap();
    drain_expecting_success(&bob_user, "bob failed to follow the accept").await;

    assert!(
        bob_user.chat(&alice_request_at_bob).await.is_none(),
        "the request of a new contact is discarded"
    );
    assert_eq!(alice_user.contacts().await.unwrap().len(), 1);
    assert_eq!(
        bob_user.contacts().await.unwrap().len(),
        1,
        "the two stay connected through one chat"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[tracing::instrument(name = "Asking back from a group", skip_all)]
async fn asking_back_from_a_group_opens_the_pending_request() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let bob = setup.add_user().await;
    let charlie = setup.add_user().await;
    // Alice and Bob share a group, through Charlie.
    setup.connect_users(&charlie, &alice).await;
    setup.connect_users(&charlie, &bob).await;
    let group_chat_id = setup.create_group(&charlie).await;
    setup
        .invite_to_group(group_chat_id, &charlie, vec![&alice, &bob])
        .await;
    let alice_user = setup.get_user(&alice).user().clone();
    let bob_user = setup.get_user(&bob).user().clone();

    let alice_chat_id = alice_user
        .add_contact_from_group(group_chat_id, bob.clone(), setup.apq_groups)
        .await
        .unwrap();
    let processed = drain_expecting_success(&bob_user, "bob failed to take the request").await;
    let [bob_chat_id] = processed.new_connections.as_slice() else {
        panic!("expected one request, got {:?}", processed.new_connections);
    };

    let bob_again = bob_user
        .add_contact_from_group(group_chat_id, alice.clone(), setup.apq_groups)
        .await
        .unwrap();
    assert_eq!(bob_again, *bob_chat_id, "the pending request opens instead");

    bob_user
        .accept_contact_request(*bob_chat_id)
        .await
        .unwrap()
        .unwrap();
    drain_expecting_success(&alice_user, "alice failed to follow the accept").await;
    assert_eq!(
        alice_user.chat(&alice_chat_id).await.unwrap().chat_type(),
        &ChatType::Connection(bob.clone())
    );
    assert_eq!(alice_user.contacts().await.unwrap().len(), 2);
}
