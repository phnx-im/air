// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Synced settings across devices.

use aircoreclient::ReadReceiptsSetting;
use airserver_test_harness::utils::{core_user_ext::CoreUserExt, setup::TestBackend};

use super::{read_receipts, self_chat_id, send_and_receive};

// A settings change on one device reaches the linked device through a
// self-group commit, in both directions. This is also the proof that the DS
// passes commits carrying AppEphemeral proposals through.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn multi_device_settings_sync() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let alice2 = alice.add_linked_device().await;

    // Bring both devices to the self-group's latest epoch.
    alice.drain().await;
    alice2.drain().await;

    // Initial key package uploads. Each upload completes only once the
    // uploading device fetches its own commit.
    alice2.outbound_service().run_once().await;
    alice2.drain().await;
    alice.drain().await;
    alice.outbound_service().run_once().await;
    alice.drain().await;
    alice2.drain().await;

    // Nothing was set before linking, so the linking payload carried an empty
    // snapshot and the new device starts unset.
    assert_eq!(read_receipts(&alice2).await, None);

    // Two quick toggles: both are applied locally right away and fold into
    // one pending change. Before the outbound service runs, no commit exists.
    let chat_id = self_chat_id(&alice).await;
    alice
        .set_synced_user_setting(&ReadReceiptsSetting(true))
        .await
        .unwrap();
    alice
        .set_synced_user_setting(&ReadReceiptsSetting(false))
        .await
        .unwrap();
    assert_eq!(read_receipts(&alice).await, Some(false));
    assert!(alice.has_pending_setting_changes().await.unwrap());
    assert!(
        alice
            .pending_chat_operation_info(chat_id)
            .await
            .unwrap()
            .is_none(),
        "the commit is only staged when the outbound service runs"
    );

    // The outbound service stages a commit from the current state and sends
    // it. Success completes the pending change.
    alice.outbound_service().run_once().await;
    assert!(!alice.has_pending_setting_changes().await.unwrap());
    assert!(
        alice
            .pending_chat_operation_info(chat_id)
            .await
            .unwrap()
            .is_none(),
        "the operation should be gone after a successful send"
    );

    // The linked device applies the folded update when processing its queue.
    alice2.drain().await;
    assert_eq!(read_receipts(&alice2).await, Some(false));

    // No feedback loop: applying a sibling's update records no pending change
    // and issues no commit on the receiving device.
    assert!(!alice2.has_pending_setting_changes().await.unwrap());
    alice2.outbound_service().run_once().await;
    assert!(
        alice2
            .pending_chat_operation_info(chat_id)
            .await
            .unwrap()
            .is_none(),
        "applying an incoming update must not issue a commit"
    );

    // And back in the other direction, from the new device.
    alice2
        .set_synced_user_setting(&ReadReceiptsSetting(true))
        .await
        .unwrap();
    alice2.outbound_service().run_once().await;
    alice.drain().await;
    assert_eq!(read_receipts(&alice).await, Some(true));
}

// Both devices change the setting before processing each other's commit. The
// DS accepts the first commit and rejects the second with a wrong epoch. The
// loser gives the setting up when it processes the winner's commit, so both
// devices converge on the DS-order winner.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn multi_device_settings_race_converges() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let alice2 = alice.add_linked_device().await;

    alice.drain().await;
    alice2.drain().await;

    // Both devices toggle while at the same epoch. The outbound runs stage
    // conflicting commits against that epoch.
    alice
        .set_synced_user_setting(&ReadReceiptsSetting(true))
        .await
        .unwrap();
    alice2
        .set_synced_user_setting(&ReadReceiptsSetting(false))
        .await
        .unwrap();

    // The old device reaches the DS first and wins.
    alice.outbound_service().run_once().await;

    // The new device's commit is rejected with a wrong epoch. The operation
    // parks until the winning commit arrives through the queue, the pending
    // change stays, and the optimistic value stays in place for now.
    alice2.outbound_service().run_once().await;
    let chat_id = self_chat_id(&alice2).await;
    let parked = alice2
        .pending_chat_operation_info(chat_id)
        .await
        .unwrap()
        .expect("losing operation should be parked");
    assert_eq!(parked.request_status, "waiting_for_queue_response");
    assert!(alice2.has_pending_setting_changes().await.unwrap());
    assert_eq!(read_receipts(&alice2).await, Some(false));

    // Processing the winner's commit applies its snapshot, deletes the parked
    // operation, and drops the covered field from the pending changes, so
    // nothing is re-issued.
    alice2.drain().await;
    assert_eq!(read_receipts(&alice2).await, Some(true));
    assert!(
        alice2
            .pending_chat_operation_info(chat_id)
            .await
            .unwrap()
            .is_none(),
        "losing operation should be deleted"
    );
    assert!(
        !alice2.has_pending_setting_changes().await.unwrap(),
        "covered pending change should be dropped"
    );

    // Nothing left to send: another outbound run stages no new commit.
    alice2.outbound_service().run_once().await;
    assert!(
        alice2
            .pending_chat_operation_info(chat_id)
            .await
            .unwrap()
            .is_none()
    );

    // The winner's own state is unaffected by its echo.
    alice.drain().await;
    assert_eq!(read_receipts(&alice).await, Some(true));

    // The self group is still usable after the race.
    send_and_receive(&alice, &[&alice2], chat_id, "still in sync").await;
}
