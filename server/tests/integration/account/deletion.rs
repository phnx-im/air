// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Deleting a user, its QS user, and the whole account.

use std::assert_matches;

use airapiclient::{as_api::AsRequestError, qs_api::QsRequestError};
use aircoreclient::clients::MarkChatAsRead;
use airprotos::queue_service::v1::listen_response;
use airserver_test_harness::utils::{
    core_user_ext::CoreUserExt,
    setup::{TestBackend, TestUser},
};
use mimi_content::MimiContent;
use tokio_stream::StreamExt;

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn delete_user() {
    let mut setup = TestBackend::single().await;

    let alice = setup.add_user().await;
    // Adding another user with the same id should fail.
    match TestUser::try_new(alice.user_id(), setup.server_url(), Some("DUMMY007")).await {
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
    TestUser::try_new(alice.user_id(), setup.server_url(), Some("DUMMY007"))
        .await
        .unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn delete_qs_user_keeps_queue() {
    let mut setup = TestBackend::single().await;

    let [alice, bob] = setup.add_users().await;
    let chat_id = setup.connect_users(&alice, &bob).await;

    // Leave a message in Alice's queue.
    let content = MimiContent::simple_markdown_message("hello".to_owned(), [0; 16]);
    bob.send_message(chat_id, content, None, MarkChatAsRead::Yes)
        .await
        .unwrap();
    bob.outbound_service().run_once().await;

    alice.qs_delete_user().await.unwrap();

    // Everything but the queue is refused.
    let error = alice.qs_publish_empty_key_packages().await.unwrap_err();
    assert_matches!(error, QsRequestError::Tonic(status) if status.code() == tonic::Code::NotFound);
    let error = alice.qs_create_client().await.unwrap_err();
    assert_matches!(error, QsRequestError::Tonic(status) if status.code() == tonic::Code::NotFound);
    if let Some(exists) = setup.qs_user_record_exists(alice.qs_user_id()).await {
        assert!(exists);
    }

    // The queue can still be read and acked.
    let (mut stream, responder) = alice.listen_queue().await.unwrap();
    let mut max_sequence_number = None;
    while let Some(response) = stream.next().await {
        match response.unwrap().event {
            Some(listen_response::Event::Message(message)) => {
                max_sequence_number = Some(message.sequence_number);
            }
            Some(listen_response::Event::Empty(_)) => break,
            Some(listen_response::Event::Payload(_))
            | Some(listen_response::Event::VersionStatus(_))
            | Some(listen_response::Event::SiblingClientState(_))
            | None => {}
        }
    }
    let max_sequence_number = max_sequence_number.expect("no message in the queue");
    responder.ack(max_sequence_number + 1).await;
    responder.close(&mut stream).await;

    // Acking does not remove the deleted user.
    if let Some(exists) = setup.qs_user_record_exists(alice.qs_user_id()).await {
        assert!(exists);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn delete_account() {
    let mut setup = TestBackend::single().await;

    let alice = setup.add_user().await;
    setup.add_username(&alice).await;

    let bob = setup.add_user().await;

    let contact_chat_id = setup.connect_users(&alice, &bob).await;

    // Create a group with Alice and Bob
    let chat_id = setup.create_group(&alice).await;
    setup.invite_to_group(chat_id, &alice, &[&bob]).await;

    // Delete the account
    let db_path = None;
    alice.delete_account(db_path).await.unwrap();

    // Check that Alice left the group
    bob.drain().await;

    let participants = setup
        .user(&bob)
        .chat_participants(contact_chat_id)
        .await
        .unwrap();
    assert_eq!(participants, [bob.user_id().clone()].into_iter().collect());

    let participants = bob.chat_participants(chat_id).await.unwrap();
    assert_eq!(participants, [bob.user_id().clone()].into_iter().collect());

    // After deletion, adding the user again should work.
    // Note: Since the user is ephemeral, there is nothing to test on the client side.
    let new_alice = TestUser::try_new(alice.user_id(), setup.server_url(), Some("DUMMY007"))
        .await
        .unwrap();
    // Adding a username to the new user should work, because the previous username was
    // deleted.
    new_alice.add_username().await;
}
