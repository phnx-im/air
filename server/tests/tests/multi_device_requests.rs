// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Connection requests across the devices of a user.

use aircoreclient::{
    AcceptContactRequestError, ChatId, ChatStatus, ChatType, DeleteExpiredContactRequestsSetting,
    DisplayName, SystemMessage, UserProfile, UsernameRecord,
    clients::{
        CoreUser,
        process::qs_stream::{QsProcessEventResult, QsStreamProcessor},
    },
};
use airserver_test_harness::utils::{core_user_ext::CoreUserExt, setup::TestBackend};
use chrono::{DateTime, Duration, Utc};
use tokio::time::timeout;
use tokio_stream::StreamExt;

use super::{
    contact_requests::{add_second_username, pending_chats_from},
    multi_device::{assert_inactive, send_and_receive},
};

/// Whether the chat holds a system message matching `predicate`.
async fn has_system_message(
    device: &CoreUser,
    chat_id: ChatId,
    predicate: impl Fn(&SystemMessage) -> bool,
) -> bool {
    device.system_messages(chat_id).await.iter().any(predicate)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn request_fetched_by_one_device_reaches_its_sibling() {
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

    // Only the device that holds the username fetches the offer.
    let chat_id = alice.receive_connection_offer(&record).await;
    assert_eq!(chat_id, bob_chat_id);
    let package_hashes = alice
        .connection_package_hashes(&record.username)
        .await
        .unwrap();
    alice.outbound_service().run_once().await;

    let processed = alice2.drain().await;
    assert_eq!(
        processed.new_connections,
        vec![chat_id],
        "the forwarded request should raise a notification"
    );
    let chat_b = alice2.expect_chat(chat_id).await;
    assert_eq!(
        chat_b.chat_type(),
        &ChatType::PendingConnection(bob.user_id().clone())
    );
    assert!(
        has_system_message(&alice2, chat_id, |message| matches!(
            message,
            SystemMessage::ReceivedHandleConnectionRequest { sender, user_handle }
                if sender == bob.user_id() && user_handle == &record.username
        ))
        .await,
        "the forwarded request should be announced like a received one"
    );

    // The device that never saw the offer accepts it.
    alice2
        .accept_contact_request(chat_id)
        .await
        .unwrap()
        .unwrap();

    alice.drain().await;
    let chat_a = alice.expect_chat(chat_id).await;
    assert_eq!(
        chat_a.chat_type(),
        &ChatType::Connection(bob.user_id().clone())
    );
    let remaining_hashes = alice
        .connection_package_hashes(&record.username)
        .await
        .unwrap();
    assert_eq!(
        remaining_hashes.len() + 1,
        package_hashes.len(),
        "the device drops the key of the package the offer consumed"
    );

    bob.drain().await;
    let chat_bob = bob.expect_chat(chat_id).await;
    assert_eq!(
        chat_bob.chat_type(),
        &ChatType::Connection(alice.user_id().clone())
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn a_streamed_forwarded_request_has_the_sender_profile() {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    let alice2 = alice.add_synced_device().await;
    let record = setup.add_username(&alice).await;
    alice.outbound_service().run_once().await;
    alice2.drain().await;

    // A default display name is the same fallback a missing profile shows.
    let display_name: DisplayName = "Bob".parse().unwrap();
    bob.update_user_profile(UserProfile {
        user_id: bob.user_id().clone(),
        display_name: display_name.clone(),
        profile_picture: None,
    })
    .await
    .unwrap();
    bob.add_contact(record.username.clone(), record.hash, setup.apq_groups)
        .await
        .unwrap()
        .unwrap();
    let chat_id = alice.receive_connection_offer(&record).await;
    alice.outbound_service().run_once().await;

    let (mut stream, responder) = alice2.listen_queue().await.unwrap();
    let mut processor = QsStreamProcessor::new(Some(responder));
    let processed = loop {
        let event = timeout(std::time::Duration::from_secs(5), stream.next())
            .await
            .expect("the forwarded request should be streamed")
            .expect("the stream should stay open")
            .unwrap();
        match processor.process_event(&alice2, event).await {
            QsProcessEventResult::Accumulated | QsProcessEventResult::Ignored => (),
            QsProcessEventResult::FullyProcessed { processed } => break processed,
            QsProcessEventResult::PartiallyProcessed { dropped, .. } => {
                panic!("the sibling dropped {dropped} messages")
            }
        }
    };

    // The app builds the notification right after processing.
    assert_eq!(processed.new_connections, vec![chat_id]);
    assert_eq!(
        alice2.user_profile(bob.user_id()).await.display_name,
        display_name,
        "the notification should show the sender's name"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn sibling_holding_the_request_follows_the_accept() {
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
    assert_eq!(
        pending_chats_from(&alice2, bob.user_id()).await,
        vec![chat_id]
    );

    alice
        .accept_contact_request(chat_id)
        .await
        .unwrap()
        .unwrap();

    let processed = alice2.drain().await;
    assert_eq!(
        processed.removed_chats,
        vec![chat_id],
        "the notification of the request should be dropped"
    );
    assert!(pending_chats_from(&alice2, bob.user_id()).await.is_empty());
    assert_eq!(
        alice2.expect_chat(chat_id).await.chat_type(),
        &ChatType::Connection(bob.user_id().clone())
    );
    assert_eq!(
        alice2.contact(bob.user_id()).await.unwrap().chat_id,
        chat_id
    );
    assert!(
        has_system_message(&alice2, chat_id, |message| matches!(
            message,
            SystemMessage::AcceptedConnectionRequest {
                contact,
                user_handle: Some(user_handle),
            } if contact == bob.user_id() && user_handle == &record.username
        ))
        .await,
        "the acceptance should name the username of the request"
    );

    bob.drain().await;
    send_and_receive(&bob, &[&alice, &alice2], chat_id, "hello alice").await;
    send_and_receive(&alice2, &[&alice, &bob], chat_id, "hello bob").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn sibling_moves_older_requests_into_the_accepted_chat() {
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

    // The newer request is accepted before it reaches the sibling, which only
    // knows the older one. The outbox then drops the newer one.
    let second_chat_id = alice.receive_connection_offer(&second).await;
    assert_ne!(second_chat_id, first_chat_id);
    alice
        .accept_contact_request(second_chat_id)
        .await
        .unwrap()
        .unwrap();
    alice.outbound_service().run_once().await;

    let processed = alice2.drain().await;
    assert_eq!(
        processed.removed_chats,
        vec![first_chat_id],
        "the notification of the older request should be dropped"
    );
    assert!(alice2.chat(&first_chat_id).await.is_none());
    assert!(pending_chats_from(&alice2, bob.user_id()).await.is_empty());
    assert_eq!(
        alice2.expect_chat(second_chat_id).await.chat_type(),
        &ChatType::Connection(bob.user_id().clone())
    );
    assert_eq!(
        alice2.contact(bob.user_id()).await.unwrap().chat_id,
        second_chat_id
    );
    assert!(
        has_system_message(&alice2, second_chat_id, |message| matches!(
            message,
            SystemMessage::ReceivedHandleConnectionRequest { sender, user_handle }
                if sender == bob.user_id() && user_handle == &first.username
        ))
        .await,
        "the older request should move into the accepted chat"
    );

    bob.drain().await;
    send_and_receive(&bob, &[&alice, &alice2], second_chat_id, "hello alice").await;
    send_and_receive(&alice2, &[&alice, &bob], second_chat_id, "hello bob").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn sibling_fetches_the_profile_the_sender_rotated() {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    let alice2 = alice.add_synced_device().await;
    let record = setup.add_username(&alice).await;
    alice.outbound_service().run_once().await;
    alice2.drain().await;

    // The offer carries the profile key from before the rotation.
    bob.add_contact(record.username.clone(), record.hash, setup.apq_groups)
        .await
        .unwrap()
        .unwrap();
    let display_name: DisplayName = "Rotated Bob".parse().unwrap();
    bob.update_user_profile(UserProfile {
        user_id: bob.user_id().clone(),
        display_name: display_name.clone(),
        profile_picture: None,
    })
    .await
    .unwrap();

    alice.receive_connection_offer(&record).await;
    assert_eq!(
        alice.user_profile(bob.user_id()).await.display_name,
        display_name
    );
    alice.outbound_service().run_once().await;

    alice2.drain().await;
    alice2.outbound_service().run_once().await;
    assert_eq!(
        alice2.user_profile(bob.user_id()).await.display_name,
        display_name,
        "the sibling should fetch the rotated profile"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn linked_device_gets_open_incoming_requests() {
    let mut setup = TestBackend::single().await;
    let [alice, bob, charlie, dave] = setup.add_users().await;

    // Alice and charlie share a group, through dave.
    let group_chat_id = setup.connected_group(&dave, &[&alice, &charlie]).await;
    let record = setup.add_username(&alice).await;

    // An open request via alice's username from bob, and one through the group
    // from charlie.
    setup
        .user(&bob)
        .add_contact(record.username.clone(), record.hash, setup.apq_groups)
        .await
        .unwrap()
        .unwrap();
    let username_chat_id = alice.receive_connection_offer(&record).await;
    setup
        .user(&charlie)
        .add_contact_from_group(group_chat_id, alice.user_id().clone(), setup.apq_groups)
        .await
        .unwrap();
    let processed = alice.drain().await;
    let [group_request_chat_id] = processed.new_connections.as_slice() else {
        panic!("expected one request, got {:?}", processed.new_connections);
    };

    let alice2 = alice.add_synced_device().await;

    let username_chat = alice2.expect_chat(username_chat_id).await;
    assert_eq!(
        username_chat.chat_type(),
        &ChatType::PendingConnection(bob.user_id().clone())
    );
    assert!(
        has_system_message(&alice2, username_chat_id, |message| matches!(
            message,
            SystemMessage::ReceivedHandleConnectionRequest { sender, user_handle }
                if sender == bob.user_id() && user_handle == &record.username
        ))
        .await
    );
    let group_request_chat = alice2.chat(group_request_chat_id).await.unwrap();
    assert_eq!(
        group_request_chat.chat_type(),
        &ChatType::PendingConnection(charlie.user_id().clone())
    );
    assert!(
        has_system_message(&alice2, *group_request_chat_id, |message| matches!(
            message,
            SystemMessage::ReceivedGroupConnectionRequest { sender, origin_chat_id }
                if sender == charlie.user_id() && origin_chat_id == &group_chat_id
        ))
        .await
    );

    // The new device accepts both requests, which the original device follows.
    for chat_id in [username_chat_id, *group_request_chat_id] {
        alice2
            .accept_contact_request(chat_id)
            .await
            .unwrap()
            .unwrap();
    }
    alice.drain().await;
    assert_eq!(
        alice.expect_chat(username_chat_id).await.chat_type(),
        &ChatType::Connection(bob.user_id().clone())
    );
    assert_eq!(
        alice.chat(group_request_chat_id).await.unwrap().chat_type(),
        &ChatType::Connection(charlie.user_id().clone())
    );
}

/// When the recipients accept the outgoing requests, relative to the new
/// device's onboarding into their connection groups.
#[derive(Debug, Clone, Copy)]
enum Acceptance {
    AfterOnboarding,
    /// Before the onboarding, with the new device processing the joins before
    /// it onboards.
    BeforeOnboardingJoinsFirst,
    /// Before the onboarding, with the new device processing the joins after
    /// it onboards.
    BeforeOnboardingJoinsLast,
}

/// A new device gets the open outgoing requests of its user and follows the
/// recipients accepting them. When they accept before it onboards, the new
/// device holds their joins only in its queue, at an epoch before the one it
/// onboards at.
async fn linked_device_gets_open_outgoing_requests(acceptance: Acceptance) {
    let mut setup = TestBackend::single().await;
    let [alice, bob, charlie, dave] = setup.add_users().await;

    // Alice and charlie share a group, through dave.
    let group_chat_id = setup.connected_group(&dave, &[&alice, &charlie]).await;
    let record = setup.add_username(&bob).await;

    // An open request to bob's username, and one through the group to charlie.
    let username_chat_id = alice
        .add_contact(record.username.clone(), record.hash, setup.apq_groups)
        .await
        .unwrap()
        .unwrap();
    let group_request_chat_id = alice
        .add_contact_from_group(group_chat_id, charlie.user_id().clone(), setup.apq_groups)
        .await
        .unwrap();

    let alice2 = alice.add_linked_device().await;
    alice.drain().await;
    if let Acceptance::AfterOnboarding = acceptance {
        alice2.outbound_service().run_once().await;
        alice.drain().await;
        alice2.drain().await;
        assert_eq!(
            alice2.expect_chat(username_chat_id).await.chat_type(),
            &ChatType::HandleConnection(record.username.clone())
        );
        assert_eq!(
            alice2.expect_chat(group_request_chat_id).await.chat_type(),
            &ChatType::TargetedMessageConnection(charlie.user_id().clone())
        );
        // The requests expire at the same time on both devices.
        for chat_id in [username_chat_id, group_request_chat_id] {
            let expires_at = request_expires_at(&alice, chat_id).await;
            assert!(expires_at.is_some());
            assert_eq!(request_expires_at(&alice2, chat_id).await, expires_at);
        }
    }

    bob.receive_connection_offer(&record).await;
    bob.accept_contact_request(username_chat_id)
        .await
        .unwrap()
        .unwrap();
    charlie.drain().await;
    charlie
        .accept_contact_request(group_request_chat_id)
        .await
        .unwrap()
        .unwrap();

    match acceptance {
        Acceptance::AfterOnboarding => {}
        Acceptance::BeforeOnboardingJoinsFirst => {
            alice2.drain().await;
            alice2.outbound_service().run_once().await;
        }
        Acceptance::BeforeOnboardingJoinsLast => {
            alice2.outbound_service().run_once().await;
            alice2.drain().await;
        }
    }

    for (label, device) in [("original", &alice), ("new", &alice2)] {
        device.drain().await;
        assert_eq!(
            device.expect_chat(username_chat_id).await.chat_type(),
            &ChatType::Connection(bob.user_id().clone()),
            "the {label} device should see the connection to bob"
        );
        assert_eq!(
            device.expect_chat(group_request_chat_id).await.chat_type(),
            &ChatType::Connection(charlie.user_id().clone()),
            "the {label} device should see the connection to charlie"
        );
    }
    assert!(
        has_system_message(&alice2, group_request_chat_id, |message| matches!(
            message,
            SystemMessage::SentGroupConnectionRequest { recipient, origin_chat_id }
                if recipient == charlie.user_id() && origin_chat_id == &group_chat_id
        ))
        .await
    );
    assert!(
        has_system_message(&alice2, username_chat_id, |message| matches!(
            message,
            SystemMessage::ReceivedConnectionConfirmation { sender, .. } if sender == bob.user_id()
        ))
        .await
    );

    send_and_receive(&alice2, &[&bob, &alice], username_chat_id, "hi bob").await;
    send_and_receive(
        &alice2,
        &[&charlie, &alice],
        group_request_chat_id,
        "hi charlie",
    )
    .await;

    // A commit of the new device passes the room state checks of the others.
    alice2.update_key(username_chat_id).await.unwrap();
    send_and_receive(&bob, &[&alice, &alice2], username_chat_id, "hi alice").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn outgoing_requests_accepted_after_onboarding() {
    linked_device_gets_open_outgoing_requests(Acceptance::AfterOnboarding).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn outgoing_requests_accepted_before_onboarding_joins_first() {
    linked_device_gets_open_outgoing_requests(Acceptance::BeforeOnboardingJoinsFirst).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn outgoing_requests_accepted_before_onboarding_joins_last() {
    linked_device_gets_open_outgoing_requests(Acceptance::BeforeOnboardingJoinsLast).await;
}

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

async fn request_expires_at(device: &CoreUser, chat_id: ChatId) -> Option<DateTime<Utc>> {
    let chat = device.expect_chat(chat_id).await;
    device.contact_request_expires_at(&chat).await.unwrap()
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
async fn an_expired_request_closes_on_the_sibling_and_is_unavailable() {
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
async fn an_echoed_expiry_delete_keeps_the_record() {
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
async fn a_request_fetched_past_its_deadline_is_dropped() {
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
async fn a_request_expiring_on_both_devices_keeps_one_record() {
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
async fn expired_requests_go_when_the_user_chose_so() {
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
