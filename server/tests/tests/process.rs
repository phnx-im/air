// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::time::Duration;

use aircoreclient::store::Store;
use airserver_test_harness::utils::setup::TestBackend;
use mimi_content::MimiContent;
use rand::{Rng, thread_rng};
use tracing::info;

/// Test that [`CoreUser::fully_process_qs_messages`] is cancellation-safe.
///
/// The test spawns a processing task and aborts it at a random point in its execution,
/// repeating until all messages are known to be accessible or a loss is detected.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[tracing::instrument(name = "Process QS messages cancellation safety", skip_all)]
async fn process_qs_messages_cancellation_safety() {
    let mut setup = TestBackend::single().await;

    // Note: It is important that the user is persisted, because we use *multiple* db connections
    // and this is not supported for in-memory sqlite.
    let alice = setup.add_persisted_user().await;
    setup.get_user_mut(&alice).add_username().await.unwrap();
    let bob = setup.add_persisted_user().await;
    let chat_id = setup.connect_users(&alice, &bob).await;

    const NUM_MESSAGES: usize = 5;

    let alice_user = setup.get_user(&alice).user.clone();
    for idx in 0..NUM_MESSAGES {
        let msg = MimiContent::simple_markdown_message("Hello bob".into(), [idx as u8; 16]);
        alice_user.send_message(chat_id, msg, None).await.unwrap();
    }
    alice_user.outbound_service().run_once().await;

    let bob_user = setup.get_user(&bob).user.clone();

    // Abort a processing task after a random short delay.
    const MAX_ATTEMPTS: usize = 50;
    for _ in 0..MAX_ATTEMPTS {
        let messages = bob_user.qs_fetch_messages().await.unwrap();
        if messages.is_empty() {
            break;
        }
        info!(num_messages = messages.len(), "processing messages");

        // Random delay in [30, 500) µs
        let delay_us = thread_rng().gen_range(30u64..500);

        tokio::select! {
            _ = tokio::time::sleep(Duration::from_micros(delay_us)) => {},
            _ = bob_user.fully_process_qs_messages(messages) => {},
        }
    }

    // Final pass without aborting
    let remaining = bob_user.qs_fetch_messages().await.unwrap();
    info!(
        num_messages = remaining.len(),
        "processing remaining messages"
    );
    if !remaining.is_empty() {
        bob_user.fully_process_qs_messages(remaining).await;
    }

    let unread = bob_user.unread_messages_count(chat_id).await;
    assert_eq!(
        unread, NUM_MESSAGES,
        "messages lost after cancelled processing"
    );
}
