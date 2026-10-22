// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Self updates of the own leaf (T and APQ) and the periodic self-update job.

use airserver_test_harness::utils::setup::TestBackend;
use chrono::{DateTime, Duration, Utc};
use tracing::info;

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn member_self_update_is_processed() {
    let mut setup = TestBackend::single().await;
    info!("Adding users");
    let [alice, bob, charlie] = setup.add_users().await;
    info!("Connecting users");
    setup.connect_users(&alice, &bob).await;
    setup.connect_users(&alice, &charlie).await;
    let chat_id = setup.create_group(&alice).await;
    info!("Inviting to group");
    setup
        .invite_to_group(chat_id, &alice, &[&bob, &charlie])
        .await;
    info!("Updating group");
    setup.update_group(chat_id, &bob).await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn self_update_runs_in_batches_of_five() {
    let mut setup = TestBackend::single().await;

    let alice = setup.add_user().await;

    let created_at = Utc::now();

    // Outbound service updates chats in batches of 5
    const UPDATE_BATCH_SIZE: usize = 5;

    let mut chat_ids = Vec::new();
    for _ in 0..UPDATE_BATCH_SIZE + 1 {
        chat_ids.push(setup.create_group(&alice).await);
    }

    // Initially, all chats are self-updated at creation time
    let mut self_updated_at = Vec::new();
    for chat_id in &chat_ids {
        let at = alice.self_updated_at(*chat_id).await.unwrap().unwrap();
        assert!(
            created_at <= at && at <= created_at + Duration::seconds(10),
            "Self update is not within 10 seconds of now",
        );
        self_updated_at.push(at);
    }

    // Set self_updated_at to the past and schedule a self update
    for chat_id in &chat_ids {
        alice
            .set_self_updated_at(*chat_id, DateTime::UNIX_EPOCH)
            .await
            .unwrap();
    }
    // Run the outbound service to update the chats
    alice
        .outbound_service()
        .schedule_self_update(DateTime::UNIX_EPOCH)
        .await
        .unwrap();
    alice.outbound_service().run_once().await;

    // The outbound service updates the chats in batches of 5
    for (idx, chat_id) in chat_ids.iter().take(UPDATE_BATCH_SIZE).enumerate() {
        let at = alice.self_updated_at(*chat_id).await.unwrap().unwrap();
        assert!(self_updated_at[idx] < at, "Self update not happened",);
    }
    assert_eq!(
        alice
            .self_updated_at(chat_ids[UPDATE_BATCH_SIZE])
            .await
            .unwrap()
            .unwrap(),
        DateTime::UNIX_EPOCH,
        "Last chat self-updated even though it should not have been"
    );

    // Another run should update the remaining chat
    alice
        .outbound_service()
        .schedule_self_update(DateTime::UNIX_EPOCH)
        .await
        .unwrap();
    alice.outbound_service().run_once().await;

    let at = alice
        .self_updated_at(chat_ids[UPDATE_BATCH_SIZE])
        .await
        .unwrap()
        .unwrap();
    assert!(
        self_updated_at[UPDATE_BATCH_SIZE] < at,
        "Self update not happened"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn self_update_continues_after_broken_chat() {
    let mut setup = TestBackend::single().await;

    let alice = setup.add_user().await;
    let broken_chat = setup.create_group(&alice).await;
    let healthy_chat = setup.create_group(&alice).await;

    // Chats are updated ordered by their self-update time, so the broken chat comes first.
    alice
        .set_self_updated_at(broken_chat, DateTime::UNIX_EPOCH)
        .await
        .unwrap();
    let healthy_updated_at = DateTime::UNIX_EPOCH + Duration::seconds(1);
    alice
        .set_self_updated_at(healthy_chat, healthy_updated_at)
        .await
        .unwrap();

    alice.corrupt_mls_group_state(broken_chat).await.unwrap();

    alice
        .outbound_service()
        .schedule_self_update(DateTime::UNIX_EPOCH)
        .await
        .unwrap();
    alice.outbound_service().run_once().await;

    let at = alice.self_updated_at(healthy_chat).await.unwrap().unwrap();
    assert!(
        healthy_updated_at < at,
        "Chat after the broken one was not self-updated"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn self_update_skips_inactive_chats() {
    let mut setup = TestBackend::single().await;

    let [alice, bob] = setup.add_users().await;
    setup.connect_users(&alice, &bob).await;
    let active_chat = setup.create_group(&alice).await;

    // Bob removes Alice from his group, which turns her chat inactive but
    // keeps the group. A left chat has no group left to self-update.
    let inactive_chat = setup.create_group(&bob).await;
    setup.invite_to_group(inactive_chat, &bob, &[&alice]).await;
    setup
        .remove_from_group(inactive_chat, &bob, &[&alice])
        .await
        .unwrap();

    // Set self_updated_at to the past for both chats
    alice
        .set_self_updated_at(active_chat, DateTime::UNIX_EPOCH)
        .await
        .unwrap();
    alice
        .set_self_updated_at(inactive_chat, DateTime::UNIX_EPOCH)
        .await
        .unwrap();

    // Run the outbound service
    alice
        .outbound_service()
        .schedule_self_update(DateTime::UNIX_EPOCH)
        .await
        .unwrap();
    alice.outbound_service().run_once().await;

    // The active chat should have been self-updated
    let active_at = alice.self_updated_at(active_chat).await.unwrap().unwrap();
    assert!(
        DateTime::UNIX_EPOCH < active_at,
        "Active chat was not self-updated"
    );

    // The inactive chat should NOT have been self-updated
    assert_eq!(
        alice.self_updated_at(inactive_chat).await.unwrap().unwrap(),
        DateTime::UNIX_EPOCH,
        "Inactive chat was self-updated even though it should not have been"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn apq_self_update_advances_both_timestamps() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let chat_id = setup.create_apq_group(&alice).await;

    let now = Utc::now();

    // Force both T and PQ self_updated_at to the past so the joint APQ
    // self-update is triggered.
    alice
        .set_self_updated_at(chat_id, DateTime::UNIX_EPOCH)
        .await
        .unwrap();
    alice
        .set_pq_self_updated_at(chat_id, DateTime::UNIX_EPOCH)
        .await
        .unwrap();

    // Run the outbound service to perform the self-update.
    alice
        .outbound_service()
        .schedule_self_update(DateTime::UNIX_EPOCH)
        .await
        .unwrap();
    alice.outbound_service().run_once().await;

    // Both T and PQ self_updated_at should now be advanced past now.
    let after_t = alice.self_updated_at(chat_id).await.unwrap().unwrap();
    let after_pq = alice.pq_self_updated_at(chat_id).await.unwrap().unwrap();

    assert!(now < after_t, "T self_updated_at was not updated");
    assert!(now < after_pq, "PQ self_updated_at was not updated");
}
