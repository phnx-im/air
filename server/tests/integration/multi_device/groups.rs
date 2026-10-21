// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Group operations across devices: own echoes, member removal, and invites
//! after linking.

use aircoreclient::{UserProfile, clients::MarkChatAsRead};
use airserver_test_harness::utils::{core_user_ext::CoreUserExt, setup::TestBackend};
use chrono::Utc;
use mimi_content::MimiContent;

#[tokio::test(flavor = "multi_thread")]
async fn multi_device_invited_after_linking_sees_own_messages_on_sibling() -> anyhow::Result<()> {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    setup.connect_users(&alice, &bob).await;

    let alice2 = alice.add_linked_device().await;
    alice.drain().await;
    alice2.drain().await;

    // Device A publishes a fresh batch via the self-group.
    {
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
    }
    alice2.drain().await;

    // Bob invites Alice: device B joins from the same welcome.
    let chat_id = setup.create_group(&bob).await;
    setup.invite_to_group(chat_id, &bob, &[&alice]).await;
    let processed = alice2.drain().await;
    assert!(
        processed
            .new_chats
            .iter()
            .any(|new_chat| new_chat.chat_id == chat_id),
        "the sibling should have joined the group from the welcome"
    );

    // Device A sends: device B must see the echo.
    setup.send_message(chat_id, &alice, &[&bob]).await;
    let processed = alice2.drain().await;
    assert!(
        processed
            .new_messages
            .iter()
            .any(|message| message.chat_id() == chat_id),
        "device B should receive the message device A sent: processed={} new_messages={} errors={:?}",
        processed.processed,
        processed.new_messages.len(),
        processed.errors,
    );

    // Device B sends: device A must see the echo.
    alice2.send_text(chat_id, "from device b").await;
    let processed = alice.drain().await;
    assert!(
        processed
            .new_messages
            .iter()
            .any(|message| message.chat_id() == chat_id),
        "device A should receive the message device B sent: processed={} new_messages={} errors={:?}",
        processed.processed,
        processed.new_messages.len(),
        processed.errors,
    );

    // Bob sends: both devices see it.
    setup.send_message(chat_id, &bob, &[&alice]).await;
    let processed = alice2.drain().await;
    assert!(
        processed
            .new_messages
            .iter()
            .any(|message| message.chat_id() == chat_id),
        "device B should receive Bob's message"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn multi_device_own_echo_confirms_unconfirmed_send() {
    const TEXT: &str = "the DS response to this one got lost";

    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    let chat_id = setup.connect_users(&alice, &bob).await;

    // Linking turns alice's leaf into a virtual-client leaf shared by both
    // devices, which is what makes the DS echo decryptable for the sender.
    let alice2 = alice.add_linked_device().await;
    alice2.outbound_service().run_once().await;

    alice.drain().await;
    alice2.drain().await;
    bob.drain().await;

    let content = MimiContent::simple_markdown_message(TEXT.to_owned(), [11u8; 16]);
    let message = alice
        .send_message(chat_id, content, None, MarkChatAsRead::Yes)
        .await
        .unwrap();
    let message_id = message.id();
    assert!(
        !message.is_sent(),
        "the message should still be waiting for the outbound service"
    );

    // The DS accepts the message and fans it out, but the sending device never
    // sees the response and therefore never confirms the send.
    alice
        .send_chat_message_without_confirmation(message_id)
        .await
        .unwrap();

    bob.drain().await;
    assert_eq!(
        bob.text_messages(chat_id)
            .await
            .iter()
            .filter(|rendered| *rendered == TEXT)
            .count(),
        1,
        "bob should have received the message once"
    );

    // The echo of our own message arrives.
    alice.drain().await;

    let stored = alice
        .message(message_id)
        .await
        .unwrap()
        .expect("the sent message should still be stored");
    assert_eq!(
        alice
            .text_messages(chat_id)
            .await
            .iter()
            .filter(|rendered| *rendered == TEXT)
            .count(),
        1,
        "the echo must not be stored as a second message"
    );
    assert!(
        stored.is_sent(),
        "the echo should have confirmed the unconfirmed send"
    );
    assert_eq!(
        alice.expect_last_message(chat_id).await.id(),
        message_id,
        "the echo must not become a newer message"
    );

    // The message left the queue with the confirmation, so there is nothing
    // left to send.
    alice.outbound_service().run_once().await;

    bob.drain().await;
    assert_eq!(
        bob.text_messages(chat_id)
            .await
            .iter()
            .filter(|rendered| *rendered == TEXT)
            .count(),
        1,
        "bob must not receive the message a second time"
    );

    // For the sibling this is a plain incoming message.
    alice2.drain().await;
    assert_eq!(
        alice2
            .text_messages(chat_id)
            .await
            .iter()
            .filter(|rendered| *rendered == TEXT)
            .count(),
        1,
        "the sibling device should have received the message once"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn multi_device_own_echo_confirms_unconfirmed_edit() {
    const TEXT: &str = "original text";
    const EDITED_TEXT: &str = "edited, the DS response to this one got lost";
    const FINAL_TEXT: &str = "edited a second time";

    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    let chat_id = setup.connect_users(&alice, &bob).await;

    // Linking turns alice's leaf into a virtual-client leaf shared by both
    // devices, which is what makes the DS echo decryptable for the sender.
    let alice2 = alice.add_linked_device().await;
    alice2.outbound_service().run_once().await;

    alice.drain().await;
    alice2.drain().await;
    bob.drain().await;

    // A regular, confirmed send of the original message.
    let message = alice.send_text(chat_id, TEXT).await;
    let message_id = message.id();
    alice.drain().await;
    alice2.drain().await;
    bob.drain().await;

    // The edit is stored and reaches the DS, but the sending device never
    // sees the response and therefore never confirms the send.
    let original = alice.expect_message(message_id).await;
    let edit = MimiContent::simple_markdown_message(EDITED_TEXT.to_owned(), [22u8; 16]);
    let edited = alice
        .send_message(chat_id, edit, Some(original), MarkChatAsRead::Yes)
        .await
        .unwrap();
    assert!(
        !edited.is_sent(),
        "the edit should still be waiting for the outbound service"
    );
    alice
        .send_chat_message_without_confirmation(message_id)
        .await
        .unwrap();

    // The echo of the edit arrives.
    alice.drain().await;

    let stored = alice.expect_message(message_id).await;
    assert!(
        stored.is_sent(),
        "the echo should have confirmed the unconfirmed edit"
    );
    assert_eq!(
        alice
            .text_messages(chat_id)
            .await
            .iter()
            .filter(|rendered| *rendered == EDITED_TEXT)
            .count(),
        1,
        "the echo must not be stored as a second message"
    );

    // The edit left the queue with the confirmation, so there is nothing left
    // to send.
    alice.outbound_service().run_once().await;

    bob.drain().await;
    assert_eq!(
        bob.text_messages(chat_id)
            .await
            .iter()
            .filter(|rendered| *rendered == EDITED_TEXT)
            .count(),
        1,
        "bob should see the edit applied once"
    );
    alice2.drain().await;
    assert_eq!(
        alice2
            .text_messages(chat_id)
            .await
            .iter()
            .filter(|rendered| *rendered == EDITED_TEXT)
            .count(),
        1,
        "the sibling device should see the edit applied once"
    );

    // Editing the message again still works. An echo that lands in the edit
    // history would block this edit with a conflict.
    let original = alice.expect_message(message_id).await;
    alice.send_edit(&original, FINAL_TEXT).await;

    bob.drain().await;
    assert_eq!(
        bob.text_messages(chat_id)
            .await
            .iter()
            .filter(|rendered| *rendered == FINAL_TEXT)
            .count(),
        1,
        "bob should see the second edit"
    );
}

/// A linked device removes a member from a group it was onboarded into. The
/// commit replaces the shared virtual-client leaf, so it has to be derived from
/// the self group's emulation epoch.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn multi_device_remove_member_from_linked_device() -> anyhow::Result<()> {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    setup.connect_users(&alice, &bob).await;
    let charlie = setup.add_user().await;
    setup.connect_users(&alice, &charlie).await;

    let chat_id = setup.create_group(&alice).await;
    setup
        .invite_to_group(chat_id, &alice, &[&bob, &charlie])
        .await;

    let alice2 = alice.add_synced_device().await;

    alice2
        .remove_users(chat_id, vec![charlie.user_id().clone()])
        .await?;

    let members = alice2.mls_chat_participants(chat_id).await.unwrap();
    assert!(
        !members.contains(charlie.user_id()),
        "the linked device should no longer see the removed member, got {members:?}"
    );

    // The sibling emulator client follows the removal on the shared leaf.
    alice.drain().await;
    assert_eq!(
        alice.group_epoch_and_own_index(chat_id).await?,
        alice2.group_epoch_and_own_index(chat_id).await?,
        "both emulator clients must stay on the same epoch and shared leaf"
    );

    Ok(())
}

/// After the sibling rotates the user profile, the linked device must still be
/// able to run a group operation: those encrypt the own user profile key for
/// the commit, so the device has to keep knowing which key is its own.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn multi_device_remove_member_after_sibling_rotated_the_profile() -> anyhow::Result<()> {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    setup.connect_users(&alice, &bob).await;
    let charlie = setup.add_user().await;
    setup.connect_users(&alice, &charlie).await;

    let chat_id = setup.create_group(&alice).await;
    setup
        .invite_to_group(chat_id, &alice, &[&bob, &charlie])
        .await;

    let alice2 = alice.add_synced_device().await;

    // The sibling rotates the user profile key and announces it to every group.
    alice
        .set_own_user_profile(UserProfile {
            user_id: alice.user_id().clone(),
            display_name: "New Alice".parse().unwrap(),
            profile_picture: None,
        })
        .await?;

    // The linked device picks the new key up and fetches the profile.
    alice2.drain().await;
    alice2.outbound_service().run_once().await;

    alice2
        .remove_users(chat_id, vec![charlie.user_id().clone()])
        .await?;

    let members = alice2.mls_chat_participants(chat_id).await.unwrap();
    assert!(
        !members.contains(charlie.user_id()),
        "the linked device should no longer see the removed member, got {members:?}"
    );

    Ok(())
}
