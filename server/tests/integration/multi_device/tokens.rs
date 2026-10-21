// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Privacy Pass tokens across devices: issuance, seed agreement and redeemed
//! tokens.

use aircommon::identifiers::Username;
use aircoreclient::clients::CoreUser;
use airprotos::auth_service::v1::OperationType;
use airserver_test_harness::utils::{core_user_ext::CoreUserExt, setup::TestBackend};
use uuid::Uuid;

// Idempotent issuance derives every token from a seed the devices agree on, so
// two devices that replenish independently end up holding byte-identical tokens
// with no token synchronization protocol. The seed reaches the second device in
// its provisioning package, which is what keeps it from proposing a seed of its
// own and losing the race for an allowance epoch the first device has already
// locked.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn multi_device_token_issuance_is_idempotent() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;

    // A lone device agrees with itself, so it can issue right away.
    alice.outbound_service().run_once().await;
    let seeds = alice.committed_token_seeds().await.unwrap();
    assert!(
        !seeds.is_empty(),
        "a lone device must agree on its own token seeds"
    );
    let first_tokens = alice
        .cached_privacy_pass_tokens(OperationType::AddUsername)
        .await
        .unwrap();
    assert!(
        !first_tokens.is_empty(),
        "the first device must hold tokens"
    );

    let alice2 = alice.add_linked_device().await;

    // The linking payload carried the agreed seeds, so the new device starts out
    // agreeing rather than proposing.
    assert_eq!(
        alice2.committed_token_seeds().await.unwrap(),
        seeds,
        "the new device must adopt its sibling's agreed seeds"
    );

    // The second device replenishes on its own. The AS answers the repeated
    // request for free, and the tokens finalize to the same bytes.
    alice2.outbound_service().run_once().await;
    assert_eq!(
        alice2
            .cached_privacy_pass_tokens(OperationType::AddUsername)
            .await
            .unwrap(),
        first_tokens,
        "both devices must hold byte-identical tokens"
    );

    // Both devices still see each other, so neither switched to a private seed.
    for (label, device) in [("first", &alice), ("second", &alice2)] {
        assert_eq!(
            device.self_group_client_ids().await.unwrap().len(),
            2,
            "the {label} device should see both leaves"
        );
        assert_eq!(
            device.committed_token_seeds().await.unwrap(),
            seeds,
            "the {label} device must keep the agreed seeds"
        );
    }
}

// A seed that has to be invented while two devices are already linked is agreed
// over the self group: a device proposes, a commit carries the proposal, and DS
// commit order decides which proposal becomes the seed. This is the path a key
// rotation takes, where neither device has a seed for the new key and the
// provisioning snapshot cannot help, so the test puts both devices into exactly
// the state a rotation leaves behind.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn multi_device_token_seed_agreement() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let alice2 = alice.add_linked_device().await;

    for device in [&alice, &alice2] {
        device.reset_privacy_pass_for_key_rotation().await.unwrap();
        assert!(
            device.committed_token_seeds().await.unwrap().is_empty(),
            "the rotation left no agreed seed behind"
        );
    }

    // Replenishment proposes, the next outbound run stages and sends the
    // proposal commit, and the sibling has to process it. Once a seed is agreed,
    // a further run fetches the batch under it.
    for _ in 0..4 {
        for device in [&alice, &alice2] {
            device.outbound_service().run_once().await;
            device.drain().await;
        }
    }

    let first_seeds = alice.committed_token_seeds().await.unwrap();
    assert!(
        !first_seeds.is_empty(),
        "the devices must have agreed on a seed"
    );
    assert_eq!(
        alice2.committed_token_seeds().await.unwrap(),
        first_seeds,
        "both devices must agree on the same seed"
    );

    // Agreeing on the seed is what makes the tokens identical.
    assert_eq!(
        alice2
            .cached_privacy_pass_tokens(OperationType::AddUsername)
            .await
            .unwrap(),
        alice
            .cached_privacy_pass_tokens(OperationType::AddUsername)
            .await
            .unwrap(),
        "both devices must hold byte-identical tokens"
    );
}

// Redemption is local to the spending device. Without a broadcast a sibling
// keeps holding a token the AS has already seen and loses one of its own
// requests to it. The spending device therefore publishes the position it
// spent, which names the same token on every device.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn multi_device_redeemed_token_is_removed_from_sibling() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;

    alice.outbound_service().run_once().await;
    let alice2 = alice.add_linked_device().await;
    alice2.outbound_service().run_once().await;
    // Catch up on the echo of our own add commit and on the new device's own
    // self-group commits, so the broadcast below is built at the DS epoch.
    alice.drain().await;

    let full_batch = cached_tokens(&alice).await;
    assert!(!full_batch.is_empty(), "the first device must hold tokens");
    assert_eq!(
        cached_tokens(&alice2).await,
        full_batch,
        "both devices must start from the same batch"
    );

    assert!(
        alice
            .add_username(fresh_username())
            .await
            .unwrap()
            .is_some(),
        "the username must be created"
    );

    let remaining = cached_tokens(&alice).await;
    assert_eq!(
        remaining.len(),
        full_batch.len() - 1,
        "creating the username spent one token"
    );
    assert_eq!(
        cached_tokens(&alice2).await,
        full_batch,
        "the sibling still holds the spent token"
    );

    // The broadcast is held back to keep it apart from the redemption. The
    // test skips that wait.
    alice.expedite_redeemed_token_broadcast().await.unwrap();
    let epochs_before = alice.self_group_epochs().await.unwrap();
    alice.outbound_service().run_once().await;

    assert!(
        alice
            .pending_redeemed_token_broadcasts()
            .await
            .unwrap()
            .is_empty(),
        "the DS accepting the message retires the broadcast"
    );
    assert_eq!(
        alice.self_group_epochs().await.unwrap(),
        epochs_before,
        "the broadcast is an application message, not a commit"
    );

    alice2.drain().await;
    assert_eq!(
        cached_tokens(&alice2).await,
        remaining,
        "the sibling must drop the redeemed token"
    );
}

// A device that joins via Welcome cannot read the messages that carried past
// redemptions, and its first batch fetch is answered from the request its
// sibling registered. The provisioning package therefore carries the redeemed
// positions, pending or not.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn multi_device_new_device_skips_redeemed_tokens() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;

    alice.outbound_service().run_once().await;
    assert!(
        alice
            .add_username(fresh_username())
            .await
            .unwrap()
            .is_some(),
        "the username must be created"
    );
    let remaining = cached_tokens(&alice).await;

    let alice2 = alice.add_linked_device().await;
    alice2.outbound_service().run_once().await;

    assert_eq!(
        cached_tokens(&alice2).await,
        remaining,
        "the new device must never hold the redeemed token"
    );
    assert!(
        alice2
            .pending_redeemed_token_broadcasts()
            .await
            .unwrap()
            .is_empty(),
        "the new device has nothing of its own to tell"
    );
}

/// The device's cached AddUsername tokens, in consumption order.
async fn cached_tokens(user: &CoreUser) -> Vec<Vec<u8>> {
    user.cached_privacy_pass_tokens(OperationType::AddUsername)
        .await
        .unwrap()
}

/// A username no other test can have taken.
fn fresh_username() -> Username {
    Username::new(format!("alice-{}", Uuid::new_v4().simple())).unwrap()
}
