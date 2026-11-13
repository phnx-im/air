// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Blocked contacts reaching the sibling.

use aircoreclient::{ChatId, ChatStatus, clients::CoreUser};
use airserver_test_harness::utils::{core_user_ext::CoreUserExt, setup::TestBackend};

use super::self_chat_id;

async fn is_blocked(device: &CoreUser, chat_id: ChatId) -> bool {
    matches!(
        device.expect_chat(chat_id).await.status(),
        ChatStatus::Blocked
    )
}

async fn has_pending_self_group_commit(device: &CoreUser) -> anyhow::Result<bool> {
    let self_chat_id = self_chat_id(device).await;
    Ok(device
        .pending_chat_operation_info(self_chat_id)
        .await?
        .is_some())
}

// Both devices change the blocked state of a contact before processing each
// other's commit. The DS accepts the first commit and rejects the second with
// a wrong epoch. The loser keeps its change parked through the winner's commit
// and sends it again. Both devices converge on the change the DS accepted last.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn concurrent_block_changes_converge() -> anyhow::Result<()> {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    let chat_id = setup.connect_users(&alice, &bob).await;
    let alice2 = alice.add_synced_device().await;

    alice.block_contact(bob.user_id().clone()).await?;
    alice2.unblock_contact(bob.user_id().clone()).await?;
    assert!(is_blocked(&alice, chat_id).await);
    assert!(!is_blocked(&alice2, chat_id).await);

    alice.outbound_service().run_once().await;
    alice2.outbound_service().run_once().await;
    assert!(
        has_pending_self_group_commit(&alice2).await?,
        "the losing commit should be parked"
    );

    alice2.drain().await;
    assert!(
        !is_blocked(&alice2, chat_id).await,
        "the parked unblock outlasts the winner's commit"
    );

    alice2.outbound_service().run_once().await;
    assert!(!has_pending_self_group_commit(&alice2).await?);
    alice.drain().await;
    alice2.drain().await;

    assert!(!is_blocked(&alice, chat_id).await);
    assert!(!is_blocked(&alice2, chat_id).await);

    Ok(())
}
