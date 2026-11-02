// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Deleting system messages on this device.

use aircoreclient::SystemMessage;
use airserver_test_harness::utils::{core_user_ext::CoreUserExt, setup::TestBackend};

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn self_chat_intro_and_device_messages_cannot_be_deleted() {
    let mut setup = TestBackend::single().await;
    let [alice] = setup.add_users().await;
    let alice2 = alice.add_linked_device().await;
    let alice2_id = alice2.own_client_id().await.unwrap();
    let self_chat_id = alice.self_chat_id().await.unwrap().unwrap();

    let intro = alice
        .system_message_id(self_chat_id, |message| {
            *message == SystemMessage::SelfChatCreated
        })
        .await;
    let linked = alice
        .system_message_id(self_chat_id, |message| {
            *message == SystemMessage::DeviceLinked(alice2_id)
        })
        .await;

    for message_id in [intro, linked] {
        assert!(alice.delete_message(message_id).await.is_err());
        assert!(alice.message(message_id).await.unwrap().is_some());
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn request_messages_cannot_be_deleted_while_the_request_is_open() {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    let record = setup.add_username(&bob).await;
    let alice_chat_id = alice
        .add_contact(record.username.clone(), record.hash, setup.apq_groups)
        .await
        .unwrap()
        .unwrap();
    let bob_chat_id = bob.receive_connection_offer(&record).await;

    let sent = alice
        .system_message_id(alice_chat_id, |message| {
            matches!(message, SystemMessage::NewHandleConnectionChat(_))
        })
        .await;
    let received = bob
        .system_message_id(bob_chat_id, |message| {
            matches!(
                message,
                SystemMessage::ReceivedHandleConnectionRequest { .. }
            )
        })
        .await;
    assert!(alice.delete_message(sent).await.is_err());
    assert!(bob.delete_message(received).await.is_err());

    bob.accept_contact_request(bob_chat_id)
        .await
        .unwrap()
        .unwrap();
    alice.drain().await;

    alice.delete_message(sent).await.unwrap();
    bob.delete_message(received).await.unwrap();
    assert!(alice.message(sent).await.unwrap().is_none());
    assert!(bob.message(received).await.unwrap().is_none());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn chat_without_messages_stays_at_the_bottom_of_the_list() {
    let mut setup = TestBackend::single().await;
    let [alice, bob, charlie] = setup.add_users().await;
    let emptied = setup.connect_users(&alice, &bob).await;
    let other = setup.connect_users(&alice, &charlie).await;
    let ordered = alice.ordered_chat_ids().await.unwrap();
    assert_ne!(ordered.last(), Some(&emptied));

    for message in alice.messages(emptied, 100).await.unwrap() {
        alice.delete_message(message.id()).await.unwrap();
    }

    assert!(alice.chat(&emptied).await.is_some());
    assert!(alice.last_message(emptied).await.unwrap().is_none());
    let ordered = alice.ordered_chat_ids().await.unwrap();
    assert_eq!(ordered.last(), Some(&emptied), "got {ordered:?}");
    assert!(ordered.contains(&other));
}
