// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Request expiry across devices.

use aircoreclient::{
    AcceptContactRequestError, ChatId, DeleteExpiredContactRequestsSetting, SystemMessage,
    UsernameRecord, clients::CoreUser,
};
use airserver_test_harness::utils::{core_user_ext::CoreUserExt, setup::TestBackend};
use chrono::{Duration, Utc};
use tokio::time::timeout;
use tokio_stream::StreamExt;

use super::request_expires_at;
use crate::{contacts::requests::pending_chats_from, multi_device::assert_inactive};

/// More than the lifetime of a connection request.
const PAST_THE_DEADLINE: Duration = Duration::days(31);

/// Sends a request from `device_a` to `bob_record` and waits until `device_b`
/// shows it too. Returns the chat of the request.
async fn send_mirrored_request(
    setup: &TestBackend,
    device_a: &CoreUser,
    device_b: &CoreUser,
    bob_record: &UsernameRecord,
) -> ChatId {
    let chat_id = device_a
        .add_contact(
            bob_record.username.clone(),
            bob_record.hash,
            setup.apq_groups,
        )
        .await
        .unwrap()
        .unwrap();
    device_b.drain().await;
    device_a.drain().await;
    let expires_at = request_expires_at(device_a, chat_id).await;
    assert!(expires_at.is_some());
    assert_eq!(
        request_expires_at(device_b, chat_id).await,
        expires_at,
        "the request should expire at the same time on both devices"
    );
    chat_id
}

/// Runs the expiry of connection requests on `device` now.
async fn expire_requests(device: &CoreUser) {
    device
        .outbound_service()
        .schedule_connection_request_expiry(Utc::now())
        .await
        .unwrap();
    device.outbound_service().run_once().await;
}

/// Asserts that the chat is closed with a single record of its sent request
/// expiring, as its last message.
async fn assert_sent_request_expired(device: &CoreUser, chat_id: ChatId, label: &str) {
    assert_inactive(device, chat_id, None, label).await;
    let messages = device.system_messages(chat_id).await;
    let records = messages
        .iter()
        .filter(|message| matches!(message, SystemMessage::SentConnectionRequestExpired))
        .count();
    assert_eq!(
        records, 1,
        "the {label} device should record the expiry once"
    );
    assert_eq!(
        messages.last(),
        Some(&SystemMessage::SentConnectionRequestExpired),
        "the expiry record should be the last message on the {label} device"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn expired_request_closes_on_sibling_and_is_unavailable() {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    let alice2 = alice.add_synced_device().await;
    let bob_record = setup.add_username(&bob).await;
    let chat_id = send_mirrored_request(&setup, &alice, &alice2, &bob_record).await;
    let bob_chat_id = bob.receive_connection_offer(&bob_record).await;

    for device in [&alice, &alice2] {
        device
            .backdate_contact_request(chat_id, PAST_THE_DEADLINE)
            .await
            .unwrap();
    }
    expire_requests(&alice).await;
    assert_sent_request_expired(&alice, chat_id, "sender").await;

    // The next run deletes the group on the DS. The sibling shares the leaf,
    // so it gets the delete commit, and keeps the chat as a record.
    alice.outbound_service().run_once().await;
    assert_sent_request_expired(&alice, chat_id, "sender").await;
    alice2.drain().await;
    assert_sent_request_expired(&alice2, chat_id, "sibling").await;

    let accepted = bob.accept_contact_request(bob_chat_id).await.unwrap();
    assert!(
        matches!(accepted, Err(AcceptContactRequestError::Unavailable)),
        "an expired request cannot be accepted, got {accepted:?}"
    );

    // The expiry frees the username for a new request.
    let new_chat_id = alice
        .add_contact(
            bob_record.username.clone(),
            bob_record.hash,
            setup.apq_groups,
        )
        .await
        .unwrap();
    assert!(new_chat_id.is_ok(), "got {new_chat_id:?}");
}

/// The DS echoes the delete of the expired request's group before its response
/// arrives.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn echoed_expiry_delete_keeps_record() {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    let bob_record = setup.add_username(&bob).await;
    // Only a plain MLS echo can be fed back to the sender.
    let chat_id = alice
        .add_contact(bob_record.username.clone(), bob_record.hash, false)
        .await
        .unwrap()
        .unwrap();
    alice
        .backdate_contact_request(chat_id, PAST_THE_DEADLINE)
        .await
        .unwrap();
    expire_requests(&alice).await;

    let echo = alice.pending_commit_message_bytes(chat_id).await.unwrap();
    alice.process_incoming_mls_message(&echo).await.unwrap();

    assert!(
        alice
            .pending_chat_operation_info(chat_id)
            .await
            .unwrap()
            .is_none(),
        "the echo should complete the parked delete"
    );
    assert_sent_request_expired(&alice, chat_id, "sender").await;
}

/// The device holding the username comes online after the deadline of a
/// request sent to it.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn request_fetched_past_deadline_is_dropped() {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    let alice2 = alice.add_synced_device().await;
    let record = setup.add_username(&alice).await;
    alice.outbound_service().run_once().await;
    alice2.drain().await;
    bob.add_contact(record.username.clone(), record.hash, setup.apq_groups)
        .await
        .unwrap()
        .unwrap();

    let (mut stream, responder) = alice.listen_username(&record).await.unwrap();
    let mut message = timeout(std::time::Duration::from_secs(5), stream.next())
        .await
        .unwrap()
        .flatten()
        .expect("the offer should be queued");
    let created_at = message
        .created_at
        .as_mut()
        .expect("the server should stamp the offer");
    created_at.seconds -= PAST_THE_DEADLINE.num_seconds();
    let message_id = message.message_id.unwrap();
    let stored = alice
        .process_username_queue_message(record.username.clone(), message)
        .await
        .unwrap();
    responder.ack(message_id.into()).await;
    assert!(stored.is_none(), "an expired offer should not be stored");
    assert!(pending_chats_from(&alice, bob.user_id()).await.is_empty());

    alice.outbound_service().run_once().await;
    alice2.drain().await;
    assert!(
        pending_chats_from(&alice2, bob.user_id()).await.is_empty(),
        "an expired offer should not be forwarded"
    );
}

/// Both devices are online when the request expires.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn request_expiring_on_both_devices_keeps_one_record() {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    let alice2 = alice.add_synced_device().await;
    let bob_record = setup.add_username(&bob).await;
    let chat_id = send_mirrored_request(&setup, &alice, &alice2, &bob_record).await;

    for device in [&alice, &alice2] {
        device
            .backdate_contact_request(chat_id, PAST_THE_DEADLINE)
            .await
            .unwrap();
        expire_requests(device).await;
    }
    // The first delete wins. The sibling gets its commit while its own delete
    // is still parked.
    alice.outbound_service().run_once().await;
    alice2.drain().await;
    assert_sent_request_expired(&alice2, chat_id, "sibling").await;

    // The parked delete finds the group gone.
    alice2.outbound_service().run_once().await;
    alice.drain().await;
    assert_sent_request_expired(&alice, chat_id, "first").await;
    assert_sent_request_expired(&alice2, chat_id, "sibling").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn expired_request_is_deleted_when_setting_on() {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    let alice2 = alice.add_synced_device().await;
    alice
        .set_synced_user_setting(&DeleteExpiredContactRequestsSetting(true))
        .await
        .unwrap();
    alice.outbound_service().run_once().await;
    alice2.drain().await;
    let bob_record = setup.add_username(&bob).await;
    let chat_id = send_mirrored_request(&setup, &alice, &alice2, &bob_record).await;

    for device in [&alice, &alice2] {
        device
            .backdate_contact_request(chat_id, PAST_THE_DEADLINE)
            .await
            .unwrap();
    }
    // The record stays while the group delete is parked, since erasing the
    // chat would drop it.
    expire_requests(&alice).await;
    assert_sent_request_expired(&alice, chat_id, "sender").await;
    alice.outbound_service().run_once().await;
    expire_requests(&alice).await;
    assert!(
        alice.chat(&chat_id).await.is_none(),
        "the sender should delete the record once the group is gone"
    );

    alice2.drain().await;
    expire_requests(&alice2).await;
    assert!(
        alice2.chat(&chat_id).await.is_none(),
        "the sibling should delete the record too"
    );
}
