// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircoreclient::clients::{CoreUser, process::process_qs::ProcessedQsMessages};
use airserver_test_harness::utils::setup::TestBackend;
use chrono::Utc;

use crate::tests::multi_device::link_new_device;

/// Fetches and processes a device's queue, asserting that every message was
/// processed without error.
async fn drain_queue(device: &CoreUser) -> anyhow::Result<ProcessedQsMessages> {
    let messages = device.qs_fetch_messages().await?;
    let expected = messages.len();
    let processed = device.fully_process_qs_messages(messages).await;
    assert!(
        processed.errors.is_empty(),
        "queue processing failed: {:?}",
        processed.errors
    );
    assert_eq!(processed.processed, expected, "not all messages processed");
    Ok(processed)
}

#[tokio::test(flavor = "multi_thread")]
async fn ensure_self_group_creates_apq_group() -> anyhow::Result<()> {
    let mut setup = TestBackend::single().await;
    let user_id = setup.add_user().await;
    let user = &setup.get_user(&user_id).user;

    user.ensure_self_group().await?;

    let is_apq = user
        .self_group_is_apq()
        .await?
        .expect("self group should be persisted");
    assert!(is_apq, "self-group must be an APQ (T+PQ) group");

    Ok(())
}

/// Key packages uploaded via the self-group only go live after the `DsCommitResponse` carrying
/// the batch id arrives through the queue.
#[tokio::test(flavor = "multi_thread")]
async fn key_package_upload_via_self_group_waits_for_commit_response() -> anyhow::Result<()> {
    let mut setup = TestBackend::single().await;
    let user_id = setup.add_user().await;
    let test_user = setup.get_user(&user_id);
    let user = &test_user.user;

    user.ensure_self_group().await?;

    // Baseline: the initial publish-path upload from user creation
    let live_before = user.live_key_package_refs().await?;
    let (t_epoch, pq_epoch) = user
        .self_group_epochs()
        .await?
        .expect("self-group should be persisted");
    let pq_epoch = pq_epoch.expect("self-group must be APQ");

    // Trigger the key package upload: stages at QS, commits via the self-group, and leaves the job
    // waiting for the queue-delivered commit response.
    user.outbound_service()
        .schedule_key_package_upload(Utc::now())
        .await?;
    user.outbound_service().run_once().await;

    // After the DS ack, the job must wait for the commit response: commit
    // unmerged, no refs flipped.
    let info = user
        .self_group_pending_operation_info()
        .await?
        .expect("upload job should be pending");
    assert_eq!(info.operation_type, "self_group_kp_upload");
    assert_eq!(info.request_status, "waiting_for_queue_response");
    assert_eq!(
        user.live_key_package_refs().await?,
        live_before,
        "refs must not go live before the commit response"
    );
    assert_eq!(
        user.self_group_epochs().await?,
        Some((t_epoch, Some(pq_epoch))),
        "commit must not be merged before the commit response"
    );

    // Process the DsCommitResponse
    let processed = test_user.fetch_and_process_qs_messages().await;
    assert!(processed >= 1, "expected at least the commit response");

    // The response finalizes the upload: job gone, commit merged in both
    // groups, new batch live.
    assert!(
        user.self_group_pending_operation_info().await?.is_none(),
        "job should be deleted after the echo"
    );

    let (new_t_epoch, new_pq_epoch) = user.self_group_epochs().await?.unwrap();
    assert_eq!(
        new_t_epoch.as_u64(),
        t_epoch.as_u64() + 1,
        "T epoch must advance by one"
    );
    assert_eq!(
        new_pq_epoch.unwrap().as_u64(),
        pq_epoch.as_u64() + 1,
        "PQ epoch must advance by one"
    );

    let (live_before_plain_refs, live_before_apq_refs) = live_before;
    let (live_plain_refs, live_apq_refs) = user.live_key_package_refs().await?;
    assert_ne!(
        live_plain_refs, live_before_plain_refs,
        "plain refs should be replaced"
    );
    assert_ne!(
        live_apq_refs, live_before_apq_refs,
        "APQ refs should be replaced"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn key_packages_from_self_group_upload_are_served_and_usable() -> anyhow::Result<()> {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let bob = setup.add_user().await;
    setup.connect_users(&alice, &bob).await;

    // Full upload cycle for alice
    {
        let test_user = setup.get_user(&alice);
        let user = &test_user.user;
        user.ensure_self_group().await?;
        user.outbound_service()
            .schedule_key_package_upload(Utc::now())
            .await?;
        user.outbound_service().run_once().await;
        test_user.fetch_and_process_qs_messages().await;
        assert!(
            user.self_group_pending_operation_info().await?.is_none(),
            "upload cycle should have completed"
        );
    }

    // Plain group: the invite consumes one of alice's T key packages from the batch
    let chat_id = setup.create_group(&bob).await;
    setup.invite_to_group(chat_id, &bob, vec![&alice]).await;
    setup.send_message(chat_id, &bob, vec![&alice], None).await;
    setup.send_message(chat_id, &alice, vec![&bob], None).await;

    // APQ group: the invite consumes one of alice's APQ key packages from the batch
    let apq_chat_id = setup.create_apq_group(&bob).await;
    setup.invite_to_group(apq_chat_id, &bob, vec![&alice]).await;
    setup
        .send_message(apq_chat_id, &bob, vec![&alice], None)
        .await;
    setup
        .send_message(apq_chat_id, &alice, vec![&bob], None)
        .await;

    Ok(())
}

/// A failed staging request (network error) leaves no job and no state changes behind; a later
/// run completes a full cycle.
#[tokio::test(flavor = "multi_thread")]
async fn key_package_upload_recovers_from_staging_failure() -> anyhow::Result<()> {
    let mut setup = TestBackend::single().await;
    let user_id = setup.add_user().await;
    let test_user = setup.get_user(&user_id);
    let user = &test_user.user;

    user.ensure_self_group().await?;
    let live_before = user.live_key_package_refs().await?;
    let epochs_before = user.self_group_epochs().await?;

    // The staging request at the QS fails with a network error. Drop *all* requests: run_once
    // issues unrelated requests (token replenishment, receipts, queue fetch) before the staging
    // one, so a single-shot drop_next_request would hit the wrong request.
    setup.listener_control_handle().set_drop_all();
    user.outbound_service()
        .schedule_key_package_upload(Utc::now())
        .await?;
    user.outbound_service().run_once().await;
    setup.listener_control_handle().set_normal();

    assert!(
        user.self_group_pending_operation_info().await?.is_none(),
        "failed staging must not leave a job behind"
    );
    assert_eq!(
        user.self_group_epochs().await?,
        epochs_before,
        "no commit must be staged after a failed staging request"
    );
    assert_eq!(
        user.live_key_package_refs().await?,
        live_before,
        "live refs must be unchanged after a failed staging request"
    );

    // A later run completes the full cycle
    user.outbound_service()
        .schedule_key_package_upload(Utc::now())
        .await?;
    user.outbound_service().run_once().await;
    test_user.fetch_and_process_qs_messages().await;

    assert!(user.self_group_pending_operation_info().await?.is_none());
    let live_after = user.live_key_package_refs().await?;
    assert_ne!(live_after.0, live_before.0, "plain refs should be replaced");
    assert_ne!(live_after.1, live_before.1, "APQ refs should be replaced");

    Ok(())
}

/// While an upload is in flight, another upload run backs off without disturbing it; after
/// completion, a full second cycle works.
#[tokio::test(flavor = "multi_thread")]
async fn key_package_upload_backs_off_while_pending() -> anyhow::Result<()> {
    let mut setup = TestBackend::single().await;
    let user_id = setup.add_user().await;
    let test_user = setup.get_user(&user_id);
    let user = &test_user.user;

    user.ensure_self_group().await?;

    // First upload: job ends up waiting for the commit response
    user.outbound_service()
        .schedule_key_package_upload(Utc::now())
        .await?;
    user.outbound_service().run_once().await;

    let info = user
        .self_group_pending_operation_info()
        .await?
        .expect("upload job should be pending");
    let epochs = user.self_group_epochs().await?;

    // A second run must back off: same job, no new commit staged
    user.outbound_service()
        .schedule_key_package_upload(Utc::now())
        .await?;
    user.outbound_service().run_once().await;

    let info_after = user
        .self_group_pending_operation_info()
        .await?
        .expect("upload job should still be pending");
    assert_eq!(info_after.request_status, info.request_status);
    assert_eq!(info_after.number_of_attempts, info.number_of_attempts);
    assert_eq!(user.self_group_epochs().await?, epochs);

    // The commit response completes the first upload despite the interleaved run
    test_user.fetch_and_process_qs_messages().await;
    assert!(user.self_group_pending_operation_info().await?.is_none());
    let live_first = user.live_key_package_refs().await?;

    // A full second cycle replaces the batch again
    user.outbound_service()
        .schedule_key_package_upload(Utc::now())
        .await?;
    user.outbound_service().run_once().await;
    test_user.fetch_and_process_qs_messages().await;

    assert!(user.self_group_pending_operation_info().await?.is_none());
    let live_second = user.live_key_package_refs().await?;
    assert_ne!(live_second.0, live_first.0);
    assert_ne!(live_second.1, live_first.1);

    Ok(())
}

/// A `DsCommitResponse` whose batch id does not match the pending upload job (state fork)
/// abandons the job: the commit is merged and the job deleted, but the batch does NOT go live
/// and the previous batch keeps serving.
#[tokio::test(flavor = "multi_thread")]
async fn mismatched_upload_commit_abandons_job() -> anyhow::Result<()> {
    let mut setup = TestBackend::single().await;
    let user_id = setup.add_user().await;
    let test_user = setup.get_user(&user_id);
    let user = &test_user.user;

    user.ensure_self_group().await?;
    let live_before = user.live_key_package_refs().await?;
    let (t_epoch, _) = user
        .self_group_epochs()
        .await?
        .expect("self-group should be persisted");

    user.outbound_service()
        .schedule_key_package_upload(Utc::now())
        .await?;
    user.outbound_service().run_once().await;

    // Fork the local job state from the commit the DS accepted, then let the
    // real `DsCommitResponse` arrive through the queue.
    user.corrupt_self_group_upload_batch_id().await?;
    test_user.fetch_and_process_qs_messages().await;

    // The commit is merged (the group must not fall behind the DS), but the
    // job is abandoned without marking the batch live.
    assert!(
        user.self_group_pending_operation_info().await?.is_none(),
        "the mismatched job should be abandoned"
    );
    let (new_t_epoch, _) = user.self_group_epochs().await?.unwrap();
    assert_eq!(new_t_epoch.as_u64(), t_epoch.as_u64() + 1);
    assert_eq!(
        user.live_key_package_refs().await?,
        live_before,
        "an abandoned upload must not change the live set"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn multi_device_sibling_derives_key_packages_from_upload_commit() -> anyhow::Result<()> {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let (device_b, _tmp) = link_new_device(&setup, &alice).await;

    // Both devices start from the merged linking commit.
    setup.get_user(&alice).fetch_and_process_qs_messages().await;
    drain_queue(&device_b).await?;

    let live_before = device_b.live_key_package_refs().await?;
    assert_eq!(
        live_before,
        (vec![], vec![]),
        "a freshly linked device has no key packages of its own"
    );

    // Device A runs a full upload cycle, including the echo that marks the batch live.
    {
        let test_user = setup.get_user(&alice);
        let user = &test_user.user;
        user.outbound_service()
            .schedule_key_package_upload(Utc::now())
            .await?;
        user.outbound_service().run_once().await;
        test_user.fetch_and_process_qs_messages().await;
        assert!(
            user.self_group_pending_operation_info().await?.is_none(),
            "upload cycle should have completed"
        );
    }
    let live_a = setup.get_user(&alice).user.live_key_package_refs().await?;
    assert!(
        !live_a.0.is_empty() && !live_a.1.is_empty(),
        "the uploader should have live plain and APQ key packages"
    );

    // Processing A's self-group commit makes A's batch B's live set too.
    drain_queue(&device_b).await?;
    assert_eq!(
        device_b.live_key_package_refs().await?,
        live_a,
        "the sibling must derive exactly the batch the uploader published"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn multi_device_key_packages_from_sibling_upload_are_usable_by_the_sibling()
-> anyhow::Result<()> {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let bob = setup.add_user().await;
    setup.connect_users(&alice, &bob).await;

    let (device_b, _tmp) = link_new_device(&setup, &alice).await;
    setup.get_user(&alice).fetch_and_process_qs_messages().await;
    drain_queue(&device_b).await?;

    // Device A publishes a fresh batch via the self-group.
    {
        let test_user = setup.get_user(&alice);
        let user = &test_user.user;
        user.outbound_service()
            .schedule_key_package_upload(Utc::now())
            .await?;
        user.outbound_service().run_once().await;
        test_user.fetch_and_process_qs_messages().await;
        assert!(
            user.self_group_pending_operation_info().await?.is_none(),
            "upload cycle should have completed"
        );
    }

    // Device B derives the batch from A's commit.
    drain_queue(&device_b).await?;

    // Bob invites Alice, consuming a key package from the batch.
    let chat_id = setup.create_group(&bob).await;
    setup.invite_to_group(chat_id, &bob, vec![&alice]).await;

    // Device B joins from the same welcome, using key material it derived rather than generated.
    let processed = drain_queue(&device_b).await?;
    assert!(
        processed.new_chats.contains(&chat_id),
        "the sibling should have joined the group from the welcome"
    );

    // And it stays a working member of the group.
    setup.send_message(chat_id, &bob, vec![&alice], None).await;
    let processed = drain_queue(&device_b).await?;
    assert!(
        processed
            .new_messages
            .iter()
            .any(|message| message.chat_id() == chat_id),
        "the sibling should receive messages sent into the group"
    );

    Ok(())
}
