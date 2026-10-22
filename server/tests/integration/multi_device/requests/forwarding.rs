// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Incoming requests reaching the sibling, and the sibling following an accept.

use aircoreclient::{
    ChatType, DisplayName, SystemMessage, UserProfile,
    clients::process::qs_stream::{QsProcessEventResult, QsStreamProcessor},
};
use airserver_test_harness::utils::{core_user_ext::CoreUserExt, setup::TestBackend};
use tokio::time::timeout;
use tokio_stream::StreamExt;

use super::has_system_message;
use crate::{
    contacts::requests::{add_second_username, pending_chats_from},
    multi_device::{send_and_receive, usernames::add_synced_username},
};

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn request_fetched_by_one_device_reaches_its_sibling() {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    let alice2 = alice.add_synced_device().await;
    let record = add_synced_username(&setup, &alice, &alice2).await;

    let bob_chat_id = bob
        .add_contact(record.username.clone(), record.hash, setup.apq_groups)
        .await
        .unwrap()
        .unwrap();

    // Only alice2 fetches the offer. It never registered the username.
    let chat_id = alice2.receive_connection_offer(&record).await;
    assert_eq!(chat_id, bob_chat_id);
    let package_hashes = alice
        .connection_package_hashes(&record.username)
        .await
        .unwrap();
    alice2.outbound_service().run_once().await;

    let processed = alice.drain().await;
    assert_eq!(
        processed.new_connections,
        vec![chat_id],
        "the forwarded request should raise a notification"
    );
    let chat_a = alice.expect_chat(chat_id).await;
    assert_eq!(
        chat_a.chat_type(),
        &ChatType::PendingConnection(bob.user_id().clone())
    );
    assert!(
        has_system_message(&alice, chat_id, |message| matches!(
            message,
            SystemMessage::ReceivedHandleConnectionRequest { sender, user_handle }
                if sender == bob.user_id() && user_handle == &record.username
        ))
        .await,
        "the forwarded request should be announced like a received one"
    );

    // The device that never saw the offer accepts it.
    alice
        .accept_contact_request(chat_id)
        .await
        .unwrap()
        .unwrap();

    alice2.drain().await;
    let chat_b = alice2.expect_chat(chat_id).await;
    assert_eq!(
        chat_b.chat_type(),
        &ChatType::Connection(bob.user_id().clone())
    );
    let remaining_hashes = alice2
        .connection_package_hashes(&record.username)
        .await
        .unwrap();
    assert_eq!(
        remaining_hashes.len() + 1,
        package_hashes.len(),
        "the sibling drops the key of the package the offer consumed"
    );
    assert_eq!(
        remaining_hashes,
        alice
            .connection_package_hashes(&record.username)
            .await
            .unwrap(),
        "both devices drop the same key"
    );

    bob.drain().await;
    let chat_bob = bob.expect_chat(chat_id).await;
    assert_eq!(
        chat_bob.chat_type(),
        &ChatType::Connection(alice.user_id().clone())
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn streamed_forwarded_request_has_sender_profile() {
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
async fn sibling_holding_request_follows_accept() {
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
async fn sibling_moves_older_requests_into_accepted_chat() {
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
async fn sibling_fetches_rotated_sender_profile() {
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
