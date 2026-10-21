// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Read markers following a sibling.

use aircoreclient::ReadReceiptsSetting;
use airserver_test_harness::utils::{core_user_ext::CoreUserExt, setup::TestBackend};
use mimi_content::MessageStatus;

use super::{one_unread_message_on_both_devices, self_chat_id};

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn read_marker_is_followed_by_sibling() {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    let chat_id = setup.connect_users(&alice, &bob).await;

    let alice2 = one_unread_message_on_both_devices(&alice, &bob, chat_id).await;

    // Device 1 reads the message and sends the receipt for it.
    let message = alice.expect_last_message(chat_id).await;
    let (marked, _) = alice
        .mark_chat_as_read(chat_id, message.id())
        .await
        .unwrap();
    assert!(marked);
    alice
        .outbound_service()
        .enqueue_receipts(
            chat_id,
            [(
                message.id(),
                message.message().mimi_id().unwrap(),
                MessageStatus::Read,
            )]
            .into_iter(),
        )
        .await
        .unwrap();
    alice.outbound_service().run_once().await;

    // The receipt is also device 1's read marker: device 2 follows it.
    alice2.drain().await;
    assert_eq!(
        alice2.unread_messages_count(chat_id).await,
        0,
        "device 2 should follow the read marker of its sibling"
    );

    // Bob writes again, so device 2 has something unread that is not ours. The
    // unread count drops our own messages by itself, but not his.
    bob.drain().await;
    bob.send_text(chat_id, "from bob again").await;

    alice.drain().await;
    alice2.drain().await;
    assert_eq!(alice2.unread_messages_count(chat_id).await, 1);

    // Sending is an implicit read of everything before it, so a message from
    // device 1 carries Bob's older one with it on device 2 as well.
    alice.send_text(chat_id, "from device 1").await;

    alice2.drain().await;
    assert_eq!(
        alice2.unread_messages_count(chat_id).await,
        0,
        "a message we sent from another device reads the chat up to it"
    );
    assert_eq!(alice2.global_unread_messages_count().await.unwrap(), 0);
}

/// With read receipts disabled, the read receipts travel through the self group.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn read_marker_uses_self_group_when_receipts_off() {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    let chat_id = setup.connect_users(&alice, &bob).await;

    let alice2 = one_unread_message_on_both_devices(&alice, &bob, chat_id).await;

    for device in [&alice, &alice2] {
        device
            .set_user_setting(&ReadReceiptsSetting(false))
            .await
            .unwrap();
    }

    // Device 1 reads the message. With read receipts off, the report goes to the
    // self chat instead of the chat with Bob (see `applogic::mark_as_read`).
    let message = alice.expect_last_message(chat_id).await;
    let (marked, _) = alice
        .mark_chat_as_read(chat_id, message.id())
        .await
        .unwrap();
    assert!(marked);
    alice
        .outbound_service()
        .enqueue_receipts(
            self_chat_id(&alice).await,
            [(
                message.id(),
                message.message().mimi_id().unwrap(),
                MessageStatus::Read,
            )]
            .into_iter(),
        )
        .await
        .unwrap();
    alice.outbound_service().run_once().await;

    alice2.drain().await;
    assert_eq!(
        alice2.unread_messages_count(chat_id).await,
        0,
        "device 2 should follow a read marker sent through the self group"
    );

    // Bob is not a member of the self group and must not learn that Alice read
    // his message.
    bob.drain().await;
    let bobs_message = bob.expect_last_message(chat_id).await;
    assert_eq!(
        bobs_message.status(),
        MessageStatus::Delivered,
        "a read marker sent through the self group must not reach the other \
         members, while delivery receipts still do"
    );
}
