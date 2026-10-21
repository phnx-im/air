// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Retracting, declining, deleting and blocking requests across devices.

use aircoreclient::{
    AcceptContactRequestError, ChatId, ChatStatus, ChatType, SystemMessage, clients::CoreUser,
};
use airserver_test_harness::utils::{core_user_ext::CoreUserExt, setup::TestBackend};

use super::has_system_message;
use crate::{
    contacts::requests::{add_second_username, pending_chats_from},
    multi_device::{assert_inactive, send_and_receive},
};

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn deleting_a_request_chat_before_forwarding_clears_the_sibling() {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    let alice2 = alice.add_synced_device().await;
    let first = setup.add_username(&alice).await;
    let second = add_second_username(&alice).await;
    alice.outbound_service().run_once().await;
    alice2.drain().await;

    for record in [&first, &second] {
        bob.add_contact(record.username.clone(), record.hash, setup.apq_groups)
            .await
            .unwrap()
            .unwrap();
    }

    let first_chat_id = alice.receive_connection_offer(&first).await;
    alice.outbound_service().run_once().await;
    alice2.drain().await;
    assert!(alice2.chat(&first_chat_id).await.is_some());

    // The newer request takes over the chat, which alice deletes before the
    // request reaches the sibling.
    let second_chat_id = alice.receive_connection_offer(&second).await;
    assert_ne!(second_chat_id, first_chat_id);
    let _ = alice.delete_chat(second_chat_id).await;
    alice.erase_chat(second_chat_id).await.unwrap();
    alice.outbound_service().run_once().await;

    alice2.drain().await;
    assert!(
        pending_chats_from(&alice2, bob.user_id()).await.is_empty(),
        "the sibling should drop the older request too"
    );
}

async fn assert_erased(device: &CoreUser, chat_id: ChatId, label: &str) {
    assert!(
        device.chat(&chat_id).await.is_none(),
        "the {label} device should erase the chat of the retracted request"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn retracting_a_request_makes_it_unavailable_to_the_recipient() {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    let alice2 = alice.add_synced_device().await;

    let bob_record = setup.add_username(&bob).await;
    let chat_id = alice
        .add_contact(
            bob_record.username.clone(),
            bob_record.hash,
            setup.apq_groups,
        )
        .await
        .unwrap()
        .unwrap();
    alice2.drain().await;
    let bob_chat_id = bob.receive_connection_offer(&bob_record).await;

    alice.retract_contact_request(chat_id).await.unwrap();
    assert_erased(&alice, chat_id, "retracting").await;
    // The siblings share the sender's leaf, so they get the delete commit.
    alice2.drain().await;
    assert_erased(&alice2, chat_id, "sibling").await;
    // The self-group deletion finds the chat gone already.
    alice.outbound_service().run_once().await;
    alice2.drain().await;
    assert!(
        alice2.retract_contact_request(chat_id).await.is_err(),
        "an erased request cannot be retracted again"
    );

    let accepted = bob.accept_contact_request(bob_chat_id).await.unwrap();
    assert!(
        matches!(accepted, Err(AcceptContactRequestError::Unavailable)),
        "a retracted request cannot be accepted, got {accepted:?}"
    );
    assert_inactive(&bob, bob_chat_id, None, "recipient").await;
    assert!(
        has_system_message(&bob, bob_chat_id, |message| matches!(
            message,
            SystemMessage::ConnectionRequestUnavailable
        ))
        .await,
        "the recipient should record that the request is gone"
    );

    // The retraction frees the username for a new request.
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

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn deleting_an_outgoing_request_retracts_it() {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    let bob_record = setup.add_username(&bob).await;

    let chat_id = alice
        .add_contact(
            bob_record.username.clone(),
            bob_record.hash,
            setup.apq_groups,
        )
        .await
        .unwrap()
        .unwrap();
    let bob_chat_id = bob.receive_connection_offer(&bob_record).await;

    alice.delete_chat(chat_id).await.unwrap();
    alice.erase_chat(chat_id).await.unwrap();

    let accepted = bob.accept_contact_request(bob_chat_id).await.unwrap();
    assert!(
        matches!(accepted, Err(AcceptContactRequestError::Unavailable)),
        "a deleted request cannot be accepted, got {accepted:?}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn retracting_offline_keeps_the_request() {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    let bob_record = setup.add_username(&bob).await;

    let chat_id = alice
        .add_contact(
            bob_record.username.clone(),
            bob_record.hash,
            setup.apq_groups,
        )
        .await
        .unwrap()
        .unwrap();
    let bob_chat_id = bob.receive_connection_offer(&bob_record).await;

    setup.listener_control_handle().set_drop_next_request();
    alice
        .retract_contact_request(chat_id)
        .await
        .expect_err("expected the retraction to fail due to a network error");
    let chat = alice.expect_chat(chat_id).await;
    assert_eq!(chat.status(), &ChatStatus::Active);
    assert!(
        alice
            .pending_chat_operation_info(chat_id)
            .await
            .unwrap()
            .is_none(),
        "no delete should stay parked"
    );

    alice.retract_contact_request(chat_id).await.unwrap();
    assert_erased(&alice, chat_id, "retracting").await;
    let accepted = bob.accept_contact_request(bob_chat_id).await.unwrap();
    assert!(
        matches!(accepted, Err(AcceptContactRequestError::Unavailable)),
        "a retracted request cannot be accepted, got {accepted:?}"
    );
}

/// Both devices retract the request before either learns about the other's
/// retraction. The second one finds the group gone.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn retracting_on_both_devices_erases_the_chat() {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    let alice2 = alice.add_synced_device().await;

    let bob_record = setup.add_username(&bob).await;
    let chat_id = alice
        .add_contact(
            bob_record.username.clone(),
            bob_record.hash,
            setup.apq_groups,
        )
        .await
        .unwrap()
        .unwrap();
    alice2.drain().await;
    alice.drain().await;

    alice.retract_contact_request(chat_id).await.unwrap();
    alice2.retract_contact_request(chat_id).await.unwrap();
    assert_erased(&alice, chat_id, "first").await;
    assert_erased(&alice2, chat_id, "late").await;

    // Both devices erased the group, so the delete commit queued for them has
    // nowhere to go.
    for device in [&alice, &alice2] {
        let processed = device.drain_unchecked().await;
        assert!(
            processed
                .errors
                .iter()
                .all(|error| error.to_string().contains("No chat found")),
            "{:?}",
            processed.errors
        );
    }

    for (device, sibling) in [(&alice, &alice2), (&alice2, &alice)] {
        device.outbound_service().run_once().await;
        sibling.drain().await;
    }
    assert_erased(&alice, chat_id, "first").await;
    assert_erased(&alice2, chat_id, "late").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn accepting_falls_back_to_the_older_request() {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    let alice2 = alice.add_synced_device().await;
    let first = setup.add_username(&alice).await;
    let second = add_second_username(&alice).await;
    alice.outbound_service().run_once().await;
    alice2.drain().await;

    let mut sent = Vec::new();
    for record in [&first, &second] {
        let chat_id = bob
            .add_contact(record.username.clone(), record.hash, setup.apq_groups)
            .await
            .unwrap()
            .unwrap();
        sent.push(chat_id);
    }
    let older_chat_id = alice.receive_connection_offer(&first).await;
    let newer_chat_id = alice.receive_connection_offer(&second).await;
    assert_eq!([older_chat_id, newer_chat_id], sent.as_slice());
    alice.outbound_service().run_once().await;
    alice2.drain().await;

    bob.retract_contact_request(newer_chat_id).await.unwrap();

    let accepted = alice
        .accept_contact_request(newer_chat_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        accepted, older_chat_id,
        "the older request should be accepted"
    );
    assert!(alice.chat(&newer_chat_id).await.is_none());
    let chat = alice.expect_chat(older_chat_id).await;
    assert_eq!(
        chat.chat_type(),
        &ChatType::Connection(bob.user_id().clone())
    );

    alice.outbound_service().run_once().await;
    alice2.drain().await;
    assert!(pending_chats_from(&alice2, bob.user_id()).await.is_empty());
    let chat_b = alice2.expect_chat(older_chat_id).await;
    assert_eq!(
        chat_b.chat_type(),
        &ChatType::Connection(bob.user_id().clone())
    );

    bob.drain().await;
    let bob_chat = bob.expect_chat(older_chat_id).await;
    assert_eq!(
        bob_chat.chat_type(),
        &ChatType::Connection(alice.user_id().clone())
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn declining_a_request_removes_it_on_the_sibling() {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    let alice2 = alice.add_synced_device().await;
    let record = setup.add_username(&alice).await;
    alice.outbound_service().run_once().await;
    alice2.drain().await;

    let bob_chat_id = bob
        .add_contact(record.username.clone(), record.hash, setup.apq_groups)
        .await
        .unwrap()
        .unwrap();
    let chat_id = alice.receive_connection_offer(&record).await;
    alice.outbound_service().run_once().await;
    alice2.drain().await;
    assert!(alice2.chat(&chat_id).await.is_some());

    alice2.decline_contact_request(chat_id).await.unwrap();
    assert!(alice2.chat(&chat_id).await.is_none());
    assert!(
        alice2.decline_contact_request(chat_id).await.is_err(),
        "a declined request cannot be declined again"
    );
    alice2.outbound_service().run_once().await;

    let processed = alice.drain().await;
    assert_eq!(
        processed.removed_chats,
        vec![chat_id],
        "the notification of the declined request is stale"
    );
    assert!(alice.chat(&chat_id).await.is_none());

    // The sender is not told.
    bob.drain().await;
    let bob_chat = bob.expect_chat(bob_chat_id).await;
    assert!(bob_chat.is_unconfirmed());
    assert_eq!(bob_chat.status(), &ChatStatus::Active);
}

/// The blocking device knows only the first request. The second one is still
/// waiting to be forwarded when the block arrives.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn blocking_a_request_sender_declines_on_the_sibling() {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    let alice2 = alice.add_synced_device().await;
    let first = setup.add_username(&alice).await;
    let second = add_second_username(&alice).await;
    alice.outbound_service().run_once().await;
    alice2.drain().await;

    for record in [&first, &second] {
        bob.add_contact(record.username.clone(), record.hash, setup.apq_groups)
            .await
            .unwrap()
            .unwrap();
    }
    alice.receive_connection_offer(&first).await;
    alice.outbound_service().run_once().await;
    alice2.drain().await;
    let second_chat_id = alice.receive_connection_offer(&second).await;

    alice2.block_contact(bob.user_id().clone()).await.unwrap();
    assert!(pending_chats_from(&alice2, bob.user_id()).await.is_empty());
    alice2.outbound_service().run_once().await;

    let processed = alice.drain().await;
    assert!(
        processed.removed_chats.contains(&second_chat_id),
        "the notification of the declined requests is stale, got {:?}",
        processed.removed_chats
    );
    assert!(
        pending_chats_from(&alice, bob.user_id()).await.is_empty(),
        "the device should decline the request the blocking sibling did not know"
    );
    alice.outbound_service().run_once().await;
    alice2.drain().await;
    assert!(pending_chats_from(&alice2, bob.user_id()).await.is_empty());
}

/// The sibling declines before it learns that the device accepted.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn a_decline_racing_an_accept_keeps_the_connection() {
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
    let chat_id = alice.receive_connection_offer(&record).await;
    alice.outbound_service().run_once().await;
    alice2.drain().await;

    alice
        .accept_contact_request(chat_id)
        .await
        .unwrap()
        .unwrap();
    alice2.decline_contact_request(chat_id).await.unwrap();
    alice2.outbound_service().run_once().await;

    alice.drain().await;
    assert_eq!(
        alice
            .chat(&chat_id)
            .await
            .map(|chat| chat.chat_type().clone()),
        Some(ChatType::Connection(bob.user_id().clone())),
        "the accepting device should keep the connection"
    );
    alice2.drain().await;
    assert_eq!(
        alice2
            .chat(&chat_id)
            .await
            .map(|chat| chat.chat_type().clone()),
        Some(ChatType::Connection(bob.user_id().clone())),
        "the declining sibling should follow the accept"
    );

    bob.drain().await;
    send_and_receive(&bob, &[&alice, &alice2], chat_id, "hello alice").await;
}
