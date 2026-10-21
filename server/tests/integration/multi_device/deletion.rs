// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Chat, group and message deletions reaching the sibling.

use aircommon::identifiers::UserId;
use aircoreclient::{
    ChatId, ChatStatus, SystemMessage,
    clients::{CoreUser, MarkChatAsRead},
};
use airserver_test_harness::utils::{core_user_ext::CoreUserExt, setup::TestBackend};
use mimi_content::MimiContent;

use super::self_chat_id;

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn multi_device_deleted_chat_is_erased_on_sibling() -> anyhow::Result<()> {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    let chat_id = setup.connected_group(&alice, &[&bob]).await;

    let alice2 = alice.add_synced_device().await;
    assert!(alice2.chat(&chat_id).await.is_some());

    alice.delete_chat(chat_id).await?;
    alice2.drain().await;
    assert!(
        alice2.chat(&chat_id).await.is_some(),
        "deleting the group leaves the chat in place until it is erased"
    );

    let self_chat_id = self_chat_id(&alice).await;
    let epochs_before = alice.self_group_epochs().await?;
    alice.erase_chat(chat_id).await?;
    assert!(alice.chat(&chat_id).await.is_none());
    alice.outbound_service().run_once().await;
    assert_ne!(
        alice.self_group_epochs().await?,
        epochs_before,
        "the deletion travels in a self-group commit"
    );
    assert!(
        alice
            .pending_chat_operation_info(self_chat_id)
            .await?
            .is_none(),
        "the commit should be gone after a successful send"
    );

    alice2.drain().await;
    assert!(
        alice2.chat(&chat_id).await.is_none(),
        "the sibling must erase the chat"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn multi_device_sole_member_group_deletion_reaches_the_sibling() -> anyhow::Result<()> {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let chat_id = setup.create_group(&alice).await;

    let alice2 = alice.add_synced_device().await;
    assert!(alice2.chat(&chat_id).await.is_some());
    assert!(alice.ds_has_group(chat_id).await?);

    let deletion = SystemMessage::DeleteGroup(alice.user_id().clone());
    alice.delete_chat(chat_id).await?;
    let messages = alice.system_messages(chat_id).await;
    assert_eq!(
        messages
            .iter()
            .filter(|message| **message == deletion)
            .count(),
        1,
        "the deleter stores one deletion message: {messages:?}"
    );
    assert!(
        !alice.ds_has_group(chat_id).await?,
        "the DS deletes the group of a sole member"
    );

    alice2.drain().await;
    assert!(alice2.expect_chat(chat_id).await.status().is_inactive());
    let sibling_messages = alice2.system_messages(chat_id).await;
    assert_eq!(
        sibling_messages
            .iter()
            .filter(|message| **message == deletion)
            .count(),
        1,
        "got {sibling_messages:?}"
    );
    assert!(
        !sibling_messages
            .iter()
            .any(|message| matches!(message, SystemMessage::Remove(_remover, _removed))),
        "got {sibling_messages:?}"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn multi_device_missed_group_deletion_is_found_by_resync() -> anyhow::Result<()> {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    let chat_id = setup.connected_group(&alice, &[&bob]).await;

    let alice2 = alice.add_synced_device().await;
    assert!(alice2.chat(&chat_id).await.is_some());

    alice.delete_chat(chat_id).await?;
    // The sibling misses the delete commit, so its leaf is still in the group
    // on the DS and a resync would rejoin the one-member group.
    alice2.discard_queue().await;
    assert_eq!(
        alice2.expect_chat(chat_id).await.status(),
        &ChatStatus::Active
    );
    let participants = alice2.chat_participants(chat_id).await;

    alice2.enqueue_group_resync(chat_id).await?;
    alice2.outbound_service().run_once().await;

    assert!(alice2.expect_chat(chat_id).await.status().is_inactive());
    let sibling_messages = alice2.system_messages(chat_id).await;
    assert_eq!(
        sibling_messages
            .iter()
            .filter(|message| **message == SystemMessage::DeleteGroup(alice.user_id().clone()))
            .count(),
        1,
        "got {sibling_messages:?}"
    );
    assert!(
        !sibling_messages
            .iter()
            .any(|message| matches!(message, SystemMessage::Remove(_remover, _removed))),
        "got {sibling_messages:?}"
    );
    assert!(
        alice2.resync_status(chat_id).await?.is_none(),
        "the resync entry is gone"
    );
    assert!(
        alice2.mls_members(chat_id).await?.is_none(),
        "the MLS group is gone"
    );
    assert_eq!(
        alice2.chat_participants(chat_id).await,
        participants,
        "the chat still lists its members"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn multi_device_deleted_group_takes_no_further_operations() -> anyhow::Result<()> {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    let chat_id = setup.connected_group(&alice, &[&bob]).await;

    let alice2 = alice.add_synced_device().await;

    alice.delete_chat(chat_id).await?;
    // The sibling's leaf is still in the group, so only the deletion stops
    // the DS from accepting its commit.
    alice2.discard_queue().await;

    let result = alice2.update_key(chat_id).await;
    assert!(
        result.is_err(),
        "the DS accepts a commit on a deleted group"
    );
    // The rejection schedules a resync, which finds the deletion.
    alice2.outbound_service().run_once().await;
    assert_deletion_announced(&alice2, chat_id, alice.user_id()).await;

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn multi_device_message_to_deleted_group_announces_the_deletion() -> anyhow::Result<()> {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    let chat_id = setup.connected_group(&alice, &[&bob]).await;

    let alice2 = alice.add_synced_device().await;

    alice.delete_chat(chat_id).await?;
    alice2.discard_queue().await;

    let content = MimiContent::simple_markdown_message("too late".to_owned(), [5; 16]);
    alice2
        .send_message(chat_id, content, None, MarkChatAsRead::Yes)
        .await?;
    // The first run has the DS reject the message, the second one resyncs.
    alice2.outbound_service().run_once().await;
    alice2.outbound_service().run_once().await;
    assert_deletion_announced(&alice2, chat_id, alice.user_id()).await;

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn multi_device_deleted_message_is_removed_from_sibling() -> anyhow::Result<()> {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    let chat_id = setup.connected_group(&alice, &[&bob]).await;

    let alice2 = alice.add_synced_device().await;

    bob.drain().await;
    let text = "delete me on every device";
    bob.send_text(chat_id, text).await;

    alice.drain().await;
    alice2.drain().await;
    assert_eq!(
        alice
            .text_messages(chat_id)
            .await
            .iter()
            .filter(|rendered| *rendered == text)
            .count(),
        1
    );
    assert_eq!(
        alice2
            .text_messages(chat_id)
            .await
            .iter()
            .filter(|rendered| *rendered == text)
            .count(),
        1
    );

    let message_id = alice
        .messages(chat_id, 100)
        .await?
        .iter()
        .find(|message| {
            message
                .message()
                .mimi_content()
                .is_some_and(|content| content.string_rendering().is_ok_and(|s| s.contains(text)))
        })
        .map(|message| message.id())
        .expect("the first device holds the message");

    let epochs_before = alice.self_group_epochs().await?;
    alice.delete_message(message_id).await?;
    alice.outbound_service().run_once().await;
    assert_eq!(
        alice
            .text_messages(chat_id)
            .await
            .iter()
            .filter(|rendered| *rendered == text)
            .count(),
        0
    );
    assert_eq!(
        alice.self_group_epochs().await?,
        epochs_before,
        "the deletion is an application message, not a commit"
    );

    alice2.drain().await;
    assert_eq!(
        alice2
            .text_messages(chat_id)
            .await
            .iter()
            .filter(|rendered| *rendered == text)
            .count(),
        0,
        "the sibling must delete the message"
    );

    Ok(())
}

/// Asserts that `device` ended the chat with exactly one message saying that
/// `deleter` deleted it, and dropped the group's MLS state.
async fn assert_deletion_announced(device: &CoreUser, chat_id: ChatId, deleter: &UserId) {
    assert!(device.expect_chat(chat_id).await.status().is_inactive());
    let messages = device.system_messages(chat_id).await;
    assert_eq!(
        messages
            .iter()
            .filter(|message| **message == SystemMessage::DeleteGroup(deleter.clone()))
            .count(),
        1,
        "got {messages:?}"
    );
    assert!(
        device.resync_status(chat_id).await.unwrap().is_none(),
        "the resync entry is gone"
    );
    assert!(
        device.mls_members(chat_id).await.unwrap().is_none(),
        "the MLS group is gone"
    );
}
