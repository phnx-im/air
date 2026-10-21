// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Server-side rate limiting.

use std::time::Duration;

use airbackend::settings::RateLimitsSettings;
use aircoreclient::clients::MarkChatAsRead;
use airserver_test_harness::utils::{
    core_user_ext::CoreUserExt,
    setup::{TestBackend, TestBackendParams},
};
use mimi_content::{MessageStatus, MimiContent};
use tracing::{info, warn};

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn rate_limit() {
    const PERIOD: Duration = Duration::from_millis(100);
    let mut setup = TestBackend::single_with_params(TestBackendParams {
        rate_limits: Some(RateLimitsSettings {
            period: PERIOD,
            burst: 100, // must be large enough to survive setup calls
        }),
        ..Default::default()
    })
    .await;

    if setup.is_external() {
        warn!("Skipping test, because it is not possible to run it in an external environment.");
        return;
    }

    let [alice, bob] = setup.add_users().await;
    let chat_id = setup.connect_users(&alice, &bob).await;

    let mut resource_exhausted = false;
    let mut held_id = None;

    // should stop with `resource_exhausted = true` at some point
    for i in 0..10 {
        // All requests from this host share one bucket. Cheap fetches empty it
        // much faster than it refills, so the send right after is limited.
        for _ in 0..1000 {
            if alice.qs_fetch_messages().await.is_err() {
                break;
            }
        }

        info!(i, "sending message");
        alice
            .send_message(
                chat_id,
                MimiContent::simple_markdown_message(format!("Hello bob {i}"), [0; 16]), // simple seed for testing
                None,
                MarkChatAsRead::Yes,
            )
            .await
            .unwrap();
        alice.outbound_service().run_once().await;

        let message = alice.expect_last_message(chat_id).await;

        // Due to the indirection of the outbound service, we can't just check
        // for errors while sending. Instead, we just check whether the message
        // was marked as sent.
        if message.is_sent() {
            info!(i, "message sent successfully");
            continue;
        }

        // A rate limited message stays queued and is not marked as failed.
        assert_ne!(message.status(), MessageStatus::Error);
        held_id = Some((message.id(), format!("Hello bob {i}")));
        resource_exhausted = true;
        break;
    }
    assert!(resource_exhausted);
    let (held_id, held_text) = held_id.unwrap();

    info!("waiting for rate limit tokens to replenish");
    // The outbound service backs off for a second after being rate limited.
    // One token per period, the held and the final message and Bob's fetch
    // need several.
    tokio::time::sleep(Duration::from_secs(1) + 5 * PERIOD).await;

    info!("sending message after rate limit tokens replenished");
    const FINAL_TEXT: &str = "after replenish";
    let message = alice.send_text(chat_id, FINAL_TEXT).await;
    assert!(message.is_sent());
    let held = alice.expect_message(held_id).await;
    assert!(held.is_sent());
    assert_ne!(held.status(), MessageStatus::Error);

    // Bob receives the held message before the later one
    bob.drain().await;
    let texts = bob.text_messages(chat_id).await;
    let held_pos = texts.iter().position(|t| *t == held_text).unwrap();
    let final_pos = texts.iter().position(|t| t == FINAL_TEXT).unwrap();
    assert!(held_pos < final_pos);
}
