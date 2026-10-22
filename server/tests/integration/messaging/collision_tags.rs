// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Collision tags of message generations and delivery receipts.

use aircommon::messages::client_ds_out::SendMessageCollisionTag;
use aircoreclient::clients::MarkChatAsRead;
use airserver_test_harness::utils::setup::TestBackend;
use mimi_content::MimiContent;

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn fresh_tags_are_accepted() {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    let chat_id = setup.connect_users(&alice, &bob).await;

    alice
        .send_message_with_fixed_collision_tags(
            chat_id,
            vec![
                SendMessageCollisionTag::generation(0xAAi64),
                SendMessageCollisionTag::delivery_receipt(0xBBi64),
            ],
        )
        .await
        .expect("send with new tags should succeed");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn reused_generation_tag_collides() {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    let chat_id = setup.connect_users(&alice, &bob).await;

    let first_message_tags = vec![
        SendMessageCollisionTag::generation(0xAAi64),
        SendMessageCollisionTag::delivery_receipt(0xBBi64),
    ];

    alice
        .send_message_with_fixed_collision_tags(chat_id, first_message_tags.clone())
        .await
        .expect("first send should succeed");

    let error = alice
        .send_message_with_fixed_collision_tags(
            chat_id,
            vec![
                SendMessageCollisionTag::generation(0xAAi64),
                SendMessageCollisionTag::delivery_receipt(0xCCi64),
            ],
        )
        .await
        .expect_err("send with colliding tag1 should be rejected");

    let colliding_tags = error.process_tag_collisions(&first_message_tags);

    assert_eq!(
        colliding_tags,
        vec![SendMessageCollisionTag::generation(0xAAi64),],
        "sequence collision tag collides"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn reused_receipt_tag_collides() {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    let chat_id = setup.connect_users(&alice, &bob).await;

    alice
        .send_message_with_fixed_collision_tags(
            chat_id,
            vec![
                SendMessageCollisionTag::generation(0xAAi64),
                SendMessageCollisionTag::delivery_receipt(0xBBi64),
            ],
        )
        .await
        .expect("first send should succeed");

    let second_message_tags = vec![
        SendMessageCollisionTag::generation(0xCCi64),
        SendMessageCollisionTag::delivery_receipt(0xBBi64),
    ];

    let error = alice
        .send_message_with_fixed_collision_tags(chat_id, second_message_tags.clone())
        .await
        .expect_err("send with colliding tag2 should be rejected");

    let colliding_tags = error.process_tag_collisions(&second_message_tags);

    assert_eq!(
        colliding_tags,
        vec![SendMessageCollisionTag::delivery_receipt(0xBBi64),],
        "the delivery receipt collides"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn identical_tags_all_collide() {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    let chat_id = setup.connect_users(&alice, &bob).await;

    let tags = vec![
        SendMessageCollisionTag::generation(0xAAi64),
        SendMessageCollisionTag::delivery_receipt(0xBBi64),
        SendMessageCollisionTag::read_receipt(0xCCi64),
    ];

    alice
        .send_message_with_fixed_collision_tags(chat_id, tags.clone())
        .await
        .expect("first send should succeed");

    let error = alice
        .send_message_with_fixed_collision_tags(chat_id, tags.clone())
        .await
        .expect_err("second send with identical tags should be rejected");

    let colliding_tags = error.process_tag_collisions(&tags);

    assert_eq!(tags, colliding_tags, "expected all tags to collide");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn tags_do_not_collide_across_senders() {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    let chat_id = setup.connect_users(&alice, &bob).await;

    let content = MimiContent::simple_markdown_message("collision-test".into(), [0u8; 16]);

    alice
        .send_message(chat_id, content.clone(), None, MarkChatAsRead::Yes)
        .await
        .expect("send from alice should succeed");

    bob.send_message(chat_id, content.clone(), None, MarkChatAsRead::Yes)
        .await
        .expect("send from bob should succeed");
}
