// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! The read marker of a request chat following the device that opened it.

use airserver_test_harness::utils::{core_user_ext::CoreUserExt, setup::TestBackend};
use mimi_content::MessageStatus;

use crate::multi_device::usernames::add_synced_username;

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn opened_request_is_read_on_sibling() {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    let alice2 = alice.add_synced_device().await;
    let record = add_synced_username(&setup, &alice, &alice2).await;
    bob.add_contact(record.username.clone(), record.hash, setup.apq_groups)
        .await
        .unwrap()
        .unwrap();
    // Device 2 fetches the offer and forwards the request to device 1.
    let chat_id = alice2.receive_connection_offer(&record).await;
    alice2.outbound_service().run_once().await;
    alice.drain().await;
    for (device, label) in [(&alice, "device 1"), (&alice2, "device 2")] {
        assert_eq!(
            device.unread_messages_count(chat_id).await,
            1,
            "{label} should show the unopened request as unread"
        );
    }

    // Device 1 opens the request. Its announcement is reported like any read
    // message. There is no group with the sender yet, so the report goes
    // through the self group.
    let announcement = alice.expect_last_message(chat_id).await;
    let (marked, read) = alice
        .mark_chat_as_read(chat_id, announcement.id())
        .await
        .unwrap();
    assert!(marked);
    assert_eq!(read.len(), 1, "the announcement should be reported");
    alice
        .outbound_service()
        .enqueue_receipts(
            chat_id,
            read.iter()
                .map(|(message_id, mimi_id)| (*message_id, mimi_id, MessageStatus::Read)),
        )
        .await
        .unwrap();
    alice.outbound_service().run_once().await;
    alice2.drain().await;
    assert_eq!(
        alice2.unread_messages_count(chat_id).await,
        0,
        "device 2 should follow the read marker of its sibling"
    );
    assert_eq!(alice2.global_unread_messages_count().await.unwrap(), 0);
    assert_eq!(alice.unread_messages_count(chat_id).await, 0);
}
