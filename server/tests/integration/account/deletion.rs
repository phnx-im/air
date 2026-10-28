// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Deleting a user, its QS user, and the whole account.

use std::assert_matches;

use airapiclient::{as_api::AsRequestError, qs_api::QsRequestError};
use aircommon::identifiers::{UserId, Username};
use aircoreclient::{
    AccountDeletionStatus, ReadReceiptsSetting, SystemMessage, UnlinkReason,
    clients::{CoreUser, MarkChatAsRead},
};
use airprotos::queue_service::v1::listen_response;
use airserver_test_harness::utils::{
    core_user_ext::CoreUserExt,
    setup::{TestBackend, TestUser},
};
use mimi_content::MimiContent;
use tokio_stream::StreamExt;

use crate::contacts::requests::add_second_username;
use crate::multi_device::{link_sibling, read_receipts};

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

/// Starts the outbound service once the queue is processed, as the app does.
/// The queue first gets the responses to the self group commits of a run.
async fn start_outbound_service(user: &CoreUser) {
    user.outbound_service().run_once().await;
    user.drain().await;
    user.outbound_service().start_and_wait().await;
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
    start_outbound_service(&alice).await;
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

    start_outbound_service(&alice).await;
    alice.delete_account().await.unwrap();

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
    start_outbound_service(&alice).await;
    alice.delete_account().await.unwrap();

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
    start_outbound_service(&alice).await;
    alice.delete_account().await.unwrap();

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

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn account_deletion_resumes_after_lost_self_group_delete_response() -> anyhow::Result<()> {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    setup.add_username(&alice).await;
    let alice2 = link_sibling(&alice).await;

    // The outbound service is stopped, so the deletion only starts.
    assert_eq!(
        alice.delete_account().await?,
        AccountDeletionStatus::Pending
    );

    // The DS accepts the self group deletion, but the response is lost.
    setup.listener_control_handle().set_drop_next_response();
    alice.outbound_service().run_once().await;
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

    assert_eq!(
        alice.delete_account().await?,
        AccountDeletionStatus::Pending
    );
    setup.listener_control_handle().set_drop_next_request();
    alice.outbound_service().run_once().await;
    assert_eq!(alice.account_unlink_reason().await?, None);
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
    assert_eq!(
        alice.delete_account().await?,
        AccountDeletionStatus::Pending
    );
    setup.listener_control_handle().set_drop_next_response();
    alice.outbound_service().run_once().await;
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

    start_outbound_service(&alice).await;
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
    start_outbound_service(&alice).await;

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

    start_outbound_service(&alice).await;
    setup.listener_control_handle().set_reject_next_request();
    assert!(alice.delete_account().await.is_err());
    assert!(
        !alice.has_unreported_account_deletion_abort().await?,
        "the caller reports the error"
    );

    alice.outbound_service().run_once().await;
    assert_eq!(alice.account_unlink_reason().await?, None);
    alice2.drain().await;
    assert_eq!(alice2.account_unlink_reason().await?, None);
    alice2.qs_publish_empty_key_packages().await?;

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn account_deletion_is_aborted_after_repeated_self_group_delete_failures()
-> anyhow::Result<()> {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let alice2 = link_sibling(&alice).await;

    // Starting the deletion again makes a failed attempt due right away.
    let mut attempts = 0;
    while !alice.account_deletion_is_aborted().await? {
        assert!(attempts < 20, "the deletion must be aborted");
        assert_eq!(
            alice.delete_account().await?,
            AccountDeletionStatus::Pending
        );
        setup.listener_control_handle().set_fail_next_request();
        alice.outbound_service().run_once().await;
        attempts += 1;
    }

    assert!(attempts > 1, "a failure is retried");
    assert_eq!(alice.account_unlink_reason().await?, None);
    assert!(
        alice.has_unreported_account_deletion_abort().await?,
        "the abort happened in the background"
    );

    // The DS never accepted a delete. Both devices keep the account, and the
    // self group works again.
    assert!(alice.self_group_pending_operation_info().await?.is_none());
    alice
        .set_synced_user_setting(&ReadReceiptsSetting(true))
        .await?;
    alice.outbound_service().run_once().await;
    alice2.drain().await;
    assert_eq!(alice2.account_unlink_reason().await?, None);
    assert_eq!(read_receipts(&alice2).await, Some(true));

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn aborted_account_deletion_resumes_once_its_accepted_delete_shows() -> anyhow::Result<()> {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let alice2 = link_sibling(&alice).await;
    upload_initial_key_packages(&alice, &alice2).await;

    // The DS accepts the self group delete, but the response is lost, and
    // every resend fails until the deletion is aborted.
    assert_eq!(
        alice.delete_account().await?,
        AccountDeletionStatus::Pending
    );
    setup.listener_control_handle().set_drop_next_response();
    alice.outbound_service().run_once().await;
    let mut attempts = 0;
    while !alice.account_deletion_is_aborted().await? {
        assert!(attempts < 20, "the deletion must be aborted");
        assert_eq!(
            alice.delete_account().await?,
            AccountDeletionStatus::Pending
        );
        setup.listener_control_handle().set_fail_next_request();
        alice.outbound_service().run_once().await;
        attempts += 1;
    }
    assert_eq!(alice.account_unlink_reason().await?, None);
    alice2.drain().await;
    assert_eq!(
        alice2.account_unlink_reason().await?,
        Some(UnlinkReason::AccountDeleted)
    );

    // The next self group commit finds the self group closed, and the
    // deletion goes on instead of resetting this device without cleanup.
    alice.drain_unchecked().await;
    alice
        .set_synced_user_setting(&ReadReceiptsSetting(true))
        .await?;
    alice.outbound_service().run_once().await;
    assert!(!alice.account_deletion_is_aborted().await?);
    alice.outbound_service().run_once().await;

    assert_eq!(
        alice.account_unlink_reason().await?,
        Some(UnlinkReason::AccountDeleted)
    );
    assert_account_deleted(&setup, alice.user_id(), &alice2, false).await;

    Ok(())
}

/// Runs the initial key package uploads of both devices, so that they don't
/// take the self group later.
async fn upload_initial_key_packages(alice: &CoreUser, alice2: &CoreUser) {
    for (device, sibling) in [(alice2, alice), (alice, alice2)] {
        device.outbound_service().run_once().await;
        device.drain().await;
        sibling.drain().await;
    }
}

async fn assert_parked_settings_commit(
    user: &CoreUser,
    request_status: &str,
) -> anyhow::Result<()> {
    let parked = user
        .self_group_pending_operation_info()
        .await?
        .expect("the settings commit is parked");
    assert_eq!(parked.operation_type, "self_group_messages");
    assert_eq!(parked.request_status, request_status);
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn account_deletion_sends_a_parked_self_group_commit_before_the_delete() -> anyhow::Result<()>
{
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let alice2 = link_sibling(&alice).await;
    upload_initial_key_packages(&alice, &alice2).await;

    alice
        .set_synced_user_setting(&ReadReceiptsSetting(true))
        .await?;
    setup.listener_control_handle().set_fail_next_request();
    alice.outbound_service().run_once().await;
    assert_parked_settings_commit(&alice, "ready_to_retry").await?;

    assert_eq!(
        alice.delete_account().await?,
        AccountDeletionStatus::Pending
    );
    alice.outbound_service().run_once().await;

    assert_eq!(
        alice.account_unlink_reason().await?,
        Some(UnlinkReason::AccountDeleted)
    );
    alice2.drain().await;
    assert_eq!(
        read_receipts(&alice2).await,
        Some(true),
        "the parked commit goes out first"
    );
    assert_eq!(
        alice2.account_unlink_reason().await?,
        Some(UnlinkReason::AccountDeleted)
    );
    assert_account_deleted(&setup, alice.user_id(), &alice2, false).await;

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn account_deletion_waits_for_the_queue_to_confirm_a_parked_self_group_commit()
-> anyhow::Result<()> {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let alice2 = link_sibling(&alice).await;
    upload_initial_key_packages(&alice, &alice2).await;

    // The DS accepts the settings commit, but the response is lost.
    alice
        .set_synced_user_setting(&ReadReceiptsSetting(true))
        .await?;
    setup.listener_control_handle().set_drop_next_response();
    alice.outbound_service().run_once().await;
    assert_parked_settings_commit(&alice, "ready_to_retry").await?;

    // The resent commit finds its epoch taken and waits for the queue. More
    // runs than the deletion has attempts neither abort nor send the delete.
    for _ in 0..12 {
        assert_eq!(
            alice.delete_account().await?,
            AccountDeletionStatus::Pending
        );
        alice.outbound_service().run_once().await;
    }
    assert_eq!(alice.account_unlink_reason().await?, None);
    assert_parked_settings_commit(&alice, "waiting_for_queue_response").await?;

    // The queue confirms the parked commit, and the delete follows it.
    alice.drain().await;
    assert!(alice.self_group_pending_operation_info().await?.is_none());
    assert_eq!(
        alice.delete_account().await?,
        AccountDeletionStatus::Pending
    );
    alice.outbound_service().run_once().await;

    assert_eq!(
        alice.account_unlink_reason().await?,
        Some(UnlinkReason::AccountDeleted)
    );
    alice2.drain().await;
    assert_eq!(read_receipts(&alice2).await, Some(true));
    assert_eq!(
        alice2.account_unlink_reason().await?,
        Some(UnlinkReason::AccountDeleted)
    );
    assert_account_deleted(&setup, alice.user_id(), &alice2, false).await;

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn account_deletion_is_aborted_after_three_lost_epochs() -> anyhow::Result<()> {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let alice2 = link_sibling(&alice).await;
    upload_initial_key_packages(&alice, &alice2).await;

    for round in 1..=3 {
        // The sibling's commit takes the epoch of the self group delete.
        alice2
            .set_synced_user_setting(&ReadReceiptsSetting(round % 2 == 1))
            .await?;
        alice2.outbound_service().run_once().await;
        assert_eq!(
            alice.delete_account().await?,
            AccountDeletionStatus::Pending
        );
        alice.outbound_service().run_once().await;
        if round == 3 {
            break;
        }

        // Waiting for the queue spends no attempt.
        assert_eq!(
            alice.delete_account().await?,
            AccountDeletionStatus::Pending
        );
        alice.outbound_service().run_once().await;
        assert!(!alice.account_deletion_is_aborted().await?);
        // The queue brings the sibling's commit, and the delete is staged anew.
        alice.drain().await;
    }

    assert!(alice.account_deletion_is_aborted().await?);
    assert_eq!(alice.account_unlink_reason().await?, None);
    alice2.drain().await;
    assert_eq!(alice2.account_unlink_reason().await?, None);
    alice2.qs_publish_empty_key_packages().await?;

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn account_deletion_continues_past_a_username_that_is_gone_already() -> anyhow::Result<()> {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    alice.outbound_service().run_once().await;

    let gone = Username::new("gone-username".to_owned())?;
    let kept = Username::new("kept-username".to_owned())?;
    for username in [&gone, &kept] {
        alice
            .add_username(username.clone())
            .await?
            .expect("the username is free");
    }
    alice.delete_username_on_server_only(&gone).await?;

    start_outbound_service(&alice).await;
    assert_eq!(
        alice.delete_account().await?,
        AccountDeletionStatus::Completed
    );

    let bob = setup.add_user().await;
    bob.outbound_service().run_once().await;
    assert!(
        bob.add_username(kept).await?.is_some(),
        "the remaining username is deleted"
    );

    Ok(())
}

/// Runs the account deletion past the QS client and stops it with a lost
/// response while it leaves the chats. The QS stream can't start the outbound
/// service from then on.
async fn delete_account_past_the_qs_client(
    setup: &TestBackend,
    alice: &CoreUser,
    alice2: &CoreUser,
) -> anyhow::Result<()> {
    assert_eq!(
        alice.delete_account().await?,
        AccountDeletionStatus::Pending
    );
    // The self group delete goes through, and the self-remove from the
    // contact chat is accepted without a response.
    setup.listener_control_handle().set_drop_response_after(1);
    alice.outbound_service().run_once().await;
    assert_eq!(alice.account_unlink_reason().await?, None);
    alice2.drain().await;
    assert_eq!(
        alice2.account_unlink_reason().await?,
        Some(UnlinkReason::AccountDeleted)
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn account_deletion_resumes_on_start_once_the_qs_client_is_deleted() -> anyhow::Result<()> {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_persisted_user().await;
    let bob = setup.add_user().await;
    setup.add_username(&alice).await;
    setup.connect_users(&alice, &bob).await;
    let alice2 = link_sibling(&alice).await;
    delete_account_past_the_qs_client(&setup, &alice, &alice2).await?;

    let restarted = CoreUser::load_with_server_url(
        setup.temp_dir().to_str().expect("utf-8 path"),
        alice.client_record_id(),
        Some(setup.server_url()),
    )
    .await?;
    restarted.outbound_service().wake_and_wait().await;

    assert_eq!(
        restarted.account_unlink_reason().await?,
        Some(UnlinkReason::AccountDeleted)
    );
    assert_account_deleted(&setup, alice.user_id(), &alice2, true).await;

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn deleting_again_starts_the_outbound_service_once_the_qs_client_is_deleted()
-> anyhow::Result<()> {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    setup.add_username(&alice).await;
    setup.connect_users(&alice, &bob).await;
    let alice2 = link_sibling(&alice).await;
    delete_account_past_the_qs_client(&setup, &alice, &alice2).await?;

    assert_eq!(
        alice.delete_account().await?,
        AccountDeletionStatus::Completed
    );
    assert_account_deleted(&setup, alice.user_id(), &alice2, true).await;

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn concurrent_account_deletions_both_complete() -> anyhow::Result<()> {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    setup.add_username(&alice).await;
    let alice2 = link_sibling(&alice).await;

    start_outbound_service(&alice).await;
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

    assert_eq!(
        alice.delete_account().await?,
        AccountDeletionStatus::Pending
    );

    // The DS accepts the self group deletion, but the response is lost.
    setup.listener_control_handle().set_drop_next_response();
    alice.outbound_service().run_once().await;

    // The retry is refused before the DS looks at the group, as it would be
    // for an expired client version.
    setup.listener_control_handle().set_reject_next_request();
    alice.outbound_service().run_once().await;
    assert_eq!(alice.account_unlink_reason().await?, None);

    // Starting the deletion again makes the deferred retry due.
    assert_eq!(
        alice.delete_account().await?,
        AccountDeletionStatus::Pending
    );
    alice.outbound_service().run_once().await;

    assert_eq!(
        alice.account_unlink_reason().await?,
        Some(UnlinkReason::AccountDeleted)
    );
    assert_account_deleted(&setup, alice.user_id(), &alice2, true).await;

    Ok(())
}
