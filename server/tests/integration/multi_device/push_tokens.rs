// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Push tokens of QS clients that no longer belong to a device of the user.

use aircommon::messages::push_token::{PushToken, PushTokenOperator};
use aircoreclient::clients::CoreUser;
use airserver_test_harness::utils::{core_user_ext::CoreUserExt, setup::TestBackend};
use chrono::DateTime;

use super::self_chat_id;

async fn set_push_token(user: &CoreUser, token: &str) {
    let push_token = PushToken::new(PushTokenOperator::Apple, token.to_owned());
    user.update_push_token(Some(push_token)).await.unwrap();
    user.outbound_service().run_once().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn unlinking_a_device_clears_its_push_token() -> anyhow::Result<()> {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let alice2 = alice.add_linked_device().await;
    alice2.outbound_service().run_once().await;
    alice2.drain().await;
    alice.drain().await;

    set_push_token(&alice, "first-device").await;
    set_push_token(&alice2, "second-device").await;
    assert!(setup.qs_client_has_push_token(&alice.qs_client_id()).await);
    assert!(setup.qs_client_has_push_token(&alice2.qs_client_id()).await);
    // The second device's outbound run commits to the self group.
    alice.drain().await;

    alice.unlink_device(alice2.own_client_id().await?).await?;

    assert!(setup.qs_client_has_push_token(&alice.qs_client_id()).await);
    assert!(!setup.qs_client_has_push_token(&alice2.qs_client_id()).await);

    // The unlinked device keeps its queue. It still learns of its removal.
    alice2.drain().await;
    assert!(alice2.is_account_unlinked().await?);
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn self_group_self_update_clears_push_tokens_of_detached_clients() -> anyhow::Result<()> {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let alice2 = alice.add_linked_device().await;
    alice2.outbound_service().run_once().await;
    alice2.drain().await;
    alice.drain().await;

    set_push_token(&alice, "first-device").await;
    set_push_token(&alice2, "second-device").await;
    // The second device's outbound run commits to the self group.
    alice.drain().await;
    let ghost = alice
        .create_detached_qs_client(PushToken::new(PushTokenOperator::Apple, "ghost".to_owned()))
        .await?;
    assert!(setup.qs_client_has_push_token(&ghost).await);

    let chat_id = self_chat_id(&alice).await;
    alice
        .set_self_updated_at(chat_id, DateTime::UNIX_EPOCH)
        .await?;
    alice
        .outbound_service()
        .schedule_self_update(DateTime::UNIX_EPOCH)
        .await?;
    alice.outbound_service().run_once().await;
    assert!(
        alice.self_updated_at(chat_id).await?.unwrap() > DateTime::UNIX_EPOCH,
        "the self group was not self-updated"
    );

    assert!(!setup.qs_client_has_push_token(&ghost).await);
    assert!(setup.qs_client_has_push_token(&alice.qs_client_id()).await);
    assert!(setup.qs_client_has_push_token(&alice2.qs_client_id()).await);
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn linking_a_device_clears_push_tokens_of_detached_clients() -> anyhow::Result<()> {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    set_push_token(&alice, "first-device").await;
    let ghost = alice
        .create_detached_qs_client(PushToken::new(PushTokenOperator::Apple, "ghost".to_owned()))
        .await?;
    assert!(setup.qs_client_has_push_token(&ghost).await);

    let alice2 = alice.add_linked_device().await;

    assert!(!setup.qs_client_has_push_token(&ghost).await);
    assert!(setup.qs_client_has_push_token(&alice.qs_client_id()).await);

    // The new device registers its push token only after joining. It keeps the token.
    set_push_token(&alice2, "second-device").await;
    assert!(setup.qs_client_has_push_token(&alice2.qs_client_id()).await);
    Ok(())
}
