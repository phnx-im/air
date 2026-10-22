// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Deleting messages for everyone: replies, receipts, edits, attachments and
//! groups.

use aircoreclient::{ChatMessage, MimiContentExt, clients::MarkChatAsRead};
use airserver_test_harness::utils::{core_user_ext::CoreUserExt, setup::TestBackend};
use mimi_content::{MessageStatus, MimiContent};

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn deleted_message_is_erased_from_databases() {
    let mut setup = TestBackend::single().await;

    // Create alice and bob
    let [alice, bob] = setup.add_users().await;

    // Connect them
    let chat_alice_bob = setup.connect_users(&alice, &bob).await;

    setup.send_message(chat_alice_bob, &alice, &[&bob]).await;

    let last_message = alice.expect_last_message(chat_alice_bob).await;

    let string = {
        last_message
            .message()
            .mimi_content()
            .unwrap()
            .string_rendering()
            .unwrap()
    };

    assert!(
        !setup
            .scan_database(&string, false, &[&alice, &bob])
            .await
            .is_empty(),
    );

    // Alice (sender) should have 0 unread messages
    assert_eq!(alice.unread_messages_count(chat_alice_bob).await, 0);
    assert_eq!(alice.global_unread_messages_count().await.unwrap(), 0);

    setup
        .delete_message(&alice, &[&bob], last_message.id())
        .await;

    // After delete-for-everyone, Alice should still have 0 unread messages
    assert_eq!(alice.unread_messages_count(chat_alice_bob).await, 0);
    assert_eq!(alice.global_unread_messages_count().await.unwrap(), 0);

    assert_eq!(
        setup.scan_database(&string, false, &[&alice, &bob]).await,
        Vec::<String>::new()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn deleted_message_gets_no_read_receipt() {
    let mut setup = TestBackend::single().await;

    let [alice, bob] = setup.add_users().await;
    let chat_alice_bob = setup.connect_users(&alice, &bob).await;

    setup.send_message(chat_alice_bob, &alice, &[&bob]).await;

    let message_id = alice.expect_last_message(chat_alice_bob).await.id();

    setup.delete_message(&alice, &[&bob], message_id).await;

    // Bob has processed the deletion, so anything he scheduled for it goes out
    // now.
    bob.outbound_service().run_once().await;

    // Opening the chat must not report a read status for the deletion either.
    let bob_last_message = bob.expect_last_message(chat_alice_bob).await;
    let (_, marked_as_read) = bob
        .mark_chat_as_read(chat_alice_bob, bob_last_message.id())
        .await
        .unwrap();
    assert!(marked_as_read.is_empty());
    bob.outbound_service().run_once().await;

    alice.drain().await;

    let deleted = alice.expect_message(message_id).await;
    assert_eq!(deleted.status(), MessageStatus::Deleted);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn deleted_message_empties_reply_quotes() -> anyhow::Result<()> {
    let mut setup = TestBackend::single().await;

    // Create Alice and Bob and connect them
    let [alice, bob] = setup.add_users().await;
    let chat_alice_bob = setup.connect_users(&alice, &bob).await;

    let alice_says_hi_to_bob = setup.send_message(chat_alice_bob, &alice, &[&bob]).await;

    let bob_says_hi_to_alice = setup.send_message(chat_alice_bob, &bob, &[&alice]).await;

    let bob_replies_to_alice = setup
        .send_reply(
            chat_alice_bob,
            &bob,
            &[&alice],
            &alice_says_hi_to_bob.mimi_id,
        )
        .await;

    // Bob replies to his own message
    let bob_replies_to_self = setup
        .send_reply(
            chat_alice_bob,
            &bob,
            &[&alice],
            &bob_says_hi_to_alice.mimi_id,
        )
        .await;

    // Alice deletes her original message for everyone
    setup
        .delete_message(&alice, &[&bob], alice_says_hi_to_bob.own_message_id)
        .await;

    // Check that the reply show that they're deleted on Alice's device

    let alice_says_hi_to_bob = alice
        .message(alice_says_hi_to_bob.own_message_id)
        .await?
        .unwrap();
    let alice_says_hi_to_bob_mimi_id = *alice_says_hi_to_bob.message().mimi_id().unwrap();
    assert!(
        alice_says_hi_to_bob.message().is_deleted(),
        "Alice's first message should be redacted (deleted)"
    );

    let bob_reply_to_alice = alice
        .message(bob_replies_to_alice.recipient_message_id(alice.user_id()))
        .await?
        .unwrap();

    let (bob_reply_to_alice_irt_mimi_id, bob_reply_to_alice_irt) =
        bob_reply_to_alice.in_reply_to().cloned().unwrap();

    assert_eq!(
        bob_reply_to_alice_irt_mimi_id, alice_says_hi_to_bob_mimi_id,
        "Bob's reply to Alice should point to the updated empty MIMI content"
    );

    assert!(
        bob_reply_to_alice_irt
            .unwrap()
            .mimi_content
            .as_ref()
            .unwrap()
            .nested_part
            .is_null_part(),
        "Bob's reply to Alice should have a loaded empty MIMI content"
    );

    // Now we do the same on Bob's device, with a twist...

    // ...bob also deletes his original message locally
    bob.delete_message(bob_says_hi_to_alice.own_message_id)
        .await?;

    // [1] check that the message that Alice deleted for everyone
    // is also redacted (marked as deleted) on Bob's device
    let bob_replies_to_alice = bob
        .message(bob_replies_to_alice.own_message_id)
        .await?
        .unwrap();

    let (bob_replies_to_alice_irt_mimi_id, bob_replies_to_alice_irt) =
        bob_replies_to_alice.in_reply_to().cloned().unwrap();

    assert_eq!(
        bob_replies_to_alice_irt_mimi_id, alice_says_hi_to_bob_mimi_id,
        "Bob's reply to Alice should point to "
    );

    assert!(
        bob_replies_to_alice_irt
            .unwrap()
            .mimi_content
            .unwrap()
            .nested_part
            .is_null_part(),
        "Bob's reply to Alice should have a loaded empty MIMI content"
    );

    // [2] check that the reply from Bob to self has a dangling reply
    // reference (invalid MIMI ID).
    let bob_replies_to_self = bob
        .message(bob_replies_to_self.own_message_id)
        .await?
        .unwrap();

    let (_, bob_replies_to_self_irt) = bob_replies_to_self.in_reply_to().unwrap();

    assert!(
        bob_replies_to_self_irt.is_none(),
        "Bob reply to self should not be loaded (invalid MIMI ID)"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn deleted_message_is_erased_in_group() {
    let mut setup = TestBackend::single().await;

    // Create alice, bob, and charlie
    let [alice, bob, charlie] = setup.add_users().await;

    // Connect alice with bob and charlie (needed for group invites)
    setup.connect_users(&alice, &bob).await;
    setup.connect_users(&alice, &charlie).await;

    // Create a group and invite bob and charlie
    let group_chat = setup.create_group(&alice).await;
    setup
        .invite_to_group(group_chat, &alice, &[&bob, &charlie])
        .await;

    // Alice sends a message to the group
    setup
        .send_message(group_chat, &alice, &[&bob, &charlie])
        .await;

    // Get the message content before deletion
    let last_message = alice.expect_last_message(group_chat).await;
    let original_content = last_message
        .message()
        .mimi_content()
        .unwrap()
        .string_rendering()
        .unwrap();

    // Verify message exists on all users
    assert!(
        !setup
            .scan_database(&original_content, false, &[&alice, &bob, &charlie])
            .await
            .is_empty()
    );

    // Alice deletes the message
    setup
        .delete_message(&alice, &[&bob, &charlie], last_message.id())
        .await;

    // Verify the content is no longer in any database
    assert_eq!(
        setup
            .scan_database(&original_content, false, &[&alice, &bob, &charlie])
            .await,
        Vec::<String>::new()
    );

    // Verify the message still exists but with NullPart content for all users
    for user in [&alice, &bob, &charlie] {
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
            deleted_message.message().is_deleted(),
            "Deleted message should have NullPart content"
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn deletion_keeps_other_messages() {
    let mut setup = TestBackend::single().await;

    let [alice, bob] = setup.add_users().await;

    let chat_id = setup.connect_users(&alice, &bob).await;

    // Send three messages with distinct content
    let contents = ["First message", "Second message to delete", "Third message"];
    for content in &contents {
        let message_content = MimiContent::simple_markdown_message(content.to_string(), [0; 16]);
        alice
            .send_message(chat_id, message_content, None, MarkChatAsRead::Yes)
            .await
            .unwrap();
    }
    alice.outbound_service().run_once().await;

    // Bob receives all messages
    bob.drain().await;

    // Helper to filter only our test messages (not system messages)
    let is_test_message = |m: &&ChatMessage| {
        m.message()
            .mimi_content()
            .map(|c| {
                let text = c.string_rendering().unwrap_or_default();
                text.contains("First")
                    || text.contains("Second")
                    || text.contains("Third")
                    || c.nested_part.is_null_part()
            })
            .unwrap_or(false)
    };

    // Verify bob has all 3 test messages
    let bob_messages = bob.messages(chat_id, 10).await.unwrap();
    let bob_test_messages: Vec<_> = bob_messages.iter().filter(is_test_message).collect();
    assert_eq!(bob_test_messages.len(), 3);

    // Get the middle message for deletion
    let alice_messages = alice.messages(chat_id, 10).await.unwrap();
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
    alice.drain().await;
    alice
        .delete_message_for_everyone(chat_id, message_to_delete_id)
        .await
        .unwrap();
    alice.outbound_service().run_once().await;

    // Bob receives the deletion
    bob.drain().await;

    // Verify first and third messages still have their original content
    let bob_messages = bob.messages(chat_id, 10).await.unwrap();
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
    let deleted_msg = bob_test_messages
        .into_iter()
        .find(|m| m.message().is_deleted());
    assert!(
        deleted_msg.is_some(),
        "Deleted message should exist with NullPart"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn deleting_edited_message_erases_edit() {
    let mut setup = TestBackend::single().await;

    let [alice, bob] = setup.add_users().await;

    let chat_id = setup.connect_users(&alice, &bob).await;

    // Alice sends a message
    setup.send_message(chat_id, &alice, &[&bob]).await;

    // Alice edits the message
    setup.edit_message(chat_id, &alice, &[&bob]).await;

    let last_message = alice.expect_last_message(chat_id).await;

    // Get the edited content for verification
    let edited_content = {
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
            .scan_database(&edited_content, false, &[&alice, &bob])
            .await
            .is_empty()
    );

    // Alice deletes the edited message
    setup
        .delete_message(&alice, &[&bob], last_message.id())
        .await;

    // Verify the edited content is no longer present
    assert_eq!(
        setup
            .scan_database(&edited_content, false, &[&alice, &bob])
            .await,
        Vec::<String>::new()
    );

    // Verify the message exists but with NullPart content
    let alice_messages = alice.messages(chat_id, 10).await.unwrap();
    let content_messages: Vec<_> = alice_messages
        .iter()
        .filter(|m| m.message().mimi_content().is_some())
        .collect();
    assert!(!content_messages.is_empty());

    let deleted_message = content_messages.last().unwrap();
    assert!(
        deleted_message.message().is_deleted(),
        "Deleted message should have NullPart content"
    );
    assert_eq!(
        deleted_message.status(),
        MessageStatus::Deleted,
        "Message status should be Deleted"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn deleted_message_drops_attachment_records() {
    let mut setup = TestBackend::single().await;

    // Create alice and bob
    let [alice, bob] = setup.add_users().await;

    // Connect them
    let chat_id = setup.connect_users(&alice, &bob).await;

    // Alice sends an attachment
    let (_returned_message_id, _external_part) = setup
        .send_attachment(
            chat_id,
            &alice,
            &[&bob],
            b"test attachment data",
            "test.bin",
        )
        .await
        .unwrap();

    // Get the actual message_id of the last message (the attachment message)
    let message_id = {
        let messages = alice.messages(chat_id, 10).await.unwrap();
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
        let attachment_infos = alice.attachment_infos_for_message(message_id).await;
        assert_eq!(
            attachment_infos.len(),
            1,
            "Attachment should exist before deletion"
        );
    }

    // Bob downloads the attachment (so he has an attachment record to verify deletion)
    {
        let messages = bob.messages(chat_id, 10).await.unwrap();
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

        let bob_attachment_infos = bob.attachment_infos_for_message(attachment_msg.id()).await;

        // Download each attachment
        for remote_attachment_info in &bob_attachment_infos {
            let (_, download_future) =
                bob.download_attachment(remote_attachment_info.attachment_id);
            download_future.await.unwrap();
        }

        // Verify Bob has the attachment
        assert!(
            !bob_attachment_infos.is_empty(),
            "Bob should have attachment after download"
        );
    }

    // Alice deletes the message (network deletion)
    // Note: We need to delete the specific attachment message, not just the last message
    {
        // Fetch and process QS messages first
        alice.drain().await;

        // Delete the specific message by ID
        alice
            .delete_message_for_everyone(chat_id, message_id)
            .await
            .unwrap();
        alice.outbound_service().run_once().await;
    }

    // Bob receives the deletion
    {
        bob.drain().await;
    }

    // Verify: Message is NullPart for Alice (the sender)
    {
        let messages = alice.messages(chat_id, 10).await.unwrap();
        let deleted = messages
            .iter()
            .find(|m| m.id() == message_id)
            .expect("Alice should still have the message");
        assert!(
            deleted.message().is_deleted(),
            "Deleted message should have NullPart content for Alice"
        );
    }

    // Verify: Bob received the deletion (message is NullPart)
    {
        let messages = bob.messages(chat_id, 10).await.unwrap();
        // Find the message that was originally an attachment but is now NullPart
        let deleted = messages
            .into_iter()
            .filter(|m| m.message().mimi_content().is_some())
            .find(|m| m.message().is_deleted())
            .expect("Bob should have a deleted (NullPart) message");
        assert!(
            deleted.message().is_deleted(),
            "Deleted message should have NullPart content for Bob"
        );
    }

    // Verify: Attachment record is deleted for the sender (Alice)
    let attachment_infos = alice.attachment_infos_for_message(message_id).await;
    assert!(
        attachment_infos.is_empty(),
        "Alice's attachment should be deleted after message deletion"
    );

    // Verify: Attachment record is deleted for Bob (the receiver)
    {
        let messages = bob.messages(chat_id, 10).await.unwrap();
        // Find the deleted message (NullPart)
        let deleted_msg = messages
            .iter()
            .filter(|m| m.message().mimi_content().is_some())
            .find(|m| m.message().is_deleted())
            .expect("Bob should have the deleted message");

        let bob_attachment_infos = bob.attachment_infos_for_message(deleted_msg.id()).await;
        assert!(
            bob_attachment_infos.is_empty(),
            "Bob's attachment should be deleted after message deletion"
        );
    }
}
