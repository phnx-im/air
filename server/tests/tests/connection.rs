// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::time::Duration;

use airapiclient::{ApiClient, as_api::AsRequestError};
use aircommon::{identifiers::UsernameHash, messages::client_as::SerializedToken, time::TimeStamp};
use aircoreclient::{ChatId, ChatType, EventMessage, Message, SystemMessage, clients::CoreUser};
use airprotos::{
    auth_service::v1::OperationType,
    client::signed_connection_package::{AnyConnectionPackage, AnyConnectionPackageIn},
};
use airserver_test_harness::utils::{core_user_ext::CoreUserExt, setup::TestBackend};
use chrono::{TimeZone, Utc};
use tokio::task::spawn_blocking;
use tokio_stream::StreamExt;

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn connect_users_via_user_handle() {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    let chat_id = setup.connect_users(&alice, &bob).await;

    let username = setup.username_record(&bob).unwrap().username;

    for (user, contact) in [(&alice, &bob), (&bob, &alice)] {
        let contacts: Vec<_> = user
            .contacts()
            .await
            .unwrap()
            .into_iter()
            .map(|contact| contact.user_id)
            .collect();
        assert_eq!(&contacts, std::slice::from_ref(contact.user_id()));
        let chat = user.expect_chat(chat_id).await;
        assert_eq!(
            chat.chat_type(),
            &ChatType::Connection(contact.user_id().clone())
        );
        assert_eq!(user.unread_messages_count(chat_id).await, 0);
    }

    assert_eq!(
        alice.system_messages(chat_id).await,
        [
            SystemMessage::NewHandleConnectionChat(username.clone()),
            SystemMessage::ReceivedConnectionConfirmation {
                sender: bob.user_id().clone(),
                user_handle: Some(username.clone()),
            },
        ]
    );
    assert_eq!(
        bob.system_messages(chat_id).await,
        [
            SystemMessage::ReceivedHandleConnectionRequest {
                sender: alice.user_id().clone(),
                user_handle: username.clone(),
            },
            SystemMessage::AcceptedConnectionRequest {
                contact: alice.user_id().clone(),
                user_handle: Some(username),
            },
        ]
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn connection_request_spends_a_token() {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    let record = setup.add_username(&bob).await;

    let before = alice
        .cached_privacy_pass_tokens(OperationType::ConnectUsername)
        .await
        .unwrap();
    assert_eq!(
        before.len(),
        usize::from(OperationType::ConnectUsername.max_tokens_allowance()),
        "registration fetches the full batch"
    );

    setup.connect_users(&alice, &bob).await;

    let after = alice
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
    let pending = alice.pending_redeemed_token_broadcasts().await.unwrap();
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
async fn connect_users_via_user_handle_uses_signed_package() {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;

    let record = setup.add_username(&bob).await;

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
async fn signed_connection_package_upload_task_replenishes_username() -> anyhow::Result<()> {
    let mut setup = TestBackend::single().await;
    let bob = setup.add_user().await;
    let record = setup.add_username(&bob).await;
    let client = ApiClient::with_endpoint(&setup.server_url())?;

    // Use up the packages published on creation, so that new ones are observable.
    drain_connection_packages(&client, record.hash).await?;
    assert!(
        fetch_connection_package(&client, record.hash)
            .await?
            .is_last_resort()
    );

    // Run the one-shot task as if the username predated signed connection packages.
    bob.outbound_service()
        .schedule_signed_connection_package_upload(vec![record.hash], Utc::now())
        .await?;
    bob.outbound_service().run_once().await;

    let state = bob
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
async fn signed_connection_package_upload_task_retries_pending_username() -> anyhow::Result<()> {
    let mut setup = TestBackend::single().await;
    let bob = setup.add_user().await;
    let record = setup.add_username(&bob).await;

    // The upload fails with a network error.
    setup.listener_control_handle().set_drop_all();
    bob.outbound_service()
        .schedule_signed_connection_package_upload(vec![record.hash], Utc::now())
        .await?;
    bob.outbound_service().run_once().await;
    setup.listener_control_handle().set_normal();

    let state = bob
        .outbound_service()
        .signed_connection_package_upload_state()
        .await?
        .expect("task should exist");
    assert!(!state.parked, "failed task must be retried");
    assert_eq!(state.pending_usernames, vec![record.hash]);

    // The retry succeeds.
    bob.outbound_service()
        .schedule_signed_connection_package_upload(state.pending_usernames, Utc::now())
        .await?;
    bob.outbound_service().run_once().await;

    let state = bob
        .outbound_service()
        .signed_connection_package_upload_state()
        .await?
        .expect("task should exist");
    assert!(state.parked);
    assert!(state.pending_usernames.is_empty());

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn signed_connection_package_upload_task_drops_unknown_username() -> anyhow::Result<()> {
    let mut setup = TestBackend::single().await;
    let bob = setup.add_user().await;

    // A username which does not exist locally is treated as done.
    bob.outbound_service()
        .schedule_signed_connection_package_upload(vec![UsernameHash::new([7; 32])], Utc::now())
        .await?;
    bob.outbound_service().run_once().await;

    let state = bob
        .outbound_service()
        .signed_connection_package_upload_state()
        .await?
        .expect("task should exist");
    assert!(state.parked);
    assert!(state.pending_usernames.is_empty());

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn connect_users_via_targeted_message() {
    let mut setup = TestBackend::single().await;
    let [alice, bob, charlie] = setup.add_users().await;

    // Alice is connected to Bob and Charlie, but Bob and Charlie are not connected.
    setup.connect_users(&alice, &bob).await;
    setup.connect_users(&alice, &charlie).await;

    // Alice creates a group and invites Bob and Charlie
    let group_chat_id = setup.create_group(&alice).await;
    setup
        .invite_to_group(group_chat_id, &alice, &[&bob, &charlie])
        .await;

    // Bob now connects to Charlie via a targeted message sent through the
    // shared group.
    let bob_chat_id = bob
        .add_contact_from_group(group_chat_id, charlie.user_id().clone(), setup.apq_groups)
        .await
        .unwrap();

    // Bob should have the right system message in the chat
    let chat_message = bob.messages(bob_chat_id, 1).await.unwrap().pop().unwrap();
    let Message::Event(EventMessage::System(SystemMessage::SentGroupConnectionRequest {
        recipient,
        origin_chat_id,
    })) = chat_message.message()
    else {
        panic!("Expected SentGroupConnectionRequest system message");
    };
    assert!(
        *recipient == *charlie.user_id(),
        "System message should indicate connection to Charlie"
    );
    assert_eq!(*origin_chat_id, group_chat_id);

    // Charlie picks up his messages
    let mut result = charlie.drain().await;

    // Charlie accepts the connection request
    charlie
        .accept_contact_request(bob_chat_id)
        .await
        .unwrap()
        .unwrap();

    // Charlie should have two messages in the new chat
    let charlie_chat_id = result.new_connections.pop().unwrap();
    let messages = charlie.messages(charlie_chat_id, 2).await.unwrap();
    let Message::Event(EventMessage::System(SystemMessage::ReceivedGroupConnectionRequest {
        sender,
        origin_chat_id,
    })) = messages[0].message()
    else {
        panic!("Expected ReceivedGroupConnectionRequest system message");
    };
    assert_eq!(
        *sender,
        *bob.user_id(),
        "System message should indicate connection from Bob"
    );
    assert_eq!(
        *origin_chat_id, group_chat_id,
        "System message should reference the group chat"
    );
    let Message::Event(EventMessage::System(SystemMessage::AcceptedConnectionRequest {
        contact,
        user_handle: None,
    })) = messages[1].message()
    else {
        panic!("Expected AcceptedConnectionRequest system message");
    };
    assert_eq!(
        *contact,
        *bob.user_id(),
        "System message should indicate acceptance of connection from Bob"
    );

    // Now Bob picks up his messages
    bob.drain().await;
    // Bob should have a system message indicating that Charlie accepted the connection
    let messages = bob.messages(bob_chat_id, 1).await.unwrap();
    let Message::Event(EventMessage::System(SystemMessage::ReceivedConnectionConfirmation {
        sender,
        user_handle: None,
    })) = messages[0].message()
    else {
        panic!("Expected ReceivedConnectionConfirmation system message");
    };
    assert!(
        *sender == *charlie.user_id(),
        "System message should indicate acceptance of connection from Charlie"
    );

    // Bob and Charlie should now be connected
    let bob_contact = bob.contact(charlie.user_id()).await;
    assert!(
        bob_contact.is_some(),
        "Bob should have Charlie as a contact"
    );
    let charlie_contact = charlie.contact(bob.user_id()).await;
    assert!(
        charlie_contact.is_some(),
        "Charlie should have Bob as a contact"
    );

    // The connection group is APQ iff requested, and both sides agree on it.
    assert_eq!(bob.chat_is_apq(bob_chat_id).await, Some(setup.apq_groups));
    assert_eq!(
        charlie.chat_is_apq(charlie_chat_id).await,
        Some(setup.apq_groups)
    );
}

/// Connects `initiator` to `peer` through the shared group and returns the connection chat as
/// seen by the initiator and by the peer.
async fn connect_from_group(
    group_chat_id: ChatId,
    initiator: &CoreUser,
    peer: &CoreUser,
    prefer_apq: bool,
) -> (ChatId, ChatId) {
    let initiator_chat_id = initiator
        .add_contact_from_group(group_chat_id, peer.user_id().clone(), prefer_apq)
        .await
        .unwrap();

    let mut result = peer.drain().await;
    let peer_chat_id = result.new_connections.pop().unwrap();
    peer.accept_contact_request(peer_chat_id)
        .await
        .unwrap()
        .unwrap();

    initiator.drain().await;

    assert!(initiator.contact(peer.user_id()).await.is_some());
    assert!(peer.contact(initiator.user_id()).await.is_some());
    (initiator_chat_id, peer_chat_id)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn connect_users_via_user_handle_apq() {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    // Asserts the APQ-ness on both sides and exchanges messages both ways.
    setup.connect_users_apq(&alice, &bob).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn connect_users_via_user_handle_not_requested_stays_t() {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    setup.connect_users_non_apq(&alice, &bob).await;
}

/// The connection request rides the APQ origin group as a T targeted message, and the connection
/// group itself is APQ.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn connect_users_via_targeted_message_apq() {
    let mut setup = TestBackend::single().await;
    let [alice, bob, charlie] = setup.add_users().await;

    let group_chat_id = setup.connected_apq_group(&alice, &[&bob, &charlie]).await;

    let (bob_chat_id, charlie_chat_id) =
        connect_from_group(group_chat_id, &bob, &charlie, true).await;

    assert_eq!(bob.chat_is_apq(bob_chat_id).await, Some(true));
    assert_eq!(charlie.chat_is_apq(charlie_chat_id).await, Some(true));

    setup.send_message(bob_chat_id, &bob, &[&charlie]).await;
    setup.send_message(bob_chat_id, &charlie, &[&bob]).await;
}

/// A connection group is resynced with the same machinery as any other group.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn resync_connection_group() {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    let chat_id = setup.connect_users(&alice, &bob).await;

    bob.enqueue_group_resync(chat_id).await.unwrap();
    bob.outbound_service().run_once().await;
    assert!(
        bob.resync_status(chat_id).await.unwrap().is_none(),
        "resync should have completed"
    );

    alice.drain().await;

    setup.send_message(chat_id, &bob, &[&alice]).await;
    setup.send_message(chat_id, &alice, &[&bob]).await;

    // A commit touching both legs after the resync shows that they are at compatible epochs.
    if setup.apq_groups {
        alice.update_apq_key(chat_id).await.unwrap();
    } else {
        alice.update_key(chat_id).await.unwrap();
    }
    bob.drain().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn sanity_checks_for_targeted_message_connections() {
    let mut setup = TestBackend::single().await;
    let [alice, bob, charlie] = setup.add_users().await;

    // Alice is connected to Bob and Charlie, but Bob and Charlie are not connected.
    setup.connect_users(&alice, &bob).await;
    setup.connect_users(&alice, &charlie).await;

    // Alice creates a group and invites Bob and Charlie
    let group_chat_id = setup.create_group(&alice).await;
    setup
        .invite_to_group(group_chat_id, &alice, &[&bob, &charlie])
        .await;

    // Alice shouldn't be able to add Bob as a contact from the group, since they are already connected.
    let res = alice
        .add_contact_from_group(group_chat_id, bob.user_id().clone(), setup.apq_groups)
        .await;
    assert!(
        res.is_err(),
        "Alice should not be able to add Bob as a contact from the group since they are already connected"
    );

    // Bob now connects to Charlie via a targeted message sent through the
    // shared group.
    bob.add_contact_from_group(group_chat_id, charlie.user_id().clone(), setup.apq_groups)
        .await
        .unwrap();

    // Bob shouldn't be able to add Charlie again.
    let res = bob
        .add_contact_from_group(group_chat_id, charlie.user_id().clone(), setup.apq_groups)
        .await;
    assert!(
        res.is_err(),
        "Bob should not be able to add Charlie again as a contact from the group"
    );
}

/// Test that the timestamp on a received connection request reflects when the
/// request was sent (server's enqueue time), not when the recipient processed it.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn connection_request_has_server_timestamp() {
    let mut setup = TestBackend::single().await;
    let apq_groups = setup.apq_groups;

    let [alice, bob] = setup.add_users().await;

    // Bob adds a username
    let bob_username_record = setup.add_username(&bob).await;
    let bob_username = bob_username_record.username.clone();

    // Alice sends a connection request to Bob
    let username_hash = spawn_blocking({
        let username = bob_username.clone();
        move || username.calculate_hash().unwrap()
    })
    .await
    .unwrap();

    alice
        .add_contact(bob_username.clone(), username_hash, apq_groups)
        .await
        .expect("fatal error")
        .expect("non-fatal error");

    // Bob fetches and processes the connection request
    let (mut stream, responder) = bob.listen_username(&bob_username_record).await.unwrap();

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

        let chat_id = bob
            .process_username_queue_message(bob_username_record.username.clone(), message)
            .await
            .unwrap()
            .map(|stored| stored.chat_id);
        bob_chat_id = chat_id.or(bob_chat_id);
        responder.ack(message_id.into()).await;
    }

    let bob_chat_id = bob_chat_id.expect("Bob should have processed at least one message");
    let server_timestamp = server_timestamp.expect("Should have captured server timestamp");

    // Get the system message and its timestamp
    let messages = bob.messages(bob_chat_id, 1).await.unwrap();
    let received_request_message = messages.first().expect("Should have at least one message");

    let Message::Event(EventMessage::System(SystemMessage::ReceivedHandleConnectionRequest {
        sender,
        user_handle,
    })) = received_request_message.message()
    else {
        panic!("Expected ReceivedHandleConnectionRequest system message");
    };

    assert_eq!(
        *sender,
        *alice.user_id(),
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
async fn ds_room_state_contains_connection_group_joiner() {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    let chat_id = setup.connect_users(&alice, &bob).await;

    let users = bob.ds_room_state_users(chat_id).await.unwrap();

    assert_eq!(
        users,
        [alice.user_id().clone(), bob.user_id().clone()]
            .into_iter()
            .collect(),
        "the DS room state should list both users of the connection group"
    );
}
