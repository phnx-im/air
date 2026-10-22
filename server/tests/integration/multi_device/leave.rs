// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Leaving a group from one device, and the sibling following.

use std::{collections::HashSet, time::Duration};

use aircommon::identifiers::UserId;
use aircoreclient::{ChatId, EventMessage, Message, SystemMessage, clients::CoreUser};
use airserver_test_harness::utils::core_user_ext::CoreUserExt;
use tokio::time::sleep;

use super::{assert_inactive, setup_group_with_linked_device};

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn leave_before_creation_echo_keeps_chat_inactive() {
    let (setup, alice, bob, _charlie, _chat_id, alice2) = setup_group_with_linked_device().await;

    // The linked device has no profile of bob, so the old device creates.
    let chat_id = alice
        .create_chat("left early".to_owned(), None, setup.apq_groups)
        .await
        .unwrap();
    alice
        .invite_users(chat_id, std::slice::from_ref(bob.user_id()))
        .await
        .unwrap();
    alice.leave_chat(chat_id).await.unwrap();

    // The creator still has its own creation echo queued.
    alice.drain().await;
    let past_members: HashSet<UserId> = [alice.user_id(), bob.user_id()]
        .into_iter()
        .cloned()
        .collect();
    assert_inactive(
        &alice,
        chat_id,
        Some(&past_members),
        "the echo must not revive the chat",
    )
    .await;
    assert!(alice.mls_chat_participants(chat_id).await.is_none());

    alice2.drain().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn leave_from_linked_device_is_followed_by_sibling() {
    let (_setup, alice, bob, charlie, chat_id, alice2) = setup_group_with_linked_device().await;

    leave_and_follow(&alice, &bob, &charlie, chat_id, &alice2, &alice).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn leave_from_old_device_is_followed_by_linked_device() {
    let (_setup, alice, bob, charlie, chat_id, alice2) = setup_group_with_linked_device().await;

    leave_and_follow(&alice, &bob, &charlie, chat_id, &alice, &alice2).await;
}

/// A leave whose DS response got lost is retried with the same proposal. The
/// DS fans the retry out to the shared leaf again, and the sibling must treat
/// it as the duplicate it is.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn retried_leave_is_ignored_by_sibling() {
    let (setup, alice, bob, charlie, chat_id, alice2) = setup_group_with_linked_device().await;
    let all_members: HashSet<UserId> = [alice.user_id(), bob.user_id(), charlie.user_id()]
        .into_iter()
        .cloned()
        .collect();

    setup.listener_control_handle().set_drop_next_response();
    alice.leave_chat(chat_id).await.unwrap();
    let pending = alice
        .pending_chat_operation_info(chat_id)
        .await
        .unwrap()
        .expect("the leave must stay pending after the dropped response");
    assert_eq!(pending.operation_type, "leave");
    assert_inactive(&alice, chat_id, Some(&all_members), "the leaver").await;
    assert!(
        alice.mls_chat_participants(chat_id).await.is_some(),
        "the leaver keeps the group for the retry"
    );

    // The DS did accept the proposal, so the sibling sees it once.
    alice2.drain().await;
    assert_eq!(
        count_self_removes(&alice2, chat_id, alice.user_id()).await,
        1
    );

    // The retry re-sends the same proposal.
    sleep(Duration::from_secs(2)).await;
    alice.drain().await;
    alice.outbound_service().run_once().await;
    assert!(
        alice
            .pending_chat_operation_info(chat_id)
            .await
            .unwrap()
            .is_none(),
        "the retry must complete the pending leave"
    );
    assert!(
        alice.mls_chat_participants(chat_id).await.is_none(),
        "the completed leave drops the group"
    );

    alice2.drain().await;
    assert_eq!(
        count_self_removes(&alice2, chat_id, alice.user_id()).await,
        1,
        "the retried self-remove must not be recorded twice"
    );
    assert_eq!(
        alice2.pending_removes(chat_id).await.unwrap(),
        vec![alice.user_id().clone()]
    );

    // Bob commits and both devices turn the chat inactive.
    bob.drain().await;
    bob.update_key(chat_id).await.unwrap();

    alice2.drain().await;
    assert_inactive(&alice2, chat_id, Some(&all_members), "the sibling").await;
    alice.drain().await;
    assert_inactive(&alice, chat_id, Some(&all_members), "the leaver").await;
    assert_eq!(
        count_self_removes(&alice2, chat_id, alice.user_id()).await,
        1
    );
    assert_eq!(
        count_self_removes(&alice, chat_id, alice.user_id()).await,
        1
    );
}

/// Both devices leave before anyone commits: the second leave proposes the
/// removal of a leaf that is already pending removal. Neither device may end up
/// with a duplicate record or a stuck pending operation, and the group must
/// still converge on the commit.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn both_devices_leaving_records_leave_once() {
    let (_setup, alice, bob, charlie, chat_id, alice2) = setup_group_with_linked_device().await;
    let all_members: HashSet<UserId> = [alice.user_id(), bob.user_id(), charlie.user_id()]
        .into_iter()
        .cloned()
        .collect();
    let remaining: HashSet<UserId> = [bob.user_id(), charlie.user_id()]
        .into_iter()
        .cloned()
        .collect();

    alice2.leave_chat(chat_id).await.unwrap();
    alice.drain().await;

    // The user leaves on the other device as well. The DS already holds the
    // leave of the shared leaf, so nothing is sent.
    alice.leave_chat(chat_id).await.unwrap();
    assert_inactive(&alice, chat_id, Some(&all_members), "the second leaver").await;

    alice2.drain().await;
    alice.drain().await;
    for (device, what) in [(&alice2, "first leaver"), (&alice, "second leaver")] {
        assert_eq!(
            count_self_removes(device, chat_id, alice.user_id()).await,
            1,
            "the {what} must record the leave exactly once"
        );
        assert!(
            device.mls_chat_participants(chat_id).await.is_none(),
            "the {what} must drop the group"
        );
    }

    bob.drain().await;
    assert_eq!(
        bob.pending_removes(chat_id).await.unwrap(),
        vec![alice.user_id().clone()],
        "bob must see the shared leaf pending removal once"
    );
    bob.update_key(chat_id).await.unwrap();
    assert_eq!(bob.mls_chat_participants(chat_id).await.unwrap(), remaining);

    for (device, what) in [(&alice2, "first leaver"), (&alice, "second leaver")] {
        device.drain().await;
        assert_inactive(device, chat_id, Some(&all_members), what).await;
        assert_eq!(
            count_self_removes(device, chat_id, alice.user_id()).await,
            1
        );
        assert!(
            device
                .pending_chat_operation_info(chat_id)
                .await
                .unwrap()
                .is_none(),
            "the {what} must not keep a pending operation"
        );
    }

    charlie.drain().await;
    assert_eq!(
        charlie.mls_chat_participants(chat_id).await.unwrap(),
        remaining
    );
}

/// How many "`user_id` left" system messages the device stores for the chat.
async fn count_self_removes(user: &CoreUser, chat_id: ChatId, user_id: &UserId) -> usize {
    user.messages(chat_id, 100)
        .await
        .unwrap()
        .iter()
        .filter(|message| match message.message() {
            Message::Event(EventMessage::System(SystemMessage::Remove(Some(remover), removed))) => {
                remover == user_id && removed == user_id
            }
            _ => false,
        })
        .count()
}

/// `leaver` leaves the group while `sibling` shares the leaf. The sibling sees
/// the pending self-remove on the shared leaf, the leaver copes with the echo of
/// its own proposal, and both devices turn the chat inactive once bob commits.
async fn leave_and_follow(
    alice: &CoreUser,
    bob: &CoreUser,
    charlie: &CoreUser,
    chat_id: ChatId,
    leaver: &CoreUser,
    sibling: &CoreUser,
) {
    let all_members: HashSet<UserId> = [alice.user_id(), bob.user_id(), charlie.user_id()]
        .into_iter()
        .cloned()
        .collect();
    let remaining: HashSet<UserId> = [bob.user_id(), charlie.user_id()]
        .into_iter()
        .cloned()
        .collect();

    leaver.leave_chat(chat_id).await.unwrap();
    assert_inactive(
        leaver,
        chat_id,
        Some(&all_members),
        "the leaver turns the chat inactive right away",
    )
    .await;
    assert_eq!(
        count_self_removes(leaver, chat_id, alice.user_id()).await,
        1
    );
    assert!(
        leaver.mls_chat_participants(chat_id).await.is_none(),
        "the leaver drops the group with the leave"
    );

    // The DS fans the self-remove back to the shared leaf, so the sibling learns
    // about it from the queue, like any other member does.
    sibling.drain().await;
    assert_eq!(
        sibling.pending_removes(chat_id).await.unwrap(),
        vec![alice.user_id().clone()],
        "the sibling should hold the pending removal of its own leaf"
    );
    assert_eq!(
        count_self_removes(sibling, chat_id, alice.user_id()).await,
        1
    );

    // The leaver already holds the proposal, so the echo must be a no-op.
    leaver.drain().await;
    assert_eq!(
        count_self_removes(leaver, chat_id, alice.user_id()).await,
        1,
        "the echo of the own self-remove must not add a second system message"
    );

    // Another member commits the pending self-remove.
    bob.drain().await;
    assert_eq!(
        bob.pending_removes(chat_id).await.unwrap(),
        vec![alice.user_id().clone()]
    );
    bob.update_key(chat_id).await.unwrap();
    assert_eq!(bob.mls_chat_participants(chat_id).await.unwrap(), remaining);

    // Both of alice's devices follow the commit that removes their shared leaf.
    sibling.drain().await;
    assert_inactive(
        sibling,
        chat_id,
        Some(&all_members),
        "the sibling turns the chat inactive with the commit",
    )
    .await;
    leaver.drain().await;
    assert_inactive(
        leaver,
        chat_id,
        Some(&all_members),
        "the leaver keeps the chat inactive",
    )
    .await;
    for (device, what) in [(leaver, "leaver"), (sibling, "sibling")] {
        assert_eq!(
            count_self_removes(device, chat_id, alice.user_id()).await,
            1,
            "the {what} must record the leave exactly once"
        );
        assert!(
            device
                .pending_chat_operation_info(chat_id)
                .await
                .unwrap()
                .is_none(),
            "the {what} must not keep a pending operation for the left group"
        );
    }

    charlie.drain().await;
    assert_eq!(
        charlie.mls_chat_participants(chat_id).await.unwrap(),
        remaining
    );
}
