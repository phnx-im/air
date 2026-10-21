// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Blocking contacts, in 1:1 chats and in groups.

use aircoreclient::{BlockedContactError, UserProfile, clients::MarkChatAsRead};
use airserver_test_harness::utils::{core_user_ext::CoreUserExt, setup::TestBackend};
use mimi_content::MimiContent;
use tracing::info;

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn blocked_contact() {
    info!("Setting up setup");
    let mut setup = TestBackend::single().await;
    info!("Creating users");
    let alice = setup.add_user().await;
    setup.add_username(&alice).await;
    info!("Created alice");
    let bob = setup.add_user().await;

    let chat_id = setup.connect_users(&alice, &bob).await;
    setup.send_message(chat_id, &alice, &[&bob]).await;
    setup.send_message(chat_id, &bob, &[&alice]).await;

    alice.block_contact(bob.user_id().clone()).await.unwrap();

    alice.drain().await;
    bob.drain().await;

    // Not possible to send a message to Bob
    let msg = MimiContent::simple_markdown_message("Hello".into(), [0; 16]);
    let res = alice
        .send_message(chat_id, msg.clone(), None, MarkChatAsRead::Yes)
        .await;
    res.unwrap_err().downcast::<BlockedContactError>().unwrap();

    assert_eq!(bob.drain().await.processed, 0);

    // Updating Alice's profile is not communicated to Bob
    alice
        .update_user_profile(UserProfile {
            user_id: alice.user_id().clone(),
            display_name: "Alice in Wonderland".parse().unwrap(),
            profile_picture: None,
        })
        .await
        .unwrap();
    assert_eq!(bob.drain().await.processed, 0);

    // Updating Bob's profile is not communicated to Alice
    bob.update_user_profile(UserProfile {
        user_id: bob.user_id().clone(),
        display_name: "Annoying Bob".parse().unwrap(),
        profile_picture: None,
    })
    .await
    .unwrap();
    // We get the message but it is dropped
    let messages = alice.qs_fetch_messages().await.unwrap();
    assert_eq!(messages.len(), 1);
    let res = alice.fully_process_qs_messages(messages).await;
    assert!(res.is_empty(), "message is dropped");

    // Messages from bob are dropped
    bob.send_message(chat_id, msg, None, MarkChatAsRead::Yes)
        .await
        .unwrap();
    bob.outbound_service().run_once().await;
    // We get the message but it is dropped
    let messages = alice.qs_fetch_messages().await.unwrap();
    assert_eq!(messages.len(), 1);
    let res = alice.fully_process_qs_messages(messages).await;
    assert!(res.is_empty(), "message is dropped");

    // Bob cannot establish a new connection with Alice
    let alice_username = setup.username_record(&alice).unwrap().username;
    let alice_username_hash = alice_username.calculate_hash().unwrap();
    bob.add_contact(
        alice_username.clone(),
        alice_username_hash,
        setup.apq_groups,
    )
    .await
    .expect("fatal error")
    .expect("non-fatal error");
    let mut messages = alice.fetch_username_messages().await.unwrap();
    assert_eq!(messages.len(), 1);

    let res = alice
        .process_username_queue_message(alice_username, messages.pop().unwrap())
        .await;
    res.unwrap_err().downcast::<BlockedContactError>().unwrap();

    // Unblock Bob
    alice.unblock_contact(bob.user_id().clone()).await.unwrap();

    // Sending messages works again
    setup.send_message(chat_id, &alice, &[&bob]).await;
    setup.send_message(chat_id, &bob, &[&alice]).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn group_with_blocked_contact() {
    let mut setup = TestBackend::single().await;

    let alice = setup.add_user().await;
    setup.add_username(&alice).await;

    let [bob, charlie] = setup.add_users().await;

    setup.connect_users(&alice, &bob).await;
    setup.connect_users(&alice, &charlie).await;

    // Create a group with alice, bob and charlie
    let chat_id = setup.create_group(&alice).await;
    setup
        .invite_to_group(chat_id, &alice, &[&bob, &charlie])
        .await;

    // Sending messages works before blocking
    setup.send_message(chat_id, &alice, &[&bob, &charlie]).await;
    setup.send_message(chat_id, &bob, &[&alice, &charlie]).await;

    // Block bob
    alice.block_contact(bob.user_id().clone()).await.unwrap();

    // Messages are still sent and received
    setup.send_message(chat_id, &bob, &[&alice, &charlie]).await;
    setup.send_message(chat_id, &alice, &[&bob, &charlie]).await;
}
