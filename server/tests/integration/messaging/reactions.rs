// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Reactions: add, retract, ordering, groups and notifications.

use airserver_test_harness::utils::{core_user_ext::CoreUserExt, setup::TestBackend};
use indexmap::indexmap;

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn reaction_reaches_message_author() {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    let chat_id = setup.connect_users(&alice, &bob).await;

    // Alice sends a message; Bob reacts to it.
    let sent = setup.send_message(chat_id, &alice, &[&bob]).await;

    bob.send_reaction(
        chat_id,
        sent.recipient_message_id(bob.user_id()),
        "🫪".to_owned(),
    )
    .await
    .unwrap();
    bob.outbound_service().run_once().await;

    // Bob sees his own reaction immediately (optimistic).
    assert_eq!(
        bob.message_reactions(sent.recipient_message_id(bob.user_id()))
            .await
            .unwrap(),
        indexmap! { "🫪".to_owned() => vec![bob.user_id().clone()] },
    );

    // Alice receives and stores the reaction on her copy of the message.
    alice.drain().await;
    assert_eq!(
        alice.message_reactions(sent.own_message_id).await.unwrap(),
        indexmap! { "🫪".to_owned() => vec![bob.user_id().clone()] },
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn retracted_reaction_is_removed_on_peer() {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    let chat_id = setup.connect_users(&alice, &bob).await;

    let sent = setup.send_message(chat_id, &alice, &[&bob]).await;

    bob.send_reaction(
        chat_id,
        sent.recipient_message_id(bob.user_id()),
        "🫪".to_owned(),
    )
    .await
    .unwrap();
    bob.outbound_service().run_once().await;
    alice.drain().await;
    assert_eq!(
        alice
            .message_reactions(sent.own_message_id)
            .await
            .unwrap()
            .len(),
        1,
    );

    // Bob retracts the reaction.
    bob.delete_reaction(
        chat_id,
        sent.recipient_message_id(bob.user_id()),
        "🫪".to_owned(),
    )
    .await
    .unwrap();
    bob.outbound_service().run_once().await;

    // Gone locally for Bob and, after processing, for Alice too.
    assert!(
        bob.message_reactions(sent.recipient_message_id(bob.user_id()))
            .await
            .unwrap()
            .is_empty()
    );
    alice.drain().await;
    assert!(
        alice
            .message_reactions(sent.own_message_id)
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn reaction_survives_network_error() {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    let chat_id = setup.connect_users(&alice, &bob).await;

    let sent = setup.send_message(chat_id, &alice, &[&bob]).await;
    let bob_target = sent.recipient_message_id(bob.user_id());

    // Flush the delivery receipt, otherwise the run aborts before reactions
    bob.outbound_service().run_once().await;
    setup.listener_control_handle().set_drop_all();
    bob.send_reaction(chat_id, bob_target, "🫪".to_owned())
        .await
        .unwrap();
    bob.outbound_service().run_once().await;
    setup.listener_control_handle().set_normal();

    // The network error keeps the reaction queued, it is not rolled back.
    let expected = indexmap! { "🫪".to_owned() => vec![bob.user_id().clone()] };
    assert_eq!(bob.message_reactions(bob_target).await.unwrap(), expected);

    bob.outbound_service().run_once().await;
    assert_eq!(bob.message_reactions(bob_target).await.unwrap(), expected);

    alice.drain().await;
    assert_eq!(
        alice.message_reactions(sent.own_message_id).await.unwrap(),
        expected,
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn queued_reaction_and_retract_apply_in_order() {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    let chat_id = setup.connect_users(&alice, &bob).await;

    let sent = setup.send_message(chat_id, &alice, &[&bob]).await;
    let bob_target = sent.recipient_message_id(bob.user_id());

    // Flush the delivery receipt, otherwise the run aborts before reactions
    bob.outbound_service().run_once().await;
    setup.listener_control_handle().set_drop_all();
    bob.send_reaction(chat_id, bob_target, "🫪".to_owned())
        .await
        .unwrap();
    bob.delete_reaction(chat_id, bob_target, "🫪".to_owned())
        .await
        .unwrap();
    bob.outbound_service().run_once().await;
    setup.listener_control_handle().set_normal();

    bob.outbound_service().run_once().await;
    assert!(bob.message_reactions(bob_target).await.unwrap().is_empty());

    alice.drain().await;
    assert!(
        alice
            .message_reactions(sent.own_message_id)
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn user_can_add_multiple_emojis() {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    let chat_id = setup.connect_users(&alice, &bob).await;

    let sent = setup.send_message(chat_id, &alice, &[&bob]).await;
    let bob_target = sent.recipient_message_id(bob.user_id());

    // Two distinct emojis are independent reactions.
    bob.send_reaction(chat_id, bob_target, "🫪".to_owned())
        .await
        .unwrap();
    bob.send_reaction(chat_id, bob_target, "👍".to_owned())
        .await
        .unwrap();
    // Re-reacting with an existing emoji is a no-op.
    bob.send_reaction(chat_id, bob_target, "🫪".to_owned())
        .await
        .unwrap();
    bob.outbound_service().run_once().await;

    alice.drain().await;
    assert_eq!(
        alice.message_reactions(sent.own_message_id).await.unwrap(),
        indexmap! { "👍".to_owned() => vec![bob.user_id().clone()], "🫪".to_owned() => vec![bob.user_id().clone()] },
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn group_reactions_list_each_reactor() {
    let mut setup = TestBackend::single().await;
    let [alice, bob, charlie] = setup.add_users().await;

    let group_chat = setup.connected_group(&alice, &[&bob, &charlie]).await;

    // Alice posts; Bob and Charlie each react with a different emoji.
    let sent = setup
        .send_message(group_chat, &alice, &[&bob, &charlie])
        .await;

    bob.send_reaction(
        group_chat,
        sent.recipient_message_id(bob.user_id()),
        "🫪".to_owned(),
    )
    .await
    .unwrap();
    bob.outbound_service().run_once().await;

    charlie
        .send_reaction(
            group_chat,
            sent.recipient_message_id(charlie.user_id()),
            "👍".to_owned(),
        )
        .await
        .unwrap();
    charlie.outbound_service().run_once().await;

    // Alice sees who reacted with what — the basis for the "who reacted" view.
    alice.drain().await;
    assert_eq!(
        alice.message_reactions(sent.own_message_id).await.unwrap(),
        indexmap! { "👍".to_owned() => vec![charlie.user_id().clone()], "🫪".to_owned() => vec![bob.user_id().clone()] },
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn reaction_notifies_message_author() {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    let chat_id = setup.connect_users(&alice, &bob).await;

    // Alice sends a message; Bob reacts to it.
    let sent = setup.send_message(chat_id, &alice, &[&bob]).await;
    bob.send_reaction(
        chat_id,
        sent.recipient_message_id(bob.user_id()),
        "🫪".to_owned(),
    )
    .await
    .unwrap();
    bob.outbound_service().run_once().await;

    // Alice processes and is notified about the reaction on her own message.
    let processed = alice.drain().await;
    assert_eq!(processed.reaction_notifications.len(), 1);
    assert_eq!(
        processed.reaction_notifications[0].reactor,
        bob.user_id().clone()
    );
    assert_eq!(processed.reaction_notifications[0].emoji, "🫪");

    // Retracting it produces no notification.
    bob.delete_reaction(
        chat_id,
        sent.recipient_message_id(bob.user_id()),
        "🫪".to_owned(),
    )
    .await
    .unwrap();
    bob.outbound_service().run_once().await;
    let processed = alice.drain().await;
    assert!(processed.reaction_notifications.is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn reaction_does_not_notify_bystander() {
    let mut setup = TestBackend::single().await;
    let [alice, bob, charlie] = setup.add_users().await;

    let group_chat = setup.connected_group(&alice, &[&bob, &charlie]).await;

    // Bob posts; Charlie reacts to Bob's message.
    let sent = setup
        .send_message(group_chat, &bob, &[&alice, &charlie])
        .await;
    charlie
        .send_reaction(
            group_chat,
            sent.recipient_message_id(charlie.user_id()),
            "🫪".to_owned(),
        )
        .await
        .unwrap();
    charlie.outbound_service().run_once().await;

    // Alice (a bystander) is not notified — it's not her message.
    let processed = alice.drain().await;
    assert!(processed.reaction_notifications.is_empty());

    // Bob (the author) is notified.
    let processed = bob.drain().await;
    assert_eq!(processed.reaction_notifications.len(), 1);
    assert_eq!(
        processed.reaction_notifications[0].reactor,
        charlie.user_id().clone()
    );
}
