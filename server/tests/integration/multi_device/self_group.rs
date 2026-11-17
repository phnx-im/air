// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! The self group: creation, resync, self update and attachments in the self
//! chat.

use aircommon::{
    credentials::keys::{LeafSigningKey, SelfGroupSigningKey},
    crypto::aead::keys::EncryptedUserProfileKey,
    messages::client_ds::{AadPayload, CloseConnectionGroupParamsAad, GroupOperationParamsAad},
};
use aircoreclient::{ChatStatus, clients::MarkChatAsRead};
use airserver_test_harness::utils::{core_user_ext::CoreUserExt, setup::TestBackend};
use chrono::{DateTime, Utc};
use tempfile::TempDir;
use uuid::Uuid;

use super::{link_sibling, self_chat_id, self_group_client_ids, send_and_receive};

#[tokio::test(flavor = "multi_thread")]
async fn self_group_is_apq() -> anyhow::Result<()> {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;

    alice.ensure_self_group().await?;

    let is_apq = alice
        .self_group_is_apq()
        .await?
        .expect("self group should be persisted");
    assert!(is_apq, "self-group must be an APQ (T+PQ) group");

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn resync_re_registers_derivation_epoch() -> anyhow::Result<()> {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    // The key package upload at the end only goes through the self group with
    // a sibling.
    let _alice2 = link_sibling(&alice).await;

    assert!(
        alice.self_group_has_derivation_epoch().await?,
        "creating the self-group registers its initial derivation epoch"
    );
    let self_chat_id = alice.self_chat_id().await?.expect("no self chat");

    alice.enqueue_group_resync(self_chat_id).await?;
    alice.outbound_service().run_once().await;
    assert!(
        alice.resync_status(self_chat_id).await?.is_none(),
        "resync should have completed"
    );

    assert!(
        alice.self_group_has_derivation_epoch().await?,
        "the resync must re-register the self-group's derivation epoch"
    );

    // The self-group backs virtual-client operations again: a key package
    // upload stages against its derivation epoch and completes.
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
    let (plain_refs, apq_refs) = alice.live_key_package_refs().await?;
    assert!(
        !plain_refs.is_empty() && !apq_refs.is_empty(),
        "the upload after the resync should have published a live batch"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn self_chat_attachment_reaches_sibling() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let alice2 = alice.add_linked_device().await;
    alice.drain().await;
    alice2.drain().await;

    let chat_id = self_chat_id(&alice).await;
    let attachment = vec![0x00, 0x01, 0x02, 0x03];
    let tmp_dir = TempDir::new().unwrap();
    let path = tmp_dir.path().join("test.bin");
    std::fs::write(&path, &attachment).unwrap();

    let upload = alice
        .upload_chat_attachment(chat_id, &path, MarkChatAsRead::Yes)
        .await
        .unwrap()
        .unwrap();
    let message = upload
        .task
        .await
        .unwrap_or_else(|error| panic!("upload in the self-group failed: {error:?}"));
    alice
        .outbound_service()
        .enqueue_chat_message(message.id())
        .await
        .unwrap();
    alice.outbound_service().run_once().await;

    alice2.drain().await;
    let pending = alice2.pending_attachments().await.unwrap();
    assert_eq!(pending.len(), 1);
    let attachment_id = pending[0];

    let (_progress, download_task) = alice2.download_attachment(attachment_id);
    download_task
        .await
        .expect("download in the self-group failed");

    let content = alice2
        .load_attachment(attachment_id)
        .await
        .unwrap()
        .into_bytes()
        .unwrap();
    assert_eq!(content, attachment);
}

/// A periodic self-update in the self group is accepted by the DS and
/// processed by the sibling device.
///
/// Self-group leaves carry a self-group credential and are signed with the
/// per-device self-group key, so both the joint APQ self-update and the pure
/// T self-update must preserve the self-group leaf capabilities and use that
/// key. A leaf built with the regular user credential defaults would be
/// rejected by the DS and by the sibling (RFC 9420 valn0104).
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn self_update_is_followed_by_sibling() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let alice2 = alice.add_linked_device().await;

    // Bring both devices to the self-group's latest epoch.
    alice.drain().await;
    alice2.drain().await;

    let chat_id = self_chat_id(&alice).await;

    // Force the joint APQ self-update (both T and PQ due).
    alice
        .set_self_updated_at(chat_id, DateTime::UNIX_EPOCH)
        .await
        .unwrap();
    alice
        .set_pq_self_updated_at(chat_id, DateTime::UNIX_EPOCH)
        .await
        .unwrap();
    let before_apq = Utc::now();
    alice
        .outbound_service()
        .schedule_self_update(DateTime::UNIX_EPOCH)
        .await
        .unwrap();
    alice.outbound_service().run_once().await;

    let after_t = alice.self_updated_at(chat_id).await.unwrap().unwrap();
    let after_pq = alice.pq_self_updated_at(chat_id).await.unwrap().unwrap();
    assert!(
        before_apq < after_t && before_apq < after_pq,
        "the DS should have accepted the APQ self-update in the self group"
    );

    // The sibling device processes the update commit.
    alice2.drain().await;

    // Force a pure T self-update (only T due).
    alice.drain().await;
    alice
        .set_self_updated_at(chat_id, DateTime::UNIX_EPOCH)
        .await
        .unwrap();
    let before_t = Utc::now();
    alice
        .outbound_service()
        .schedule_self_update(DateTime::UNIX_EPOCH)
        .await
        .unwrap();
    alice.outbound_service().run_once().await;

    let after_t = alice.self_updated_at(chat_id).await.unwrap().unwrap();
    assert!(
        before_t < after_t,
        "the DS should have accepted the T self-update in the self group"
    );

    alice2.drain().await;

    // Both devices are still in sync afterwards.
    send_and_receive(&alice, &[&alice2], chat_id, "after self-update").await;
    send_and_receive(&alice2, &[&alice], chat_id, "and back").await;
}

/// A resync of the self group rejoins with the per-device self-group
/// credential.
///
/// The external commit must carry a self-group credential and self-group leaf
/// capabilities. Rejoining with the user credential would be rejected, since
/// the self group accepts only self-group credentials.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn resync_keeps_sibling_in_sync() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let alice2 = alice.add_linked_device().await;

    // Bring both devices to the self-group's latest epoch.
    alice.drain().await;
    alice2.drain().await;

    let chat_id = self_chat_id(&alice).await;
    let (epoch_before, _) = alice
        .group_epoch_and_own_index(chat_id)
        .await
        .unwrap()
        .unwrap();

    // The old device resyncs into the self group.
    alice.enqueue_group_resync(chat_id).await.unwrap();
    assert!(
        alice.resync_status(chat_id).await.unwrap().is_some(),
        "resync should be queued"
    );
    alice.outbound_service().run_once().await;
    assert!(
        alice.resync_status(chat_id).await.unwrap().is_none(),
        "resync should have completed"
    );

    // The external commit advanced the epoch, so the rejoin actually happened.
    let (epoch_after, _) = alice
        .group_epoch_and_own_index(chat_id)
        .await
        .unwrap()
        .unwrap();
    assert!(
        epoch_before < epoch_after,
        "the resync commit should have advanced the self-group epoch"
    );

    // The sibling device processes the rejoin commit.
    alice2.drain().await;

    // Both devices still see two distinct self-group clients and stay in sync.
    self_group_client_ids(&alice).await;
    self_group_client_ids(&alice2).await;
    send_and_receive(&alice, &[&alice2], chat_id, "after resync").await;
    send_and_receive(&alice2, &[&alice], chat_id, "and back").await;
}

/// A malicious DS forges an external commit that gives a new leaf the
/// self-group credential of a sibling with a fresh signature key and removes
/// the sibling's leaf. The other devices must reject it with `expected_error`.
async fn sibling_leaf_takeover_is_rejected(aad: AadPayload, expected_error: &str) {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let alice2 = alice.add_linked_device().await;
    let alice3 = alice.add_linked_device().await;
    alice.drain().await;
    alice2.drain().await;
    alice3.drain().await;

    // The forged commit needs a blank leaf left of the victim's.
    alice
        .unlink_device(alice2.own_client_id().await.unwrap())
        .await
        .unwrap();
    alice.drain().await;
    alice3.drain().await;

    let chat_id = self_chat_id(&alice).await;
    let members_before = alice.mls_members(chat_id).await.unwrap().unwrap();

    // The victim's own device stands in for the DS. It sends nothing.
    let commit = alice3
        .forge_self_group_leaf_takeover(alice3.own_client_id().await.unwrap(), aad)
        .await
        .unwrap();

    let Err(error) = alice.process_incoming_apq_mls_message(&commit).await else {
        panic!("the forged commit takes over the sibling's leaf");
    };
    assert!(
        format!("{error:#}").contains(expected_error),
        "unexpected error: {error:#}"
    );
    let members_after = alice.mls_members(chat_id).await.unwrap().unwrap();
    assert_eq!(members_after, members_before);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn sibling_leaf_takeover_by_resync_is_rejected() {
    sibling_leaf_takeover_is_rejected(
        AadPayload::Resync,
        "self-group resync changes the signature key of the removed leaf",
    )
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn sibling_leaf_takeover_by_group_operation_is_rejected() {
    let aad = AadPayload::GroupOperation(GroupOperationParamsAad {
        new_encrypted_user_profile_keys: Vec::new(),
    });
    sibling_leaf_takeover_is_rejected(aad, "self-group external commit is no resync").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn external_delete_commit_is_rejected() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    alice.ensure_self_group().await.unwrap();
    alice.drain().await;

    let chat_id = self_chat_id(&alice).await;
    let members_before = alice.mls_members(chat_id).await.unwrap().unwrap();

    // Alice's own device stands in for the DS. It sends nothing.
    let (group_id, pq_group_id, ear_key) = alice.self_group_ds_keys().await.unwrap();
    let signer = LeafSigningKey::SelfGroup(SelfGroupSigningKey::generate(Uuid::new_v4()).unwrap());
    let commit = alice
        .forge_external_commit(
            group_id,
            pq_group_id,
            &ear_key,
            &signer,
            AadPayload::DeleteGroup,
        )
        .await
        .unwrap();

    let Err(error) = alice.process_incoming_apq_mls_message(&commit).await else {
        panic!("the forged commit deletes the self group");
    };
    assert!(
        format!("{error:#}").contains("self-group external commit is no resync"),
        "unexpected error: {error:#}"
    );
    assert_eq!(alice.account_unlink_reason().await.unwrap(), None);
    assert_eq!(
        alice.chat(&chat_id).await.unwrap().status(),
        &ChatStatus::Active
    );
    let members_after = alice.mls_members(chat_id).await.unwrap().unwrap();
    assert_eq!(members_after, members_before);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn connection_group_close_into_the_self_group_is_rejected() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let mallory = setup.add_user().await;
    alice.ensure_self_group().await.unwrap();
    alice.drain().await;

    // With a single device, the self group has one member, like a connection
    // group.
    let chat_id = self_chat_id(&alice).await;
    let members_before = alice.mls_members(chat_id).await.unwrap().unwrap();
    assert_eq!(members_before.len(), 1);

    // Mallory stands in for the DS, which can register an account of its own.
    // She sends nothing.
    let (group_id, pq_group_id, ear_key) = alice.self_group_ds_keys().await.unwrap();
    let signer = LeafSigningKey::User(mallory.signing_key_for_test().clone());
    let aad = AadPayload::CloseConnectionGroup(CloseConnectionGroupParamsAad {
        encrypted_user_profile_key: EncryptedUserProfileKey::random(),
    });
    let commit = mallory
        .forge_external_commit(group_id, pq_group_id, &ear_key, &signer, aad)
        .await
        .unwrap();

    let Err(error) = alice.process_incoming_apq_mls_message(&commit).await else {
        panic!("the forged commit adds Mallory to Alice's self group");
    };
    assert!(
        format!("{error:#}").contains("self-group external commit is no resync"),
        "unexpected error: {error:#}"
    );
    assert_eq!(
        alice.chat(&chat_id).await.unwrap().status(),
        &ChatStatus::Active
    );
    let members_after = alice.mls_members(chat_id).await.unwrap().unwrap();
    assert_eq!(members_after, members_before);
}
