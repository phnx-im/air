// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use mimi_content::{MessageStatus, MimiContent};
use rand::{Rng, distributions::Alphanumeric, rngs::OsRng};

use aircoreclient::{
    ChatId, ChatMessage, MimiContentExt, ReadReceiptsSetting, clients::CoreUser, store::Store,
};
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
#[tracing::instrument(name = "Delete message in group", skip_all)]
async fn delete_message_in_group() {
    let mut setup = TestBackend::single().await;

    // Create alice, bob, and charlie
    let alice = setup.add_user().await;
    let bob = setup.add_user().await;
    let charlie = setup.add_user().await;

    // Connect alice with bob and charlie (needed for group invites)
    setup.connect_users(&alice, &bob).await;
    setup.connect_users(&alice, &charlie).await;

    // Create a group and invite bob and charlie
    let group_chat = setup.create_group(&alice).await;
    setup
        .invite_to_group(group_chat, &alice, vec![&bob, &charlie])
        .await;

    // Alice sends a message to the group
    setup
        .send_message(group_chat, &alice, vec![&bob, &charlie])
        .await;

    // Get the message content before deletion
    let alice_user = &setup.get_user(&alice).user;
    let last_message = alice_user.last_message(group_chat).await.unwrap().unwrap();
    let original_content = last_message
        .message()
        .mimi_content()
        .unwrap()
        .string_rendering()
        .unwrap();

    // Verify message exists on all users
    assert!(
        !setup
            .scan_database(&original_content, false, vec![&alice, &bob, &charlie])
            .await
            .is_empty()
    );

    // Alice deletes the message
    setup
        .delete_message(group_chat, &alice, vec![&bob, &charlie])
        .await;

    // Verify the content is no longer in any database
    assert_eq!(
        setup
            .scan_database(&original_content, false, vec![&alice, &bob, &charlie])
            .await,
        Vec::<String>::new()
    );

    // Verify the message still exists but with NullPart content for all users
    for user_id in [&alice, &bob, &charlie] {
        let user = &setup.get_user(user_id).user;
        let messages = user.messages(group_chat, 10).await.unwrap();
        // Find a content message (not system messages)
        let content_messages: Vec<_> = messages
            .iter()
            .filter(|m| m.message().mimi_content().is_some())
            .collect();
        assert!(
            !content_messages.is_empty(),
            "User should still have the deleted message placeholder"
        );
        let deleted_message = content_messages.last().unwrap();
        assert!(
            deleted_message
                .message()
                .mimi_content()
                .unwrap()
                .nested_part
                .part
                == mimi_content::NestedPartContent::NullPart,
            "Deleted message should have NullPart content"
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[tracing::instrument(name = "Delete message preserves other messages", skip_all)]
async fn delete_message_preserves_other_messages() {
    let mut setup = TestBackend::single().await;

    let alice = setup.add_user().await;
    let bob = setup.add_user().await;

    let chat_id = setup.connect_users(&alice, &bob).await;

    let alice_user = &setup.get_user(&alice).user;
    let bob_user = &setup.get_user(&bob).user;

    // Send three messages with distinct content
    let contents = ["First message", "Second message to delete", "Third message"];
    for content in &contents {
        let message_content = MimiContent::simple_markdown_message(content.to_string(), [0; 16]);
        alice_user
            .send_message(chat_id, message_content, None)
            .await
            .unwrap();
    }
    alice_user.outbound_service().run_once().await;

    // Bob receives all messages
    let bob_test_user = setup.get_user(&bob);
    bob_test_user.fetch_and_process_qs_messages().await;

    // Helper to filter only our test messages (not system messages)
    let is_test_message = |m: &&ChatMessage| {
        m.message()
            .mimi_content()
            .map(|c| {
                let text = c.string_rendering().unwrap_or_default();
                text.contains("First")
                    || text.contains("Second")
                    || text.contains("Third")
                    || c.nested_part.part == mimi_content::NestedPartContent::NullPart
            })
            .unwrap_or(false)
    };

    // Verify bob has all 3 test messages
    let bob_messages = bob_user.messages(chat_id, 10).await.unwrap();
    let bob_test_messages: Vec<_> = bob_messages.iter().filter(is_test_message).collect();
    assert_eq!(bob_test_messages.len(), 3);

    // Get the middle message for deletion
    let alice_messages = alice_user.messages(chat_id, 10).await.unwrap();
    let alice_content_messages: Vec<_> = alice_messages
        .iter()
        .filter(|m| {
            m.message()
                .mimi_content()
                .map(|c| c.string_rendering().unwrap_or_default().contains("Second"))
                .unwrap_or(false)
        })
        .collect();
    assert_eq!(alice_content_messages.len(), 1);
    let message_to_delete = alice_content_messages[0];

    // Alice deletes the middle message using the proper API
    let message_to_delete_id = message_to_delete.id();
    let alice_test_user = setup.get_user(&alice);
    alice_test_user.fetch_and_process_qs_messages().await;
    alice_user
        .delete_message(chat_id, message_to_delete_id)
        .await
        .unwrap();
    alice_user.outbound_service().run_once().await;

    // Bob receives the deletion
    bob_test_user.fetch_and_process_qs_messages().await;

    // Verify first and third messages still have their original content
    let bob_messages = bob_user.messages(chat_id, 10).await.unwrap();
    let bob_test_messages: Vec<_> = bob_messages.iter().filter(is_test_message).collect();

    // Should still have 3 message slots (First, NullPart for deleted, Third)
    assert_eq!(bob_test_messages.len(), 3);

    // Check that "First" and "Third" messages are intact
    let first_msg = bob_test_messages.iter().find(|m| {
        m.message()
            .mimi_content()
            .map(|c| c.string_rendering().unwrap_or_default().contains("First"))
            .unwrap_or(false)
    });
    assert!(first_msg.is_some(), "First message should still exist");

    let third_msg = bob_test_messages.iter().find(|m| {
        m.message()
            .mimi_content()
            .map(|c| c.string_rendering().unwrap_or_default().contains("Third"))
            .unwrap_or(false)
    });
    assert!(third_msg.is_some(), "Third message should still exist");

    // Check that deleted message is now NullPart
    let deleted_msg = bob_test_messages.iter().find(|m| {
        m.message()
            .mimi_content()
            .map(|c| c.nested_part.part == mimi_content::NestedPartContent::NullPart)
            .unwrap_or(false)
    });
    assert!(
        deleted_msg.is_some(),
        "Deleted message should exist with NullPart"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[tracing::instrument(name = "Delete edited message", skip_all)]
async fn delete_edited_message() {
    let mut setup = TestBackend::single().await;

    let alice = setup.add_user().await;
    let bob = setup.add_user().await;

    let chat_id = setup.connect_users(&alice, &bob).await;

    // Alice sends a message
    setup.send_message(chat_id, &alice, vec![&bob]).await;

    // Alice edits the message
    setup.edit_message(chat_id, &alice, vec![&bob]).await;

    // Get the edited content for verification
    let edited_content = {
        let alice_user = &setup.get_user(&alice).user;
        let last_message = alice_user.last_message(chat_id).await.unwrap().unwrap();

        // Verify the message has been edited
        assert!(
            last_message.edited_at().is_some(),
            "Message should have edited_at timestamp"
        );

        last_message
            .message()
            .mimi_content()
            .unwrap()
            .string_rendering()
            .unwrap()
    };

    // Verify edited content exists
    assert!(
        !setup
            .scan_database(&edited_content, false, vec![&alice, &bob])
            .await
            .is_empty()
    );

    // Alice deletes the edited message
    setup.delete_message(chat_id, &alice, vec![&bob]).await;

    // Verify the edited content is no longer present
    assert_eq!(
        setup
            .scan_database(&edited_content, false, vec![&alice, &bob])
            .await,
        Vec::<String>::new()
    );

    // Verify the message exists but with NullPart content
    let alice_user = &setup.get_user(&alice).user;
    let alice_messages = alice_user.messages(chat_id, 10).await.unwrap();
    let content_messages: Vec<_> = alice_messages
        .iter()
        .filter(|m| m.message().mimi_content().is_some())
        .collect();
    assert!(!content_messages.is_empty());

    let deleted_message = content_messages.last().unwrap();
    assert!(
        deleted_message
            .message()
            .mimi_content()
            .unwrap()
            .nested_part
            .part
            == mimi_content::NestedPartContent::NullPart,
        "Deleted message should have NullPart content"
    );
    assert_eq!(
        deleted_message.status(),
        MessageStatus::Deleted,
        "Message status should be Deleted"
    );
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

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[tracing::instrument(name = "Delete message with attachment", skip_all)]
async fn delete_message_with_attachment() {
    let mut setup = TestBackend::single().await;

    // Create alice and bob
    let alice = setup.add_user().await;
    let bob = setup.add_user().await;

    // Connect them
    let chat_id = setup.connect_users(&alice, &bob).await;

    // Alice sends an attachment
    let (_returned_message_id, _external_part) = setup
        .send_attachment(
            chat_id,
            &alice,
            vec![&bob],
            b"test attachment data",
            "test.bin",
        )
        .await
        .unwrap();

    // Get the actual message_id of the last message (the attachment message)
    let message_id = {
        let alice_user = &setup.get_user(&alice).user;
        let messages = alice_user.messages(chat_id, 10).await.unwrap();
        // Find the attachment message (has ExternalPart)
        let attachment_msg = messages
            .iter()
            .filter(|m| m.message().mimi_content().is_some())
            .find(|m| {
                let mut has_attachment = false;
                m.message()
                    .mimi_content()
                    .unwrap()
                    .visit_attachments(|_| {
                        has_attachment = true;
                        Ok(())
                    })
                    .unwrap();
                has_attachment
            })
            .expect("Should have an attachment message");
        attachment_msg.id()
    };

    // Verify attachment exists before deletion
    {
        let alice_user = &setup.get_user(&alice).user;
        let attachment_ids = alice_user
            .attachment_ids_for_message(message_id)
            .await
            .unwrap();
        assert_eq!(
            attachment_ids.len(),
            1,
            "Attachment should exist before deletion"
        );
    }

    // Bob downloads the attachment (so he has an attachment record to verify deletion)
    {
        let bob_user = &setup.get_user(&bob).user;
        let messages = bob_user.messages(chat_id, 10).await.unwrap();
        let attachment_msg = messages
            .iter()
            .filter(|m| m.message().mimi_content().is_some())
            .find(|m| {
                let mut has_attachment = false;
                m.message()
                    .mimi_content()
                    .unwrap()
                    .visit_attachments(|_| {
                        has_attachment = true;
                        Ok(())
                    })
                    .unwrap();
                has_attachment
            })
            .expect("Bob should have the attachment message");

        let bob_attachment_ids = bob_user
            .attachment_ids_for_message(attachment_msg.id())
            .await
            .unwrap();

        // Download each attachment
        for attachment_id in &bob_attachment_ids {
            let (_, download_future) = bob_user.download_attachment(*attachment_id);
            download_future.await.unwrap();
        }

        // Verify Bob has the attachment
        assert!(
            !bob_attachment_ids.is_empty(),
            "Bob should have attachment after download"
        );
    }

    // Alice deletes the message (network deletion)
    // Note: We need to delete the specific attachment message, not just the last message
    {
        let alice_test_user = setup.get_user_mut(&alice);
        let alice_user = &mut alice_test_user.user;

        // Fetch and process QS messages first
        let qs_messages = alice_user.qs_fetch_messages().await.unwrap();
        alice_user.fully_process_qs_messages(qs_messages).await;

        // Delete the specific message by ID
        alice_user
            .delete_message(chat_id, message_id)
            .await
            .unwrap();
        alice_user.outbound_service().run_once().await;
    }

    // Bob receives the deletion
    {
        let bob_test_user = setup.get_user_mut(&bob);
        let bob_user = &mut bob_test_user.user;
        let qs_messages = bob_user.qs_fetch_messages().await.unwrap();
        bob_user.fully_process_qs_messages(qs_messages).await;
    }

    // Verify: Message is NullPart for Alice (the sender)
    {
        let alice_user = &setup.get_user(&alice).user;
        let messages = alice_user.messages(chat_id, 10).await.unwrap();
        let deleted = messages
            .iter()
            .find(|m| m.id() == message_id)
            .expect("Alice should still have the message");
        assert_eq!(
            deleted.message().mimi_content().unwrap().nested_part.part,
            mimi_content::NestedPartContent::NullPart,
            "Deleted message should have NullPart content for Alice"
        );
    }

    // Verify: Bob received the deletion (message is NullPart)
    {
        let bob_user = &setup.get_user(&bob).user;
        let messages = bob_user.messages(chat_id, 10).await.unwrap();
        // Find the message that was originally an attachment but is now NullPart
        let deleted = messages
            .iter()
            .filter(|m| m.message().mimi_content().is_some())
            .find(|m| {
                m.message().mimi_content().unwrap().nested_part.part
                    == mimi_content::NestedPartContent::NullPart
            })
            .expect("Bob should have a deleted (NullPart) message");
        assert_eq!(
            deleted.message().mimi_content().unwrap().nested_part.part,
            mimi_content::NestedPartContent::NullPart,
            "Deleted message should have NullPart content for Bob"
        );
    }

    // Verify: Attachment record is deleted for the sender (Alice)
    let alice_user = &setup.get_user(&alice).user;
    let attachment_ids = alice_user
        .attachment_ids_for_message(message_id)
        .await
        .unwrap();
    assert!(
        attachment_ids.is_empty(),
        "Alice's attachment should be deleted after message deletion"
    );

    // Verify: Attachment record is deleted for Bob (the receiver)
    {
        let bob_user = &setup.get_user(&bob).user;
        let messages = bob_user.messages(chat_id, 10).await.unwrap();
        // Find the deleted message (NullPart)
        let deleted_msg = messages
            .iter()
            .filter(|m| m.message().mimi_content().is_some())
            .find(|m| {
                m.message().mimi_content().unwrap().nested_part.part
                    == mimi_content::NestedPartContent::NullPart
            })
            .expect("Bob should have the deleted message");

        let bob_attachment_ids = bob_user
            .attachment_ids_for_message(deleted_msg.id())
            .await
            .unwrap();
        assert!(
            bob_attachment_ids.is_empty(),
            "Bob's attachment should be deleted after message deletion"
        );
    }
}
