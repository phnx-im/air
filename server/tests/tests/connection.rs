// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::time::Duration;

use airapiclient::{ApiClient, as_api::AsRequestError};
use aircommon::{
    identifiers::{UserId, UsernameHash},
    messages::client_as::SerializedToken,
    time::TimeStamp,
};
use aircoreclient::{ChatId, EventMessage, Message, SystemMessage};
use airprotos::{
    auth_service::v1::OperationType,
    client::signed_connection_package::{AnyConnectionPackage, AnyConnectionPackageIn},
};
use airserver_test_harness::utils::setup::TestBackend;
use chrono::{TimeZone, Utc};
use tokio::task::spawn_blocking;
use tokio_stream::StreamExt;

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[tracing::instrument(name = "Connect users test", skip_all)]
async fn connect_users_via_user_handle() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let bob = setup.add_user().await;
    setup.connect_users(&alice, &bob).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[tracing::instrument(name = "A connection request spends a token", skip_all)]
async fn connection_request_spends_a_token() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let bob = setup.add_user().await;
    let record = setup.get_user_mut(&bob).add_username().await.unwrap();

    let alice_user = setup.get_user(&alice).user().clone();
    let before = alice_user
        .cached_privacy_pass_tokens(OperationType::ConnectUsername)
        .await
        .unwrap();
    assert_eq!(
        before.len(),
        usize::from(OperationType::ConnectUsername.max_tokens_allowance()),
        "registration fetches the full batch"
    );

    setup.connect_users(&alice, &bob).await;

    let after = alice_user
        .cached_privacy_pass_tokens(OperationType::ConnectUsername)
        .await
        .unwrap();
    assert_eq!(after.len(), before.len() - 1, "the request spent one token");
    let spent = before
        .iter()
        .find(|token| !after.contains(token))
        .cloned()
        .unwrap();

    // The redemption is recorded for the siblings.
    let pending = alice_user
        .pending_redeemed_token_broadcasts()
        .await
        .unwrap();
    assert_eq!(pending.len(), 1, "one batch has a redemption to broadcast");
    assert_eq!(pending[0].operation_type, OperationType::ConnectUsername);
    assert_eq!(pending[0].token_indices.len(), 1);

    // The AS does not take the same token twice.
    let client = ApiClient::with_endpoint(&setup.server_url()).unwrap();
    let result = client
        .as_connect_username(record.hash, Some(SerializedToken::new(spent)))
        .await;
    let Err(AsRequestError::Tonic(status)) = result else {
        panic!("a spent token must be rejected with a status");
    };
    assert_eq!(status.code(), tonic::Code::Unauthenticated);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[tracing::instrument(name = "Connect users via signed connection package", skip_all)]
async fn connect_users_via_user_handle_uses_signed_package() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let bob = setup.add_user().await;

    let record = setup.get_user_mut(&bob).add_username().await.unwrap();

    // A new client asks for a signed package and gets one, carrying the
    // owner's features.
    let client = ApiClient::with_endpoint(&setup.server_url()).unwrap();
    let (package, _responder) = client.as_connect_username(record.hash, None).await.unwrap();
    assert!(matches!(package, AnyConnectionPackageIn::Signed(_)));
    let package = package.verify(&record.hash).unwrap();
    let AnyConnectionPackage::Signed(package) = package else {
        panic!("expected signed connection package");
    };
    assert_eq!(package.username_hash(), &record.hash);
    assert!(package.air_features().pq_groups);

    setup.connect_users(&alice, &bob).await;
}

/// Fetches and verifies one connection package for the username.
async fn fetch_connection_package(
    client: &ApiClient,
    hash: UsernameHash,
) -> anyhow::Result<AnyConnectionPackage> {
    let (package, _responder) = client.as_connect_username(hash, None).await?;
    Ok(package.verify(&hash)?)
}

/// Consumes connection packages until the server hands out the last resort one.
async fn drain_connection_packages(client: &ApiClient, hash: UsernameHash) -> anyhow::Result<()> {
    for _ in 0..100 {
        if fetch_connection_package(client, hash)
            .await?
            .is_last_resort()
        {
            return Ok(());
        }
    }
    anyhow::bail!("connection packages not drained after 100 fetches");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[tracing::instrument(name = "Signed connection package upload task", skip_all)]
async fn signed_connection_package_upload_task_replenishes_username() -> anyhow::Result<()> {
    let mut setup = TestBackend::single().await;
    let bob = setup.add_user().await;
    let record = setup.get_user_mut(&bob).add_username().await?;
    let user = setup.get_user(&bob).user();
    let client = ApiClient::with_endpoint(&setup.server_url())?;

    // Use up the packages published on creation, so that new ones are observable.
    drain_connection_packages(&client, record.hash).await?;
    assert!(
        fetch_connection_package(&client, record.hash)
            .await?
            .is_last_resort()
    );

    // Run the one-shot task as if the username predated signed connection packages.
    user.outbound_service()
        .schedule_signed_connection_package_upload(vec![record.hash], Utc::now())
        .await?;
    user.outbound_service().run_once().await;

    let state = user
        .outbound_service()
        .signed_connection_package_upload_state()
        .await?
        .expect("task should exist");
    assert!(state.parked, "task should be parked after success");
    assert!(state.pending_usernames.is_empty());

    let package = fetch_connection_package(&client, record.hash).await?;
    assert!(matches!(package, AnyConnectionPackage::Signed(_)));
    assert!(
        !package.is_last_resort(),
        "the task should have published fresh packages"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[tracing::instrument(name = "Signed connection package upload task retry", skip_all)]
async fn signed_connection_package_upload_task_retries_pending_username() -> anyhow::Result<()> {
    let mut setup = TestBackend::single().await;
    let bob = setup.add_user().await;
    let record = setup.get_user_mut(&bob).add_username().await?;
    let user = setup.get_user(&bob).user();

    // The upload fails with a network error.
    setup.listener_control_handle().set_drop_all();
    user.outbound_service()
        .schedule_signed_connection_package_upload(vec![record.hash], Utc::now())
        .await?;
    user.outbound_service().run_once().await;
    setup.listener_control_handle().set_normal();

    let state = user
        .outbound_service()
        .signed_connection_package_upload_state()
        .await?
        .expect("task should exist");
    assert!(!state.parked, "failed task must be retried");
    assert_eq!(state.pending_usernames, vec![record.hash]);

    // The retry succeeds.
    user.outbound_service()
        .schedule_signed_connection_package_upload(state.pending_usernames, Utc::now())
        .await?;
    user.outbound_service().run_once().await;

    let state = user
        .outbound_service()
        .signed_connection_package_upload_state()
        .await?
        .expect("task should exist");
    assert!(state.parked);
    assert!(state.pending_usernames.is_empty());

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[tracing::instrument(
    name = "Signed connection package upload task unknown username",
    skip_all
)]
async fn signed_connection_package_upload_task_drops_unknown_username() -> anyhow::Result<()> {
    let mut setup = TestBackend::single().await;
    let bob = setup.add_user().await;
    let user = setup.get_user(&bob).user();

    // A username which does not exist locally is treated as done.
    user.outbound_service()
        .schedule_signed_connection_package_upload(vec![UsernameHash::new([7; 32])], Utc::now())
        .await?;
    user.outbound_service().run_once().await;

    let state = user
        .outbound_service()
        .signed_connection_package_upload_state()
        .await?
        .expect("task should exist");
    assert!(state.parked);
    assert!(state.pending_usernames.is_empty());

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[tracing::instrument(name = "Connect users via targeted message", skip_all)]
async fn connect_users_via_targeted_message() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let bob = setup.add_user().await;
    let charlie = setup.add_user().await;

    // Alice is connected to Bob and Charlie, but Bob and Charlie are not connected.
    setup.connect_users(&alice, &bob).await;
    setup.connect_users(&alice, &charlie).await;

    // Alice creates a group and invites Bob and Charlie
    let group_chat_id = setup.create_group(&alice).await;
    setup
        .invite_to_group(group_chat_id, &alice, vec![&bob, &charlie])
        .await;
    let alice_user = &setup.get_user(&alice).user;
    let group_chat = alice_user.chat(&group_chat_id).await.unwrap();

    // Bob now connects to Charlie via a targeted message sent through the
    // shared group.
    let bob_user = &setup.get_user(&bob).user;
    let bob_chat_id = bob_user
        .add_contact_from_group(group_chat_id, charlie.clone(), setup.apq_groups)
        .await
        .unwrap();

    // Bob should have the right system message in the chat
    let chat_message = bob_user
        .messages(bob_chat_id, 1)
        .await
        .unwrap()
        .pop()
        .unwrap();
    let Message::Event(EventMessage::System(SystemMessage::NewDirectConnectionChat(user_id))) =
        chat_message.message()
    else {
        panic!("Expected NewDirectConnectionChat system message");
    };
    assert!(
        *user_id == charlie,
        "System message should indicate connection to Charlie"
    );

    // Charlie picks up his messages
    let charlie_user = &setup.get_user(&charlie).user;
    let qs_messages = charlie_user.qs_fetch_messages().await.unwrap();
    let mut result = charlie_user.fully_process_qs_messages(qs_messages).await;
    assert!(
        result.errors.is_empty(),
        "Charlie should process Bob's targeted message without errors"
    );

    // Charlie accepts the connection request
    charlie_user
        .accept_contact_request(bob_chat_id)
        .await
        .unwrap()
        .unwrap();

    // Charlie should have two messages in the new chat
    let charlie_chat_id = result.new_connections.pop().unwrap();
    let messages = charlie_user.messages(charlie_chat_id, 2).await.unwrap();
    let Message::Event(EventMessage::System(SystemMessage::ReceivedDirectConnectionRequest {
        sender,
        chat_name,
    })) = messages[0].message()
    else {
        panic!("Expected NewDirectConnectionChat system message");
    };
    assert_eq!(
        *sender, bob,
        "System message should indicate connection from Bob"
    );
    assert_eq!(
        *chat_name,
        group_chat.attributes().unwrap().title,
        "System message should have the correct chat title"
    );
    let Message::Event(EventMessage::System(SystemMessage::AcceptedConnectionRequest {
        contact,
        user_handle: None,
    })) = messages[1].message()
    else {
        panic!("Expected AcceptedConnectionRequest system message");
    };
    assert_eq!(
        *contact, bob,
        "System message should indicate acceptance of connection from Bob"
    );

    // Now Bob picks up his messages
    let bob_user = &setup.get_user(&bob).user;
    let qs_messages = bob_user.qs_fetch_messages().await.unwrap();
    let result = bob_user.fully_process_qs_messages(qs_messages).await;
    assert!(
        result.errors.is_empty(),
        "Bob should process Charlie's response without errors"
    );
    // Bob should have a system message indicating that Charlie accepted the connection
    let messages = bob_user.messages(bob_chat_id, 1).await.unwrap();
    let Message::Event(EventMessage::System(SystemMessage::ReceivedConnectionConfirmation {
        sender,
        user_handle: None,
    })) = messages[0].message()
    else {
        panic!("Expected ReceivedConnectionConfirmation system message");
    };
    assert!(
        *sender == charlie,
        "System message should indicate acceptance of connection from Charlie"
    );

    // Bob and Charlie should now be connected
    let bob_contact = bob_user.contact(&charlie).await;
    assert!(
        bob_contact.is_some(),
        "Bob should have Charlie as a contact"
    );
    let charlie_user = &setup.get_user(&charlie).user;
    let charlie_contact = charlie_user.contact(&bob).await;
    assert!(
        charlie_contact.is_some(),
        "Charlie should have Bob as a contact"
    );

    // The connection group is APQ iff requested, and both sides agree on it.
    assert_eq!(
        bob_user.chat_is_apq(bob_chat_id).await,
        Some(setup.apq_groups)
    );
    assert_eq!(
        charlie_user.chat_is_apq(charlie_chat_id).await,
        Some(setup.apq_groups)
    );
}

/// Connects `initiator` to `peer` through the shared group and returns the connection chat as
/// seen by the initiator and by the peer.
async fn connect_from_group(
    setup: &TestBackend,
    group_chat_id: ChatId,
    initiator: &UserId,
    peer: &UserId,
    prefer_apq: bool,
) -> (ChatId, ChatId) {
    let initiator_user = &setup.get_user(initiator).user;
    let initiator_chat_id = initiator_user
        .add_contact_from_group(group_chat_id, peer.clone(), prefer_apq)
        .await
        .unwrap();

    let peer_user = &setup.get_user(peer).user;
    let qs_messages = peer_user.qs_fetch_messages().await.unwrap();
    let mut result = peer_user.fully_process_qs_messages(qs_messages).await;
    assert!(
        result.errors.is_empty(),
        "peer should process the connection request without errors: {:?}",
        result.errors
    );
    let peer_chat_id = result.new_connections.pop().unwrap();
    peer_user
        .accept_contact_request(peer_chat_id)
        .await
        .unwrap()
        .unwrap();

    let qs_messages = initiator_user.qs_fetch_messages().await.unwrap();
    let result = initiator_user.fully_process_qs_messages(qs_messages).await;
    assert!(
        result.errors.is_empty(),
        "initiator should process the confirmation without errors: {:?}",
        result.errors
    );

    assert!(initiator_user.contact(peer).await.is_some());
    assert!(peer_user.contact(initiator).await.is_some());
    (initiator_chat_id, peer_chat_id)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[tracing::instrument(name = "Connect users via user handle in APQ mode", skip_all)]
async fn connect_users_via_user_handle_apq() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let bob = setup.add_user().await;
    // Asserts the APQ-ness on both sides and exchanges messages both ways.
    setup.connect_users_apq(&alice, &bob).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[tracing::instrument(name = "Connection group stays T when APQ is not requested", skip_all)]
async fn connect_users_via_user_handle_not_requested_stays_t() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let bob = setup.add_user().await;
    setup.connect_users_non_apq(&alice, &bob).await;
}

/// The connection request rides the APQ origin group as a T targeted message, and the connection
/// group itself is APQ.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[tracing::instrument(name = "Connect users via targeted message in APQ mode", skip_all)]
async fn connect_users_via_targeted_message_apq() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let bob = setup.add_user().await;
    let charlie = setup.add_user().await;

    setup.connect_users(&alice, &bob).await;
    setup.connect_users(&alice, &charlie).await;

    let group_chat_id = setup.create_apq_group(&alice).await;
    setup
        .invite_to_group(group_chat_id, &alice, vec![&bob, &charlie])
        .await;

    let (bob_chat_id, charlie_chat_id) =
        connect_from_group(&setup, group_chat_id, &bob, &charlie, true).await;

    let bob_user = &setup.get_user(&bob).user;
    let charlie_user = &setup.get_user(&charlie).user;
    assert_eq!(bob_user.chat_is_apq(bob_chat_id).await, Some(true));
    assert_eq!(charlie_user.chat_is_apq(charlie_chat_id).await, Some(true));

    setup
        .send_message(bob_chat_id, &bob, vec![&charlie], None)
        .await;
    setup
        .send_message(bob_chat_id, &charlie, vec![&bob], None)
        .await;
}

/// A connection group is resynced with the same machinery as any other group.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[tracing::instrument(name = "Resync connection group", skip_all)]
async fn resync_connection_group() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let bob = setup.add_user().await;
    let chat_id = setup.connect_users(&alice, &bob).await;

    let bob_user = &setup.get_user(&bob).user;
    bob_user.enqueue_group_resync(chat_id).await.unwrap();
    bob_user.outbound_service().run_once().await;
    assert!(
        bob_user.resync_status(chat_id).await.unwrap().is_none(),
        "resync should have completed"
    );

    let alice_user = &setup.get_user(&alice).user;
    let qs_messages = alice_user.qs_fetch_messages().await.unwrap();
    let result = alice_user.fully_process_qs_messages(qs_messages).await;
    assert!(
        result.errors.is_empty(),
        "Alice should process Bob's rejoin without errors: {:?}",
        result.errors
    );

    setup.send_message(chat_id, &bob, vec![&alice], None).await;
    setup.send_message(chat_id, &alice, vec![&bob], None).await;

    // A commit touching both legs after the resync shows that they are at compatible epochs.
    let alice_user = &setup.get_user(&alice).user;
    if setup.apq_groups {
        alice_user.update_apq_key(chat_id).await.unwrap();
    } else {
        alice_user.update_key(chat_id).await.unwrap();
    }
    let bob_user = &setup.get_user(&bob).user;
    let qs_messages = bob_user.qs_fetch_messages().await.unwrap();
    let result = bob_user.fully_process_qs_messages(qs_messages).await;
    assert!(
        result.errors.is_empty(),
        "Bob should process Alice's follow-up commit without errors: {:?}",
        result.errors
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[tracing::instrument(name = "Sanity checks for targeted message connections", skip_all)]
async fn sanity_checks_for_targeted_message_connections() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let bob = setup.add_user().await;
    let charlie = setup.add_user().await;

    // Alice is connected to Bob and Charlie, but Bob and Charlie are not connected.
    setup.connect_users(&alice, &bob).await;
    setup.connect_users(&alice, &charlie).await;

    // Alice creates a group and invites Bob and Charlie
    let group_chat_id = setup.create_group(&alice).await;
    setup
        .invite_to_group(group_chat_id, &alice, vec![&bob, &charlie])
        .await;

    // Alice shouldn't be able to add Bob as a contact from the group, since they are already connected.
    let alice = setup.get_user(&alice);
    let alice_user = &alice.user;
    let res = alice_user
        .add_contact_from_group(group_chat_id, bob.clone(), setup.apq_groups)
        .await;
    assert!(
        res.is_err(),
        "Alice should not be able to add Bob as a contact from the group since they are already connected"
    );

    // Bob now connects to Charlie via a targeted message sent through the
    // shared group.
    let bob = setup.get_user(&bob);
    let bob_user = &bob.user;
    bob_user
        .add_contact_from_group(group_chat_id, charlie.clone(), setup.apq_groups)
        .await
        .unwrap();

    // Bob shouldn't be able to add Charlie again.
    let res = bob_user
        .add_contact_from_group(group_chat_id, charlie.clone(), setup.apq_groups)
        .await;
    assert!(
        res.is_err(),
        "Bob should not be able to add Charlie again as a contact from the group"
    );
}

/// Test that the timestamp on a received connection request reflects when the
/// request was sent (server's enqueue time), not when the recipient processed it.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[tracing::instrument(name = "Connection request timestamp test", skip_all)]
async fn connection_request_has_server_timestamp() {
    let mut setup = TestBackend::single().await;
    let apq_groups = setup.apq_groups;

    let alice = setup.add_user().await;
    let bob = setup.add_user().await;

    // Bob adds a username
    let test_bob = setup.get_user_mut(&bob);
    let bob_username_record = test_bob.add_username().await.unwrap();
    let bob_username = bob_username_record.username.clone();

    // Alice sends a connection request to Bob
    let test_alice = setup.get_user_mut(&alice);
    let alice_user = &mut test_alice.user;
    let username_hash = spawn_blocking({
        let username = bob_username.clone();
        move || username.calculate_hash().unwrap()
    })
    .await
    .unwrap();

    alice_user
        .add_contact(bob_username.clone(), username_hash, apq_groups)
        .await
        .expect("fatal error")
        .expect("non-fatal error");

    // Bob fetches and processes the connection request
    let test_bob = setup.get_user_mut(&bob);
    let bob_user = &mut test_bob.user;
    let (mut stream, responder) = bob_user
        .listen_username(&bob_username_record)
        .await
        .unwrap();

    // Process handle queue messages, extracting the server timestamp before processing
    let mut bob_chat_id = None;
    let mut server_timestamp = None;
    while let Some(Some(message)) = tokio::time::timeout(Duration::from_millis(500), stream.next())
        .await
        .unwrap()
    {
        let message_id = message.message_id.unwrap();

        // Extract the server's created_at timestamp from the message
        let created_at = message
            .created_at
            .as_ref()
            .expect("Message should have created_at timestamp");
        server_timestamp = Some(TimeStamp::from(
            chrono::Utc
                .timestamp_opt(created_at.seconds, created_at.nanos as u32)
                .single()
                .expect("Valid timestamp"),
        ));

        let chat_id = bob_user
            .process_username_queue_message(bob_username_record.username.clone(), message)
            .await
            .unwrap();
        bob_chat_id = Some(chat_id);
        responder.ack(message_id.into()).await;
    }

    let bob_chat_id = bob_chat_id.expect("Bob should have processed at least one message");
    let server_timestamp = server_timestamp.expect("Should have captured server timestamp");

    // Get the system message and its timestamp
    let messages = bob_user.messages(bob_chat_id, 1).await.unwrap();
    let received_request_message = messages.first().expect("Should have at least one message");

    let Message::Event(EventMessage::System(SystemMessage::ReceivedHandleConnectionRequest {
        sender,
        user_handle,
    })) = received_request_message.message()
    else {
        panic!("Expected ReceivedHandleConnectionRequest system message");
    };

    assert_eq!(
        *sender, alice,
        "System message should indicate connection from Alice"
    );
    assert_eq!(
        *user_handle, bob_username,
        "System message should have the correct username"
    );

    // The system message timestamp should exactly match the server's created_at timestamp
    let message_timestamp = received_request_message.timestamp();
    let server_timestamp_chrono: chrono::DateTime<chrono::Utc> = server_timestamp.into();

    assert_eq!(
        message_timestamp, server_timestamp_chrono,
        "System message timestamp should match the server's created_at timestamp exactly"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[tracing::instrument(name = "DS records the connection group joiner", skip_all)]
async fn ds_room_state_contains_connection_group_joiner() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let bob = setup.add_user().await;
    let chat_id = setup.connect_users(&alice, &bob).await;

    let users = setup
        .get_user(&bob)
        .user
        .ds_room_state_users(chat_id)
        .await
        .unwrap();

    assert_eq!(
        users,
        [alice, bob].into_iter().collect(),
        "the DS room state should list both users of the connection group"
    );
}
