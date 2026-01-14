// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use mimi_content::{MessageStatus, MimiContent};
use rand::{Rng, distributions::Alphanumeric, rngs::OsRng};

use aircoreclient::{ChatId, ChatMessage, ReadReceiptsSetting, clients::CoreUser, store::Store};
use airserver_test_harness::utils::setup::{TestBackend, TestUser};

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[tracing::instrument(name = "Edit message", skip_all)]
async fn edit_message() {
    let mut setup = TestBackend::single().await;

    // Create alice and bob
    let alice = setup.add_user().await;
    let bob = setup.add_user().await;

    // Connect them
    let chat_alice_bob = setup.connect_users(&alice, &bob).await;

    setup.send_message(chat_alice_bob, &alice, vec![&bob]).await;

    setup.edit_message(chat_alice_bob, &alice, vec![&bob]).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[tracing::instrument(name = "Delete message", skip_all)]
async fn delete_message() {
    let mut setup = TestBackend::single().await;

    // Create alice and bob
    let alice = setup.add_user().await;
    let bob = setup.add_user().await;

    // Connect them
    let chat_alice_bob = setup.connect_users(&alice, &bob).await;

    setup.send_message(chat_alice_bob, &alice, vec![&bob]).await;

    let alice_user = &setup.get_user(&alice).user;
    let last_message = alice_user
        .last_message(chat_alice_bob)
        .await
        .unwrap()
        .unwrap();

    let string = last_message
        .message()
        .mimi_content()
        .unwrap()
        .string_rendering()
        .unwrap();

    assert!(
        !setup
            .scan_database(&string, false, vec![&alice, &bob])
            .await
            .is_empty(),
    );

    setup
        .delete_message(chat_alice_bob, &alice, vec![&bob])
        .await;

    assert_eq!(
        setup
            .scan_database(&string, false, vec![&alice, &bob])
            .await,
        Vec::<String>::new()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[tracing::instrument(name = "Message retrieval test", skip_all)]
async fn retrieve_chat_messages() {
    let mut setup = TestBackend::single().await;

    let alice = setup.add_user().await;
    let bob = setup.add_user().await;

    let chat_id = setup.connect_users(&alice, &bob).await;

    let alice_user = &setup.get_user(&alice).user;

    let number_of_messages = 10;
    let mut messages_sent = vec![];
    for _ in 0..number_of_messages {
        let message: String = OsRng
            .sample_iter(&Alphanumeric)
            .take(32)
            .map(char::from)
            .collect();
        let message_content = MimiContent::simple_markdown_message(message, [0; 16]); // simple seed for testing
        let message = alice_user
            .send_message(chat_id, message_content, None)
            .await
            .unwrap();
        messages_sent.push(message);
    }

    // Let's see what Alice's messages for this chat look like.
    let messages_retrieved = setup
        .get_user(&alice)
        .user
        .messages(chat_id, number_of_messages)
        .await
        .unwrap();

    assert_eq!(messages_retrieved.len(), messages_sent.len());
    assert_eq!(messages_retrieved, messages_sent);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[tracing::instrument(name = "Marking messages as read test", skip_all)]
async fn mark_as_read() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let bob = setup.add_user().await;
    let charlie = setup.add_user().await;

    let alice_bob_chat = setup.connect_users(&alice, &bob).await;
    let bob_charlie_chat = setup.connect_users(&bob, &charlie).await;

    // Send a few messages
    async fn send_messages(
        user: &CoreUser,
        chat_id: ChatId,
        number_of_messages: usize,
    ) -> Vec<ChatMessage> {
        let mut messages_sent = vec![];
        for _ in 0..number_of_messages {
            let message: String = OsRng
                .sample_iter(&Alphanumeric)
                .take(32)
                .map(char::from)
                .collect();
            let message_content = MimiContent::simple_markdown_message(message, [0; 16]); // simple seed for testing
            let message = user
                .send_message(chat_id, message_content, None)
                .await
                .unwrap();
            messages_sent.push(message);
        }
        user.outbound_service().run_once().await;
        messages_sent
    }

    let num_messages = 10;
    let alice_test_user = setup.get_user(&alice);
    let alice = &alice_test_user.user;
    send_messages(alice, alice_bob_chat, num_messages).await;

    // Message status starts at Unread
    let last_message = alice.last_message(alice_bob_chat).await.unwrap().unwrap();
    assert_eq!(last_message.status(), MessageStatus::Unread);

    // All messages should be unread
    let bob_test_user = setup.get_user(&bob);
    bob_test_user.fetch_and_process_qs_messages().await;
    let bob_user = &bob_test_user.user;
    let unread_message_count = bob_user.unread_messages_count(alice_bob_chat).await;
    assert_eq!(unread_message_count, num_messages);
    let global_unread_message_count = bob_user.global_unread_messages_count().await.unwrap();
    assert_eq!(global_unread_message_count, num_messages);

    // Bob sends scheduled receipts
    bob_user.outbound_service().run_once().await;

    // Alice sees the delivery receipt
    let num_processed = alice_test_user.fetch_and_process_qs_messages().await;
    assert_eq!(num_processed, 1);
    let last_message = alice.last_message(alice_bob_chat).await.unwrap().unwrap();
    assert_eq!(last_message.status(), MessageStatus::Delivered);

    // Bob reads the messages
    let last_message = bob_user
        .last_message(alice_bob_chat)
        .await
        .unwrap()
        .unwrap();
    let last_message_id = last_message.id();
    let last_message_mimi_id = last_message.message().mimi_id().unwrap();
    bob_user
        .outbound_service()
        .enqueue_receipts(
            alice_bob_chat,
            [(last_message_id, last_message_mimi_id, MessageStatus::Read)].into_iter(),
        )
        .await
        .unwrap();
    bob_user.outbound_service().run_once().await;

    // Alice sees the read receipt
    let num_processed = alice_test_user.fetch_and_process_qs_messages().await;
    assert_eq!(num_processed, 1);
    let last_message = alice.last_message(alice_bob_chat).await.unwrap().unwrap();
    assert_eq!(last_message.status(), MessageStatus::Read);

    // Let's send some messages between bob and charlie s.t. we can test the
    // global unread messages count.
    let charlie_test_user = setup.get_user(&charlie);
    let charlie = &charlie_test_user.user;
    let messages_sent = send_messages(charlie, bob_charlie_chat, num_messages).await;

    let qs_messages = bob_user.qs_fetch_messages().await.unwrap();
    let bob_messages_sent = bob_user.fully_process_qs_messages(qs_messages).await;

    // Let's mark all but the last two messages as read (we subtract 3, because
    // the vector is 0-indexed).
    let timestamp = bob_messages_sent.new_messages[messages_sent.len() - 3].timestamp();

    bob_user
        .mark_as_read([(bob_charlie_chat, timestamp)])
        .await
        .unwrap();

    // Check if we were successful
    let unread_message_count = bob_user.unread_messages_count(bob_charlie_chat).await;
    assert_eq!(unread_message_count, 2);

    // We expect the global unread messages count to be that of both
    // chats, i.e. the `expected_unread_message_count` plus
    // `number_of_messages`, because none of the messages between alice and
    // charlie had been read.
    let global_unread_messages_count = bob_user.global_unread_messages_count().await.unwrap();
    assert_eq!(global_unread_messages_count, num_messages + 2);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[tracing::instrument(name = "Read receipts setting test", skip_all)]
async fn read_receipts_setting() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let bob = setup.add_user().await;

    // Establish a direct chat between Alice and Bob.
    let alice_bob_chat = setup.connect_users(&alice, &bob).await;
    let alice_test_user = setup.get_user(&alice);
    let alice_user = &alice_test_user.user;
    let bob_test_user = setup.get_user(&bob);
    let bob_user = &bob_test_user.user;

    async fn send_and_process_delivery(
        sender: &TestUser,
        receiver: &TestUser,
        chat_id: ChatId,
        message: &str,
    ) {
        // Send a message and process delivery receipts on both ends.
        sender
            .user
            .send_message(
                chat_id,
                MimiContent::simple_markdown_message(message.into(), [0; 16]),
                None,
            )
            .await
            .unwrap();
        sender.user.outbound_service().run_once().await;

        receiver.fetch_and_process_qs_messages().await;
        receiver.user.outbound_service().run_once().await;
        sender.fetch_and_process_qs_messages().await;
    }

    async fn send_read_receipt(user: &CoreUser, chat_id: ChatId) {
        // Enqueue a read receipt for the most recent message in the chat.
        let last_message = user.last_message(chat_id).await.unwrap().unwrap();
        let last_message_id = last_message.id();
        let last_message_mimi_id = last_message.message().mimi_id().unwrap();
        user.outbound_service()
            .enqueue_receipts(
                chat_id,
                [(last_message_id, last_message_mimi_id, MessageStatus::Read)].into_iter(),
            )
            .await
            .unwrap();
        user.outbound_service().run_once().await;
    }

    // We enable read receipts
    alice_user
        .set_user_setting(&ReadReceiptsSetting(true))
        .await
        .unwrap();
    // Send a message and ensure delivery receipts are processed before read
    // receipts.
    send_and_process_delivery(
        alice_test_user,
        bob_test_user,
        alice_bob_chat,
        "receipts on",
    )
    .await;
    let last_message = alice_user
        .last_message(alice_bob_chat)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(last_message.status(), MessageStatus::Delivered);
    // Bob sends a read receipt and Alice should receive it.
    send_read_receipt(bob_user, alice_bob_chat).await;
    alice_test_user.fetch_and_process_qs_messages().await;
    let last_message = alice_user
        .last_message(alice_bob_chat)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(last_message.status(), MessageStatus::Read);

    // We disable read receipts
    alice_user
        .set_user_setting(&ReadReceiptsSetting(false))
        .await
        .unwrap();
    // Send a message and confirm the delivery receipt is still processed.
    send_and_process_delivery(
        alice_test_user,
        bob_test_user,
        alice_bob_chat,
        "receipts off",
    )
    .await;
    let last_message = alice_user
        .last_message(alice_bob_chat)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(last_message.status(), MessageStatus::Delivered);
    // Bob sends a read receipt but Alice should ignore it, the last message is
    // therefore still a delivery receipt.
    send_read_receipt(bob_user, alice_bob_chat).await;
    alice_test_user.fetch_and_process_qs_messages().await;
    let last_message = alice_user
        .last_message(alice_bob_chat)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(last_message.status(), MessageStatus::Delivered);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn message_sending_failures() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let bob = setup.add_user().await;

    let chat_id = setup.connect_users(&alice, &bob).await;

    let alice_user = &setup.get_user(&alice).user;

    let content = MimiContent::simple_markdown_message("Hello".to_string(), [0; 16]);

    // Make server drop messages
    setup.listener_control_handle().unwrap().set_drop_all();

    // Send three messages
    for _ in 0..3 {
        alice_user
            .send_message(chat_id, content.clone(), None)
            .await
            .unwrap();
    }
    alice_user.outbound_service().run_once().await;
    // Check that messages are marked as failed
    let messages = alice_user.messages(chat_id, 3).await.unwrap();
    for message in messages {
        let status = message.status();
        if status != MessageStatus::Error {
            panic!(
                "Message should be marked as error. Actual status: {:?}",
                status
            );
        }
    }
}
