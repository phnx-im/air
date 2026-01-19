// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{collections::HashSet, slice, time::Duration};

use airapiclient::{ApiClient, as_api::AsRequestError, qs_api::QsRequestError};
use airbackend::settings::RateLimitsSettings;
use aircommon::{
    assert_matches,
    credentials::keys::HandleSigningKey,
    identifiers::{QsClientId, UserHandle, UserId},
    mls_group_config::MAX_PAST_EPOCHS,
};
use aircoreclient::{
    ChatId,
    clients::{
        QueueEvent,
        process::process_qs::{ProcessedQsMessages, QsNotificationProcessor, QsStreamProcessor},
        queue_event,
    },
    outbound_service::KEY_PACKAGES,
    store::Store,
};

use airprotos::{
    auth_service::v1::auth_service_server,
    common::v1::{StatusDetails, StatusDetailsCode},
    delivery_service::v1::delivery_service_server,
    queue_service::v1::queue_service_server,
};
use airserver_test_harness::utils::setup::{TestBackend, TestBackendParams, TestUser};
use chrono::Utc;
use mimi_content::MimiContent;
use rand::thread_rng;
use semver::VersionReq;
use tokio::{
    task::JoinSet,
    time::{sleep, timeout},
};
use tokio_stream::StreamExt;
use tonic::transport::Channel;
use tonic_health::pb::{
    HealthCheckRequest, health_check_response::ServingStatus, health_client::HealthClient,
};
use tracing::{info, warn};

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[tracing::instrument(name = "Rate limit test", skip_all)]
async fn rate_limit() {
    let mut setup = TestBackend::single_with_params(TestBackendParams {
        rate_limits: Some(RateLimitsSettings {
            period: Duration::from_secs(1), // replenish one token every 500ms
            burst: 30,                      // allow total 30 request
        }),
        ..Default::default()
    })
    .await;

    if setup.is_external() {
        warn!("Skipping test, because it is not possible to run it in an external environment.");
        return;
    }

    let alice = setup.add_user().await;
    let bob = setup.add_user().await;
    let chat_id = setup.connect_users(&alice, &bob).await;

    let alice = &setup.get_user(&alice);

    let mut resource_exhausted = false;

    // should stop with `resource_exhausted = true` at some point
    for i in 0..100 {
        info!(i, "sending message");
        alice
            .user
            .send_message(
                chat_id,
                MimiContent::simple_markdown_message("Hello bob".into(), [0; 16]), // simple seed for testing
                None,
            )
            .await
            .unwrap();
        alice.user.outbound_service().run_once().await;

        let message = alice.user.last_message(chat_id).await.unwrap().unwrap();

        // Due to the indirection of the outbound service, we can't just check
        // for errors while sending. Instead, we just check whether the message
        // was marked as sent.
        if message.is_sent() {
            info!(i, "message sent successfully");
            continue;
        }

        resource_exhausted = true;
        break;
    }
    assert!(resource_exhausted);

    info!("waiting for rate limit tokens to replenish");
    tokio::time::sleep(Duration::from_secs(1)).await; // replenish

    info!("sending message after rate limit tokens replenished");
    alice
        .user
        .send_message(
            chat_id,
            MimiContent::simple_markdown_message("Hello bob".into(), [0; 16]), // simple seed for testing
            None,
        )
        .await
        .unwrap();
    alice.user.outbound_service().run_once().await;

    let message = alice.user.last_message(chat_id).await.unwrap().unwrap();

    assert!(message.is_sent());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[tracing::instrument(name = "User deletion triggers", skip_all)]
async fn user_deletion_triggers() {
    let mut setup = TestBackend::single().await;
    // Create alice and bob
    let alice = setup.add_user().await;
    let bob = setup.add_user().await;
    let charlie = setup.add_user().await;

    // Connect alice and bob
    setup.connect_users(&alice, &bob).await;
    // Connect alice and charlie
    setup.connect_users(&alice, &charlie).await;

    // Note that bob and charlie are not connected.

    // Alice creates a group and invites bob and charlie
    let chat_id = setup.create_group(&alice).await;
    setup
        .invite_to_group(chat_id, &alice, vec![&bob, &charlie])
        .await;

    // Bob should have a user profile for charlie now, even though they
    // are not connected.
    let bob_user = &setup.get_user(&bob).user;
    let bob_user_profile_charlie = bob_user.user_profile(&charlie).await;
    assert!(bob_user_profile_charlie.user_id == charlie);

    // Now charlie leaves the group
    setup.leave_group(chat_id, &charlie).await.unwrap();
    // Bob should not have a user profile for charlie anymore.

    let bob = setup.get_user(&bob);
    let bob_user_profile_charlie = bob.user.user_profile(&charlie).await;
    assert_eq!(
        bob_user_profile_charlie,
        aircoreclient::UserProfile::from_user_id(&charlie)
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[tracing::instrument(name = "Health check test", skip_all)]
async fn health_check() {
    let setup = TestBackend::single().await;
    let channel = Channel::from_shared(setup.server_url().to_string())
        .unwrap()
        .connect()
        .await
        .unwrap();
    let mut client = HealthClient::new(channel);

    let names = [
        auth_service_server::SERVICE_NAME,
        delivery_service_server::SERVICE_NAME,
        queue_service_server::SERVICE_NAME,
    ];

    for name in names {
        let response = client
            .check(HealthCheckRequest {
                service: name.to_string(),
            })
            .await;
        if let Err(error) = response {
            panic!("Health check failed for service {name}: {error}");
        }
        let response = response.unwrap().into_inner();
        assert_eq!(
            ServingStatus::try_from(response.status).unwrap(),
            ServingStatus::Serving
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[tracing::instrument(name = "Max past epochs", skip_all)]
async fn max_past_epochs() {
    let mut setup = TestBackend::single().await;

    let alice = setup.add_user().await;
    setup.get_user_mut(&alice).add_user_handle().await.unwrap();

    let bob = setup.add_user().await;

    let contact_chat_id = setup.connect_users(&alice, &bob).await;

    // To test proper handling of application messages from past epochs, we have
    // Alice locally create updates without sending them to the server. Bob can then
    // send messages based on his (old) epoch for Alice to process.

    // Create MAX_PAST_EPOCHS updates and send a message from Bob to Alice after
    // each update.
    for _ in 0..MAX_PAST_EPOCHS {
        let result = update_and_send_message(&mut setup, contact_chat_id, &alice, &bob).await;
        assert!(
            result.errors.is_empty(),
            "Alice should process Bob's message without errors"
        );
    }

    // Repeat one more time, this time we expect an error
    let result = update_and_send_message(&mut setup, contact_chat_id, &alice, &bob).await;
    let error = &result.errors[0].to_string();
    assert_eq!(
        error.to_string(),
        "Could not process message: ValidationError(UnableToDecrypt(SecretTreeError(TooDistantInThePast)))".to_string(),
        "Alice should fail to process Bob's message with a TooDistantInThePast error"
    );
}

async fn update_and_send_message(
    setup: &mut TestBackend,
    contact_chat_id: ChatId,
    alice: &UserId,
    bob: &UserId,
) -> ProcessedQsMessages {
    let alice_user = &setup.get_user(alice).user;
    // alice creates an update and sends it to the ds
    alice_user.update_key(contact_chat_id).await.unwrap();
    // bob creates a message based on his (old) epoch for alice
    let bob_user = &setup.get_user(bob).user;
    let msg = MimiContent::simple_markdown_message("message".to_owned(), [0; 16]);
    bob_user
        .send_message(contact_chat_id, msg, None)
        .await
        .unwrap();
    bob_user.outbound_service().run_once().await;
    // alice fetches and processes bob's message
    let alice_user = &setup.get_user(alice).user;
    let qs_messages = alice_user.qs_fetch_messages().await.unwrap();
    alice_user.fully_process_qs_messages(qs_messages).await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[tracing::instrument(name = "Ratchet tolerance", skip_all)]
async fn ratchet_tolerance() {
    let mut setup = TestBackend::single().await;

    let alice = setup.add_user().await;
    setup.get_user_mut(&alice).add_user_handle().await.unwrap();

    let bob = setup.add_user().await;

    let contact_chat_id = setup.connect_users(&alice, &bob).await;

    // To test the tolerance of the ratchet, we have Alice send a bunch of
    // messages and then give Bob only the last one to process.
    let alice_user = &setup.get_user(&alice).user;
    for _ in 0..5 {
        let msg = MimiContent::simple_markdown_message("message".to_owned(), [0; 16]);
        alice_user
            .send_message(contact_chat_id, msg, None)
            .await
            .unwrap();
    }
    alice_user.outbound_service().run_once().await;

    let bob_user = &setup.get_user(&bob).user;
    let mut qs_messages = bob_user.qs_fetch_messages().await.unwrap();
    // Give Bob only the last message to process
    let last_message = qs_messages.pop().unwrap();
    let result = bob_user.fully_process_qs_messages(vec![last_message]).await;
    assert!(
        result.errors.is_empty(),
        "Bob should process Alice's last message without errors"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[tracing::instrument(name = "Client sequence number race", skip_all)]
async fn client_sequence_number_race() {
    let mut setup = TestBackend::single().await;

    let alice = setup.add_user().await;
    setup.get_user_mut(&alice).add_user_handle().await.unwrap();

    let bob = setup.add_user().await;

    let chat_id = setup.connect_users(&alice, &bob).await;

    info!("Alice sending messages to queue");

    let alice = setup.get_user(&alice);

    const NUM_SENDERS: usize = 5;
    const NUM_MESSAGES: usize = 10;
    let alice_user = alice.user.clone();
    for _ in 0..NUM_SENDERS {
        let alice_user = alice_user.clone();
        tokio::spawn(async move {
            for _ in 0..NUM_MESSAGES {
                const SALT: [u8; 16] = [0; 16];
                let message = MimiContent::simple_markdown_message("Hello bob".into(), SALT);
                alice_user
                    .send_message(chat_id, message, None)
                    .await
                    .unwrap();
                alice_user.outbound_service().run_once().await;
            }
        });
    }

    info!("Bob getting messages from queue");

    const NUM_CLIENTS: usize = 2;
    let mut join_set = JoinSet::new();

    let bob_user = setup.get_user(&bob).user.clone();
    let (processed, processed_rx) = tokio::sync::watch::channel(0);

    for _ in 0..NUM_CLIENTS {
        let bob_user = bob_user.clone();
        let processed = processed.clone();
        let mut processed_rx = processed_rx.clone();
        join_set.spawn(async move {
            loop {
                if *processed.borrow() == NUM_SENDERS * NUM_MESSAGES {
                    break;
                }

                let Ok((mut stream, responder)) = bob_user.listen_queue().await else {
                    continue;
                };

                let mut handler = QsStreamProcessor::with_responder(bob_user.clone(), responder);

                loop {
                    let finished =
                        processed_rx.wait_for(|processed| *processed == NUM_SENDERS * NUM_MESSAGES);
                    let event = tokio::select! {
                        _ = finished => break,
                        event = stream.next() => event
                    };
                    let Some(event) = event else {
                        break;
                    };

                    let result = handler
                        .process_event(event, &mut NoopNotificationProcessor)
                        .await;

                    processed.send_modify(|processed| {
                        *processed += result.processed();
                    });
                    if result.is_partially_processed() {
                        break; // stop the stream when only partially processed
                    }
                }
            }
        });
    }
    join_set.join_all().await; // panics on error

    assert_eq!(*processed.borrow(), NUM_SENDERS * NUM_MESSAGES);
}

struct NoopNotificationProcessor;

impl QsNotificationProcessor for NoopNotificationProcessor {
    async fn show_notifications(&mut self, _: ProcessedQsMessages) {}
}

// TODO: Re-enable once we have implemented a resync UX.
//#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[allow(dead_code)]
#[tracing::instrument(name = "Resync", skip_all)]
async fn resync() {
    let mut setup = TestBackend::single().await;

    let alice = setup.add_user().await;
    setup.get_user_mut(&alice).add_user_handle().await.unwrap();

    let bob = setup.add_user().await;
    let charlie = setup.add_user().await;

    setup.connect_users(&alice, &bob).await;
    setup.connect_users(&alice, &charlie).await;

    let chat_id = setup.create_group(&alice).await;
    setup.invite_to_group(chat_id, &alice, vec![&bob]).await;

    // To trigger resync, we have Alice add Charlie to the group and Bob
    // fetching, but not processing the commit.
    let alice_user = &setup.get_user(&alice).user;

    // Alice creates a invites charlie and sends the commit to the DS
    alice_user
        .invite_users(chat_id, slice::from_ref(&charlie))
        .await
        .unwrap();

    // Bob fetches the invite and acks it s.t. it's removed from the queue,
    // but does not process it. This is to simulate Bob missing the commit.
    let bob_user = &setup.get_user(&bob).user;
    let qs_messages = bob_user.qs_fetch_messages().await.unwrap();
    let [message] = qs_messages.as_slice() else {
        panic!("Bob should have one message in the queue");
    };
    let (stream, responder) = bob_user.listen_queue().await.unwrap();
    responder.ack(message.sequence_number + 1).await;
    sleep(Duration::from_secs(1)).await;
    drop(stream);

    // Alice performs an update, which bob fetches and processes, triggering a
    // resync.
    let alice_user = &setup.get_user(&alice).user;
    alice_user.update_key(chat_id).await.unwrap();

    // Bob fetches and processes the update
    let bob_user = &setup.get_user(&bob).user;
    let qs_messages = bob_user.qs_fetch_messages().await.unwrap();
    let result = bob_user.fully_process_qs_messages(qs_messages).await;
    // Run outbound service to complete the rejoin process
    bob_user.outbound_service().run_once().await;
    // Instead of throwing an error, Bob should have re-synced as part of processing the update.
    assert!(
        result.errors.is_empty(),
        "Bob should process Alice's update and message without errors"
    );

    let alice_user = &setup.get_user(&alice).user;

    // Alice processes Bob's rejoin
    let qs_messages = alice_user.qs_fetch_messages().await.unwrap();
    let result = alice_user.fully_process_qs_messages(qs_messages).await;

    assert!(
        result.errors.is_empty(),
        "Alice should process Bob's rejoin without errors"
    );

    // Bob should have rejoined the group and should be able to send a message.
    setup
        .send_message(chat_id, &bob, vec![&alice, &charlie])
        .await;

    let alice_user = &setup.get_user(&alice).user;

    // When Alice sends another message, Bob should be able to process it without errors.
    alice_user
        .send_message(
            chat_id,
            MimiContent::simple_markdown_message("message".to_owned(), [0; 16]),
            None,
        )
        .await
        .unwrap();

    let bob_user = &setup.get_user(&bob).user;
    let qs_messages = bob_user.qs_fetch_messages().await.unwrap();
    let result = bob_user.fully_process_qs_messages(qs_messages).await;
    assert!(
        result.errors.is_empty(),
        "Bob should process Alice's message without errors"
    );

    // Now Alice leaves the group, which means that if Bob resyncs again, he
    // should commit the SelfRemove proposal in the process.
    let alice_user = &setup.get_user(&alice).user;

    // Alice sends an update, which Bob misses again.
    alice_user.update_key(chat_id).await.unwrap();

    // Bob fetches the update and acks it s.t. it's removed from the queue,
    // but does not process it. This is to simulate Bob missing the commit.
    let bob_user = &setup.get_user(&bob).user;
    let qs_messages = bob_user.qs_fetch_messages().await.unwrap();
    let [message] = qs_messages.as_slice() else {
        panic!("Bob should have one message in the queue");
    };
    let (stream, responder) = bob_user.listen_queue().await.unwrap();
    responder.ack(message.sequence_number + 1).await;
    sleep(Duration::from_secs(1)).await;
    drop(stream);

    // Now Alice leaves the group, which means that if Bob resyncs again, he
    // should commit the SelfRemove proposal in the process.
    let alice_user = &setup.get_user(&alice).user;
    alice_user.leave_chat(chat_id).await.unwrap();

    // Bob fetches and processes his messages, which should trigger a resync.
    let bob_user = &setup.get_user(&bob).user;

    // Alice is still part of the group.
    let participants = bob_user.group_members(chat_id).await.unwrap();
    assert_eq!(
        participants,
        [alice.clone(), bob.clone(), charlie.clone()]
            .into_iter()
            .collect()
    );

    let qs_messages = bob_user.qs_fetch_messages().await.unwrap();
    let result = bob_user.fully_process_qs_messages(qs_messages).await;
    bob_user.outbound_service().run_once().await;
    // Instead of throwing an error, Bob should have re-synced as part of processing the update.
    assert!(
        result.errors.is_empty(),
        "Bob should process Alice's update without errors"
    );

    // Alice not in the group anymore.
    let participants = bob_user.group_members(chat_id).await.unwrap();
    assert_eq!(
        participants,
        [bob.clone(), charlie.clone()].into_iter().collect()
    );

    // Messages should reach Charlie.
    setup.send_message(chat_id, &bob, vec![&charlie]).await;

    // Charlie should also only see Bob in the group.
    let charlie_user = &setup.get_user(&charlie).user;

    let participants = charlie_user.group_members(chat_id).await.unwrap();
    assert_eq!(
        participants,
        [bob.clone(), charlie.clone()].into_iter().collect()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[tracing::instrument(name = "Key Package Upload", skip_all)]
async fn key_package_upload() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let bob = setup.add_user().await;
    setup.connect_users(&alice, &bob).await;
    // Exhaust Bob's key packages
    // We collect Bob's encryption keys. They should be unique every time.
    let mut encryption_keys = HashSet::new();

    let create_chat_and_invite_bob = async |setup: &mut TestBackend| {
        let chat_id = setup.create_group(&alice).await;
        let alice_user = &setup.get_user(&alice).user;
        alice_user
            .invite_users(chat_id, slice::from_ref(&bob))
            .await
            .unwrap();
        let bob_user = &setup.get_user(&bob).user;
        let messages = bob_user.qs_fetch_messages().await.unwrap();
        let res = bob_user.fully_process_qs_messages(messages).await;
        assert!(
            res.errors.is_empty(),
            "Bob should process Alice's invitation without errors"
        );
        bob_user
            .mls_members(chat_id)
            .await
            .unwrap()
            .unwrap()
            .into_iter()
            .find(|m| m.index.usize() == 1)
            .unwrap()
            .encryption_key
    };

    for _ in 0..(KEY_PACKAGES + 1) {
        let bob_encryption_key = create_chat_and_invite_bob(&mut setup).await;
        assert!(encryption_keys.insert(bob_encryption_key));
    }

    let bob_encryption_key = create_chat_and_invite_bob(&mut setup).await;
    assert!(
        !encryption_keys.insert(bob_encryption_key),
        "Alice should have reused Bob's last resort KeyPackage"
    );

    // Bob uploads new KeyPackages
    let bob_user = &setup.get_user(&bob).user;
    let now = Utc::now();
    bob_user
        .schedule_key_package_upload(now - chrono::Duration::minutes(5))
        .await
        .unwrap();
    bob_user.outbound_service().run_once().await;

    // Invite Bob again, should get a new KeyPackage
    let bob_encryption_key = create_chat_and_invite_bob(&mut setup).await;
    assert!(
        encryption_keys.insert(bob_encryption_key),
        "Bob should have a new KeyPackage after uploading new ones"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[tracing::instrument(name = "Invitation code", skip_all)]
async fn invitation_code() {
    const UNREDEEMABLE_CODE: &str = "E111E000";
    let setup = TestBackend::single_with_params(TestBackendParams {
        invitation_only: true,
        unredeemable_code: Some(UNREDEEMABLE_CODE.to_owned()),
        ..Default::default()
    })
    .await;

    // working code
    let user_id = UserId::random(setup.domain().clone());
    let code = setup.invitation_codes().first().unwrap();
    assert!(
        TestUser::try_new(&user_id, setup.server_url().clone(), code)
            .await
            .is_ok()
    );

    // code used twice
    let user_id = UserId::random(setup.domain().clone());
    let code = setup.invitation_codes().first().unwrap();
    let error = TestUser::try_new(&user_id, setup.server_url().clone(), code)
        .await
        .unwrap_err();
    let error = error.downcast::<AsRequestError>().unwrap();
    assert_matches!(error, AsRequestError::Tonic(status)
        if status.code() == tonic::Code::InvalidArgument
    );

    // not working code
    let user_id = UserId::random(setup.domain().clone());
    let code = "DUMMY007";
    let error = TestUser::try_new(&user_id, setup.server_url().clone(), code)
        .await
        .unwrap_err();
    let error = error.downcast::<AsRequestError>().unwrap();
    assert_matches!(error, AsRequestError::Tonic(status)
        if status.code() == tonic::Code::InvalidArgument
    );

    // unredeemable code (first use)
    let user_id = UserId::random(setup.domain().clone());
    assert!(
        TestUser::try_new(&user_id, setup.server_url().clone(), UNREDEEMABLE_CODE)
            .await
            .is_ok()
    );

    // unredeemable code (second use)
    let user_id = UserId::random(setup.domain().clone());
    assert!(
        TestUser::try_new(&user_id, setup.server_url().clone(), UNREDEEMABLE_CODE)
            .await
            .is_ok()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[tracing::instrument(
    name = "Unsupported client version on listen handle and queue",
    skip_all
)]
async fn unsupported_client_version() {
    let setup = TestBackend::single_with_params(TestBackendParams {
        client_version_req: Some(VersionReq::parse("^0.1.0").unwrap()),
        ..Default::default()
    })
    .await;

    let client = ApiClient::with_endpoint(&setup.server_url()).unwrap();

    let handle = UserHandle::new("test-handle".to_string()).unwrap();
    let signing_key = HandleSigningKey::generate().unwrap();
    let hash = handle.calculate_hash().unwrap();

    let res = client.as_listen_handle(hash, &signing_key).await;
    let status = match res {
        Err(AsRequestError::Tonic(status)) => status,
        Err(error) => panic!("Unexpected error type: {error:?}"),
        Ok(_) => panic!("Expected error"),
    };
    assert_eq!(status.code(), tonic::Code::FailedPrecondition);

    let details = StatusDetails::from_status(&status).unwrap();
    assert_matches!(details.code(), StatusDetailsCode::VersionUnsupported);

    let client_id = QsClientId::random(&mut thread_rng());
    let res = client.qs_listen_queue(client_id, 0).await;
    match res {
        Err(QsRequestError::Tonic(status)) => status,
        Err(error) => panic!("Unexpected error type: {error:?}"),
        Ok(_) => panic!("Expected error"),
    };
    assert_eq!(status.code(), tonic::Code::FailedPrecondition);

    let details = StatusDetails::from_status(&status).unwrap();
    assert_matches!(details.code(), StatusDetailsCode::VersionUnsupported);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[tracing::instrument(name = "Listen stream eviction", skip_all)]
async fn listen_stream_eviction() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;

    let alice_test_user = setup.get_user_mut(&alice);
    let handle_record = alice_test_user.add_user_handle().await.unwrap();

    let alice_user = alice_test_user.user.clone();

    // Handle messages stream is evicted when another stream is opened
    let (mut stream_a, _responder_a) = alice_user.listen_handle(&handle_record).await.unwrap();
    assert_matches!(
        stream_a.next().await,
        Some(None),
        "should receive empty message"
    );

    let (mut stream_b, _responder_b) = alice_user.listen_handle(&handle_record).await.unwrap();
    assert_matches!(
        stream_b.next().await,
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
    assert!(
        timeout(Duration::from_millis(100), stream_b.next())
            .await
            .is_err(),
        "second stream is still open"
    );

    // QS events stream is evicted when another stream is opened
    let (mut stream_a, _responder_a) = alice_user.listen_queue().await.unwrap();
    assert_matches!(
        stream_a.next().await,
        Some(QueueEvent {
            event: Some(queue_event::Event::Empty(_)),
        })
    );

    let (mut stream_b, _responder_b) = alice_user.listen_queue().await.unwrap();
    assert_matches!(
        stream_b.next().await,
        Some(QueueEvent {
            event: Some(queue_event::Event::Empty(_)),
        })
    );

    assert!(
        timeout(Duration::from_millis(100), stream_a.next())
            .await
            .unwrap()
            .is_none(),
        "first stream is not closed"
    );
    assert!(
        timeout(Duration::from_millis(100), stream_b.next())
            .await
            .is_err(),
        "second stream is closed"
    );
}
