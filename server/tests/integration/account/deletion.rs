// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Deleting a user, its QS user, and the whole account.

use std::assert_matches;

use airapiclient::{as_api::AsRequestError, qs_api::QsRequestError};
use aircommon::identifiers::UserId;
use aircoreclient::{
    AccountDeletionStatus, SystemMessage, UnlinkReason,
    clients::{CoreUser, MarkChatAsRead},
};
use airprotos::queue_service::v1::listen_response;
use airserver_test_harness::utils::{
    core_user_ext::CoreUserExt,
    setup::{TestBackend, TestUser},
};
use mimi_content::MimiContent;
use tokio_stream::StreamExt;

use crate::multi_device::link_sibling;

/// Asserts that the account is gone on the server: the sibling's QS client
/// belongs to a deleted QS user, and the user id can be registered again.
/// With `check_username`, the new user also gets the old username, which needs
/// it deleted.
async fn assert_account_deleted(
    setup: &TestBackend,
    user_id: &UserId,
    sibling: &CoreUser,
    check_username: bool,
) {
    let error = sibling.qs_publish_empty_key_packages().await.unwrap_err();
    assert_matches!(error, QsRequestError::Tonic(status) if status.code() == tonic::Code::NotFound);
    let new_user = TestUser::try_new(user_id, setup.server_url(), Some("DUMMY007"))
        .await
        .expect("the user id is free again");
    if check_username {
        new_user.add_username().await;
    }
}

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
    assert_eq!(
        alice.delete_account().await.unwrap(),
        AccountDeletionStatus::Completed
    );
    assert_eq!(
        alice.account_unlink_reason().await.unwrap(),
        Some(UnlinkReason::AccountDeleted)
    );

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
async fn account_deletion_resumes_after_lost_self_group_delete_response() -> anyhow::Result<()> {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    setup.add_username(&alice).await;
    let alice2 = link_sibling(&alice).await;

    // The DS accepts the self group deletion, but the response is lost.
    setup.listener_control_handle().set_drop_next_response();
    assert_eq!(
        alice.delete_account().await?,
        AccountDeletionStatus::Pending
    );
    assert_eq!(alice.account_unlink_reason().await?, None);
    alice2.drain().await;
    assert_eq!(
        alice2.account_unlink_reason().await?,
        Some(UnlinkReason::AccountDeleted)
    );

    // The retried delete finds the self group closed and continues.
    alice.outbound_service().run_once().await;

    assert_eq!(
        alice.account_unlink_reason().await?,
        Some(UnlinkReason::AccountDeleted)
    );
    assert_account_deleted(&setup, alice.user_id(), &alice2, true).await;

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn account_deletion_resumes_after_dropped_self_group_delete() -> anyhow::Result<()> {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    setup.add_username(&alice).await;
    let alice2 = link_sibling(&alice).await;

    setup.listener_control_handle().set_drop_next_request();
    assert_eq!(
        alice.delete_account().await?,
        AccountDeletionStatus::Pending
    );
    alice2.drain().await;
    assert_eq!(alice2.account_unlink_reason().await?, None);

    alice.outbound_service().run_once().await;

    assert_eq!(
        alice.account_unlink_reason().await?,
        Some(UnlinkReason::AccountDeleted)
    );
    alice2.drain().await;
    assert_eq!(
        alice2.account_unlink_reason().await?,
        Some(UnlinkReason::AccountDeleted)
    );
    assert_account_deleted(&setup, alice.user_id(), &alice2, true).await;

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn account_deletion_resumes_after_restart() -> anyhow::Result<()> {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_persisted_user().await;
    let alice2 = link_sibling(&alice).await;

    // The self group deletion goes through, but the device stops before it
    // learns about it.
    setup.listener_control_handle().set_drop_next_response();
    assert_eq!(
        alice.delete_account().await?,
        AccountDeletionStatus::Pending
    );
    alice2.drain().await;
    assert_eq!(
        alice2.account_unlink_reason().await?,
        Some(UnlinkReason::AccountDeleted)
    );
    assert_eq!(alice.account_unlink_reason().await?, None);

    let restarted = CoreUser::load_with_server_url(
        setup.temp_dir().to_str().expect("utf-8 path"),
        alice.client_record_id(),
        Some(setup.server_url()),
    )
    .await?;
    restarted.outbound_service().run_once().await;

    assert_eq!(
        restarted.account_unlink_reason().await?,
        Some(UnlinkReason::AccountDeleted)
    );
    assert_account_deleted(&setup, alice.user_id(), &alice2, false).await;

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn linked_account_deletion_leaves_contacts_and_groups() -> anyhow::Result<()> {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    setup.add_username(&alice).await;
    let contact_chat_id = setup.connect_users(&alice, &bob).await;
    let group_chat_id = setup.create_group(&alice).await;
    setup.invite_to_group(group_chat_id, &alice, &[&bob]).await;
    let alice2 = link_sibling(&alice).await;

    assert_eq!(
        alice.delete_account().await?,
        AccountDeletionStatus::Completed
    );

    alice2.drain().await;
    assert_eq!(
        alice2.account_unlink_reason().await?,
        Some(UnlinkReason::AccountDeleted)
    );

    bob.drain().await;
    for chat_id in [contact_chat_id, group_chat_id] {
        assert_eq!(
            bob.chat_participants(chat_id).await,
            Some([bob.user_id().clone()].into_iter().collect())
        );
    }
    let removals = bob
        .system_messages(group_chat_id)
        .await
        .into_iter()
        .filter(|message| {
            matches!(message, SystemMessage::Remove(_, removed) if removed == alice.user_id())
        })
        .count();
    assert_eq!(removals, 1, "bob sees alice leave the group once");

    assert_account_deleted(&setup, alice.user_id(), &alice2, true).await;

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn account_deletion_is_dropped_when_the_self_group_delete_cannot_be_built()
-> anyhow::Result<()> {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let alice2 = link_sibling(&alice).await;

    let self_chat_id = alice
        .self_chat_id()
        .await?
        .expect("the device has a self chat");
    alice.corrupt_mls_group_state(self_chat_id).await?;

    assert!(alice.delete_account().await.is_err());

    // Nothing was sent, and nothing goes on in the background.
    alice.outbound_service().run_once().await;
    assert_eq!(alice.account_unlink_reason().await?, None);
    alice2.drain().await;
    assert_eq!(alice2.account_unlink_reason().await?, None);
    alice2.qs_publish_empty_key_packages().await?;

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn account_deletion_is_dropped_when_the_ds_refuses_the_self_group_delete()
-> anyhow::Result<()> {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let alice2 = link_sibling(&alice).await;

    setup.listener_control_handle().set_reject_next_request();
    assert!(alice.delete_account().await.is_err());

    alice.outbound_service().run_once().await;
    assert_eq!(alice.account_unlink_reason().await?, None);
    alice2.drain().await;
    assert_eq!(alice2.account_unlink_reason().await?, None);
    alice2.qs_publish_empty_key_packages().await?;

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn account_deletion_gives_up_after_repeated_self_group_delete_failures() -> anyhow::Result<()>
{
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let alice2 = link_sibling(&alice).await;

    setup.listener_control_handle().set_fail_next_request();
    assert_eq!(
        alice.delete_account().await?,
        AccountDeletionStatus::Pending
    );
    let mut attempts = 1;
    while alice.account_unlink_reason().await?.is_none() {
        assert!(attempts < 20, "the deletion must be given up");
        setup.listener_control_handle().set_fail_next_request();
        alice.outbound_service().run_once().await;
        attempts += 1;
    }

    assert!(attempts > 1, "a failure is retried");
    assert_eq!(
        alice.account_unlink_reason().await?,
        Some(UnlinkReason::AccountDeleted)
    );
    // The DS never accepted a delete, so the sibling keeps the account.
    alice2.drain().await;
    assert_eq!(alice2.account_unlink_reason().await?, None);

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn concurrent_account_deletions_share_the_job() -> anyhow::Result<()> {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    setup.add_username(&alice).await;
    let alice2 = link_sibling(&alice).await;

    let (first, second) = tokio::join!(alice.delete_account(), alice.delete_account());
    assert_eq!(first?, AccountDeletionStatus::Completed);
    assert_eq!(second?, AccountDeletionStatus::Completed);

    assert_eq!(
        alice.account_unlink_reason().await?,
        Some(UnlinkReason::AccountDeleted)
    );
    alice2.drain().await;
    assert_eq!(
        alice2.account_unlink_reason().await?,
        Some(UnlinkReason::AccountDeleted)
    );
    assert_account_deleted(&setup, alice.user_id(), &alice2, true).await;

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn refused_retry_keeps_an_account_deletion_whose_delete_was_accepted() -> anyhow::Result<()> {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    setup.add_username(&alice).await;
    let alice2 = link_sibling(&alice).await;

    // The DS accepts the self group deletion, but the response is lost.
    setup.listener_control_handle().set_drop_next_response();
    assert_eq!(
        alice.delete_account().await?,
        AccountDeletionStatus::Pending
    );

    // The retry is refused before the DS looks at the group, as it would be
    // for an expired client version.
    setup.listener_control_handle().set_reject_next_request();
    alice.outbound_service().run_once().await;
    assert_eq!(alice.account_unlink_reason().await?, None);

    alice.outbound_service().run_once().await;

    assert_eq!(
        alice.account_unlink_reason().await?,
        Some(UnlinkReason::AccountDeleted)
    );
    assert_account_deleted(&setup, alice.user_id(), &alice2, true).await;

    Ok(())
}
