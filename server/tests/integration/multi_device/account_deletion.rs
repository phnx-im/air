// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! An account deletion reaching the sibling through the closed self group.

use aircoreclient::{AccountDeletionStatus, ReadReceiptsSetting, UnlinkReason, clients::CoreUser};
use airserver_test_harness::utils::{core_user_ext::CoreUserExt, setup::TestBackend};

use super::{link_sibling, self_chat_id};

/// Deletes the self group the way an account deletion does.
async fn delete_self_group(device: &CoreUser) -> anyhow::Result<()> {
    device.delete_chat(self_chat_id(device).await).await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn account_deletion_reaches_offline_sibling() -> anyhow::Result<()> {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let alice2 = link_sibling(&alice).await;

    delete_self_group(&alice).await?;

    alice2.drain().await;
    assert_eq!(
        alice2.account_unlink_reason().await?,
        Some(UnlinkReason::AccountDeleted)
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn account_deletion_found_on_self_group_resync() -> anyhow::Result<()> {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let alice2 = link_sibling(&alice).await;

    delete_self_group(&alice).await?;

    // The sibling resyncs without processing the announcement in its queue.
    alice2
        .enqueue_group_resync(self_chat_id(&alice2).await)
        .await?;
    alice2.outbound_service().run_once().await;

    assert_eq!(
        alice2.account_unlink_reason().await?,
        Some(UnlinkReason::AccountDeleted)
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn account_deletion_found_by_self_group_operation() -> anyhow::Result<()> {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let alice2 = link_sibling(&alice).await;
    // The sibling's self group is gone once it ends the self chat.
    let self_chat_id = self_chat_id(&alice2).await;

    delete_self_group(&alice).await?;

    // Without processing the deletion, the sibling commits a setting change
    // to the self group, which the DS rejects as closed.
    alice2
        .set_synced_user_setting(&ReadReceiptsSetting(true))
        .await?;
    alice2.outbound_service().run_once().await;
    assert_eq!(
        alice2.account_unlink_reason().await?,
        Some(UnlinkReason::AccountDeleted)
    );

    // The queued resync ends the self chat.
    alice2.outbound_service().run_once().await;
    let self_chat = alice2
        .chat(&self_chat_id)
        .await
        .expect("the self chat is kept");
    assert!(self_chat.status().is_inactive());
    assert_eq!(
        alice2.account_unlink_reason().await?,
        Some(UnlinkReason::AccountDeleted)
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn account_deletion_retries_after_sibling_commit() -> anyhow::Result<()> {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let alice2 = link_sibling(&alice).await;

    alice2
        .set_synced_user_setting(&ReadReceiptsSetting(true))
        .await?;
    alice2.outbound_service().run_once().await;
    assert!(
        alice2.self_group_epochs().await? > alice.self_group_epochs().await?,
        "the sibling commit must be ahead of the deleting device"
    );

    alice.outbound_service().start_and_wait().await;
    assert_eq!(
        alice.delete_account().await?,
        AccountDeletionStatus::Pending
    );

    // Catching up on the sibling commit unblocks the deletion.
    alice.stop_outbound_service().await;
    alice.drain().await;
    alice.outbound_service().start_and_wait().await;
    assert_eq!(
        alice.delete_account().await?,
        AccountDeletionStatus::Completed
    );

    alice2.drain().await;
    assert_eq!(
        alice2.account_unlink_reason().await?,
        Some(UnlinkReason::AccountDeleted)
    );

    Ok(())
}
