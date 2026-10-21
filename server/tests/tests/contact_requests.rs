// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! How the contact requests between two users fold into one chat and end with
//! one connection.

use aircommon::identifiers::{UserId, Username};
use aircoreclient::{ChatId, ChatType, SystemMessage, UsernameRecord, clients::CoreUser};
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

/// A username of `user` besides the one the harness registers.
pub(crate) async fn add_second_username(user: &CoreUser) -> UsernameRecord {
    user.outbound_service().run_once().await;
    let suffix: String = user
        .user_id()
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
async fn requests_of_one_sender_fold_into_the_chat_of_the_newest() {
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
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn an_offer_fetched_after_accepting_a_newer_one_is_dropped() {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    let first = setup.add_username(&bob).await;
    let second = add_second_username(&bob).await;
    for record in [&first, &second] {
        alice
            .add_contact(record.username.clone(), record.hash, setup.apq_groups)
            .await
            .unwrap()
            .unwrap();
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
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn crossed_requests_leave_one_connection() {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    let alice_record = setup.add_username(&alice).await;
    let bob_record = setup.add_username(&bob).await;

    alice
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
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn asking_back_from_a_group_opens_the_pending_request() {
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
