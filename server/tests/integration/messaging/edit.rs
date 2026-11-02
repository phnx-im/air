// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Editing messages, incl. edits racing receipts and replayed edits.

use std::time::Duration;

use aircommon::identifiers::MimiId;
use aircoreclient::{
    ChatId, MessageId,
    clients::{CoreUser, MarkChatAsRead},
};
use airserver_test_harness::utils::{core_user_ext::CoreUserExt, setup::TestBackend};
use mimi_content::{MessageStatus, MimiContent};

use super::stored_content;

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn edit_message() {
    let mut setup = TestBackend::single().await;

    // Create alice and bob
    let [alice, bob] = setup.add_users().await;

    // Connect them
    let chat_alice_bob = setup.connect_users(&alice, &bob).await;

    setup.send_message(chat_alice_bob, &alice, &[&bob]).await;

    setup.edit_message(chat_alice_bob, &alice, &[&bob]).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn edit_is_delivered_when_original_receipt_is_still_queued() {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    let chat_id = setup.connect_users(&alice, &bob).await;

    alice.drain().await;
    bob.drain().await;

    // Alice sends and edits before Bob picks up either message.
    let original = alice.send_text(chat_id, "hello").await;
    assert!(original.is_sent());
    alice.send_edit(&original, "edited").await;

    // Bob applies both at once, so the receipt for the original is still
    // queued when the one for the edit is scheduled.
    bob.drain().await;
    bob.outbound_service().run_once().await;

    alice.drain().await;
    let status = alice.expect_message(original.id()).await.status();
    assert_eq!(status, MessageStatus::Delivered);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[ignore = "edit of an unsent message is lost"]
async fn edit_of_unsent_message_is_delivered() {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    let chat_id = setup.connect_users(&alice, &bob).await;

    alice.drain().await;
    bob.drain().await;

    // Alice edits before the outbound service sent the original.
    let content = MimiContent::simple_markdown_message("hello".to_owned(), rand::random());
    let original = alice
        .send_message(chat_id, content, None, MarkChatAsRead::Yes)
        .await
        .unwrap();
    assert!(!original.is_sent());
    alice.send_edit(&original, "edited").await;

    bob.drain().await;
    assert_eq!(bob.text_messages(chat_id).await, ["edited"]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn stale_read_receipt_does_not_apply_to_edited_message() {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    let chat_id = setup.connect_users(&alice, &bob).await;

    let sent = setup.send_message(chat_id, &alice, &[&bob]).await;
    let message_id = sent.own_message_id;

    // Alice edits before Bob reads the original.
    let original = alice.expect_message(message_id).await;
    alice.send_edit(&original, "edited").await;
    assert_eq!(
        own_message_status(&alice, message_id).await,
        MessageStatus::Unread
    );

    // Bob reads the original. The receipt names the replaced version.
    let read = read_chat(&bob, chat_id).await;
    assert_eq!(read.len(), 1);
    alice.drain().await;
    assert_eq!(
        own_message_status(&alice, message_id).await,
        MessageStatus::Unread
    );

    // Only the receipt for the edited version counts.
    bob.drain().await;
    bob.outbound_service().run_once().await;
    alice.drain().await;
    assert_eq!(
        own_message_status(&alice, message_id).await,
        MessageStatus::Delivered
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn replayed_edit_is_ignored() {
    let mut setup = TestBackend::single().await;

    let [alice, bob] = setup.add_users().await;
    let chat_id = setup.connect_users(&alice, &bob).await;

    setup.send_message(chat_id, &alice, &[&bob]).await;
    let message_id = setup.edit_message(chat_id, &alice, &[&bob]).await;
    let first_edit = stored_content(&alice, message_id).await;

    // Bob receives the first edit a second time. The replay must not be
    // applied again.
    alice
        .send_mimi_content_without_confirmation(chat_id, first_edit.clone())
        .await
        .unwrap();
    process_replayed_edit(&bob).await;

    // A second edit still applies. A replay that lands in the edit history
    // would block it.
    setup.edit_message(chat_id, &alice, &[&bob]).await;
    let second_edit = stored_content(&alice, message_id).await;

    // Bob receives the first edit a third time, now as a superseded version.
    alice
        .send_mimi_content_without_confirmation(chat_id, first_edit)
        .await
        .unwrap();
    process_replayed_edit(&bob).await;

    // The replay must not revert the second edit.
    let bob_message = bob.expect_last_message(chat_id).await;
    assert_eq!(
        bob_message.message().mimi_content(),
        Some(&second_edit),
        "the replayed superseded edit must not revert the newer edit"
    );

    // The replays did not pollute the edit history: a third edit applies.
    setup.edit_message(chat_id, &alice, &[&bob]).await;
}

/// Fetches and processes the user's queue after a replayed edit, asserting
/// that the replay neither fails nor is stored as a new message.
async fn process_replayed_edit(user: &CoreUser) {
    let processed = user.drain().await;
    assert!(
        processed.new_messages.is_empty(),
        "a replayed edit must not be stored as a new message"
    );
}

/// Marks the chat as read until its last message and sends the read receipts
/// this produced. Returns what was newly read.
async fn read_chat(user: &CoreUser, chat_id: ChatId) -> Vec<(MessageId, MimiId)> {
    let last_message = user.expect_last_message(chat_id).await;
    let (_, read) = user
        .mark_chat_as_read(chat_id, last_message.id())
        .await
        .unwrap();
    let statuses = read
        .iter()
        .map(|(id, mimi_id)| (*id, mimi_id, MessageStatus::Read));
    user.outbound_service()
        .enqueue_receipts(chat_id, statuses)
        .await
        .unwrap();
    user.outbound_service().run_once().await;
    read
}

async fn own_message_status(user: &CoreUser, message_id: MessageId) -> MessageStatus {
    user.expect_message(message_id).await.status()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "an edit made while the original is being sent never reaches the peer"]
async fn edit_during_send_is_delivered() {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    let chat_id = setup.connect_users(&alice, &bob).await;
    alice.drain().await;
    bob.drain().await;

    let content = MimiContent::simple_markdown_message("hello".to_owned(), rand::random());
    let original = alice
        .send_message(chat_id, content, None, MarkChatAsRead::Yes)
        .await
        .unwrap();

    // Alice edits while the DS holds the send of the original.
    setup
        .listener_control_handle()
        .set_delay_next_request_to("SendMessage", Duration::from_millis(500));
    let sender = alice.clone();
    let send =
        tokio::spawn(async move { sender.outbound_service().send_queued_messages_once().await });
    tokio::time::sleep(Duration::from_millis(200)).await;
    let edit = MimiContent::simple_markdown_message("edited".to_owned(), rand::random());
    alice
        .send_message(chat_id, edit, Some(original.clone()), MarkChatAsRead::Yes)
        .await
        .unwrap();
    send.await.unwrap().unwrap();

    alice.outbound_service().run_once().await;
    bob.drain().await;
    assert_eq!(bob.text_messages(chat_id).await, ["edited"]);
}
