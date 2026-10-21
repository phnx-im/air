// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Connecting users via a username handle (T and APQ), and handle lookup
//! errors.

use std::{assert_matches, collections::BTreeSet, time::Duration};

use airapiclient::{ApiClient, as_api::AsRequestError};
use aircommon::{identifiers::Username, messages::client_as::SerializedToken, time::TimeStamp};
use aircoreclient::{AddUsernameContactError, ChatType, EventMessage, Message, SystemMessage};
use airprotos::auth_service::v1::OperationType;
use airserver_test_harness::utils::{core_user_ext::CoreUserExt, setup::TestBackend};
use chrono::TimeZone;
use tokio::task::spawn_blocking;
use tokio_stream::StreamExt;

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn connect_creates_contacts_and_system_messages() {
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
async fn connection_request_spends_token() {
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
async fn connect_creates_contacts_and_system_messages_apq() {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    // Asserts the APQ-ness on both sides and exchanges messages both ways.
    setup.connect_users_apq(&alice, &bob).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn connect_without_apq_request_stays_t() {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    setup.connect_users_non_apq(&alice, &bob).await;
}

/// Test that the timestamp on a received connection request reflects when the
/// request was sent (server's enqueue time), not when the recipient processed it.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn request_message_uses_server_timestamp() {
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
async fn ds_room_state_lists_both_connection_users() {
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

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn unknown_username_request_is_free() {
    let mut setup = TestBackend::single().await;

    let alice = setup.add_user().await;

    let username = Username::new("non-existent".to_owned()).unwrap();
    let hash = username.calculate_hash().unwrap();
    let tokens_before: BTreeSet<_> = alice
        .cached_privacy_pass_tokens(OperationType::ConnectUsername)
        .await
        .unwrap()
        .into_iter()
        .collect();

    let res = alice
        .add_contact(username, hash, setup.apq_groups)
        .await
        .unwrap();

    assert_matches!(res, Err(AddUsernameContactError::UsernameNotFound));

    // The AS did not redeem the token, so the request costs nothing.
    let tokens_after: BTreeSet<_> = alice
        .cached_privacy_pass_tokens(OperationType::ConnectUsername)
        .await
        .unwrap()
        .into_iter()
        .collect();
    assert_eq!(tokens_after, tokens_before);
    assert!(
        alice
            .pending_redeemed_token_broadcasts()
            .await
            .unwrap()
            .is_empty()
    );
}
