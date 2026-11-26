// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Read markers following a sibling.

use aircoreclient::{ChatMuted, ReadReceiptsSetting};
use airserver_test_harness::utils::{
    controlled_listener::Mode, core_user_ext::CoreUserExt, setup::TestBackend,
};
use mimi_content::MessageStatus;

use super::{one_unread_message_on_both_devices, set_push_token};

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn read_marker_is_followed_by_sibling() {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    let chat_id = setup.connect_users(&alice, &bob).await;

    let alice2 = one_unread_message_on_both_devices(&alice, &bob, chat_id).await;

    // Device 1 reads the message and sends the receipt for it.
    alice.read_chat(chat_id, true).await;

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

    // Device 1 reads the message. With read receipts off, the report only goes
    // to the self group.
    alice.read_chat(chat_id, false).await;

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

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn read_marker_is_not_sent_without_linked_device() {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    let chat_id = setup.connect_users(&alice, &bob).await;

    bob.send_text(chat_id, "hello").await;
    alice.drain().await;
    alice.outbound_service().run_once().await;

    // Alice has a self group, but no other device follows her read marker.
    // With read receipts off, the read must not send anything, so the armed
    // rejection stays unused.
    let control = setup.listener_control_handle();
    control.set_reject_next_request_to("SendMessage");
    let read = alice.read_chat(chat_id, false).await;
    assert_eq!(read.len(), 1);
    assert_eq!(
        control.mode(),
        Mode::RejectNextRequest,
        "a read without a linked device should not reach the self group"
    );
    control.set_normal();
}

/// A read on one device wakes the other device with a push through the self
/// group, whatever the read receipts setting. Neither the reading device nor the
/// other members get a push for it, and nobody gets one in a muted chat. A
/// delivery receipt wakes up nobody.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn read_marker_pushes_only_the_sibling() {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    let chat_id = setup.connect_users(&alice, &bob).await;

    let alice2 = one_unread_message_on_both_devices(&alice, &bob, chat_id).await;
    let devices = [(&alice, "alice"), (&alice2, "alice2"), (&bob, "bob")];
    for (device, token) in devices {
        set_push_token(device, token).await;
    }

    let rounds = [(true, false), (false, false), (true, true), (false, true)];
    for (read_receipts_enabled, muted) in rounds {
        if muted {
            alice
                .set_chat_muted_until(chat_id, Some(ChatMuted::Forever))
                .await
                .unwrap();
        }
        // Settles the self group commits of the push token updates and the
        // mute, and Bob's message of the previous round.
        for _ in 0..2 {
            for (device, _) in devices {
                device.outbound_service().run_once().await;
                device.drain().await;
            }
        }
        assert!(alice2.unread_messages_count(chat_id).await > 0);
        assert_eq!(alice2.expect_chat(chat_id).await.is_muted(), muted);
        setup.take_sent_pushes();

        alice.read_chat(chat_id, read_receipts_enabled).await;

        let expected: &[&str] = if muted { &[] } else { &["alice2"] };
        assert_eq!(
            setup.take_sent_pushes(),
            expected,
            "read receipts enabled: {read_receipts_enabled}, muted: {muted}"
        );
        alice2.drain().await;
        assert_eq!(alice2.unread_messages_count(chat_id).await, 0);

        bob.send_text(chat_id, "from bob again").await;
        setup.take_sent_pushes();
        alice.drain().await;
        alice.outbound_service().run_once().await;
        assert_eq!(
            setup.take_sent_pushes(),
            Vec::<String>::new(),
            "a delivery receipt should not push, muted: {muted}"
        );
    }
}
