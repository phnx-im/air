// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Usernames across the devices of a user.

use aircommon::identifiers::USERNAME_REFRESH_THRESHOLD;
use aircoreclient::{UsernameRecord, clients::CoreUser};
use airprotos::auth_service::v1::OperationType;
use airserver_test_harness::utils::{core_user_ext::CoreUserExt, setup::TestBackend};
use chrono::{Duration, Utc};

use crate::contacts::requests::add_second_username;

/// Asserts that both devices hold the same username with the same keys.
pub(crate) async fn assert_same_username(
    device_a: &CoreUser,
    device_b: &CoreUser,
    record: &UsernameRecord,
) {
    let records_b = device_b.username_records().await.unwrap();
    let [record_b] = records_b.as_slice() else {
        panic!("the sibling should hold exactly one username, got {records_b:?}");
    };
    assert_eq!(record_b.username, record.username);
    assert_eq!(record_b.hash, record.hash);
    assert_eq!(
        record_b.signing_key.verifying_key(),
        record.signing_key.verifying_key(),
        "the sibling needs the signing key to act on the username"
    );
    assert_eq!(
        device_b
            .connection_package_hashes(&record.username)
            .await
            .unwrap(),
        device_a
            .connection_package_hashes(&record.username)
            .await
            .unwrap(),
        "the sibling needs the package keys to read offers"
    );
}

/// Adds a username on `device_a` and hands it to `device_b`.
pub(crate) async fn add_synced_username(
    setup: &TestBackend,
    device_a: &CoreUser,
    device_b: &CoreUser,
) -> UsernameRecord {
    let record = setup.add_username(device_a).await;
    device_a.outbound_service().run_once().await;
    device_b.drain().await;
    record
}

async fn sorted_username_tokens(device: &CoreUser) -> Vec<Vec<u8>> {
    let mut tokens = device
        .cached_privacy_pass_tokens(OperationType::AddUsername)
        .await
        .unwrap();
    tokens.sort();
    tokens
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn sibling_gets_a_new_username() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let alice2 = alice.add_synced_device().await;

    let record = add_synced_username(&setup, &alice, &alice2).await;

    assert_same_username(&alice, &alice2, &record).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn removing_a_username_on_a_sibling_removes_it_everywhere() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let alice2 = alice.add_synced_device().await;
    let record = add_synced_username(&setup, &alice, &alice2).await;

    // alice2 never registered the username. It deletes the username on the AS
    // with the synced signing key.
    alice2.remove_username(&record.username).await.unwrap();
    alice2.outbound_service().run_once().await;
    alice.drain().await;

    assert!(alice.usernames().await.unwrap().is_empty());
    assert!(
        alice
            .connection_package_hashes(&record.username)
            .await
            .unwrap()
            .is_empty(),
        "the package keys go with the username"
    );
    let exists = alice
        .check_username_exists(record.username.clone())
        .await
        .unwrap();
    assert!(exists.is_none(), "the username should be gone on the AS");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn an_offer_the_sibling_cannot_read_stays_in_the_queue() {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    let alice2 = alice.add_synced_device().await;
    let record = add_synced_username(&setup, &alice, &alice2).await;
    // This simulates a sibling still waiting for the commit with new package
    // keys.
    alice2
        .forget_connection_package_keys(&record.username)
        .await
        .unwrap();

    let bob_chat_id = bob
        .add_contact(record.username.clone(), record.hash, setup.apq_groups)
        .await
        .unwrap()
        .unwrap();

    let stored = alice2.fetch_and_process_username_messages().await.unwrap();
    assert!(stored.is_empty());
    let stored = alice.fetch_and_process_username_messages().await.unwrap();
    let [stored] = stored.as_slice() else {
        panic!("the sibling should leave the offer in the queue, got {stored:?}");
    };
    assert_eq!(stored.chat_id, bob_chat_id);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn a_refresh_a_sibling_already_did_spends_no_token() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let alice2 = alice.add_synced_device().await;
    let record = add_synced_username(&setup, &alice, &alice2).await;

    // The AS has a fresh registration. alice2 pretends to have missed the last
    // refresh. It finds the username due.
    alice2
        .set_username_refreshed_at(&record.username, Utc::now() - Duration::days(100))
        .await
        .unwrap();
    let tokens = sorted_username_tokens(&alice2).await;
    assert!(!tokens.is_empty(), "the device needs a token to refresh");

    alice2
        .outbound_service()
        .schedule_username_refresh(Utc::now())
        .await
        .unwrap();
    alice2.outbound_service().run_once().await;

    assert_eq!(
        sorted_username_tokens(&alice2).await,
        tokens,
        "the AS redeemed nothing, so the token is back in the cache"
    );
    let refreshed_at = alice2
        .username_refreshed_at(&record.username)
        .await
        .unwrap()
        .unwrap();
    assert!(
        refreshed_at > Utc::now() - USERNAME_REFRESH_THRESHOLD,
        "the device learned from the AS that the username is not due"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn a_refresh_of_a_removed_username_spends_no_token() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let alice2 = alice.add_synced_device().await;
    let removed = add_synced_username(&setup, &alice, &alice2).await;
    let kept = add_second_username(&alice).await;
    alice.outbound_service().run_once().await;
    alice2.drain().await;

    // alice2 has not seen the release yet. It loads the due usernames in
    // insertion order. The removed one comes first.
    alice.remove_username(&removed.username).await.unwrap();
    for record in [&removed, &kept] {
        alice2
            .set_username_refreshed_at(&record.username, Utc::now() - Duration::days(100))
            .await
            .unwrap();
    }
    let tokens = sorted_username_tokens(&alice2).await;
    assert!(!tokens.is_empty(), "the device needs a token to refresh");

    alice2
        .outbound_service()
        .schedule_username_refresh(Utc::now())
        .await
        .unwrap();
    alice2.outbound_service().run_once().await;

    assert_eq!(
        sorted_username_tokens(&alice2).await,
        tokens,
        "the AS redeemed nothing, so the token is back in the cache"
    );
    let refreshed_at = alice2
        .username_refreshed_at(&kept.username)
        .await
        .unwrap()
        .unwrap();
    assert!(
        refreshed_at > Utc::now() - USERNAME_REFRESH_THRESHOLD,
        "the refresh moves on to the next username"
    );
}
