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

use crate::contacts::requests::add_second_username;

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn deleted_user_id_can_register_again() {
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
async fn deleted_qs_user_keeps_readable_queue() {
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
async fn deleted_account_leaves_chats_and_frees_username() {
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

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[ignore = "a chat with a pending commit keeps the deleted account in all chats"]
async fn deleted_account_leaves_chats_besides_one_with_pending_commit() {
    let mut setup = TestBackend::single().await;
    let [alice, bob, charlie] = setup.add_users().await;
    setup.connect_users(&alice, &bob).await;
    setup.connect_users(&alice, &charlie).await;
    let pending_chat_id = setup.create_group(&alice).await;
    setup
        .invite_to_group(pending_chat_id, &alice, &[&bob])
        .await;
    let chat_id = setup.create_group(&alice).await;
    setup.invite_to_group(chat_id, &alice, &[&charlie]).await;

    // The lost response leaves the commit pending.
    setup.listener_control_handle().set_drop_next_response();
    alice.update_key(pending_chat_id).await.unwrap_err();

    alice.delete_account(None).await.unwrap();

    charlie.drain().await;
    let participants = charlie.chat_participants(chat_id).await.unwrap();
    assert_eq!(
        participants,
        [charlie.user_id().clone()].into_iter().collect()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[ignore = "one failed username deletion keeps the remaining usernames registered"]
async fn deleted_account_frees_usernames_after_one_fails() {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    let first = setup.add_username(&alice).await;
    let second = add_second_username(&alice).await;

    setup
        .listener_control_handle()
        .set_reject_next_request_to("DeleteUsername");
    alice.delete_account(None).await.unwrap();

    // Only the username whose deletion failed may remain.
    let mut remaining = 0;
    for record in [first, second] {
        if bob
            .check_username_exists(record.username)
            .await
            .unwrap()
            .is_some()
        {
            remaining += 1;
        }
    }
    assert!(remaining <= 1, "{remaining} usernames remain registered");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[ignore = "one failed leave stops the deleted account from leaving the other chats"]
async fn deleted_account_leaves_chats_after_one_leave_fails() {
    let mut setup = TestBackend::single().await;
    let [alice, bob, charlie] = setup.add_users().await;
    setup.connect_users(&alice, &bob).await;
    setup.connect_users(&alice, &charlie).await;
    let bob_chat_id = setup.create_group(&alice).await;
    setup.invite_to_group(bob_chat_id, &alice, &[&bob]).await;
    let charlie_chat_id = setup.create_group(&alice).await;
    setup
        .invite_to_group(charlie_chat_id, &alice, &[&charlie])
        .await;

    let self_remove = if setup.apq_groups {
        "ApqSelfRemove"
    } else {
        "SelfRemove"
    };
    setup
        .listener_control_handle()
        .set_reject_next_request_to(self_remove);
    alice.delete_account(None).await.unwrap();

    // Only the chat whose leave failed may keep alice.
    let mut remaining = 0;
    for (member, chat_id) in [(&bob, bob_chat_id), (&charlie, charlie_chat_id)] {
        member.drain_unchecked().await;
        if member
            .chat_participants(chat_id)
            .await
            .unwrap()
            .contains(alice.user_id())
        {
            remaining += 1;
        }
    }
    assert!(remaining <= 1, "alice remains in {remaining} chats");
}
