// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Read markers and read receipts, incl. the read receipts setting.

use aircoreclient::{
    ChatId, ChatMessage, ReadReceiptsSetting,
    clients::{CoreUser, MarkChatAsRead},
};
use airserver_test_harness::utils::{core_user_ext::CoreUserExt, setup::TestBackend};
use mimi_content::{MessageStatus, MimiContent};
use rand::{RngExt, distr::Alphanumeric};

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn mark_as_read() {
    let mut setup = TestBackend::single().await;
    let [alice, bob, charlie] = setup.add_users().await;

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
            let message: String = rand::rng()
                .sample_iter(&Alphanumeric)
                .take(32)
                .map(char::from)
                .collect();
            let message_content = MimiContent::simple_markdown_message(message, [0; 16]); // simple seed for testing
            let message = user
                .send_message(chat_id, message_content, None, MarkChatAsRead::Yes)
                .await
                .unwrap();
            messages_sent.push(message);
        }
        user.outbound_service().run_once().await;
        messages_sent
    }

    let num_messages = 10;
    send_messages(&alice, alice_bob_chat, num_messages).await;

    // Message status starts at Unread
    let last_message = alice.expect_last_message(alice_bob_chat).await;
    assert_eq!(last_message.status(), MessageStatus::Unread);

    // All messages should be unread
    bob.drain().await;
    let unread_message_count = bob.unread_messages_count(alice_bob_chat).await;
    assert_eq!(unread_message_count, num_messages);
    let global_unread_message_count = bob.global_unread_messages_count().await.unwrap();
    assert_eq!(global_unread_message_count, num_messages);

    // Bob sends scheduled receipts
    bob.outbound_service().run_once().await;

    // Alice sees the delivery receipt
    let num_processed = alice.drain().await.processed;
    assert_eq!(num_processed, 1);
    let last_message = alice.expect_last_message(alice_bob_chat).await;
    assert_eq!(last_message.status(), MessageStatus::Delivered);

    // Bob reads the messages
    let last_message = bob.expect_last_message(alice_bob_chat).await;
    let last_message_id = last_message.id();
    let last_message_mimi_id = last_message.message().mimi_id().unwrap();
    bob.outbound_service()
        .enqueue_receipts(
            alice_bob_chat,
            [(last_message_id, last_message_mimi_id, MessageStatus::Read)].into_iter(),
        )
        .await
        .unwrap();
    bob.outbound_service().run_once().await;

    // Alice sees the read receipt
    let num_processed = alice.drain().await.processed;
    assert_eq!(num_processed, 1);
    let last_message = alice.expect_last_message(alice_bob_chat).await;
    assert_eq!(last_message.status(), MessageStatus::Read);

    // Let's send some messages between bob and charlie s.t. we can test the
    // global unread messages count.
    let messages_sent = send_messages(&charlie, bob_charlie_chat, num_messages).await;

    let bob_messages_sent = bob.drain().await;

    // Let's mark all but the last two messages as read (we subtract 3, because
    // the vector is 0-indexed).
    let timestamp = bob_messages_sent.new_messages[messages_sent.len() - 3].timestamp();

    bob.mark_as_read([(bob_charlie_chat, timestamp)])
        .await
        .unwrap();

    // Check if we were successful
    let unread_message_count = bob.unread_messages_count(bob_charlie_chat).await;
    assert_eq!(unread_message_count, 2);

    // We expect the global unread messages count to be that of both
    // chats, i.e. the `expected_unread_message_count` plus
    // `number_of_messages`, because none of the messages between alice and
    // charlie had been read.
    let global_unread_messages_count = bob.global_unread_messages_count().await.unwrap();
    assert_eq!(global_unread_messages_count, num_messages + 2);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn read_receipts_setting() {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;

    // Establish a direct chat between Alice and Bob.
    let alice_bob_chat = setup.connect_users(&alice, &bob).await;

    async fn send_and_process_delivery(
        sender: &CoreUser,
        receiver: &CoreUser,
        chat_id: ChatId,
        message: &str,
    ) {
        // Send a message and process delivery receipts on both ends.
        sender.send_text(chat_id, message).await;

        receiver.drain().await;
        receiver.outbound_service().run_once().await;
        sender.drain().await;
    }

    async fn send_read_receipt(user: &CoreUser, chat_id: ChatId) {
        // Enqueue a read receipt for the most recent message in the chat.
        let last_message = user.expect_last_message(chat_id).await;
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
    alice
        .set_user_setting(&ReadReceiptsSetting(true))
        .await
        .unwrap();
    // Send a message and ensure delivery receipts are processed before read
    // receipts.
    send_and_process_delivery(&alice, &bob, alice_bob_chat, "receipts on").await;
    let last_message = alice.expect_last_message(alice_bob_chat).await;
    assert_eq!(last_message.status(), MessageStatus::Delivered);
    // Bob sends a read receipt and Alice should receive it.
    send_read_receipt(&bob, alice_bob_chat).await;
    alice.drain().await;
    let last_message = alice.expect_last_message(alice_bob_chat).await;
    assert_eq!(last_message.status(), MessageStatus::Read);

    // We disable read receipts
    alice
        .set_user_setting(&ReadReceiptsSetting(false))
        .await
        .unwrap();
    // Send a message and confirm the delivery receipt is still processed.
    send_and_process_delivery(&alice, &bob, alice_bob_chat, "receipts off").await;
    let last_message = alice.expect_last_message(alice_bob_chat).await;
    assert_eq!(last_message.status(), MessageStatus::Delivered);
    // Bob sends a read receipt but Alice should ignore it, the last message is
    // therefore still a delivery receipt.
    send_read_receipt(&bob, alice_bob_chat).await;
    alice.drain().await;
    let last_message = alice.expect_last_message(alice_bob_chat).await;
    assert_eq!(last_message.status(), MessageStatus::Delivered);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn peer_read_receipt_does_not_move_our_read_marker() {
    let mut setup = TestBackend::single().await;
    let [alice, bob, charlie] = setup.add_users().await;
    let group_chat_id = setup.connected_group(&alice, &[&bob, &charlie]).await;

    // The invite helper leaves a message from every member behind. Clear Alice's
    // marker, so what follows is about Charlie's message alone.
    let newest = alice.expect_last_message(group_chat_id).await;
    alice
        .mark_chat_as_read(group_chat_id, newest.id())
        .await
        .unwrap();

    // Charlie writes. Neither Alice nor Bob has read it.
    let sent = setup
        .send_message(group_chat_id, &charlie, &[&alice, &bob])
        .await;

    assert_eq!(alice.unread_messages_count(group_chat_id).await, 1);

    // Bob reads it and reports that to the whole group, Alice included.
    let bobs_copy = bob
        .expect_message(sent.recipient_message_id(bob.user_id()))
        .await;
    bob.outbound_service()
        .enqueue_receipts(
            group_chat_id,
            [(
                bobs_copy.id(),
                bobs_copy.message().mimi_id().unwrap(),
                MessageStatus::Read,
            )]
            .into_iter(),
        )
        .await
        .unwrap();
    bob.outbound_service().run_once().await;

    alice.drain().await;

    // Alice records Bob's status ...
    let alices_copy = alice
        .expect_message(sent.recipient_message_id(alice.user_id()))
        .await;
    assert_eq!(
        alices_copy.status(),
        MessageStatus::Read,
        "Bob's read receipt should be recorded on Alice's copy of the message"
    );

    // ... but it is Bob's read marker, not hers.
    assert_eq!(
        alice.unread_messages_count(group_chat_id).await,
        1,
        "a read receipt from another member must not clear our unread messages"
    );
}
