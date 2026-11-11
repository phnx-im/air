// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Sending messages: failure handling, retries, pending proposals, and reading
//! messages back.

use std::time::Duration;

use aircoreclient::{UploadTaskError, clients::MarkChatAsRead};
use airserver_test_harness::utils::{core_user_ext::CoreUserExt, setup::TestBackend};
use mimi_content::{MessageStatus, MimiContent};
use rand::{RngExt, distr::Alphanumeric};

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn sent_messages_are_listed_in_order() {
    let mut setup = TestBackend::single().await;

    let [alice, bob] = setup.add_users().await;

    let chat_id = setup.connect_users(&alice, &bob).await;

    let number_of_messages = 10;
    let mut messages_sent = vec![];
    for _ in 0..number_of_messages {
        let message: String = rand::rng()
            .sample_iter(&Alphanumeric)
            .take(32)
            .map(char::from)
            .collect();
        let message_content = MimiContent::simple_markdown_message(message, [0; 16]); // simple seed for testing
        let message = alice
            .send_message(chat_id, message_content, None, MarkChatAsRead::Yes)
            .await
            .unwrap();
        messages_sent.push(message);
    }

    // Let's see what Alice's messages for this chat look like.
    let messages_retrieved = alice.messages(chat_id, number_of_messages).await.unwrap();

    assert_eq!(messages_retrieved.len(), messages_sent.len());
    assert_eq!(messages_retrieved, messages_sent);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn dropped_sends_are_marked_failed() {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;

    let chat_id = setup.connect_users(&alice, &bob).await;

    let content = MimiContent::simple_markdown_message("Hello".to_string(), [0; 16]);

    // Make server drop messages
    setup.listener_control_handle().set_drop_all();

    // Send three messages
    for _ in 0..3 {
        alice
            .send_message(chat_id, content.clone(), None, MarkChatAsRead::Yes)
            .await
            .unwrap();
    }
    alice.outbound_service().run_once().await;
    // Check that messages are marked as failed
    let messages = alice.messages(chat_id, 3).await.unwrap();
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
async fn transient_send_failure_is_retried() {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    let chat_id = setup.connect_users(&alice, &bob).await;

    // Flush what the connection setup left queued, so that the injected failure
    // lands on the message sent below.
    alice.outbound_service().run_once().await;

    const TEXT: &str = "retried after a transient failure";
    let content = MimiContent::simple_markdown_message(TEXT.to_owned(), [11u8; 16]);
    let message = alice
        .send_message(chat_id, content, None, MarkChatAsRead::Yes)
        .await
        .unwrap();
    assert!(
        !message.is_sent(),
        "sending is deferred to the outbound service"
    );

    // The DS rejects the first attempt with an availability error, which is
    // recoverable, so the outbound service retries it.
    setup.listener_control_handle().set_drop_next_request();
    alice
        .outbound_service()
        .send_queued_messages_once()
        .await
        .unwrap();

    let stored = alice.expect_message(message.id()).await;
    assert!(
        stored.is_sent(),
        "a transient failure must not stop the message from being sent"
    );
    assert_ne!(
        stored.status(),
        MessageStatus::Error,
        "a retried message must not be marked as failed"
    );

    bob.drain().await;
    assert_eq!(
        bob.text_messages(chat_id)
            .await
            .iter()
            .filter(|rendered| *rendered == TEXT)
            .count(),
        1,
        "the recipient receives the retried message exactly once"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn lost_response_is_retried_and_deduplicated() {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    let chat_id = setup.connect_users(&alice, &bob).await;

    alice.outbound_service().run_once().await;

    const TEXT: &str = "delivered twice, stored once";
    let content = MimiContent::simple_markdown_message(TEXT.to_owned(), [12u8; 16]);
    let message = alice
        .send_message(chat_id, content, None, MarkChatAsRead::Yes)
        .await
        .unwrap();

    // The DS accepts and fans out the first attempt, but the response never
    // reaches Alice. The retry sends the same content again.
    setup.listener_control_handle().set_drop_next_response();
    alice
        .outbound_service()
        .send_queued_messages_once()
        .await
        .unwrap();

    let stored = alice.expect_message(message.id()).await;
    assert!(
        stored.is_sent(),
        "a lost response must not stop the message from being sent"
    );

    bob.drain().await;
    assert_eq!(
        bob.text_messages(chat_id)
            .await
            .iter()
            .filter(|rendered| *rendered == TEXT)
            .count(),
        1,
        "the recipient deduplicates the two deliveries of the same message"
    );
    let bob_message = bob.expect_last_message(chat_id).await;
    assert_eq!(
        bob_message
            .message()
            .mimi_content()
            .unwrap()
            .string_rendering()
            .unwrap(),
        TEXT
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn fatal_send_failure_fails_only_that_message() {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    let chat_id = setup.connect_users(&alice, &bob).await;

    alice.outbound_service().run_once().await;

    const DOOMED_TEXT: &str = "rejected for good";
    const HEALTHY_TEXT: &str = "sent after a fatal failure";
    let doomed = alice
        .send_message(
            chat_id,
            MimiContent::simple_markdown_message(DOOMED_TEXT.to_owned(), [13u8; 16]),
            None,
            MarkChatAsRead::Yes,
        )
        .await
        .unwrap();
    // The queue is ordered by insertion time, so the pause makes sure the
    // doomed message is dequeued first.
    tokio::time::sleep(Duration::from_millis(5)).await;
    let healthy = alice
        .send_message(
            chat_id,
            MimiContent::simple_markdown_message(HEALTHY_TEXT.to_owned(), [14u8; 16]),
            None,
            MarkChatAsRead::Yes,
        )
        .await
        .unwrap();

    // The DS rejects the first send with a permanent error. Only that message
    // fails, the run goes on with the next queued message.
    setup.listener_control_handle().set_reject_next_request();
    alice
        .outbound_service()
        .send_queued_messages_once()
        .await
        .unwrap();

    assert_eq!(
        alice.expect_message(doomed.id()).await.status(),
        MessageStatus::Error,
        "a message the DS rejects for good must be marked as failed"
    );
    assert!(
        alice.expect_message(healthy.id()).await.is_sent(),
        "a fatal failure must not fail the rest of the queue"
    );

    bob.drain().await;
    assert_eq!(
        bob.text_messages(chat_id)
            .await
            .iter()
            .filter(|rendered| *rendered == HEALTHY_TEXT)
            .count(),
        1,
        "the message queued behind the failed one is delivered"
    );
    assert_eq!(
        bob.text_messages(chat_id)
            .await
            .iter()
            .filter(|rendered| *rendered == DOOMED_TEXT)
            .count(),
        0,
        "the rejected message never reaches the recipient"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn send_commits_pending_proposals() {
    let mut setup = TestBackend::single().await;
    let [alice, bob, charlie] = setup.add_users().await;

    let chat_id = setup.connected_group(&alice, &[&bob, &charlie]).await;

    // Bob leaves, which proposes his own removal. Nobody commits it for Alice.
    bob.leave_chat(chat_id).await.unwrap();

    alice.drain().await;
    assert_eq!(
        alice.pending_removes(chat_id).await.unwrap(),
        vec![bob.user_id().clone()],
        "Alice's group should hold Bob's pending removal"
    );

    const TEXT: &str = "sent on top of a pending proposal";
    let message = alice
        .send_message(
            chat_id,
            MimiContent::simple_markdown_message(TEXT.to_owned(), [15u8; 16]),
            None,
            MarkChatAsRead::Yes,
        )
        .await
        .unwrap();
    alice
        .outbound_service()
        .send_queued_messages_once()
        .await
        .unwrap();

    assert!(
        alice.expect_message(message.id()).await.is_sent(),
        "the send path commits the pending proposal instead of failing"
    );
    assert!(
        alice.pending_removes(chat_id).await.unwrap().is_empty(),
        "the pending proposal is committed"
    );
    assert!(
        !alice
            .chat_participants(chat_id)
            .await
            .unwrap()
            .contains(bob.user_id()),
        "Bob is removed by the commit"
    );

    charlie.drain().await;
    assert_eq!(
        charlie
            .text_messages(chat_id)
            .await
            .iter()
            .filter(|rendered| *rendered == TEXT)
            .count(),
        1,
        "the remaining member receives the message"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn send_keeps_message_when_its_commit_loses_the_epoch() {
    let mut setup = TestBackend::single().await;
    let [alice, bob, charlie] = setup.add_users().await;
    let chat_id = setup.connected_group(&alice, &[&bob, &charlie]).await;

    // Charlie leaves, which proposes his own removal.
    charlie.leave_chat(chat_id).await.unwrap();
    alice.drain().await;
    bob.drain().await;

    // Alice commits the proposal before Bob learns about her commit.
    alice.update_key(chat_id).await.unwrap();

    const TEXT: &str = "sent after a lost epoch race";
    let content = MimiContent::simple_markdown_message(TEXT.to_owned(), [16u8; 16]);
    let message = bob
        .send_message(chat_id, content, None, MarkChatAsRead::Yes)
        .await
        .unwrap();

    // Bob's commit of the proposal hits a wrong epoch and is parked.
    bob.outbound_service()
        .send_queued_messages_once()
        .await
        .unwrap();
    let pending = bob
        .pending_chat_operation_info(chat_id)
        .await
        .unwrap()
        .expect("the rejected commit should be parked");
    assert_eq!(pending.request_status, "waiting_for_queue_response");
    assert!(!bob.expect_message(message.id()).await.is_sent());

    // Processing Alice's commit drops the parked one. The next run sends.
    bob.drain().await;
    bob.outbound_service().run_once().await;
    assert!(
        bob.expect_message(message.id()).await.is_sent(),
        "a lost epoch race must not drop the message"
    );

    alice.drain().await;
    assert_eq!(
        alice
            .text_messages(chat_id)
            .await
            .iter()
            .filter(|rendered| *rendered == TEXT)
            .count(),
        1,
        "Alice receives the message"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn message_queued_before_a_block_is_marked_failed() {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    let chat_id = setup.connect_users(&alice, &bob).await;

    const TEXT: &str = "queued before the block";
    let content = MimiContent::simple_markdown_message(TEXT.to_owned(), [17u8; 16]);
    let message = alice
        .send_message(chat_id, content, None, MarkChatAsRead::Yes)
        .await
        .unwrap();
    alice.block_contact(bob.user_id().clone()).await.unwrap();

    alice.outbound_service().run_once().await;
    assert_eq!(
        alice.expect_message(message.id()).await.status(),
        MessageStatus::Error,
        "the user sees that the message failed"
    );

    // The failed message left the queue. Unblocking does not send it.
    alice.unblock_contact(bob.user_id().clone()).await.unwrap();
    alice.outbound_service().run_once().await;
    let stored = alice.expect_message(message.id()).await;
    assert!(!stored.is_sent());
    assert_eq!(stored.status(), MessageStatus::Error);

    bob.drain().await;
    assert!(
        !bob.text_messages(chat_id)
            .await
            .iter()
            .any(|rendered| rendered == TEXT),
        "Bob never receives the message"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn attachments_finished_after_a_block_are_marked_failed() {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    let chat_id = setup.connect_users(&alice, &bob).await;

    let path = setup.temp_dir().join("attachment.bin");
    std::fs::write(&path, [0u8, 1, 2, 3]).unwrap();
    let (_, _, upload) = alice
        .upload_chat_attachment(chat_id, &path, MarkChatAsRead::Yes)
        .await
        .unwrap()
        .unwrap();
    let (_, _, failing_upload) = alice
        .upload_chat_attachment(chat_id, &path, MarkChatAsRead::Yes)
        .await
        .unwrap()
        .unwrap();

    // Alice blocks Bob before the uploads finish.
    alice.block_contact(bob.user_id().clone()).await.unwrap();

    let uploaded = upload.await.unwrap();
    alice
        .outbound_service()
        .enqueue_chat_message(uploaded.id())
        .await
        .unwrap();

    setup
        .listener_control_handle()
        .set_reject_next_request_to("ProvisionAttachment");
    let Err(UploadTaskError::Failed {
        message_id: failed_id,
        ..
    }) = failing_upload.await
    else {
        panic!("an upload the DS rejects should fail");
    };
    alice
        .outbound_service()
        .fail_enqueued_chat_message(failed_id)
        .await
        .unwrap();

    // Only the send stage runs. The attachment recovery of a full run would
    // enqueue the uploaded message by itself.
    alice
        .outbound_service()
        .send_queued_messages_once()
        .await
        .unwrap();
    for message_id in [uploaded.id(), failed_id] {
        assert_eq!(
            alice.expect_message(message_id).await.status(),
            MessageStatus::Error,
            "the user sees that the message failed"
        );
    }
}
