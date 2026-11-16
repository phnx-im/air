// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Self-group credentials signed with the user signing key: signing, publishing with every own
//! self-group commit, and devices that sign next to devices that don't.

use aircommon::credentials::{LeafCredential, SelfGroupCredential};
use aircoreclient::{ReadReceiptsSetting, UnlinkReason, clients::CoreUser};
use airserver_test_harness::utils::{core_user_ext::CoreUserExt, setup::TestBackend};
use chrono::{DateTime, Utc};

use super::{link_sibling, self_chat_id, send_and_receive};

/// The credential on the own T leaf of `device`'s self group.
async fn own_leaf_credential(device: &CoreUser) -> SelfGroupCredential {
    let credential = device
        .self_group()
        .await
        .unwrap()
        .expect("device should have a self group")
        .own_credential()
        .unwrap();
    let LeafCredential::SelfGroup(credential) = credential else {
        panic!("self-group leaf must carry a SelfGroupCredential");
    };
    credential
}

/// The number of signed T leaves in `device`'s self group.
async fn signed_leaves(device: &CoreUser) -> usize {
    let credentials = device
        .self_group()
        .await
        .unwrap()
        .expect("device should have a self group")
        .credentials()
        .unwrap();
    credentials
        .iter()
        .filter(|credential| match credential {
            LeafCredential::SelfGroup(credential) => credential.is_signed(),
            LeafCredential::User(_) => false,
        })
        .count()
}

async fn assert_pq_credentials_empty(device: &CoreUser) {
    let contents = device
        .self_group()
        .await
        .unwrap()
        .expect("device should have a self group")
        .pq_credential_contents()
        .unwrap();
    assert!(
        contents.iter().all(Vec::is_empty),
        "the PQ leaves must keep their empty credential"
    );
}

/// Runs the self update of `device` in its self group through the self-update task.
///
/// With `pq`, both legs are due and the task sends an APQ self update. Otherwise only the T leg
/// is due and the task sends a T-only self update.
async fn self_update(device: &CoreUser, pq: bool) {
    let chat_id = self_chat_id(device).await;
    device
        .set_self_updated_at(chat_id, DateTime::UNIX_EPOCH)
        .await
        .unwrap();
    if pq {
        device
            .set_pq_self_updated_at(chat_id, DateTime::UNIX_EPOCH)
            .await
            .unwrap();
    }
    let before = Utc::now();
    device
        .outbound_service()
        .schedule_self_update(DateTime::UNIX_EPOCH)
        .await
        .unwrap();
    device.outbound_service().run_once().await;
    let after = device.self_updated_at(chat_id).await.unwrap().unwrap();
    assert!(
        before < after,
        "the DS should have accepted the self update"
    );
}

/// Puts `device` into the state of a device from before credentials were signed: its stored
/// credential and its own leaf are unsigned. `siblings` process the commit that unsigns the leaf.
async fn make_unsigned(device: &CoreUser, siblings: &[&CoreUser]) {
    device.unsign_stored_self_group_credential().await.unwrap();
    self_update(device, true).await;
    for sibling in siblings {
        sibling.drain().await;
    }
    assert!(!own_leaf_credential(device).await.is_signed());
}

/// Puts `device` into the state of a device that signed its stored credential on load but hasn't
/// published it yet: its own leaf is unsigned.
async fn make_unpublished(device: &CoreUser, siblings: &[&CoreUser]) {
    make_unsigned(device, siblings).await;
    device.sign_stored_self_group_credential().await.unwrap();
    assert!(
        device
            .stored_self_group_credential()
            .await
            .unwrap()
            .is_signed()
    );
}

/// Asserts that the own leaf of `device` carries its signed stored credential.
async fn assert_published(device: &CoreUser) {
    let stored = device.stored_self_group_credential().await.unwrap();
    assert!(stored.is_signed(), "the stored credential must be signed");
    assert_eq!(
        own_leaf_credential(device).await,
        stored,
        "the own leaf must carry the stored credential"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn new_self_group_and_linked_device_are_signed() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    alice.ensure_self_group().await.unwrap();
    assert_published(&alice).await;

    let alice2 = link_sibling(&alice).await;
    assert_published(&alice2).await;
    assert_eq!(signed_leaves(&alice).await, 2);
    assert_eq!(signed_leaves(&alice2).await, 2);
    assert_pq_credentials_empty(&alice).await;

    let chat_id = self_chat_id(&alice).await;
    send_and_receive(&alice, &[&alice2], chat_id, "signed").await;
    send_and_receive(&alice2, &[&alice], chat_id, "and back").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn an_unsigned_self_group_gets_signed() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let alice2 = link_sibling(&alice).await;
    make_unsigned(&alice, &[&alice2]).await;
    make_unsigned(&alice2, &[&alice]).await;
    assert_eq!(signed_leaves(&alice).await, 0);

    for (device, sibling) in [(&alice, &alice2), (&alice2, &alice)] {
        device.sign_stored_self_group_credential().await.unwrap();
        self_update(device, true).await;
        sibling.drain().await;
        assert_published(device).await;
    }
    assert_eq!(signed_leaves(&alice).await, 2);
    assert_eq!(signed_leaves(&alice2).await, 2);

    let chat_id = self_chat_id(&alice).await;
    send_and_receive(&alice, &[&alice2], chat_id, "signed").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn self_updates_publish_the_credential() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let alice2 = link_sibling(&alice).await;

    // The self-update task sends an APQ self update when both legs are due, and a T-only one
    // when only the T leg is due.
    for pq in [true, false] {
        make_unpublished(&alice, &[&alice2]).await;
        self_update(&alice, pq).await;
        alice2.drain().await;
        assert_published(&alice).await;
        assert_eq!(
            own_leaf_credential(&alice).await.client_id(),
            alice.own_client_id().await.unwrap()
        );
        assert_pq_credentials_empty(&alice).await;
        assert_eq!(signed_leaves(&alice2).await, 2);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn a_key_package_upload_publishes_the_credential() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let alice2 = link_sibling(&alice).await;
    make_unpublished(&alice, &[&alice2]).await;

    alice
        .outbound_service()
        .schedule_key_package_upload(Utc::now())
        .await
        .unwrap();
    alice.outbound_service().run_once().await;
    // The commit response finalizes the upload and merges the commit.
    alice.drain().await;
    alice2.drain().await;

    assert_published(&alice).await;
    assert_pq_credentials_empty(&alice).await;
    assert_eq!(signed_leaves(&alice2).await, 2);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn an_add_publishes_the_credential() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let alice2 = link_sibling(&alice).await;
    make_unpublished(&alice, &[&alice2]).await;

    let alice3 = link_sibling(&alice).await;
    alice2.drain().await;

    assert_published(&alice).await;
    assert_published(&alice3).await;
    for device in [&alice, &alice2, &alice3] {
        assert_eq!(signed_leaves(device).await, 3);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn a_remove_publishes_the_credential() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let alice2 = link_sibling(&alice).await;
    let alice3 = link_sibling(&alice).await;
    alice2.drain().await;
    make_unpublished(&alice, &[&alice2, &alice3]).await;

    alice
        .unlink_device(alice3.own_client_id().await.unwrap())
        .await
        .unwrap();
    alice2.drain().await;

    assert_published(&alice).await;
    assert_eq!(signed_leaves(&alice2).await, 2);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn a_settings_commit_publishes_the_credential() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let alice2 = link_sibling(&alice).await;
    make_unpublished(&alice, &[&alice2]).await;

    alice
        .set_synced_user_setting(&ReadReceiptsSetting(true))
        .await
        .unwrap();
    alice.outbound_service().run_once().await;
    alice.drain().await;
    alice2.drain().await;

    assert!(!alice.has_pending_setting_changes().await.unwrap());
    assert_published(&alice).await;
    assert_eq!(signed_leaves(&alice2).await, 2);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn a_token_seed_commit_publishes_the_credential() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let alice2 = link_sibling(&alice).await;
    make_unpublished(&alice, &[&alice2]).await;

    alice.reset_privacy_pass_for_key_rotation().await.unwrap();
    assert!(alice.committed_token_seeds().await.unwrap().is_empty());
    alice
        .outbound_service()
        .schedule_token_replenishment(Utc::now())
        .await
        .unwrap();
    // Replenishment proposes a seed, and a later run stages and sends the seed commit. Only alice
    // runs its outbound service, so the seed commit is the only commit she sends.
    for _ in 0..4 {
        alice.outbound_service().run_once().await;
        alice.drain().await;
        alice2.drain().await;
    }

    assert!(
        !alice.committed_token_seeds().await.unwrap().is_empty(),
        "the seed commit must have gone through"
    );
    assert_published(&alice).await;
    assert_eq!(signed_leaves(&alice2).await, 2);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn a_published_credential_stays_the_same() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let alice2 = link_sibling(&alice).await;
    let before = own_leaf_credential(&alice).await;
    assert!(before.is_signed());

    self_update(&alice, true).await;
    alice2.drain().await;

    assert_eq!(own_leaf_credential(&alice).await, before);
    assert_eq!(
        before.to_credential().unwrap(),
        alice
            .stored_self_group_credential()
            .await
            .unwrap()
            .to_credential()
            .unwrap()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn a_resync_publishes_the_credential_among_unsigned_leaves() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let alice2 = link_sibling(&alice).await;
    let alice3 = link_sibling(&alice).await;
    alice2.drain().await;
    make_unsigned(&alice2, &[&alice, &alice3]).await;
    make_unpublished(&alice, &[&alice2, &alice3]).await;

    // One leaf is signed, two aren't, and alice's stored credential is signed.
    let chat_id = self_chat_id(&alice).await;
    alice.enqueue_group_resync(chat_id).await.unwrap();
    alice.outbound_service().run_once().await;
    assert!(
        alice.resync_status(chat_id).await.unwrap().is_none(),
        "resync should have completed"
    );
    alice2.drain().await;
    alice3.drain().await;

    assert_published(&alice).await;
    assert_eq!(signed_leaves(&alice2).await, 2);
    send_and_receive(&alice, &[&alice2, &alice3], chat_id, "after resync").await;
    send_and_receive(&alice2, &[&alice, &alice3], chat_id, "and back").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn an_unsigned_device_links_a_signing_device() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    alice.ensure_self_group().await.unwrap();
    make_unsigned(&alice, &[]).await;

    let alice2 = link_sibling(&alice).await;
    assert_published(&alice2).await;
    assert!(!own_leaf_credential(&alice).await.is_signed());
    assert_eq!(signed_leaves(&alice).await, 1);

    // Commits in both directions still go through.
    self_update(&alice2, true).await;
    alice.drain().await;
    self_update(&alice, true).await;
    alice2.drain().await;
    let chat_id = self_chat_id(&alice).await;
    send_and_receive(&alice, &[&alice2], chat_id, "mixed").await;
    send_and_receive(&alice2, &[&alice], chat_id, "and back").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn a_signing_device_shares_the_self_group_with_an_unsigned_device() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let alice2 = link_sibling(&alice).await;
    make_unsigned(&alice2, &[&alice]).await;
    assert_eq!(signed_leaves(&alice).await, 1);

    // A further device links through the signing device, and the unsigned device follows.
    let alice3 = link_sibling(&alice).await;
    alice2.drain().await;
    assert_eq!(signed_leaves(&alice2).await, 2);

    self_update(&alice2, true).await;
    alice.drain().await;
    alice3.drain().await;
    let chat_id = self_chat_id(&alice).await;
    send_and_receive(&alice2, &[&alice, &alice3], chat_id, "mixed").await;
    send_and_receive(&alice3, &[&alice, &alice2], chat_id, "and back").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn an_unsigned_device_deletes_the_account() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let alice2 = link_sibling(&alice).await;
    make_unpublished(&alice, &[&alice2]).await;

    // The sibling and the DS only accept a delete commit that keeps the sender's credential, so
    // the commit must not publish the signed one.
    alice.delete_chat(self_chat_id(&alice).await).await.unwrap();

    alice2.drain().await;
    assert_eq!(
        alice2.account_unlink_reason().await.unwrap(),
        Some(UnlinkReason::AccountDeleted)
    );
}
