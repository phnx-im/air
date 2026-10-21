// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! State a newly linked device gets: settings, connection chats, and catching
//! up with the self group.

use aircoreclient::{ChatStatus, ChatType, ReadReceiptsSetting};
use airserver_test_harness::utils::{core_user_ext::CoreUserExt, setup::TestBackend};

use super::{read_receipts, send_and_receive};

// A setting stored before any linking travels to the new device in the
// provisioning package and is applied during bootstrap.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn linking_payload_carries_settings() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;

    // No self-group exists yet, so the setting is only stored locally.
    alice
        .set_synced_user_setting(&ReadReceiptsSetting(false))
        .await
        .unwrap();
    assert_eq!(read_receipts(&alice).await, Some(false));

    let alice2 = alice.add_linked_device().await;

    // The new device starts from the snapshot in the provisioning package,
    // without processing any queue messages.
    assert_eq!(read_receipts(&alice2).await, Some(false));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn onboarding_survives_self_group_advancing() {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    let group_chat_id = setup.connected_group(&alice, &[&bob]).await;

    let alice2 = alice.add_linked_device().await;
    alice2.outbound_service().run_once().await;

    // Device 3's onboarding into the group is queued here, but not sent yet.
    let alice3 = alice.add_linked_device().await;

    // Linking a fourth device advances the self group, so the emulation epoch
    // current when device 3 was linked is now stale. Device 4 registers
    // emulation epochs only from its own join onwards, so it cannot follow a
    // commit derived from that older epoch.
    let alice4 = alice.add_linked_device().await;

    alice.drain().await;
    alice2.drain().await;
    alice3.drain_unchecked().await;
    alice4.drain_unchecked().await;

    // Device 4 onboards first, so device 3's commit has to be followed by a
    // device that was not in the self group at device 3's linking epoch.
    alice4.outbound_service().run_once().await;
    alice.drain().await;
    alice2.drain().await;
    alice3.drain_unchecked().await;

    alice3.outbound_service().run_once().await;
    assert!(
        alice3.resync_status(group_chat_id).await.unwrap().is_none(),
        "device 3 should have completed onboarding into the higher-level group"
    );

    alice4.drain().await;
    assert_eq!(
        alice4
            .group_epoch_and_own_index(group_chat_id)
            .await
            .unwrap(),
        alice3
            .group_epoch_and_own_index(group_chat_id)
            .await
            .unwrap(),
        "device 4 should stay on the shared leaf after device 3 onboards"
    );

    send_and_receive(
        &alice3,
        &[&alice, &alice2, &alice4],
        group_chat_id,
        "from device 3 after a late onboarding",
    )
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn linked_device_inherits_connection_chats() {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    let connection_chat_id = setup.connect_users(&alice, &bob).await;

    let alice2 = alice.add_linked_device().await;
    alice2.outbound_service().run_once().await;

    assert!(
        alice2
            .resync_status(connection_chat_id)
            .await
            .unwrap()
            .is_none(),
        "onboarding into the connection group should have completed"
    );

    let chat = alice2
        .chat(&connection_chat_id)
        .await
        .expect("linked device should have inherited the connection chat");
    assert!(
        matches!(chat.chat_type(), ChatType::Connection(user_id) if user_id == bob.user_id()),
        "inherited chat should be a confirmed connection chat, got {:?}",
        chat.chat_type()
    );
    assert_eq!(chat.status(), &ChatStatus::Active);

    // The contact carries the friendship token and WAI key, which are not
    // recoverable from the group and therefore have to ride the linking channel.
    let contact = alice2
        .contact(bob.user_id())
        .await
        .expect("linked device should have inherited the contact");
    assert_eq!(contact.chat_id, connection_chat_id);

    // The old device has to follow the onboarding external commit onto the
    // virtual client's new leaf.
    alice.drain().await;
    assert_eq!(
        alice
            .group_epoch_and_own_index(connection_chat_id)
            .await
            .unwrap(),
        alice2
            .group_epoch_and_own_index(connection_chat_id)
            .await
            .unwrap(),
        "both emulator clients must land on the same epoch and shared leaf"
    );

    send_and_receive(
        &alice2,
        &[&alice],
        connection_chat_id,
        "hello bob, from the linked device",
    )
    .await;
    send_and_receive(
        &alice,
        &[&alice2],
        connection_chat_id,
        "hello bob, from the old device",
    )
    .await;

    send_and_receive(
        &bob,
        &[&alice2, &alice],
        connection_chat_id,
        "hello alice, from bob",
    )
    .await;

    // Inviting bob into a fresh group is what actually consumes the transferred key
    // material.
    let group_chat_id = alice2
        .create_chat(
            "group from the linked device".to_owned(),
            None,
            setup.apq_groups,
        )
        .await
        .unwrap();
    alice2
        .invite_users(group_chat_id, std::slice::from_ref(bob.user_id()))
        .await
        .expect("failed to invite bob");

    bob.drain().await;
    assert!(
        bob.chat(&group_chat_id).await.is_some(),
        "bob should have joined the group created by the linked device"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn linked_device_skips_blocked_connection_chats() {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    let connection_chat_id = setup.connect_users(&alice, &bob).await;

    alice.block_contact(bob.user_id().clone()).await.unwrap();

    let alice2 = alice.add_linked_device().await;
    alice2.outbound_service().run_once().await;

    assert!(
        alice2.chat(&connection_chat_id).await.is_none(),
        "a blocked connection chat should not be conveyed to a linked device"
    );
}
