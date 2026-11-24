// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Mentions of chat members in messages.

use airserver_test_harness::utils::{core_user_ext::CoreUserExt, setup::TestBackend};

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn mention_sets_mentions_me() {
    let mut setup = TestBackend::single().await;

    let [alice, bob, charlie] = setup.add_users().await;

    let group_chat = setup.connected_group(&alice, &[&bob, &charlie]).await;

    // Alice mentions Bob in the group. Her own message never sets the flag.
    let message = alice
        .send_text(group_chat, &format!("Hi [@Bob]({})!", bob.user_id().uri()))
        .await;
    assert!(!message.mentions_me());
    assert_eq!(alice.unread_counts(group_chat).await.mentions, 0);

    // Bob receives the message with the flag set
    let processed = bob.drain().await;
    let message = processed.new_messages.last().unwrap();
    assert!(message.mentions_me());
    assert_eq!(bob.unread_counts(group_chat).await.mentions, 1);

    // Charlie is not mentioned
    let processed = charlie.drain().await;
    let message = processed.new_messages.last().unwrap();
    assert!(!message.mentions_me());
    assert_eq!(charlie.unread_counts(group_chat).await.mentions, 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn own_mention_from_sibling_does_not_set_mentions_me() {
    let mut setup = TestBackend::single().await;

    let [alice, bob] = setup.add_users().await;

    let group_chat = setup.connected_group(&alice, &[&bob]).await;
    let alice2 = alice.add_synced_device().await;

    alice
        .send_text(
            group_chat,
            &format!("Note to [@Alice]({})", alice.user_id().uri()),
        )
        .await;

    // The sibling stores the mention, but the message is Alice's own
    let processed = alice2.drain().await;
    let message = processed.new_messages.last().unwrap();
    assert_eq!(message.mentions(), [alice.user_id().clone()]);
    assert!(!message.mentions_me());
    assert_eq!(alice2.unread_counts(group_chat).await.mentions, 0);
}
