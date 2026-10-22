// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Device limit when linking, and shrinking a self group above the limit.

use std::collections::HashSet;

use aircoreclient::ReadReceiptsSetting;
use airserver_test_harness::utils::{
    core_user_ext::CoreUserExt,
    setup::{TestBackend, TestBackendParams},
};
use chrono::{DateTime, Utc};

use super::{assert_link_rejected_at_limit, read_receipts, self_chat_id, send_and_receive};

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn link_is_rejected_at_device_limit() -> anyhow::Result<()> {
    let mut setup = TestBackend::single_with_params(TestBackendParams {
        max_devices: Some(2),
        ..Default::default()
    })
    .await;
    let alice = setup.add_user().await;
    let alice2 = alice.add_linked_device().await;
    alice.drain().await;
    alice2.drain().await;

    assert_link_rejected_at_limit(&setup, &alice, 2).await?;

    // The self group is unchanged.
    let a_id = alice.own_client_id().await?;
    let b_id = alice2.own_client_id().await?;
    let members: HashSet<_> = alice.self_group_client_ids().await?.into_iter().collect();
    assert_eq!(members, HashSet::from([a_id, b_id]));

    // After unlinking a device, linking works again.
    alice.unlink_device(b_id).await?;
    let alice3 = alice.add_linked_device().await;
    let c_id = alice3.own_client_id().await?;
    let members: HashSet<_> = alice.self_group_client_ids().await?.into_iter().collect();
    assert_eq!(members, HashSet::from([a_id, c_id]));

    Ok(())
}

/// A self group above the device limit (e.g. after the limit was lowered)
/// still accepts commits that don't grow it, so the user can sync and unlink.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn self_group_above_limit_can_shrink() -> anyhow::Result<()> {
    let mut setup = TestBackend::single_with_params(TestBackendParams {
        max_devices: Some(3),
        ..Default::default()
    })
    .await;
    let alice = setup.add_user().await;
    let alice2 = alice.add_linked_device().await;
    alice2.drain().await;
    let alice3 = alice.add_linked_device().await;
    alice.drain().await;
    alice2.drain().await;
    alice3.drain().await;

    // Linking makes device 1 upload key packages, which completes only once
    // it fetches its own commit.
    alice.outbound_service().run_once().await;
    alice.drain().await;
    alice2.drain().await;
    alice3.drain().await;

    setup.set_max_devices(1);

    // Settings sync
    alice
        .set_synced_user_setting(&ReadReceiptsSetting(true))
        .await?;
    alice.outbound_service().run_once().await;
    assert!(!alice.has_pending_setting_changes().await?);
    alice2.drain().await;
    alice3.drain().await;
    assert_eq!(read_receipts(&alice2).await, Some(true));

    // Self update
    let chat_id = self_chat_id(&alice).await;
    alice
        .set_self_updated_at(chat_id, DateTime::UNIX_EPOCH)
        .await?;
    let before = Utc::now();
    alice
        .outbound_service()
        .schedule_self_update(DateTime::UNIX_EPOCH)
        .await?;
    alice.outbound_service().run_once().await;
    let after = alice.self_updated_at(chat_id).await?.unwrap();
    assert!(
        before < after,
        "the DS should accept a self-update above the limit"
    );
    alice2.drain().await;
    alice3.drain().await;

    // Growing is still rejected.
    assert_link_rejected_at_limit(&setup, &alice, 1).await?;

    // Unlinking shrinks the group while it stays above the limit.
    let a_id = alice.own_client_id().await?;
    let b_id = alice2.own_client_id().await?;
    let c_id = alice3.own_client_id().await?;
    alice.unlink_device(c_id).await?;
    alice2.drain().await;
    let members: HashSet<_> = alice.self_group_client_ids().await?.into_iter().collect();
    assert_eq!(members, HashSet::from([a_id, b_id]));

    send_and_receive(&alice, &[&alice2], chat_id, "after shrinking").await;

    Ok(())
}
