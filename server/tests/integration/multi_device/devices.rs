// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Managing linked devices: unlinking and renaming.

use aircoreclient::UnlinkReason;
use airserver_test_harness::utils::{core_user_ext::CoreUserExt, setup::TestBackend};
use uuid::Uuid;

/// Unlinking drops exactly the target's leaf and leaves the remover in place.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn unlink_removes_only_target_leaf() -> anyhow::Result<()> {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let alice2 = alice.add_linked_device().await;
    alice.drain().await;
    alice2.drain().await;

    let a_id = alice.own_client_id().await?;
    let b_id = alice2.own_client_id().await?;

    alice.unlink_device(b_id).await?;

    let members = alice.self_group_client_ids().await?;
    assert_eq!(members, vec![a_id], "only A must remain in the self group");

    // The removed device must be able to follow the commit that removed it,
    // which is what lets it notice and tear itself down.
    alice2.drain().await;

    Ok(())
}

/// The unlinked device notices on its next queue drain and flags itself. The
/// flag is what the app watches to delete its local data and return to the
/// welcome screen.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn unlinked_device_flags_itself() -> anyhow::Result<()> {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let alice2 = alice.add_linked_device().await;
    alice.drain().await;
    alice2.drain().await;

    let b_id = alice2.own_client_id().await?;
    assert_eq!(alice2.account_unlink_reason().await?, None);

    alice.unlink_device(b_id).await?;
    alice2.drain().await;

    assert_eq!(
        alice2.account_unlink_reason().await?,
        Some(UnlinkReason::Unlinked),
        "the removed device must know it was unlinked"
    );
    assert_eq!(
        alice.account_unlink_reason().await?,
        None,
        "the remover must not flag itself"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn unlink_unknown_client_id_fails() -> anyhow::Result<()> {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let alice2 = alice.add_linked_device().await;
    alice.drain().await;
    alice2.drain().await;

    let error = alice
        .unlink_device(Uuid::from_u128(0xbeef))
        .await
        .expect_err("unlinking an unknown client id must fail");
    assert!(
        error.to_string().contains("not in the self group"),
        "unexpected error: {error}"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn unlink_self_fails() -> anyhow::Result<()> {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let alice2 = alice.add_linked_device().await;
    alice.drain().await;
    alice2.drain().await;

    let a_id = alice.own_client_id().await?;
    let error = alice
        .unlink_device(a_id)
        .await
        .expect_err("a device cannot unlink itself");
    assert!(
        error.to_string().contains("itself"),
        "unexpected error: {error}"
    );

    Ok(())
}

/// A rename on one device reaches the other through the self group.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn rename_propagates_to_sibling() -> anyhow::Result<()> {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let alice2 = alice.add_linked_device().await;
    alice.drain().await;
    alice2.drain().await;

    let b_id = alice2.own_client_id().await?;
    alice2.rename_device(b_id, "Phone".to_owned()).await?;
    alice2.outbound_service().run_once().await;
    alice.drain().await;

    let seen = alice
        .linked_devices()
        .await?
        .into_iter()
        .find(|device| device.client_id == b_id)
        .expect("A must know about B");
    assert_eq!(seen.name, "Phone");

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[ignore = "an unlinked device keeps receiving and decrypting new messages"]
async fn unlinked_device_cannot_read_new_messages() -> anyhow::Result<()> {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    let chat_id = setup.connect_users(&alice, &bob).await;
    let alice2 = alice.add_synced_device().await;

    let b_id = alice2.own_client_id().await?;
    alice.unlink_device(b_id).await?;
    alice2.drain_unchecked().await;
    bob.drain().await;

    // The unlinked device ignores its flag and keeps fetching.
    bob.send_text(chat_id, "after unlink").await;
    alice2.drain_unchecked().await;

    assert!(
        !alice2
            .text_messages(chat_id)
            .await
            .iter()
            .any(|text| text.contains("after unlink")),
        "the unlinked device read a message sent after the unlink"
    );

    Ok(())
}
