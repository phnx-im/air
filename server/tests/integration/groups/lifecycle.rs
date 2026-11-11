// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Creating T and APQ groups, sending into them, room policy and commit
//! confirmation via the queue.

use std::slice;

use aircoreclient::SystemMessage;

use airserver_test_harness::utils::{core_user_ext::CoreUserExt, setup::TestBackend};
use chrono::{Duration, Utc};
use tracing::info;

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn create_group() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    setup.create_group(&alice).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn inviting_nobody_is_noop() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let chat_id = setup.create_group(&alice).await;

    // Inviting an empty user list should be a no-op
    let result = alice
        .invite_users(chat_id, &[])
        .await
        .expect("inviting an empty user list should succeed");
    assert!(result.messages.is_empty());
    assert!(result.users_not_added.is_empty());

    let participants = alice.chat_participants(chat_id).await.unwrap();
    assert_eq!(participants.len(), 1);
    assert!(participants.contains(alice.user_id()));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn create_group_apq() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    setup.create_apq_group(&alice).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn send_message() {
    info!("Setting up setup");
    let mut setup = TestBackend::single().await;
    info!("Creating users");
    let [alice, bob] = setup.add_users().await;
    let chat_id = setup.connect_users(&alice, &bob).await;
    setup.send_message(chat_id, &alice, &[&bob]).await;
    setup.send_message(chat_id, &bob, &[&alice]).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn send_message_apq() {
    let mut setup = TestBackend::single().await;
    let [alice, bob, charlie] = setup.add_users().await;
    let chat_id = setup.connected_apq_group(&alice, &[&bob, &charlie]).await;
    setup.send_message(chat_id, &alice, &[&bob, &charlie]).await;
    setup.send_message(chat_id, &bob, &[&alice, &charlie]).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn members_can_invite_and_remove_each_other() {
    let mut setup = TestBackend::single().await;

    // Create alice and bob
    let [alice, bob, charlie] = setup.add_users().await;

    // Connect them
    let _chat_alice_bob = setup.connect_users(&alice, &bob).await;
    let _chat_alice_charlie = setup.connect_users(&alice, &charlie).await;
    let _chat_bob_charlie = setup.connect_users(&bob, &charlie).await;

    // Create an independent group and invite bob.
    let chat_id = setup.create_group(&alice).await;

    setup.invite_to_group(chat_id, &alice, &[&bob]).await;

    // Bob can invite charlie
    setup.invite_to_group(chat_id, &bob, &[&charlie]).await;

    // Charlie can kick alice
    setup
        .remove_from_group(chat_id, &charlie, &[&alice])
        .await
        .unwrap();

    // Charlie can kick bob
    setup
        .remove_from_group(chat_id, &charlie, &[&bob])
        .await
        .unwrap();

    // TODO: This currently fails
    // Charlie can leave and an empty room remains
    // setup.leave_group(chat_id, &charlie).await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn dropped_commit_response_is_confirmed_via_queue() {
    let mut setup = TestBackend::single().await;

    let [alice, bob] = setup.add_users().await;

    setup.connect_users(&alice, &bob).await;
    let chat_id = setup.create_group(&alice).await;

    // Make server drop connection instead of sending the commit response
    let group_operation = if setup.apq_groups {
        "ApqGroupOperation"
    } else {
        "GroupOperation"
    };
    setup
        .listener_control_handle()
        .set_drop_next_response_to(group_operation);

    let _ = alice
        .invite_users(chat_id, slice::from_ref(bob.user_id()))
        .await
        .expect_err("No error despite server dropping messages");

    // Bob should not be in the group.
    let number_of_members = alice.mls_members(chat_id).await.unwrap().unwrap().len();
    assert_eq!(number_of_members, 1);

    // Set server to normal networking mode
    setup.listener_control_handle().set_normal();

    // At this point, Alice has a pending commit. Once she receives the
    // confirmation from the queue, she should be able to create another commit.
    let qs_messages = alice.qs_fetch_messages().await.unwrap();
    println!("Number of QS messages: {}", qs_messages.len());
    alice.fully_process_qs_messages(qs_messages).await;

    // Bob should now be in the group.
    let number_of_members = alice.mls_members(chat_id).await.unwrap().unwrap().len();
    assert_eq!(number_of_members, 2);

    alice
        .update_key(chat_id)
        .await
        .expect("No error despite server dropping messages");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn creation_without_siblings_is_echoed_as_a_receipt() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    // The receipt of the self group
    alice.drain().await;

    let chat_id = setup.create_group(&alice).await;
    let messages_before = alice.messages(chat_id, 100).await.unwrap();

    let queued = alice.qs_fetch_messages().await.unwrap();
    assert_eq!(queued.len(), 1, "the DS should echo the creation");
    let processed = alice.fully_process_qs_messages(queued).await;
    assert!(
        processed.errors.is_empty(),
        "the receipt of a known group must be dropped silently: {:?}",
        processed.errors
    );
    assert!(processed.new_chats.is_empty());
    assert_eq!(
        alice.messages(chat_id, 100).await.unwrap(),
        messages_before,
        "the receipt must not add messages"
    );
}

fn create_group_method(setup: &TestBackend) -> &'static str {
    if setup.apq_groups {
        "CreateApqGroup"
    } else {
        "CreateGroup"
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn lost_creation_answer_is_confirmed_by_the_receipt() {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    setup.connect_users(&alice, &bob).await;
    alice.drain().await;
    let chats_before = alice.ordered_chat_ids().await.unwrap();

    setup
        .listener_control_handle()
        .set_drop_next_response_to(create_group_method(&setup));
    alice
        .create_chat("answer lost".to_owned(), None, setup.apq_groups)
        .await
        .expect_err("the lost answer should surface as an error");
    setup.listener_control_handle().set_normal();

    assert_eq!(
        alice.ordered_chat_ids().await.unwrap(),
        chats_before,
        "a pending group has no chat"
    );
    assert_eq!(alice.pending_group_creations().await.unwrap(), 1);

    // The DS created the group, so its receipt confirms it
    alice.drain().await;
    assert_eq!(alice.pending_group_creations().await.unwrap(), 0);
    let chat_id = alice
        .ordered_chat_ids()
        .await
        .unwrap()
        .into_iter()
        .find(|chat_id| !chats_before.contains(chat_id))
        .expect("the receipt should create the chat");
    let chat = alice.expect_chat(chat_id).await;
    assert_eq!(chat.attributes().unwrap().title(), "answer lost");
    assert!(
        alice
            .system_messages(chat_id)
            .await
            .contains(&SystemMessage::CreateGroup(alice.user_id().clone()))
    );

    // The confirmed group works on the DS
    alice
        .invite_users(chat_id, slice::from_ref(bob.user_id()))
        .await
        .unwrap();
    bob.drain().await;
    assert!(bob.chat(&chat_id).await.is_some());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn creation_without_receipt_is_discarded_after_a_drain() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    alice.drain().await;
    let chats_before = alice.ordered_chat_ids().await.unwrap();

    // The request never reaches the DS, but the client can't tell
    setup
        .listener_control_handle()
        .set_drop_next_request_to(create_group_method(&setup));
    alice
        .create_chat("never created".to_owned(), None, setup.apq_groups)
        .await
        .expect_err("the dropped request should surface as an error");
    setup.listener_control_handle().set_normal();
    assert_eq!(alice.pending_group_creations().await.unwrap(), 1);

    // A drain right after the attempt proves nothing yet
    alice.drain().await;
    assert_eq!(alice.pending_group_creations().await.unwrap(), 1);

    // A drain well after the attempt would have contained the receipt
    alice
        .discard_unconfirmed_group_creations(Utc::now() + Duration::minutes(3))
        .await
        .unwrap();
    assert_eq!(alice.pending_group_creations().await.unwrap(), 0);
    assert_eq!(alice.ordered_chat_ids().await.unwrap(), chats_before);

    setup.create_group(&alice).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn rejected_creation_is_discarded_at_once() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;

    setup
        .listener_control_handle()
        .set_reject_next_request_to(create_group_method(&setup));
    alice
        .create_chat("rejected".to_owned(), None, setup.apq_groups)
        .await
        .expect_err("the rejection should surface as an error");
    setup.listener_control_handle().set_normal();

    assert_eq!(alice.pending_group_creations().await.unwrap(), 0);
}
