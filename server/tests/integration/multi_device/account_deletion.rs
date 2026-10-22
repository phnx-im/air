// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! An account deletion reaching the sibling through the closed self group.

use std::time::Duration;

use aircommon::messages::QueueMessage;
use aircoreclient::{
    ReadReceiptsSetting, UnlinkReason,
    clients::{CoreUser, ListenResponse, listen_response},
};
use airserver_test_harness::utils::{core_user_ext::CoreUserExt, setup::TestBackend};
use tokio_stream::{Stream, StreamExt};

use super::{link_sibling, self_chat_id};

/// Reads the listen stream up to the next queue message.
async fn next_queue_message(
    stream: &mut (impl Stream<Item = Result<ListenResponse, tonic::Status>> + Unpin),
) -> anyhow::Result<Option<QueueMessage>> {
    while let Some(response) = stream.next().await {
        match response?.event {
            Some(listen_response::Event::Message(message)) => return Ok(Some(message.try_into()?)),
            Some(listen_response::Event::Empty(_)) => return Ok(None),
            Some(listen_response::Event::Payload(_))
            | Some(listen_response::Event::VersionStatus(_))
            | Some(listen_response::Event::SiblingClientState(_))
            | None => {}
        }
    }
    anyhow::bail!("listen stream ended")
}

/// Deletes the self group the way an account deletion does.
async fn delete_self_group(device: &CoreUser) -> anyhow::Result<()> {
    device.delete_chat(self_chat_id(device).await).await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn account_deletion_tears_down_sibling() -> anyhow::Result<()> {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let alice2 = link_sibling(&alice).await;

    let (mut stream, _responder) = alice2.listen_queue().await?;
    while next_queue_message(&mut stream).await?.is_some() {}

    delete_self_group(&alice).await?;

    tokio::time::timeout(Duration::from_secs(10), async {
        while alice2.account_unlink_reason().await?.is_none() {
            let Some(message) = next_queue_message(&mut stream).await? else {
                continue;
            };
            alice2.fully_process_qs_messages(vec![message]).await;
        }
        anyhow::Ok(())
    })
    .await??;

    assert_eq!(
        alice2.account_unlink_reason().await?,
        Some(UnlinkReason::AccountDeleted)
    );

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
