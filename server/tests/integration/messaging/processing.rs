// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! QS message processing: cancellation safety and partial processing.

use std::time::Duration;

use aircoreclient::clients::{
    MarkChatAsRead,
    process::qs_stream::{QsProcessEventResult, QsStreamProcessor},
};
use airserver_test_harness::utils::setup::TestBackend;
use mimi_content::MimiContent;
use rand::RngExt;
use tokio_stream::StreamExt;
use tracing::info;

/// Test that [`CoreUser::fully_process_qs_messages`] is cancellation-safe.
///
/// The test spawns a processing task and aborts it at a random point in its execution,
/// repeating until all messages are known to be accessible or a loss is detected.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn process_qs_messages_cancellation_safety() {
    let mut setup = TestBackend::single().await;

    // Note: It is important that the user is persisted, because we use *multiple* db connections
    // and this is not supported for in-memory sqlite.
    let alice = setup.add_persisted_user().await;
    setup.add_username(&alice).await;
    let bob = setup.add_persisted_user().await;
    let chat_id = setup.connect_users(&alice, &bob).await;

    const NUM_MESSAGES: usize = 5;

    for idx in 0..NUM_MESSAGES {
        let msg = MimiContent::simple_markdown_message("Hello bob".into(), [idx as u8; 16]);
        alice
            .send_message(chat_id, msg, None, MarkChatAsRead::Yes)
            .await
            .unwrap();
    }
    alice.outbound_service().run_once().await;

    // Abort a processing task after a random short delay.
    const MAX_ATTEMPTS: usize = 50;
    for _ in 0..MAX_ATTEMPTS {
        let messages = bob.qs_fetch_messages().await.unwrap();
        if messages.is_empty() {
            break;
        }
        info!(num_messages = messages.len(), "processing messages");

        // Random delay in [30, 500) µs
        let delay_us = rand::rng().random_range(30u64..500);

        tokio::select! {
            _ = tokio::time::sleep(Duration::from_micros(delay_us)) => {},
            _ = bob.fully_process_qs_messages(messages) => {},
        }
    }

    // Final pass without aborting
    let remaining = bob.qs_fetch_messages().await.unwrap();
    info!(
        num_messages = remaining.len(),
        "processing remaining messages"
    );
    if !remaining.is_empty() {
        bob.fully_process_qs_messages(remaining).await;
    }

    let unread = bob.unread_messages_count(chat_id).await;
    assert_eq!(
        unread, NUM_MESSAGES,
        "messages lost after cancelled processing"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn qs_stream_processor_partially_processes_messages() {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;

    let connection_chat_id = setup.connect_users(&alice, &bob).await;
    let group_chat_id = setup.create_group(&alice).await;
    setup.invite_to_group(group_chat_id, &alice, &[&bob]).await;

    // Remove bob chat only locally on their client
    bob.erase_chat(group_chat_id).await.unwrap();

    let content = MimiContent::simple_markdown_message("Hello from Alice!".to_owned(), [0; 16]);

    alice
        .send_message(
            connection_chat_id,
            content.clone(),
            None,
            MarkChatAsRead::Yes,
        )
        .await
        .unwrap();
    alice
        .send_message(group_chat_id, content.clone(), None, MarkChatAsRead::Yes)
        .await
        .unwrap();
    alice
        .send_message(connection_chat_id, content, None, MarkChatAsRead::Yes)
        .await
        .unwrap();
    alice.outbound_service().run_once().await;

    let (mut stream, responder) = bob.listen_queue().await.unwrap();
    let mut processor = QsStreamProcessor::new(Some(responder));

    while let Some(Ok(message)) = stream.next().await {
        match processor.process_event(&bob, message).await {
            QsProcessEventResult::Accumulated => (),
            QsProcessEventResult::Ignored => (),
            QsProcessEventResult::FullyProcessed { processed } => {
                assert_eq!(processed.processed, 3);
                assert_eq!(processed.errors.len(), 1);
                return;
            }
            QsProcessEventResult::PartiallyProcessed { .. } => unreachable!(),
        }
    }
}

// #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
// #[tracing::instrument(name = "Client sequence number race", skip_all)]
// async fn client_sequence_number_race() {
//     init_test_logging();

//     let mut setup = TestBackend::single().await;

//     let alice = setup.add_user().await;
//     setup.get_user_mut(&alice).add_username().await;

//     let bob = setup.add_user().await;

//     let chat_id = setup.connect_users(&alice, &bob).await;

//     info!("Alice sending messages to queue");

//     let alice = setup.get_user(&alice);

//     const NUM_SENDERS: usize = 5;
//     const NUM_MESSAGES: usize = 10;
//     let alice_user = alice.user.clone();
//     for _ in 0..NUM_SENDERS {
//         let alice_user = alice_user.clone();
//         tokio::spawn(async move {
//             for _ in 0..NUM_MESSAGES {
//                 const SALT: [u8; 16] = [0; 16];
//                 let message = MimiContent::simple_markdown_message("Hello bob".into(), SALT);
//                 alice_user
//                     .send_message(chat_id, message, None)
//                     .await
//                     .unwrap();
//                 alice_user.outbound_service().run_once().await;
//             }
//         });
//     }

//     info!("Bob getting messages from queue");

//     const NUM_CLIENTS: usize = 2;
//     let mut join_set = JoinSet::new();

//     let bob_user = setup.user(&bob).clone();
//     let (processed, processed_rx) = tokio::sync::watch::channel(0);

//     for _ in 0..NUM_CLIENTS {
//         let bob_user = bob_user.clone();
//         let processed = processed.clone();
//         let mut processed_rx = processed_rx.clone();
//         join_set.spawn(async move {
//             loop {
//                 if *processed.borrow() == NUM_SENDERS * NUM_MESSAGES {
//                     break;
//                 }

//                 let Ok((mut stream, responder)) = bob_user.listen_queue().await else {
//                     continue;
//                 };

//                 let mut handler = QsStreamProcessor::new(Some(responder));

//                 loop {
//                     let finished =
//                         processed_rx.wait_for(|processed| *processed == NUM_SENDERS * NUM_MESSAGES);
//                     let event = tokio::select! {
//                         _ = tokio::time::sleep(Duration::from_secs(30)) => panic!("timeout waiting for condition: test failed!"),
//                         _ = finished => break,
//                         event = stream.next() => event
//                     };
//                     let Some(event) = event else {
//                         break;
//                     };

//                     let result = handler.process_event(&bob_user, event).await;

//                     processed.send_modify(|processed| {
//                         *processed += result.processed();
//                     });
//                     if result.is_partially_processed() {
//                         break; // stop the stream when only partially processed
//                     }
//                 }
//             }
//         });
//     }
//     join_set.join_all().await; // panics on error

//     assert_eq!(*processed.borrow(), NUM_SENDERS * NUM_MESSAGES);
// }
