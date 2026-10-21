// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Key package upload via the self group, and siblings deriving and using the
//! uploaded key packages.

use airserver_test_harness::utils::{core_user_ext::CoreUserExt, setup::TestBackend};
use chrono::Utc;

use super::link_sibling;

/// Key packages uploaded via the self-group only go live after the `DsCommitResponse` carrying
/// the batch id arrives through the queue.
#[tokio::test(flavor = "multi_thread")]
async fn key_package_upload_via_self_group_waits_for_commit_response() -> anyhow::Result<()> {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let _alice2 = link_sibling(&alice).await;

    // Baseline: the initial publish-path upload from user creation
    let live_before = alice.live_key_package_refs().await?;
    let (t_epoch, pq_epoch) = alice
        .self_group_epochs()
        .await?
        .expect("self-group should be persisted");
    let pq_epoch = pq_epoch.expect("self-group must be APQ");

    // Trigger the key package upload: stages at QS, commits via the self-group, and leaves the job
    // waiting for the queue-delivered commit response.
    alice
        .outbound_service()
        .schedule_key_package_upload(Utc::now())
        .await?;
    alice.outbound_service().run_once().await;

    // After the DS ack, the job must wait for the commit response: commit
    // unmerged, no refs flipped.
    let info = alice
        .self_group_pending_operation_info()
        .await?
        .expect("upload job should be pending");
    assert_eq!(info.operation_type, "self_group_kp_upload");
    assert_eq!(info.request_status, "waiting_for_queue_response");
    assert_eq!(
        alice.live_key_package_refs().await?,
        live_before,
        "refs must not go live before the commit response"
    );
    assert_eq!(
        alice.self_group_epochs().await?,
        Some((t_epoch, Some(pq_epoch))),
        "commit must not be merged before the commit response"
    );

    // Process the DsCommitResponse
    let processed = alice.drain().await.processed;
    assert!(processed >= 1, "expected at least the commit response");

    // The response finalizes the upload: job gone, commit merged in both
    // groups, new batch live.
    assert!(
        alice.self_group_pending_operation_info().await?.is_none(),
        "job should be deleted after the echo"
    );

    let (new_t_epoch, new_pq_epoch) = alice.self_group_epochs().await?.unwrap();
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
    let (live_plain_refs, live_apq_refs) = alice.live_key_package_refs().await?;
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
async fn single_device_uploads_key_packages_via_publish() -> anyhow::Result<()> {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;

    alice.ensure_self_group().await?;
    let live_before = alice.live_key_package_refs().await?;
    let epochs_before = alice.self_group_epochs().await?;

    alice
        .outbound_service()
        .schedule_key_package_upload(Utc::now())
        .await?;
    alice.outbound_service().run_once().await;

    // Without siblings there is nobody to derive the batch from a commit, so
    // the self group is left alone and the batch goes live right away.
    assert!(alice.self_group_pending_operation_info().await?.is_none());
    assert_eq!(alice.self_group_epochs().await?, epochs_before);
    let live_after = alice.live_key_package_refs().await?;
    assert_ne!(live_after.0, live_before.0, "plain refs should be replaced");
    assert_ne!(live_after.1, live_before.1, "APQ refs should be replaced");

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn key_packages_from_self_group_upload_are_served_and_usable() -> anyhow::Result<()> {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    setup.connect_users(&alice, &bob).await;
    let _alice2 = link_sibling(&alice).await;

    // Full upload cycle for alice
    {
        alice
            .outbound_service()
            .schedule_key_package_upload(Utc::now())
            .await?;
        alice.outbound_service().run_once().await;
        alice.drain().await;
        assert!(
            alice.self_group_pending_operation_info().await?.is_none(),
            "upload cycle should have completed"
        );
    }

    // Plain group: the invite consumes one of alice's T key packages from the batch
    let chat_id = setup.create_group(&bob).await;
    setup.invite_to_group(chat_id, &bob, &[&alice]).await;
    setup.send_message(chat_id, &bob, &[&alice]).await;
    setup.send_message(chat_id, &alice, &[&bob]).await;

    // APQ group: the invite consumes one of alice's APQ key packages from the batch
    let apq_chat_id = setup.create_apq_group(&bob).await;
    setup.invite_to_group(apq_chat_id, &bob, &[&alice]).await;
    setup.send_message(apq_chat_id, &bob, &[&alice]).await;
    setup.send_message(apq_chat_id, &alice, &[&bob]).await;

    Ok(())
}

/// A failed staging request (network error) leaves no job and no state changes behind; a later
/// run completes a full cycle.
#[tokio::test(flavor = "multi_thread")]
async fn key_package_upload_recovers_from_staging_failure() -> anyhow::Result<()> {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let _alice2 = link_sibling(&alice).await;

    let live_before = alice.live_key_package_refs().await?;
    let epochs_before = alice.self_group_epochs().await?;

    // The staging request at the QS fails with a network error. Drop *all* requests: run_once
    // issues unrelated requests (token replenishment, receipts, queue fetch) before the staging
    // one, so a single-shot drop_next_request would hit the wrong request.
    setup.listener_control_handle().set_drop_all();
    alice
        .outbound_service()
        .schedule_key_package_upload(Utc::now())
        .await?;
    alice.outbound_service().run_once().await;
    setup.listener_control_handle().set_normal();

    assert!(
        alice.self_group_pending_operation_info().await?.is_none(),
        "failed staging must not leave a job behind"
    );
    assert_eq!(
        alice.self_group_epochs().await?,
        epochs_before,
        "no commit must be staged after a failed staging request"
    );
    assert_eq!(
        alice.live_key_package_refs().await?,
        live_before,
        "live refs must be unchanged after a failed staging request"
    );

    // A later run completes the full cycle
    alice
        .outbound_service()
        .schedule_key_package_upload(Utc::now())
        .await?;
    alice.outbound_service().run_once().await;
    alice.drain().await;

    assert!(alice.self_group_pending_operation_info().await?.is_none());
    let live_after = alice.live_key_package_refs().await?;
    assert_ne!(live_after.0, live_before.0, "plain refs should be replaced");
    assert_ne!(live_after.1, live_before.1, "APQ refs should be replaced");

    Ok(())
}

/// While an upload is in flight, another upload run backs off without disturbing it; after
/// completion, a full second cycle works.
#[tokio::test(flavor = "multi_thread")]
async fn key_package_upload_backs_off_while_pending() -> anyhow::Result<()> {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let _alice2 = link_sibling(&alice).await;

    // First upload: job ends up waiting for the commit response
    alice
        .outbound_service()
        .schedule_key_package_upload(Utc::now())
        .await?;
    alice.outbound_service().run_once().await;

    let info = alice
        .self_group_pending_operation_info()
        .await?
        .expect("upload job should be pending");
    let epochs = alice.self_group_epochs().await?;

    // A second run must back off: same job, no new commit staged
    alice
        .outbound_service()
        .schedule_key_package_upload(Utc::now())
        .await?;
    alice.outbound_service().run_once().await;

    let info_after = alice
        .self_group_pending_operation_info()
        .await?
        .expect("upload job should still be pending");
    assert_eq!(info_after.request_status, info.request_status);
    assert_eq!(info_after.number_of_attempts, info.number_of_attempts);
    assert_eq!(alice.self_group_epochs().await?, epochs);

    // The commit response completes the first upload despite the interleaved run
    alice.drain().await;
    assert!(alice.self_group_pending_operation_info().await?.is_none());
    let live_first = alice.live_key_package_refs().await?;

    // A full second cycle replaces the batch again
    alice
        .outbound_service()
        .schedule_key_package_upload(Utc::now())
        .await?;
    alice.outbound_service().run_once().await;
    alice.drain().await;

    assert!(alice.self_group_pending_operation_info().await?.is_none());
    let live_second = alice.live_key_package_refs().await?;
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
    let alice = setup.add_user().await;
    let _alice2 = link_sibling(&alice).await;

    let live_before = alice.live_key_package_refs().await?;
    let (t_epoch, _) = alice
        .self_group_epochs()
        .await?
        .expect("self-group should be persisted");

    alice
        .outbound_service()
        .schedule_key_package_upload(Utc::now())
        .await?;
    alice.outbound_service().run_once().await;

    // Fork the local job state from the commit the DS accepted, then let the
    // real `DsCommitResponse` arrive through the queue.
    alice.corrupt_self_group_upload_batch_id().await?;
    alice.drain().await;

    // The commit is merged (the group must not fall behind the DS), but the
    // job is abandoned without marking the batch live.
    assert!(
        alice.self_group_pending_operation_info().await?.is_none(),
        "the mismatched job should be abandoned"
    );
    let (new_t_epoch, _) = alice.self_group_epochs().await?.unwrap();
    assert_eq!(new_t_epoch.as_u64(), t_epoch.as_u64() + 1);
    assert_eq!(
        alice.live_key_package_refs().await?,
        live_before,
        "an abandoned upload must not change the live set"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn multi_device_sibling_derives_key_packages_from_upload_commit() -> anyhow::Result<()> {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let alice2 = alice.add_linked_device().await;

    // Both devices start from the merged linking commit.
    alice.drain().await;
    alice2.drain().await;

    let live_before = alice2.live_key_package_refs().await?;
    assert_eq!(
        live_before,
        (vec![], vec![]),
        "a freshly linked device has no key packages of its own"
    );

    // Device A runs a full upload cycle, including the echo that marks the batch live.
    {
        alice
            .outbound_service()
            .schedule_key_package_upload(Utc::now())
            .await?;
        alice.outbound_service().run_once().await;
        alice.drain().await;
        assert!(
            alice.self_group_pending_operation_info().await?.is_none(),
            "upload cycle should have completed"
        );
    }
    let live_a = alice.live_key_package_refs().await?;
    assert!(
        !live_a.0.is_empty() && !live_a.1.is_empty(),
        "the uploader should have live plain and APQ key packages"
    );

    // Processing A's self-group commit makes A's batch B's live set too.
    alice2.drain().await;
    assert_eq!(
        alice2.live_key_package_refs().await?,
        live_a,
        "the sibling must derive exactly the batch the uploader published"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn multi_device_key_packages_from_sibling_upload_are_usable_by_the_sibling()
-> anyhow::Result<()> {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    setup.connect_users(&alice, &bob).await;

    let alice2 = alice.add_linked_device().await;
    alice.drain().await;
    alice2.drain().await;

    // Device A publishes a fresh batch via the self-group.
    {
        alice
            .outbound_service()
            .schedule_key_package_upload(Utc::now())
            .await?;
        alice.outbound_service().run_once().await;
        alice.drain().await;
        assert!(
            alice.self_group_pending_operation_info().await?.is_none(),
            "upload cycle should have completed"
        );
    }

    // Device B derives the batch from A's commit.
    alice2.drain().await;

    // Bob invites Alice, consuming a key package from the batch.
    let chat_id = setup.create_group(&bob).await;
    setup.invite_to_group(chat_id, &bob, &[&alice]).await;

    // Device B joins from the same welcome, using key material it derived rather than generated.
    let processed = alice2.drain().await;
    assert!(
        processed
            .new_chats
            .iter()
            .any(|new_chat| new_chat.chat_id == chat_id),
        "the sibling should have joined the group from the welcome"
    );

    // And it stays a working member of the group.
    setup.send_message(chat_id, &bob, &[&alice]).await;
    let processed = alice2.drain().await;
    assert!(
        processed
            .new_messages
            .iter()
            .any(|message| message.chat_id() == chat_id),
        "the sibling should receive messages sent into the group"
    );

    Ok(())
}
