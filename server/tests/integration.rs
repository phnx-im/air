// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{collections::HashSet, fs, io::Cursor, slice, time::Duration};

use airapiclient::{ApiClient, as_api::AsRequestError, qs_api::QsRequestError};
use airbackend::settings::RateLimitsSettings;
use airprotos::{
    auth_service::v1::auth_service_server,
    common::v1::{StatusDetails, StatusDetailsCode},
    delivery_service::v1::delivery_service_server,
    queue_service::v1::queue_service_server,
};
use base64::{Engine, prelude::BASE64_STANDARD};
use chrono::Utc;
use image::{ImageBuffer, Rgba};
use mimi_content::{MessageStatus, MimiContent, content_container::NestedPartContent};
use rand::{Rng, distributions::Alphanumeric, rngs::OsRng};

use aircommon::{
    assert_matches,
    credentials::keys::HandleSigningKey,
    identifiers::{QsClientId, UserHandle, UserId},
    mls_group_config::MAX_PAST_EPOCHS,
};
use aircoreclient::{
    AddHandleContactError, AddHandleContactResult, Asset, AttachmentProgressEvent,
    BlockedContactError, ChatId, ChatMessage, DisplayName, EventMessage, Message, SystemMessage,
    UserProfile,
    clients::{
        CoreUser,
        process::process_qs::{ProcessedQsMessages, QsNotificationProcessor, QsStreamProcessor},
    },
    outbound_service::KEY_PACKAGES,
    store::Store,
};
use airserver_test_harness::utils::setup::{TestBackend, TestUser};
use png::Encoder;
use semver::VersionReq;
use sha2::{Digest, Sha256};
use tokio::{task::JoinSet, time::sleep};
use tokio_stream::StreamExt;
use tonic::transport::Channel;
use tonic_health::pb::{
    HealthCheckRequest, health_check_response::ServingStatus, health_client::HealthClient,
};
use tracing::{info, warn};
use tracing_subscriber::EnvFilter;

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[tracing::instrument(name = "Connect users test", skip_all)]
async fn connect_users_via_user_handle() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let bob = setup.add_user().await;
    setup.connect_users(&alice, &bob).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[tracing::instrument(name = "Send message test", skip_all)]
async fn send_message() {
    info!("Setting up setup");
    let mut setup = TestBackend::single().await;
    info!("Creating users");
    let alice = setup.add_user().await;
    let bob = setup.add_user().await;
    let chat_id = setup.connect_users(&alice, &bob).await;
    setup.send_message(chat_id, &alice, vec![&bob]).await;
    setup.send_message(chat_id, &bob, vec![&alice]).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[tracing::instrument(name = "Rate limit test", skip_all)]
async fn rate_limit() {
    init_test_tracing();

    let mut setup = TestBackend::single_with_params(
        Some(RateLimitsSettings {
            period: Duration::from_secs(1), // replenish one token every 500ms
            burst: 30,                      // allow total 30 request
        }),
        None,
    )
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
#[tracing::instrument(name = "Create group test", skip_all)]
async fn create_group() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    setup.create_group(&alice).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[tracing::instrument(name = "Invite to group test", skip_all)]
async fn invite_to_group() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let bob = setup.add_user().await;
    let charlie = setup.add_user().await;
    setup.connect_users(&alice, &bob).await;
    setup.connect_users(&alice, &charlie).await;
    let chat_id = setup.create_group(&alice).await;
    setup
        .invite_to_group(chat_id, &alice, vec![&bob, &charlie])
        .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[tracing::instrument(name = "Invite to group test", skip_all)]
async fn update_group() {
    let mut setup = TestBackend::single().await;
    info!("Adding users");
    let alice = setup.add_user().await;
    let bob = setup.add_user().await;
    let charlie = setup.add_user().await;
    info!("Connecting users");
    setup.connect_users(&alice, &bob).await;
    setup.connect_users(&alice, &charlie).await;
    let chat_id = setup.create_group(&alice).await;
    info!("Inviting to group");
    setup
        .invite_to_group(chat_id, &alice, vec![&bob, &charlie])
        .await;
    info!("Updating group");
    setup.update_group(chat_id, &bob).await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[tracing::instrument(name = "Remove from group test", skip_all)]
async fn remove_from_group() {
    let mut setup = TestBackend::single().await;

    let alice = setup.add_user().await;
    let bob = setup.add_user().await;
    let charlie = setup.add_user().await;
    let dave = setup.add_user().await;

    setup.connect_users(&alice, &bob).await;
    setup.connect_users(&alice, &charlie).await;
    setup.connect_users(&alice, &dave).await;
    let chat_id = setup.create_group(&alice).await;
    setup
        .invite_to_group(chat_id, &alice, vec![&bob, &charlie, &dave])
        .await;
    // Check that Charlie has a user profile stored for bob, even though
    // he hasn't connected with them.
    let charlie_user = &setup.get_user(&charlie).user;
    let charlie_user_profile_bob = charlie_user.user_profile(&bob).await;
    assert!(charlie_user_profile_bob.user_id == bob);

    setup
        .remove_from_group(chat_id, &alice, vec![&bob])
        .await
        .unwrap();

    // Now that charlie is not in a group with Bob anymore, the user profile
    // should be the default one derived from the client id.
    let charlie_user = &setup.get_user(&charlie).user;
    let charlie_user_profile_bob = charlie_user.user_profile(&bob).await;
    assert_eq!(charlie_user_profile_bob, UserProfile::from_user_id(&bob));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[tracing::instrument(name = "Re-add to group test", skip_all)]
async fn re_add_client() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let bob = setup.add_user().await;
    setup.connect_users(&alice, &bob).await;
    let chat_id = setup.create_group(&alice).await;
    setup.invite_to_group(chat_id, &alice, vec![&bob]).await;
    for _ in 0..10 {
        setup
            .remove_from_group(chat_id, &alice, vec![&bob])
            .await
            .unwrap();
        setup.invite_to_group(chat_id, &alice, vec![&bob]).await;
    }
    setup.send_message(chat_id, &alice, vec![&bob]).await;
    setup.send_message(chat_id, &bob, vec![&alice]).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[tracing::instrument(name = "Invite to group test", skip_all)]
async fn leave_group() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let bob = setup.add_user().await;
    setup.connect_users(&alice, &bob).await;
    let chat_id = setup.create_group(&alice).await;
    setup.invite_to_group(chat_id, &alice, vec![&bob]).await;
    setup.leave_group(chat_id, &bob).await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[tracing::instrument(name = "Invite to group test", skip_all)]
async fn delete_group() {
    init_test_tracing();

    let mut setup = TestBackend::single().await;

    let alice = setup.add_user().await;
    let bob = setup.add_user().await;
    setup.connect_users(&alice, &bob).await;
    let chat_id = setup.create_group(&alice).await;
    setup.invite_to_group(chat_id, &alice, vec![&bob]).await;
    let delete_group = setup.delete_group(chat_id, &alice);
    delete_group.await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[tracing::instrument(name = "Create user", skip_all)]
async fn create_user() {
    let mut setup = TestBackend::single().await;
    setup.add_user().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[tracing::instrument(name = "Communication and persistence", skip_all)]
async fn communication_and_persistence() {
    let mut setup = TestBackend::single().await;

    // Create alice and bob
    let alice = setup.add_user().await;
    let bob = setup.add_user().await;

    // Connect them
    let chat_alice_bob = setup.connect_users(&alice, &bob).await;

    // Test the connection chat by sending messages back and forth.
    setup.send_message(chat_alice_bob, &alice, vec![&bob]).await;
    setup.send_message(chat_alice_bob, &bob, vec![&alice]).await;

    let count_18 = setup
        .scan_database("\x18", true, vec![&alice, &bob])
        .await
        .len();
    let count_19 = setup
        .scan_database("\x19", true, vec![&alice, &bob])
        .await
        .len();

    let good = count_18 < count_19 * 3 / 2;

    // TODO: Remove the ! in front of !good when we have fixed our code.
    assert!(
        !good,
        "Having too many 0x18 is an indicator for using Vec<u8> instead of ByteBuf"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[tracing::instrument(name = "Edit message", skip_all)]
async fn edit_message() {
    let mut setup = TestBackend::single().await;

    // Create alice and bob
    let alice = setup.add_user().await;
    let bob = setup.add_user().await;

    // Connect them
    let chat_alice_bob = setup.connect_users(&alice, &bob).await;

    setup.send_message(chat_alice_bob, &alice, vec![&bob]).await;

    setup.edit_message(chat_alice_bob, &alice, vec![&bob]).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[tracing::instrument(name = "Delete message", skip_all)]
async fn delete_message() {
    let mut setup = TestBackend::single().await;

    // Create alice and bob
    let alice = setup.add_user().await;
    let bob = setup.add_user().await;

    // Connect them
    let chat_alice_bob = setup.connect_users(&alice, &bob).await;

    setup.send_message(chat_alice_bob, &alice, vec![&bob]).await;

    let alice_user = &setup.get_user(&alice).user;
    let last_message = alice_user
        .last_message(chat_alice_bob)
        .await
        .unwrap()
        .unwrap();

    let string = last_message
        .message()
        .mimi_content()
        .unwrap()
        .string_rendering()
        .unwrap();

    assert!(
        !setup
            .scan_database(&string, false, vec![&alice, &bob])
            .await
            .is_empty(),
    );

    setup
        .delete_message(chat_alice_bob, &alice, vec![&bob])
        .await;

    assert_eq!(
        setup
            .scan_database(&string, false, vec![&alice, &bob])
            .await,
        Vec::<String>::new()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[tracing::instrument(name = "Room policy", skip_all)]
async fn room_policy() {
    let mut setup = TestBackend::single().await;

    // Create alice and bob
    let alice = setup.add_user().await;
    let bob = setup.add_user().await;
    let charlie = setup.add_user().await;

    // Connect them
    let _chat_alice_bob = setup.connect_users(&alice, &bob).await;
    let _chat_alice_charlie = setup.connect_users(&alice, &charlie).await;
    let _chat_bob_charlie = setup.connect_users(&bob, &charlie).await;

    // Create an independent group and invite bob.
    let chat_id = setup.create_group(&alice).await;

    setup.invite_to_group(chat_id, &alice, vec![&bob]).await;

    // Bob can invite charlie
    setup.invite_to_group(chat_id, &bob, vec![&charlie]).await;

    // Charlie can kick alice
    setup
        .remove_from_group(chat_id, &charlie, vec![&alice])
        .await
        .unwrap();

    // Charlie can kick bob
    setup
        .remove_from_group(chat_id, &charlie, vec![&bob])
        .await
        .unwrap();

    // TODO: This currently fails
    // Charlie can leave and an empty room remains
    // setup.leave_group(chat_id, &charlie).await.unwrap();
}

fn test_picture_bytes() -> Vec<u8> {
    // Create a new ImgBuf with width: 1px and height: 1px
    let mut img = ImageBuffer::new(200, 200);

    // Put a single pixel in the image
    img.put_pixel(0, 0, Rgba([0u8, 0u8, 255u8, 255u8])); // Blue pixel

    // A Cursor for in-memory writing of bytes
    let mut buffer = Cursor::new(Vec::new());

    {
        // Create a new PNG encoder
        let mut encoder = Encoder::new(&mut buffer, 200, 200);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().unwrap();

        // Encode the image data.
        writer.write_image_data(&img).unwrap();
    }

    // Get the PNG data bytes
    buffer.into_inner()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[tracing::instrument(name = "User profile exchange test", skip_all)]
async fn exchange_user_profiles() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;

    // Set a user profile for alice
    let alice_display_name: DisplayName = "4l1c3".parse().unwrap();

    let png_bytes = test_picture_bytes();

    let alice_profile_picture = Asset::Value(png_bytes.clone());

    let alice_profile = UserProfile {
        user_id: alice.clone(),
        display_name: alice_display_name.clone(),
        profile_picture: Some(alice_profile_picture.clone()),
    };
    let alice_user = &setup.get_user(&alice).user;
    alice_user
        .set_own_user_profile(alice_profile)
        .await
        .unwrap();

    let bob = setup.add_user().await;

    // Set a user profile for
    let bob_display_name: DisplayName = "B0b".parse().unwrap();
    let bob_profile_picture = Asset::Value(png_bytes.clone());
    let bob_user_profile = UserProfile {
        user_id: bob.clone(),
        display_name: bob_display_name.clone(),
        profile_picture: Some(bob_profile_picture.clone()),
    };

    let bob_user = &setup.get_user(&bob).user;
    bob_user
        .set_own_user_profile(bob_user_profile)
        .await
        .unwrap();
    let new_profile = bob_user.own_user_profile().await.unwrap();
    let Asset::Value(compressed_profile_picture) = new_profile.profile_picture.unwrap().clone();

    setup.connect_users(&alice, &bob).await;

    let bob_user_profile = setup.get_user(&alice).user.user_profile(&bob).await;

    let profile_picture = bob_user_profile
        .profile_picture
        .unwrap()
        .clone()
        .value()
        .unwrap()
        .to_vec();

    assert_eq!(profile_picture, compressed_profile_picture);

    assert!(bob_user_profile.display_name == bob_display_name);

    let alice_user = &setup.get_user(&alice).user;

    let alice_user_profile = alice_user.user_profile(&alice).await;

    assert_eq!(alice_user_profile.display_name, alice_display_name);

    let new_user_profile = UserProfile {
        user_id: alice.clone(),
        display_name: "New Alice".parse().unwrap(),
        profile_picture: None,
    };

    alice_user
        .set_own_user_profile(new_user_profile.clone())
        .await
        .unwrap();

    let bob_user = &setup.get_user(&bob).user;
    let qs_messages = bob_user.qs_fetch_messages().await.unwrap();
    bob_user.fully_process_qs_messages(qs_messages).await;
    let alice_user_profile = bob_user.user_profile(&alice).await;

    assert_eq!(alice_user_profile, new_user_profile);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[tracing::instrument(name = "Message retrieval test", skip_all)]
async fn retrieve_chat_messages() {
    let mut setup = TestBackend::single().await;

    let alice = setup.add_user().await;
    let bob = setup.add_user().await;

    let chat_id = setup.connect_users(&alice, &bob).await;

    let alice_user = &setup.get_user(&alice).user;

    let number_of_messages = 10;
    let mut messages_sent = vec![];
    for _ in 0..number_of_messages {
        let message: String = OsRng
            .sample_iter(&Alphanumeric)
            .take(32)
            .map(char::from)
            .collect();
        let message_content = MimiContent::simple_markdown_message(message, [0; 16]); // simple seed for testing
        let message = alice_user
            .send_message(chat_id, message_content, None)
            .await
            .unwrap();
        messages_sent.push(message);
    }

    // Let's see what Alice's messages for this chat look like.
    let messages_retrieved = setup
        .get_user(&alice)
        .user
        .messages(chat_id, number_of_messages)
        .await
        .unwrap();

    assert_eq!(messages_retrieved.len(), messages_sent.len());
    assert_eq!(messages_retrieved, messages_sent);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[tracing::instrument(name = "Marking messages as read test", skip_all)]
async fn mark_as_read() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let bob = setup.add_user().await;
    let charlie = setup.add_user().await;

    let alice_bob_chat = setup.connect_users(&alice, &bob).await;
    let bob_charlie_chat = setup.connect_users(&bob, &charlie).await;

    // Send a few messages
    async fn send_messages(
        user: &CoreUser,
        chat_id: ChatId,
        number_of_messages: usize,
    ) -> Vec<ChatMessage> {
        let mut messages_sent = vec![];
        for _ in 0..number_of_messages {
            let message: String = OsRng
                .sample_iter(&Alphanumeric)
                .take(32)
                .map(char::from)
                .collect();
            let message_content = MimiContent::simple_markdown_message(message, [0; 16]); // simple seed for testing
            let message = user
                .send_message(chat_id, message_content, None)
                .await
                .unwrap();
            messages_sent.push(message);
        }
        user.outbound_service().run_once().await;
        messages_sent
    }

    let num_messages = 10;
    let alice_test_user = setup.get_user(&alice);
    let alice = &alice_test_user.user;
    send_messages(alice, alice_bob_chat, num_messages).await;

    // Message status starts at Unread
    let last_message = alice.last_message(alice_bob_chat).await.unwrap().unwrap();
    assert_eq!(last_message.status(), MessageStatus::Unread);

    // All messages should be unread
    let bob_test_user = setup.get_user(&bob);
    bob_test_user.fetch_and_process_qs_messages().await;
    let bob_user = &bob_test_user.user;
    let unread_message_count = bob_user.unread_messages_count(alice_bob_chat).await;
    assert_eq!(unread_message_count, num_messages);
    let global_unread_message_count = bob_user.global_unread_messages_count().await.unwrap();
    assert_eq!(global_unread_message_count, num_messages);

    // Bob sends scheduled receipts
    bob_user.outbound_service().run_once().await;

    // Alice sees the delivery receipt
    let num_processed = alice_test_user.fetch_and_process_qs_messages().await;
    assert_eq!(num_processed, 1);
    let last_message = alice.last_message(alice_bob_chat).await.unwrap().unwrap();
    assert_eq!(last_message.status(), MessageStatus::Delivered);

    // Bob reads the messages
    let last_message = bob_user
        .last_message(alice_bob_chat)
        .await
        .unwrap()
        .unwrap();
    let last_message_id = last_message.id();
    let last_message_mimi_id = last_message.message().mimi_id().unwrap();
    bob_user
        .outbound_service()
        .enqueue_receipts(
            alice_bob_chat,
            [(last_message_id, last_message_mimi_id, MessageStatus::Read)].into_iter(),
        )
        .await
        .unwrap();
    bob_user.outbound_service().run_once().await;

    // Alice sees the read receipt
    let num_processed = alice_test_user.fetch_and_process_qs_messages().await;
    assert_eq!(num_processed, 1);
    let last_message = alice.last_message(alice_bob_chat).await.unwrap().unwrap();
    assert_eq!(last_message.status(), MessageStatus::Read);

    // Let's send some messages between bob and charlie s.t. we can test the
    // global unread messages count.
    let charlie_test_user = setup.get_user(&charlie);
    let charlie = &charlie_test_user.user;
    let messages_sent = send_messages(charlie, bob_charlie_chat, num_messages).await;

    let qs_messages = bob_user.qs_fetch_messages().await.unwrap();
    let bob_messages_sent = bob_user.fully_process_qs_messages(qs_messages).await;

    // Let's mark all but the last two messages as read (we subtract 3, because
    // the vector is 0-indexed).
    let timestamp = bob_messages_sent.new_messages[messages_sent.len() - 3].timestamp();

    bob_user
        .mark_as_read([(bob_charlie_chat, timestamp)])
        .await
        .unwrap();

    // Check if we were successful
    let unread_message_count = bob_user.unread_messages_count(bob_charlie_chat).await;
    assert_eq!(unread_message_count, 2);

    // We expect the global unread messages count to be that of both
    // chats, i.e. the `expected_unread_message_count` plus
    // `number_of_messages`, because none of the messages between alice and
    // charlie had been read.
    let global_unread_messages_count = bob_user.global_unread_messages_count().await.unwrap();
    assert_eq!(global_unread_messages_count, num_messages + 2);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[tracing::instrument(name = "User persistence test", skip_all)]
async fn client_persistence() {
    // Create and persist the user.
    let mut setup = TestBackend::single().await;
    let alice = setup.add_persisted_user().await;

    let db_path = setup.temp_dir().to_owned();

    // Try to load the user from the database.
    CoreUser::load(alice.clone(), db_path.to_str().unwrap())
        .await
        .unwrap();

    let client_db_path = db_path.join(format!("{}@{}.db", alice.uuid(), alice.domain()));
    assert!(client_db_path.exists());

    setup.delete_user(&alice).await;

    assert!(!client_db_path.exists());
    assert!(
        CoreUser::load(alice.clone(), db_path.to_str().unwrap())
            .await
            .is_err()
    );

    // `CoreUser::load` opened the client DB, and so it was re-created.
    fs::remove_file(client_db_path).unwrap();
    fs::remove_file(db_path.join("air.db")).unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[tracing::instrument(name = "Test server error if unknown user", skip_all)]
async fn error_if_user_doesnt_exist() {
    let mut setup = TestBackend::single().await;

    let alice = setup.add_user().await;
    let alice_user = &setup.get_user(&alice).user;

    let res = alice_user
        .add_contact(UserHandle::new("non_existent".to_owned()).unwrap())
        .await
        .unwrap();

    assert!(matches!(
        res,
        AddHandleContactResult::Err(AddHandleContactError::HandleNotFound)
    ));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[tracing::instrument(name = "Delete user test", skip_all)]
async fn delete_user() {
    let mut setup = TestBackend::single().await;

    let alice = setup.add_user().await;
    // Adding another user with the same id should fail.
    match TestUser::try_new(&alice, setup.server_url()).await {
        Ok(_) => panic!("Should not be able to create a user with the same id"),
        Err(e) => match e.downcast_ref::<AsRequestError>().unwrap() {
            AsRequestError::Tonic(status) => {
                assert_eq!(status.code(), tonic::Code::AlreadyExists);
            }
            _ => panic!("Unexpected error type: {e}"),
        },
    }

    setup.delete_user(&alice).await;
    // After deletion, adding the user again should work.
    // Note: Since the user is ephemeral, there is nothing to test on the client side.
    TestUser::try_new(&alice, setup.server_url()).await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[tracing::instrument(name = "Update user profile on group join test", skip_all)]
async fn update_user_profile_on_group_join() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let bob = setup.add_user().await;
    let charlie = setup.add_user().await;

    // Alice and Bob are connected.
    let _alice_bob_chat = setup.connect_users(&alice, &bob).await;
    // Bob and Charlie are connected.
    let _bob_charlie_chat = setup.connect_users(&bob, &charlie).await;

    // Alice updates her profile.
    let alice_display_name: DisplayName = "4l1c3".parse().unwrap();
    let alice_profile = UserProfile {
        user_id: alice.clone(),
        display_name: alice_display_name.clone(),
        profile_picture: None,
    };
    setup
        .get_user(&alice)
        .user
        .set_own_user_profile(alice_profile)
        .await
        .unwrap();

    // Bob doesn't fetch his queue, so he doesn't know about Alice's new profile.
    // He creates a group and invites Charlie.
    let chat_id = setup.create_group(&bob).await;

    let bob_user = &setup.get_user(&bob).user;
    bob_user
        .invite_users(chat_id, slice::from_ref(&charlie))
        .await
        .unwrap();

    // Charlie accepts the invitation.
    let charlie_user = &setup.get_user(&charlie).user;
    let charlie_qs_messages = charlie_user.qs_fetch_messages().await.unwrap();
    charlie_user
        .fully_process_qs_messages(charlie_qs_messages)
        .await;

    // Bob now invites Alice
    let bob_user = &setup.get_user(&bob).user;
    bob_user
        .invite_users(chat_id, slice::from_ref(&alice))
        .await
        .unwrap();

    // Charlie processes his messages again, this will fail, because he will
    // unsuccessfully try to download Alice's old profile.
    let charlie_user = &setup.get_user(&charlie).user;
    let charlie_qs_messages = charlie_user.qs_fetch_messages().await.unwrap();
    let result = charlie_user
        .fully_process_qs_messages(charlie_qs_messages)
        .await;

    assert!(result.changed_chats.is_empty());
    assert!(result.new_chats.is_empty());
    assert!(result.new_messages.is_empty());
    let err = &result.errors[0];
    let AsRequestError::Tonic(tonic_err) = err.downcast_ref().unwrap() else {
        panic!("Unexpected error type");
    };
    assert_eq!(tonic_err.code(), tonic::Code::InvalidArgument);
    assert_eq!(tonic_err.message(), "No ciphertext matching index");

    // Alice accepts the invitation.
    let alice_user = &setup.get_user(&alice).user;
    let alice_qs_messages = alice_user.qs_fetch_messages().await.unwrap();
    alice_user
        .fully_process_qs_messages(alice_qs_messages)
        .await;

    // While processing her messages, Alice should have issued a profile update

    // Charlie picks up his messages.
    let charlie_user = &setup.get_user(&charlie).user;
    let charlie_qs_messages = charlie_user.qs_fetch_messages().await.unwrap();
    charlie_user
        .fully_process_qs_messages(charlie_qs_messages)
        .await;
    // Charlie should now have Alice's new profile.
    let charlie_user_profile = charlie_user.user_profile(&alice).await;
    assert_eq!(charlie_user_profile.display_name, alice_display_name);
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
#[tracing::instrument(name = "Send attachment test", skip_all)]
async fn send_attachment() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let bob = setup.add_user().await;
    let chat_id = setup.connect_users(&alice, &bob).await;

    let attachment = vec![0x00, 0x01, 0x02, 0x03];
    let (_message_id, external_part) = setup
        .send_attachment(chat_id, &alice, vec![&bob], &attachment, "test.bin")
        .await;

    let attachment_id = match &external_part {
        NestedPartContent::ExternalPart {
            content_type,
            url,
            filename,
            size,
            content_hash,
            ..
        } => {
            assert_eq!(content_type, "application/octet-stream");
            assert_eq!(filename, "test.bin");
            assert_eq!(*size, attachment.len() as u64);

            let sha256sum = Sha256::digest(&attachment);
            assert_eq!(sha256sum.as_slice(), content_hash.as_slice());

            url.parse().unwrap()
        }
        _ => panic!("unexpected attachment type"),
    };

    let bob_test_user = setup.get_user(&bob);
    let bob = &bob_test_user.user;

    let (progress, download_task) = bob.download_attachment(attachment_id);

    let progress_events = progress.stream().collect::<Vec<_>>();

    let (progress_events, res) = tokio::join!(progress_events, download_task);
    res.expect("Download task failed");

    assert_matches!(
        progress_events.first().unwrap(),
        AttachmentProgressEvent::Init
    );
    assert_matches!(
        progress_events.last().unwrap(),
        AttachmentProgressEvent::Completed
    );

    let content = bob
        .load_attachment(attachment_id)
        .await
        .unwrap()
        .into_bytes()
        .unwrap();
    match external_part {
        NestedPartContent::ExternalPart {
            size, content_hash, ..
        } => {
            assert_eq!(content.len() as u64, size);
            let sha256sum = Sha256::digest(&content);
            assert_eq!(sha256sum.as_slice(), content_hash.as_slice());
        }
        _ => panic!("unexpected attachment type"),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[tracing::instrument(name = "Send image attachment test", skip_all)]
async fn send_image_attachment() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let bob = setup.add_user().await;
    let chat_id = setup.connect_users(&alice, &bob).await;

    // A base64 encoded blue PNG image 100x75 pixels.
    const SAMPLE_PNG_BASE64: &str = "\
    iVBORw0KGgoAAAANSUhEUgAAAGQAAABLAQMAAAC81rD0AAAABGdBTUEAALGPC/xhBQAAACBjSFJN\
    AAB6JgAAgIQAAPoAAACA6AAAdTAAAOpgAAA6mAAAF3CculE8AAAABlBMVEUAAP7////DYP5JAAAA\
    AWJLR0QB/wIt3gAAAAlwSFlzAAALEgAACxIB0t1+/AAAAAd0SU1FB+QIGBcKN7/nP/UAAAASSURB\
    VDjLY2AYBaNgFIwCdAAABBoAAaNglfsAAAAZdEVYdGNvbW1lbnQAQ3JlYXRlZCB3aXRoIEdJTVDn\
    r0DLAAAAJXRFWHRkYXRlOmNyZWF0ZQAyMDIwLTA4LTI0VDIzOjEwOjU1KzAzOjAwkHdeuQAAACV0\
    RVh0ZGF0ZTptb2RpZnkAMjAyMC0wOC0yNFQyMzoxMDo1NSswMzowMOEq5gUAAAAASUVORK5CYII=";

    let attachment = BASE64_STANDARD.decode(SAMPLE_PNG_BASE64).unwrap();
    let (_message_id, external_part) = setup
        .send_attachment(chat_id, &alice, vec![&bob], &attachment, "test.png")
        .await;

    let alice = setup.get_user(&alice);
    alice.user.outbound_service().run_once().await;

    let attachment_id = match &external_part {
        NestedPartContent::ExternalPart {
            content_type,
            url,
            filename,
            size,
            content_hash,
            ..
        } => {
            assert_eq!(content_type, "image/webp");
            assert_eq!(filename, "test.webp");
            assert_eq!(*size, 100);
            assert_eq!(
                content_hash.as_slice(),
                hex::decode("c8cb184c4242c38c3bc8fb26c521377778d9038b9d7dd03f31b9be701269a673")
                    .unwrap()
                    .as_slice()
            );

            url.parse().unwrap()
        }
        _ => panic!("unexpected attachment type"),
    };

    let bob_test_user = setup.get_user(&bob);
    let bob = &bob_test_user.user;

    let (progress, download_task) = bob.download_attachment(attachment_id);

    let progress_events = progress.stream().collect::<Vec<_>>();

    let (progress_events, res) = tokio::join!(progress_events, download_task);
    res.expect("Download task failed");

    assert_matches!(
        progress_events.first().unwrap(),
        AttachmentProgressEvent::Init
    );
    assert_matches!(
        progress_events.last().unwrap(),
        AttachmentProgressEvent::Completed
    );

    let content = bob
        .load_attachment(attachment_id)
        .await
        .unwrap()
        .into_bytes()
        .unwrap();
    match external_part {
        NestedPartContent::ExternalPart {
            size, content_hash, ..
        } => {
            assert_eq!(content.len() as u64, size);
            let sha256sum = Sha256::digest(&content);
            assert_eq!(sha256sum.as_slice(), content_hash.as_slice());
        }
        _ => panic!("unexpected attachment type"),
    }
}

fn init_test_tracing() {
    let _ = tracing_subscriber::fmt::fmt()
        .with_test_writer()
        .with_env_filter(EnvFilter::from_default_env())
        .try_init();
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
        UserProfile::from_user_id(&charlie)
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[tracing::instrument(name = "Blocked contact", skip_all)]
async fn blocked_contact() {
    info!("Setting up setup");
    let mut setup = TestBackend::single().await;
    info!("Creating users");
    let alice = setup.add_user().await;
    setup.get_user_mut(&alice).add_user_handle().await.unwrap();
    info!("Created alice");
    let bob = setup.add_user().await;

    let chat_id = setup.connect_users(&alice, &bob).await;
    setup.send_message(chat_id, &alice, vec![&bob]).await;
    setup.send_message(chat_id, &bob, vec![&alice]).await;

    let alice_test_user = setup.get_user(&alice);
    let alice_user = &alice_test_user.user;
    let bob_test_user = setup.get_user(&bob);
    let bob_user = &bob_test_user.user;

    alice_user.block_contact(bob.clone()).await.unwrap();

    alice_test_user.fetch_and_process_qs_messages().await;
    bob_test_user.fetch_and_process_qs_messages().await;

    // Not possible to send a message to Bob
    let msg = MimiContent::simple_markdown_message("Hello".into(), [0; 16]);
    let res = alice_user.send_message(chat_id, msg.clone(), None).await;
    res.unwrap_err().downcast::<BlockedContactError>().unwrap();

    assert_eq!(bob_test_user.fetch_and_process_qs_messages().await, 0);

    // Updating Alice's profile is not communicated to Bob
    alice_user
        .update_user_profile(UserProfile {
            user_id: alice.clone(),
            display_name: "Alice in Wonderland".parse().unwrap(),
            profile_picture: None,
        })
        .await
        .unwrap();
    assert_eq!(bob_test_user.fetch_and_process_qs_messages().await, 0);

    // Updating Bob's profile is not communicated to Alice
    bob_user
        .update_user_profile(UserProfile {
            user_id: bob.clone(),
            display_name: "Annoying Bob".parse().unwrap(),
            profile_picture: None,
        })
        .await
        .unwrap();
    // We get the message but it is dropped
    let messages = alice_test_user.user.qs_fetch_messages().await.unwrap();
    assert_eq!(messages.len(), 1);
    let res = alice_user.fully_process_qs_messages(messages).await;
    assert!(res.is_empty(), "message is dropped");

    // Messages from bob are dropped
    bob_user.send_message(chat_id, msg, None).await.unwrap();
    bob_test_user.user.outbound_service().run_once().await;
    // We get the message but it is dropped
    let messages = alice_test_user.user.qs_fetch_messages().await.unwrap();
    assert_eq!(messages.len(), 1);
    let res = alice_test_user
        .user
        .fully_process_qs_messages(messages)
        .await;
    assert!(res.is_empty(), "message is dropped");

    // Bob cannot establish a new connection with Alice
    let alice_handle = alice_test_user
        .user_handle_record
        .as_ref()
        .unwrap()
        .handle
        .clone();
    bob_test_user
        .user
        .add_contact(alice_handle.clone())
        .await
        .unwrap();
    let mut messages = alice_test_user.user.fetch_handle_messages().await.unwrap();
    assert_eq!(messages.len(), 1);

    let res = alice_test_user
        .user
        .process_handle_queue_message(&alice_handle, messages.pop().unwrap())
        .await;
    res.unwrap_err().downcast::<BlockedContactError>().unwrap();

    // Unblock Bob
    alice_test_user
        .user
        .unblock_contact(bob.clone())
        .await
        .unwrap();

    // Sending messages works again
    setup.send_message(chat_id, &alice, vec![&bob]).await;
    setup.send_message(chat_id, &bob, vec![&alice]).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[tracing::instrument(name = "Group with blocked contacts", skip_all)]
async fn group_with_blocked_contact() {
    let mut setup = TestBackend::single().await;

    let alice = setup.add_user().await;
    setup.get_user_mut(&alice).add_user_handle().await.unwrap();

    let bob = setup.add_user().await;
    let charlie = setup.add_user().await;

    setup.connect_users(&alice, &bob).await;
    setup.connect_users(&alice, &charlie).await;

    // Create a group with alice, bob and charlie
    let chat_id = setup.create_group(&alice).await;
    setup
        .invite_to_group(chat_id, &alice, vec![&bob, &charlie])
        .await;

    // Sending messages works before blocking
    setup
        .send_message(chat_id, &alice, vec![&bob, &charlie])
        .await;
    setup
        .send_message(chat_id, &bob, vec![&alice, &charlie])
        .await;

    // Block bob
    let alice_user = &setup.get_user(&alice).user;
    alice_user.block_contact(bob.clone()).await.unwrap();

    // Messages are still sent and received
    setup
        .send_message(chat_id, &bob, vec![&alice, &charlie])
        .await;
    setup
        .send_message(chat_id, &alice, vec![&bob, &charlie])
        .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[tracing::instrument(name = "Delete account", skip_all)]
async fn delete_account() {
    let mut setup = TestBackend::single().await;

    let alice = setup.add_user().await;
    setup.get_user_mut(&alice).add_user_handle().await.unwrap();

    let bob = setup.add_user().await;

    let contact_chat_id = setup.connect_users(&alice, &bob).await;

    // Create a group with Alice and Bob
    let chat_id = setup.create_group(&alice).await;
    setup.invite_to_group(chat_id, &alice, vec![&bob]).await;

    // Delete the account
    let db_path = None;
    setup
        .get_user(&alice)
        .user
        .delete_account(db_path)
        .await
        .unwrap();

    // Check that Alice left the group
    let bob_user = &setup.get_user(&bob).user;
    let qs_messages = bob_user.qs_fetch_messages().await.unwrap();
    let result = bob_user.fully_process_qs_messages(qs_messages).await;
    assert!(result.errors.is_empty());

    let participants = setup
        .get_user(&bob)
        .user
        .chat_participants(contact_chat_id)
        .await
        .unwrap();
    assert_eq!(participants, [bob.clone()].into_iter().collect());

    let participants = setup
        .get_user(&bob)
        .user
        .chat_participants(chat_id)
        .await
        .unwrap();
    assert_eq!(participants, [bob.clone()].into_iter().collect());

    // After deletion, adding the user again should work.
    // Note: Since the user is ephemeral, there is nothing to test on the client side.
    let mut new_alice = TestUser::try_new(&alice, setup.server_url()).await.unwrap();
    // Adding a user handle to the new user should work, because the previous user handle was
    // deleted.
    new_alice.add_user_handle().await.unwrap();
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
    responder.ack(message.sequence_number + 1).await.unwrap();
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
    responder.ack(message.sequence_number + 1).await.unwrap();
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
#[tracing::instrument(name = "Update group data", skip_all)]
async fn update_group_data() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let bob = setup.add_user().await;
    let charlie = setup.add_user().await;

    let _alice_bob_chat = setup.connect_users(&alice, &bob).await;
    let _alice_charlie_chat = setup.connect_users(&alice, &charlie).await;

    // Alice creates a group and invites Bob and Charlie
    let chat_id = setup.create_group(&alice).await;
    setup
        .invite_to_group(chat_id, &alice, vec![&bob, &charlie])
        .await;

    // Alice updates the group picture
    let alice_user = &setup.get_user(&alice).user;
    let picture = test_picture_bytes();
    alice_user
        .set_chat_picture(chat_id, Some(picture.clone()))
        .await
        .unwrap();

    let expected_picture = alice_user
        .chat(&chat_id)
        .await
        .unwrap()
        .attributes
        .picture
        .unwrap()
        .clone();

    // Bob and Charlie should now have the updated group picture
    for user_id in [&bob, &charlie] {
        let user = &setup.get_user(user_id).user;
        // Fetch and process messages to get the update
        let qs_messages = user.qs_fetch_messages().await.unwrap();
        let result = user.fully_process_qs_messages(qs_messages).await;
        assert!(
            result.errors.is_empty(),
            "{:?} should process Alice's update without errors",
            user_id
        );
        let actual_picture = user
            .chat(&chat_id)
            .await
            .unwrap()
            .attributes
            .picture
            .unwrap()
            .clone();
        assert_eq!(actual_picture, expected_picture);
    }

    // Now Bob updates the group title
    let title = "New Group Title".to_string();
    let bob_user = &setup.get_user(&bob).user;
    bob_user
        .set_chat_title(chat_id, title.clone())
        .await
        .unwrap();

    for user_id in [&alice, &charlie] {
        let user = &setup.get_user_mut(user_id).user;
        // Fetch and process messages to get the update
        let qs_messages = user.qs_fetch_messages().await.unwrap();
        let result = user.fully_process_qs_messages(qs_messages).await;
        assert!(
            result.errors.is_empty(),
            "{:?} should process Bob's update without errors",
            user_id
        );
        let actual_title = user.chat(&chat_id).await.unwrap().attributes.title.clone();
        assert_eq!(actual_title, title);
    }
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
        .add_contact_from_group(group_chat_id, charlie.clone())
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
        group_chat.attributes().title,
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
        .add_contact_from_group(group_chat_id, bob.clone())
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
        .add_contact_from_group(group_chat_id, charlie.clone())
        .await
        .unwrap();

    // Bob shouldn't be able to add Charlie again.
    let res = bob_user
        .add_contact_from_group(group_chat_id, charlie.clone())
        .await;
    assert!(
        res.is_err(),
        "Bob should not be able to add Charlie again as a contact from the group"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[tracing::instrument(name = "Handle sanity checks test", skip_all)]
async fn handle_sanity_checks() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let bob = setup.add_user().await;

    let bob = setup.get_user_mut(&bob);
    let handle_record = bob.add_user_handle().await.unwrap();
    let bob_handle = handle_record.handle.clone();

    let alice = setup.get_user_mut(&alice);
    let handle_record = alice.add_user_handle().await.unwrap();
    let alice_handle = handle_record.handle.clone();
    let alice_user = &alice.user;
    let res = alice_user.add_contact(alice_handle.clone()).await.unwrap();
    assert!(
        matches!(
            res,
            AddHandleContactResult::Err(AddHandleContactError::OwnHandle)
        ),
        "Should not be able to add own handle as contact"
    );

    // Try to add Bob twice
    let res = alice_user.add_contact(bob_handle.clone()).await.unwrap();
    assert!(
        matches!(res, AddHandleContactResult::Ok(_)),
        "Should be able to add Bob as contact"
    );
    let res = alice_user.add_contact(bob_handle.clone()).await.unwrap();
    assert!(
        matches!(
            res,
            AddHandleContactResult::Err(AddHandleContactError::DuplicateRequest)
        ),
        "Should not be able to add Bob twice"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[tracing::instrument(name = "Check handle exists", skip_all)]
async fn check_handle_exists() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let alice_user = &setup.get_user(&alice).user;

    let random_number = rand::thread_rng().gen_range(100_000..1_000_000);
    let alice_handle = UserHandle::new(format!("alice-{}", random_number)).unwrap();

    let handle_exists = alice_user.check_handle_exists(&alice_handle).await.unwrap();
    assert!(!handle_exists, "Alice's handle should not exist yet");

    alice_user
        .add_user_handle(alice_handle.clone())
        .await
        .unwrap();

    let exists = alice_user.check_handle_exists(&alice_handle).await.unwrap();
    assert!(exists, "Alice's handle should exist");

    alice_user.remove_user_handle(&alice_handle).await.unwrap();
    let exists = alice_user.check_handle_exists(&alice_handle).await.unwrap();
    assert!(!exists, "Alice's handle should not exist after removal");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[tracing::instrument(
    name = "Unsupported client version on listen handle and queue",
    skip_all
)]
async fn unsupported_client_version() {
    let setup =
        TestBackend::single_with_params(None, Some(VersionReq::parse("^0.1.0").unwrap())).await;

    let client = ApiClient::new(setup.server_url().as_str()).unwrap();

    let handle = UserHandle::new("test_handle".to_string()).unwrap();
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

    let client_id = QsClientId::random(&mut OsRng);
    let res = client.listen_queue(client_id, 0).await;
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
async fn message_sending_failures() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let bob = setup.add_user().await;

    let chat_id = setup.connect_users(&alice, &bob).await;

    let alice_user = &setup.get_user(&alice).user;

    let content = MimiContent::simple_markdown_message("Hello".to_string(), [0; 16]);

    // Make server drop messages
    setup.listener_control_handle().unwrap().set_drop_all();

    // Send three messages
    for _ in 0..3 {
        alice_user
            .send_message(chat_id, content.clone(), None)
            .await
            .unwrap();
    }
    alice_user.outbound_service().run_once().await;
    // Check that messages are marked as failed
    let messages = alice_user.messages(chat_id, 3).await.unwrap();
    for message in messages {
        let status = message.status();
        if status != MessageStatus::Error {
            panic!(
                "Message should be marked as error. Actual status: {:?}",
                status
            );
        }
    }
}
