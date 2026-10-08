// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Connection requests across the devices of a user.

use aircoreclient::{
    ChatId, ChatType, DisplayName, EventMessage, Message, SystemMessage, UserProfile,
    clients::CoreUser,
};
use airserver_test_harness::utils::setup::TestBackend;

use super::{
    contact_requests::{add_second_username, pending_chats_from},
    group_bootstrap::{
        add_username, drain_expecting_success, link_sibling, receive_connection_offer,
    },
    multi_device::{drain_queue, link_new_device, send_and_receive},
};

/// Whether the chat holds a system message matching `predicate`.
async fn has_system_message(
    device: &CoreUser,
    chat_id: ChatId,
    predicate: impl Fn(&SystemMessage) -> bool,
) -> bool {
    device
        .messages(chat_id, 100)
        .await
        .unwrap()
        .iter()
        .any(|message| match message.message() {
            Message::Event(EventMessage::System(system_message)) => predicate(system_message),
            _ => false,
        })
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[tracing::instrument(name = "Request received by a sibling", skip_all)]
async fn request_fetched_by_one_device_reaches_its_sibling() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let bob = setup.add_user().await;
    let (device_a, device_b, _tmp) = link_sibling(&setup, &alice).await;
    let record = add_username(&mut setup, &alice).await;
    device_a.outbound_service().run_once().await;
    drain_expecting_success(&device_b, "the sibling failed to follow the username").await;

    let bob_user = setup.get_user(&bob).user().clone();
    let bob_chat_id = bob_user
        .add_contact(record.username.clone(), record.hash, setup.apq_groups)
        .await
        .unwrap()
        .unwrap();

    // Only the device that holds the username fetches the offer.
    let chat_id = receive_connection_offer(&device_a, &record).await;
    assert_eq!(chat_id, bob_chat_id);
    let package_hashes = device_a
        .connection_package_hashes(&record.username)
        .await
        .unwrap();
    device_a.outbound_service().run_once().await;

    let processed =
        drain_expecting_success(&device_b, "the sibling failed to take the request").await;
    assert_eq!(
        processed.new_connections,
        vec![chat_id],
        "the forwarded request should raise a notification"
    );
    let chat_b = device_b.chat(&chat_id).await.unwrap();
    assert_eq!(
        chat_b.chat_type(),
        &ChatType::PendingConnection(bob.clone())
    );
    assert!(
        has_system_message(&device_b, chat_id, |message| matches!(
            message,
            SystemMessage::ReceivedHandleConnectionRequest { sender, user_handle }
                if sender == &bob && user_handle == &record.username
        ))
        .await,
        "the forwarded request should be announced like a received one"
    );

    // The device that never saw the offer accepts it.
    device_b
        .accept_contact_request(chat_id)
        .await
        .unwrap()
        .unwrap();

    drain_expecting_success(&device_a, "the device failed to follow the accept").await;
    let chat_a = device_a.chat(&chat_id).await.unwrap();
    assert_eq!(chat_a.chat_type(), &ChatType::Connection(bob.clone()));
    let remaining_hashes = device_a
        .connection_package_hashes(&record.username)
        .await
        .unwrap();
    assert_eq!(
        remaining_hashes.len() + 1,
        package_hashes.len(),
        "the device drops the key of the package the offer consumed"
    );

    drain_expecting_success(&bob_user, "bob failed to follow the accept").await;
    let chat_bob = bob_user.chat(&chat_id).await.unwrap();
    assert_eq!(chat_bob.chat_type(), &ChatType::Connection(alice.clone()));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[tracing::instrument(name = "Sibling with the request follows the accept", skip_all)]
async fn sibling_holding_the_request_follows_the_accept() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let bob = setup.add_user().await;
    let (device_a, device_b, _tmp) = link_sibling(&setup, &alice).await;
    let record = add_username(&mut setup, &alice).await;
    device_a.outbound_service().run_once().await;
    drain_expecting_success(&device_b, "the sibling failed to follow the username").await;

    let bob_user = setup.get_user(&bob).user().clone();
    bob_user
        .add_contact(record.username.clone(), record.hash, setup.apq_groups)
        .await
        .unwrap()
        .unwrap();
    let chat_id = receive_connection_offer(&device_a, &record).await;
    device_a.outbound_service().run_once().await;
    drain_expecting_success(&device_b, "the sibling failed to take the request").await;
    assert_eq!(pending_chats_from(&device_b, &bob).await, vec![chat_id]);

    device_a
        .accept_contact_request(chat_id)
        .await
        .unwrap()
        .unwrap();

    let processed =
        drain_expecting_success(&device_b, "the sibling failed to follow the accept").await;
    assert_eq!(
        processed.removed_chats,
        vec![chat_id],
        "the notification of the request should be dropped"
    );
    assert!(pending_chats_from(&device_b, &bob).await.is_empty());
    assert_eq!(
        device_b.chat(&chat_id).await.unwrap().chat_type(),
        &ChatType::Connection(bob.clone())
    );
    assert_eq!(device_b.contact(&bob).await.unwrap().chat_id, chat_id);
    assert!(
        has_system_message(&device_b, chat_id, |message| matches!(
            message,
            SystemMessage::AcceptedConnectionRequest {
                contact,
                user_handle: Some(user_handle),
            } if contact == &bob && user_handle == &record.username
        ))
        .await,
        "the acceptance should name the username of the request"
    );

    drain_queue(&bob_user).await;
    send_and_receive(&bob_user, &[&device_a, &device_b], chat_id, "hello alice").await;
    send_and_receive(&device_b, &[&device_a, &bob_user], chat_id, "hello bob").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[tracing::instrument(name = "Sibling moves older requests into the accepted chat", skip_all)]
async fn sibling_moves_older_requests_into_the_accepted_chat() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let bob = setup.add_user().await;
    let (device_a, device_b, _tmp) = link_sibling(&setup, &alice).await;
    let first = add_username(&mut setup, &alice).await;
    let second = add_second_username(&setup, &alice).await;
    device_a.outbound_service().run_once().await;
    drain_expecting_success(&device_b, "the sibling failed to follow the usernames").await;

    let bob_user = setup.get_user(&bob).user().clone();
    for record in [&first, &second] {
        bob_user
            .add_contact(record.username.clone(), record.hash, setup.apq_groups)
            .await
            .unwrap()
            .unwrap();
    }
    let first_chat_id = receive_connection_offer(&device_a, &first).await;
    device_a.outbound_service().run_once().await;
    drain_expecting_success(&device_b, "the sibling failed to take the first request").await;

    // The newer request is accepted before it reaches the sibling, which only
    // knows the older one. The outbox then drops the newer one.
    let second_chat_id = receive_connection_offer(&device_a, &second).await;
    assert_ne!(second_chat_id, first_chat_id);
    device_a
        .accept_contact_request(second_chat_id)
        .await
        .unwrap()
        .unwrap();
    device_a.outbound_service().run_once().await;

    let processed =
        drain_expecting_success(&device_b, "the sibling failed to follow the accept").await;
    assert_eq!(
        processed.removed_chats,
        vec![first_chat_id],
        "the notification of the older request should be dropped"
    );
    assert!(device_b.chat(&first_chat_id).await.is_none());
    assert!(pending_chats_from(&device_b, &bob).await.is_empty());
    assert_eq!(
        device_b.chat(&second_chat_id).await.unwrap().chat_type(),
        &ChatType::Connection(bob.clone())
    );
    assert_eq!(
        device_b.contact(&bob).await.unwrap().chat_id,
        second_chat_id
    );
    assert!(
        has_system_message(&device_b, second_chat_id, |message| matches!(
            message,
            SystemMessage::ReceivedHandleConnectionRequest { sender, user_handle }
                if sender == &bob && user_handle == &first.username
        ))
        .await,
        "the older request should move into the accepted chat"
    );

    drain_queue(&bob_user).await;
    send_and_receive(
        &bob_user,
        &[&device_a, &device_b],
        second_chat_id,
        "hello alice",
    )
    .await;
    send_and_receive(
        &device_b,
        &[&device_a, &bob_user],
        second_chat_id,
        "hello bob",
    )
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[tracing::instrument(name = "Sibling fetches a rotated sender profile", skip_all)]
async fn sibling_fetches_the_profile_the_sender_rotated() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let bob = setup.add_user().await;
    let (device_a, device_b, _tmp) = link_sibling(&setup, &alice).await;
    let record = add_username(&mut setup, &alice).await;
    device_a.outbound_service().run_once().await;
    drain_expecting_success(&device_b, "the sibling failed to follow the username").await;

    // The offer carries the profile key from before the rotation.
    let bob_user = setup.get_user(&bob).user().clone();
    bob_user
        .add_contact(record.username.clone(), record.hash, setup.apq_groups)
        .await
        .unwrap()
        .unwrap();
    let display_name: DisplayName = "Rotated Bob".parse().unwrap();
    bob_user
        .update_user_profile(UserProfile {
            user_id: bob.clone(),
            display_name: display_name.clone(),
            profile_picture: None,
        })
        .await
        .unwrap();

    receive_connection_offer(&device_a, &record).await;
    assert_eq!(device_a.user_profile(&bob).await.display_name, display_name);
    device_a.outbound_service().run_once().await;

    drain_expecting_success(&device_b, "the sibling failed to take the request").await;
    device_b.outbound_service().run_once().await;
    assert_eq!(
        device_b.user_profile(&bob).await.display_name,
        display_name,
        "the sibling should fetch the rotated profile"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[tracing::instrument(name = "Linked device gets open requests", skip_all)]
async fn linked_device_gets_open_incoming_requests() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let bob = setup.add_user().await;
    let charlie = setup.add_user().await;
    let dave = setup.add_user().await;

    // Alice and charlie share a group, through dave.
    setup.connect_users(&dave, &alice).await;
    setup.connect_users(&dave, &charlie).await;
    let group_chat_id = setup.create_group(&dave).await;
    setup
        .invite_to_group(group_chat_id, &dave, vec![&alice, &charlie])
        .await;
    let record = add_username(&mut setup, &alice).await;
    let device_a = setup.get_user(&alice).user().clone();

    // An open request via alice's username from bob, and one through the group
    // from charlie.
    setup
        .get_user(&bob)
        .user()
        .add_contact(record.username.clone(), record.hash, setup.apq_groups)
        .await
        .unwrap()
        .unwrap();
    let username_chat_id = receive_connection_offer(&device_a, &record).await;
    setup
        .get_user(&charlie)
        .user()
        .add_contact_from_group(group_chat_id, alice.clone(), setup.apq_groups)
        .await
        .unwrap();
    let processed =
        drain_expecting_success(&device_a, "alice failed to take the group request").await;
    let [group_request_chat_id] = processed.new_connections.as_slice() else {
        panic!("expected one request, got {:?}", processed.new_connections);
    };

    let (device_a, device_b, _tmp) = link_sibling(&setup, &alice).await;

    let username_chat = device_b.chat(&username_chat_id).await.unwrap();
    assert_eq!(
        username_chat.chat_type(),
        &ChatType::PendingConnection(bob.clone())
    );
    assert!(
        has_system_message(&device_b, username_chat_id, |message| matches!(
            message,
            SystemMessage::ReceivedHandleConnectionRequest { sender, user_handle }
                if sender == &bob && user_handle == &record.username
        ))
        .await
    );
    let group_request_chat = device_b.chat(group_request_chat_id).await.unwrap();
    assert_eq!(
        group_request_chat.chat_type(),
        &ChatType::PendingConnection(charlie.clone())
    );
    assert!(
        has_system_message(&device_b, *group_request_chat_id, |message| matches!(
            message,
            SystemMessage::ReceivedGroupConnectionRequest { sender, origin_chat_id }
                if sender == &charlie && origin_chat_id == &group_chat_id
        ))
        .await
    );

    // The new device accepts both requests, which the original device follows.
    for chat_id in [username_chat_id, *group_request_chat_id] {
        device_b
            .accept_contact_request(chat_id)
            .await
            .unwrap()
            .unwrap();
    }
    drain_expecting_success(&device_a, "the device failed to follow the accepts").await;
    assert_eq!(
        device_a.chat(&username_chat_id).await.unwrap().chat_type(),
        &ChatType::Connection(bob.clone())
    );
    assert_eq!(
        device_a
            .chat(group_request_chat_id)
            .await
            .unwrap()
            .chat_type(),
        &ChatType::Connection(charlie.clone())
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
    let alice = setup.add_user().await;
    let bob = setup.add_user().await;
    let charlie = setup.add_user().await;
    let dave = setup.add_user().await;

    // Alice and charlie share a group, through dave.
    setup.connect_users(&dave, &alice).await;
    setup.connect_users(&dave, &charlie).await;
    let group_chat_id = setup.create_group(&dave).await;
    setup
        .invite_to_group(group_chat_id, &dave, vec![&alice, &charlie])
        .await;
    let record = add_username(&mut setup, &bob).await;
    let device_a = setup.get_user(&alice).user().clone();

    // An open request to bob's username, and one through the group to charlie.
    let username_chat_id = device_a
        .add_contact(record.username.clone(), record.hash, setup.apq_groups)
        .await
        .unwrap()
        .unwrap();
    let group_request_chat_id = device_a
        .add_contact_from_group(group_chat_id, charlie.clone(), setup.apq_groups)
        .await
        .unwrap();

    let (device_b, _tmp) = link_new_device(&setup, &alice).await;
    drain_queue(&device_a).await;
    if let Acceptance::AfterOnboarding = acceptance {
        device_b.outbound_service().run_once().await;
        drain_queue(&device_a).await;
        drain_queue(&device_b).await;
        assert_eq!(
            device_b.chat(&username_chat_id).await.unwrap().chat_type(),
            &ChatType::HandleConnection(record.username.clone())
        );
        assert_eq!(
            device_b
                .chat(&group_request_chat_id)
                .await
                .unwrap()
                .chat_type(),
            &ChatType::TargetedMessageConnection(charlie.clone())
        );
    }

    let bob_user = setup.get_user(&bob).user().clone();
    receive_connection_offer(&bob_user, &record).await;
    bob_user
        .accept_contact_request(username_chat_id)
        .await
        .unwrap()
        .unwrap();
    let charlie_user = setup.get_user(&charlie).user().clone();
    drain_expecting_success(&charlie_user, "charlie failed to take the request").await;
    charlie_user
        .accept_contact_request(group_request_chat_id)
        .await
        .unwrap()
        .unwrap();

    match acceptance {
        Acceptance::AfterOnboarding => {}
        Acceptance::BeforeOnboardingJoinsFirst => {
            drain_expecting_success(&device_b, "the new device failed on the joins").await;
            device_b.outbound_service().run_once().await;
        }
        Acceptance::BeforeOnboardingJoinsLast => {
            device_b.outbound_service().run_once().await;
            drain_expecting_success(&device_b, "the new device failed on the joins").await;
        }
    }

    for (label, device) in [("original", &device_a), ("new", &device_b)] {
        drain_expecting_success(device, "a device failed to follow the accepts").await;
        assert_eq!(
            device.chat(&username_chat_id).await.unwrap().chat_type(),
            &ChatType::Connection(bob.clone()),
            "the {label} device should see the connection to bob"
        );
        assert_eq!(
            device
                .chat(&group_request_chat_id)
                .await
                .unwrap()
                .chat_type(),
            &ChatType::Connection(charlie.clone()),
            "the {label} device should see the connection to charlie"
        );
    }
    assert!(
        has_system_message(&device_b, group_request_chat_id, |message| matches!(
            message,
            SystemMessage::SentGroupConnectionRequest { recipient, origin_chat_id }
                if recipient == &charlie && origin_chat_id == &group_chat_id
        ))
        .await
    );
    assert!(
        has_system_message(&device_b, username_chat_id, |message| matches!(
            message,
            SystemMessage::ReceivedConnectionConfirmation { sender, .. } if sender == &bob
        ))
        .await
    );

    send_and_receive(
        &device_b,
        &[&bob_user, &device_a],
        username_chat_id,
        "hi bob",
    )
    .await;
    send_and_receive(
        &device_b,
        &[&charlie_user, &device_a],
        group_request_chat_id,
        "hi charlie",
    )
    .await;

    // A commit of the new device passes the room state checks of the others.
    device_b.update_key(username_chat_id).await.unwrap();
    send_and_receive(
        &bob_user,
        &[&device_a, &device_b],
        username_chat_id,
        "hi alice",
    )
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[tracing::instrument(name = "Requests accepted after onboarding", skip_all)]
async fn outgoing_requests_accepted_after_onboarding() {
    linked_device_gets_open_outgoing_requests(Acceptance::AfterOnboarding).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[tracing::instrument(name = "Requests accepted before onboarding, joins first", skip_all)]
async fn outgoing_requests_accepted_before_onboarding_joins_first() {
    linked_device_gets_open_outgoing_requests(Acceptance::BeforeOnboardingJoinsFirst).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[tracing::instrument(name = "Requests accepted before onboarding, joins last", skip_all)]
async fn outgoing_requests_accepted_before_onboarding_joins_last() {
    linked_device_gets_open_outgoing_requests(Acceptance::BeforeOnboardingJoinsLast).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[tracing::instrument(name = "Deleting a request chat before forwarding", skip_all)]
async fn deleting_a_request_chat_before_forwarding_clears_the_sibling() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let bob = setup.add_user().await;
    let (device_a, device_b, _tmp) = link_sibling(&setup, &alice).await;
    let first = add_username(&mut setup, &alice).await;
    let second = add_second_username(&setup, &alice).await;
    device_a.outbound_service().run_once().await;
    drain_expecting_success(&device_b, "the sibling failed to follow the usernames").await;

    let bob_user = setup.get_user(&bob).user().clone();
    for record in [&first, &second] {
        bob_user
            .add_contact(record.username.clone(), record.hash, setup.apq_groups)
            .await
            .unwrap()
            .unwrap();
    }

    let first_chat_id = receive_connection_offer(&device_a, &first).await;
    device_a.outbound_service().run_once().await;
    drain_expecting_success(&device_b, "the sibling failed to take the first request").await;
    assert!(device_b.chat(&first_chat_id).await.is_some());

    // The newer request takes over the chat, which alice deletes before the
    // request reaches the sibling.
    let second_chat_id = receive_connection_offer(&device_a, &second).await;
    assert_ne!(second_chat_id, first_chat_id);
    let _ = device_a.delete_chat(second_chat_id).await;
    device_a.erase_chat(second_chat_id).await.unwrap();
    device_a.outbound_service().run_once().await;

    drain_expecting_success(&device_b, "the sibling failed to follow the deletion").await;
    assert!(
        pending_chats_from(&device_b, &bob).await.is_empty(),
        "the sibling should drop the older request too"
    );
}
