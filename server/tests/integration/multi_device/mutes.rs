// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Chat mute settings synced between siblingdevices.

use aircoreclient::{ChatId, ChatMuted, clients::CoreUser};
use airserver_test_harness::utils::{core_user_ext::CoreUserExt, setup::TestBackend};
use anyhow::Context as _;
use chrono::{Duration, Utc};

use super::self_chat_id;

async fn muted_until(device: &CoreUser, chat_id: ChatId) -> Option<ChatMuted> {
    device.expect_chat(chat_id).await.muted_until
}

/// Mutes the chat for eight hours and returns the mute the device stored.
///
/// The clock is finer than the milliseconds the wire format holds, so the
/// siblings only match if the device stores the value they receive.
async fn mute_for_eight_hours(device: &CoreUser, chat_id: ChatId) -> anyhow::Result<ChatMuted> {
    let until = ChatMuted::Until(Utc::now() + Duration::hours(8));
    device.set_chat_muted_until(chat_id, Some(until)).await?;
    muted_until(device, chat_id)
        .await
        .context("the chat should be muted")
}

async fn has_pending_self_group_commit(device: &CoreUser) -> anyhow::Result<bool> {
    let self_chat_id = self_chat_id(device).await;
    Ok(device
        .pending_chat_operation_info(self_chat_id)
        .await?
        .is_some())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn chat_mute_syncs_both_directions() -> anyhow::Result<()> {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    let chat_id = setup.connected_group(&alice, &[&bob]).await;
    let alice2 = alice.add_synced_device().await;
    assert_eq!(muted_until(&alice2, chat_id).await, None);

    alice
        .set_chat_muted_until(chat_id, Some(ChatMuted::Forever))
        .await?;
    assert_eq!(muted_until(&alice, chat_id).await, Some(ChatMuted::Forever));
    alice.outbound_service().run_once().await;
    assert!(
        !has_pending_self_group_commit(&alice).await?,
        "the commit should be gone after a successful send"
    );

    alice2.drain().await;
    assert_eq!(
        muted_until(&alice2, chat_id).await,
        Some(ChatMuted::Forever)
    );

    let until = mute_for_eight_hours(&alice2, chat_id).await?;
    alice2.outbound_service().run_once().await;
    alice.drain().await;
    assert_eq!(muted_until(&alice, chat_id).await, Some(until));

    alice2.set_chat_muted_until(chat_id, None).await?;
    alice2.outbound_service().run_once().await;
    alice.drain().await;
    assert_eq!(muted_until(&alice, chat_id).await, None);

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn a_linked_device_starts_with_the_mutes() -> anyhow::Result<()> {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    let muted_group = setup.connected_group(&alice, &[&bob]).await;
    let muted_forever = setup.create_group(&alice).await;
    let unmuted = setup.create_group(&alice).await;

    let until = mute_for_eight_hours(&alice, muted_group).await?;
    alice
        .set_chat_muted_until(muted_forever, Some(ChatMuted::Forever))
        .await?;
    // We send the commit before linking. The new device can only learn the
    // mutes from the linking payload.
    alice.outbound_service().run_once().await;
    assert!(!has_pending_self_group_commit(&alice).await?);

    let alice2 = alice.add_synced_device().await;

    assert_eq!(muted_until(&alice2, muted_group).await, Some(until));
    assert_eq!(
        muted_until(&alice2, muted_forever).await,
        Some(ChatMuted::Forever)
    );
    assert_eq!(muted_until(&alice2, unmuted).await, None);

    Ok(())
}

// The new device processes the sibling's commit before it onboards into the
// chats. The chats it creates later hold the changed mutes and not the ones
// from the linking snapshot.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn a_mute_changed_before_onboarding_reaches_the_new_device() -> anyhow::Result<()> {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    let unmuted_later = setup.connected_group(&alice, &[&bob]).await;
    let muted_later = setup.create_group(&alice).await;
    alice
        .set_chat_muted_until(unmuted_later, Some(ChatMuted::Forever))
        .await?;

    let alice2 = alice.add_linked_device().await;
    // The new device's key package commit would make ours stale.
    alice.drain().await;
    alice.set_chat_muted_until(unmuted_later, None).await?;
    let until = mute_for_eight_hours(&alice, muted_later).await?;
    alice.outbound_service().run_once().await;
    assert!(!has_pending_self_group_commit(&alice).await?);
    alice2.drain().await;

    alice2.outbound_service().run_once().await;
    alice.drain().await;
    alice2.drain().await;

    assert_eq!(muted_until(&alice2, unmuted_later).await, None);
    assert_eq!(muted_until(&alice2, muted_later).await, Some(until));

    Ok(())
}

// Both devices change the mute before processing each other's commit. The DS
// accepts the first commit and rejects the second with a wrong epoch. The
// loser keeps its change parked through the winner's commit and sends it
// again. Both devices converge on the change the DS accepted last.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn concurrent_mutes_converge() -> anyhow::Result<()> {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    let chat_id = setup.connected_group(&alice, &[&bob]).await;
    let alice2 = alice.add_synced_device().await;

    alice
        .set_chat_muted_until(chat_id, Some(ChatMuted::Forever))
        .await?;
    let until = mute_for_eight_hours(&alice2, chat_id).await?;

    alice.outbound_service().run_once().await;
    alice2.outbound_service().run_once().await;
    assert!(
        has_pending_self_group_commit(&alice2).await?,
        "the losing commit should be parked"
    );

    alice2.drain().await;
    assert_eq!(
        muted_until(&alice2, chat_id).await,
        Some(until),
        "the parked change outlasts the winner's commit"
    );

    alice2.outbound_service().run_once().await;
    assert!(!has_pending_self_group_commit(&alice2).await?);
    alice.drain().await;
    alice2.drain().await;

    assert_eq!(muted_until(&alice, chat_id).await, Some(until));
    assert_eq!(muted_until(&alice2, chat_id).await, Some(until));

    Ok(())
}
