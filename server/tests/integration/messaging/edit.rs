// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Editing messages, incl. edits racing receipts and replayed edits.

use aircommon::identifiers::MimiId;
use aircoreclient::{
    ChatId, EDIT_WINDOW, MAX_EDITS, MessageEditError, MessageId,
    clients::{CoreUser, MarkChatAsRead},
};
use airserver_test_harness::utils::{core_user_ext::CoreUserExt, setup::TestBackend};
use chrono::Utc;
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

/// The other members never received an unsent message, so an edit of it
/// would replace a version they do not have.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn edit_of_unsent_message_is_refused() {
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
    assert!(!original.is_sent());
    let edit = MimiContent::simple_markdown_message("edited".to_owned(), rand::random());
    let result = alice
        .send_message(chat_id, edit, Some(original), MarkChatAsRead::Yes)
        .await;
    assert!(result.is_err());

    alice.outbound_service().run_once().await;
    bob.drain().await;
    assert_eq!(bob.text_messages(chat_id).await, ["hello"]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn edit_limit_is_enforced_by_sender_and_receiver() {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    let chat_id = setup.connect_users(&alice, &bob).await;

    alice.drain().await;
    bob.drain().await;

    let mut message = alice.send_text(chat_id, "v0").await;
    for edit in 1..=MAX_EDITS {
        message = alice.send_edit(&message, &format!("v{edit}")).await;
    }
    bob.drain().await;
    let last_version = format!("v{MAX_EDITS}");
    assert_eq!(
        bob.text_messages(chat_id).await,
        std::slice::from_ref(&last_version)
    );

    // Alice's client refuses another edit.
    let edit = MimiContent::simple_markdown_message("one too many".to_owned(), rand::random());
    let error = alice
        .send_message(
            chat_id,
            edit.clone(),
            Some(message.clone()),
            MarkChatAsRead::Yes,
        )
        .await
        .unwrap_err();
    assert_eq!(
        error.downcast_ref::<MessageEditError>(),
        Some(&MessageEditError::LimitReached)
    );

    // Bob ignores an edit that bypasses the check.
    let mut edit = edit;
    edit.replaces = message
        .message()
        .mimi_id()
        .map(|mimi_id| mimi_id.as_slice().to_vec());
    alice
        .send_mimi_content_without_confirmation(chat_id, edit)
        .await
        .unwrap();
    bob.drain().await;
    assert_eq!(bob.text_messages(chat_id).await, [last_version]);
}

/// The last own message is what the edit shortcut picks up.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn last_editable_message_respects_edit_limits() {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    let chat_id = setup.connect_users(&alice, &bob).await;

    alice.drain().await;
    bob.drain().await;

    let mut message = alice.send_text(chat_id, "v0").await;
    let last_editable_id = async |now| {
        alice
            .last_editable_message(chat_id, now)
            .await
            .unwrap()
            .map(|message| message.id())
    };
    assert_eq!(last_editable_id(Utc::now()).await, Some(message.id()));
    let window_end = message.timestamp() + EDIT_WINDOW;
    assert_eq!(last_editable_id(window_end).await, None);

    for edit in 1..MAX_EDITS {
        message = alice.send_edit(&message, &format!("v{edit}")).await;
    }
    assert_eq!(last_editable_id(Utc::now()).await, Some(message.id()));

    alice.send_edit(&message, &format!("v{MAX_EDITS}")).await;
    assert_eq!(last_editable_id(Utc::now()).await, None);
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
