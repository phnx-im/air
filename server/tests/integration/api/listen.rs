// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Listen stream: version status, eviction and durable acks.

use std::{assert_matches, time::Duration};

use airbackend::{settings::VersionExpiration, version::VersionPolicy};
use aircoreclient::clients::{ListenResponse, listen_response};
use airprotos::queue_service::v1::VersionStatus;
use airserver_test_harness::utils::{
    core_user_ext::CoreUserExt,
    setup::{TestBackend, TestBackendParams},
};
use chrono::Utc;
use semver::Version;
use tokio::time::timeout;
use tokio_stream::StreamExt;

use super::skip_version_status;

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn listen_queue_version_status() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;

    let (mut stream, _responder) = alice.listen_queue().await.unwrap();

    // The first event reports the version status, then the queue-empty sentinel follows.
    assert_matches!(
        stream.next().await,
        Some(Ok(ListenResponse {
            event: Some(listen_response::Event::VersionStatus(VersionStatus {
                expires_at: None,
                max_devices: 10,
            })),
        }))
    );
    assert_matches!(
        stream.next().await,
        Some(Ok(ListenResponse {
            event: Some(listen_response::Event::Empty(_)),
        }))
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn listen_queue_version_status_expiring() {
    let expires_on = Utc::now() + chrono::Duration::days(10);
    let mut setup = TestBackend::single_with_params(TestBackendParams {
        version_policy: VersionPolicy::new(vec![VersionExpiration {
            older_than: Version::new(999, 0, 0),
            expires_on,
        }]),
        ..Default::default()
    })
    .await;

    let alice = setup.add_user().await;

    let (mut stream, _responder) = alice.listen_queue().await.unwrap();

    // The first event reports the upcoming expiry, then the queue-empty sentinel follows.
    let response = stream.next().await.unwrap().unwrap();
    let Some(listen_response::Event::VersionStatus(status)) = response.event else {
        panic!("expected version status, got {response:?}");
    };
    assert_eq!(status.expires_at.unwrap().seconds, expires_on.timestamp());
    assert_matches!(
        stream.next().await,
        Some(Ok(ListenResponse {
            event: Some(listen_response::Event::Empty(_)),
        }))
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn listen_stream_eviction() {
    let mut setup = TestBackend::single_with_params(TestBackendParams {
        max_devices: Some(2),
        ..Default::default()
    })
    .await;
    let alice = setup.add_user().await;

    let username_record = setup.add_username(&alice).await;

    // Every device of the user listens to the same username, so username
    // message streams run side by side up to the device limit.
    let (mut stream_a, _responder_a) = alice.listen_username(&username_record).await.unwrap();
    assert_matches!(
        stream_a.next().await,
        Some(None),
        "should receive empty message"
    );

    let (mut stream_b, _responder_b) = alice.listen_username(&username_record).await.unwrap();
    assert_matches!(
        stream_b.next().await,
        Some(None),
        "should receive empty message"
    );

    for (label, stream) in [("first", &mut stream_a), ("second", &mut stream_b)] {
        assert!(
            timeout(Duration::from_millis(100), stream.next())
                .await
                .is_err(),
            "{label} stream is still open"
        );
    }

    // Beyond the device limit, the oldest username stream is evicted.
    let (mut stream_c, _responder_c) = alice.listen_username(&username_record).await.unwrap();
    assert_matches!(
        stream_c.next().await,
        Some(None),
        "should receive empty message"
    );

    assert!(
        timeout(Duration::from_millis(100), stream_a.next())
            .await
            .unwrap()
            .is_none(),
        "first stream is closed"
    );
    for (label, stream) in [("second", &mut stream_b), ("third", &mut stream_c)] {
        assert!(
            timeout(Duration::from_millis(100), stream.next())
                .await
                .is_err(),
            "{label} stream is still open"
        );
    }

    // QS events stream is evicted when another stream is opened
    let (mut stream_a, _responder_a) = alice.listen_queue().await.unwrap();
    skip_version_status(&mut stream_a).await;
    assert_matches!(
        stream_a.next().await,
        Some(Ok(ListenResponse {
            event: Some(listen_response::Event::Empty(_)),
        }))
    );

    let (mut stream_b, _responder_b) = alice.listen_queue().await.unwrap();
    skip_version_status(&mut stream_b).await;
    assert_matches!(
        stream_b.next().await,
        Some(Ok(ListenResponse {
            event: Some(listen_response::Event::Empty(_)),
        }))
    );

    let status = timeout(Duration::from_millis(100), stream_a.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap_err();
    assert_eq!(
        status.code(),
        tonic::Code::Aborted,
        "first stream is evicted"
    );
    assert!(
        timeout(Duration::from_millis(100), stream_a.next())
            .await
            .unwrap()
            .is_none(),
        "first stream is closed"
    );
    assert!(
        timeout(Duration::from_millis(100), stream_b.next())
            .await
            .is_err(),
        "second stream is still open"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn listen_stream_durable_acks() {
    let mut setup = TestBackend::single().await;

    let [alice, bob] = setup.add_users().await;
    let chat_id = setup.connect_users(&alice, &bob).await;

    alice.send_text(chat_id, "hello bob").await;

    // Bob receives the message on the listen stream and acks it.
    let (mut stream, responder) = bob.listen_queue().await.unwrap();
    skip_version_status(&mut stream).await;
    let sequence_number = match stream.next().await {
        Some(Ok(ListenResponse {
            event: Some(listen_response::Event::Message(message)),
        })) => message.sequence_number,
        event => panic!("expected a queue message, got {event:?}"),
    };
    responder.ack(sequence_number + 1).await;

    // Half-close the request stream. The server handles all requests sent
    // before, then closes the response stream with OK. Observing OK confirms
    // that the ack is durable.
    drop(responder);
    let terminal = timeout(Duration::from_secs(5), async {
        loop {
            match stream.next().await {
                Some(Ok(_)) => continue,
                Some(Err(status)) => break Some(status),
                None => break None,
            }
        }
    })
    .await
    .expect("stream closes after half-close");
    assert_matches!(terminal, None, "stream closes with OK");

    // On reconnect, the acked message is not redelivered.
    let (mut stream, _responder) = bob.listen_queue().await.unwrap();
    skip_version_status(&mut stream).await;
    assert_matches!(
        stream.next().await,
        Some(Ok(ListenResponse {
            event: Some(listen_response::Event::Empty(_)),
        })),
        "acked message is not redelivered"
    );
}
