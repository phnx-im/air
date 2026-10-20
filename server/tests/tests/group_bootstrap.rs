// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Sibling emulator clients following a group one of their siblings created or
//! externally joined.

use std::time::Duration;

use aircommon::identifiers::UserId;
use aircoreclient::{
    ChatId, ChatStatus, ChatType, EventMessage, Message, SystemMessage, UsernameRecord,
    clients::CoreUser,
};
use airserver_test_harness::utils::{core_user_ext::CoreUserExt, setup::TestBackend};
use tempfile::TempDir;
use tokio::time::timeout;
use tokio_stream::StreamExt;

use super::multi_device::{link_new_device, send_and_receive};

/// Links a second device to `user_id` and brings both devices up to date.
///
/// Returns the original device and the new one. The [`TempDir`] holds the new
/// device's database and must stay alive as long as it is used.
pub(crate) async fn link_sibling(
    setup: &TestBackend,
    user_id: &UserId,
) -> (CoreUser, CoreUser, TempDir) {
    let (device_b, tmp) = link_new_device(setup, user_id).await;
    // Onboarding into the pre-existing groups runs in the background.
    device_b.outbound_service().run_once().await;
    let device_a = setup.user(user_id).clone();
    device_a.drain().await;
    device_b.drain().await;
    // Linking makes device a upload key packages, which completes only once it
    // fetches its own commit.
    device_a.outbound_service().run_once().await;
    device_a.drain().await;
    device_b.drain().await;
    (device_a, device_b, tmp)
}

/// Drains the username queue of `record` and returns the pending chat the
/// connection offer in it created.
pub(crate) async fn receive_connection_offer(user: &CoreUser, record: &UsernameRecord) -> ChatId {
    drain_username_queue(user, record)
        .await
        .expect("the connection offer should have created a pending chat")
}

/// Drains the username queue of `record` and returns the pending chat a
/// connection offer in it created, if any.
pub(crate) async fn drain_username_queue(
    user: &CoreUser,
    record: &UsernameRecord,
) -> Option<ChatId> {
    let (mut stream, responder) = user.listen_username(record).await.unwrap();
    let mut chat_id = None;
    while let Some(Some(message)) = timeout(Duration::from_millis(500), stream.next())
        .await
        .unwrap()
    {
        let message_id = message.message_id.unwrap();
        chat_id = user
            .process_username_queue_message(record.username.clone(), message)
            .await
            .unwrap()
            .map(|stored| stored.chat_id)
            .or(chat_id);
        responder.ack(message_id.into()).await;
    }
    chat_id
}

/// Asserts that both devices sit on the same epoch and share the virtual
/// client's leaf in `chat_id`.
async fn assert_same_epoch_and_leaf(
    device_a: &CoreUser,
    device_b: &CoreUser,
    chat_id: ChatId,
    context: &str,
) {
    let a = device_a.group_epoch_and_own_index(chat_id).await.unwrap();
    assert!(
        a.is_some(),
        "{context}: the acting device has no group state"
    );
    assert_eq!(
        device_b.group_epoch_and_own_index(chat_id).await.unwrap(),
        a,
        "{context}: the sibling is not on the acting device's epoch and leaf"
    );
    assert_eq!(
        device_b.chat_is_apq(chat_id).await,
        device_a.chat_is_apq(chat_id).await,
        "{context}: the sibling did not install the same legs"
    );
}

/// A sibling installs a group chat its sibling created and can use it.
async fn sibling_follows_a_created_group_chat(apq: bool) {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    setup.connect_users(&alice, &bob).await;

    let (device_a, device_b, _tmp) = link_sibling(&setup, &alice).await;

    let chat_id = if apq {
        setup.create_apq_group(&alice).await
    } else {
        setup.create_non_apq_group(&alice).await
    };

    device_b.drain().await;

    let chat_a = device_a
        .chat(&chat_id)
        .await
        .expect("the creating device should have the chat");
    let chat_b = device_b
        .chat(&chat_id)
        .await
        .expect("the sibling should have installed the chat");
    assert_eq!(chat_b.status(), &ChatStatus::Active);
    assert!(chat_b.chat_type().is_group());
    // The title comes from the group data extension in the snapshot's
    // GroupInfo, not from the blob.
    assert_eq!(
        chat_b.attributes().unwrap().title(),
        chat_a.attributes().unwrap().title(),
        "the sibling should read the title out of the snapshot"
    );

    assert_same_epoch_and_leaf(&device_a, &device_b, chat_id, "after the creation echo").await;

    // Only the group's own creation is recorded, not a second one from the
    // echo.
    let messages = device_b.messages(chat_id, 100).await.unwrap();
    let creations = messages
        .iter()
        .filter(|message| {
            matches!(
                message.message(),
                Message::Event(EventMessage::System(SystemMessage::CreateGroup(user_id)))
                    if user_id == &alice
            )
        })
        .count();
    assert_eq!(creations, 1, "the sibling should record one creation");

    // The group is usable from both devices once a third party is in it.
    device_a
        .invite_users(chat_id, std::slice::from_ref(&bob))
        .await
        .unwrap();
    device_b.drain().await;
    let bob_user = setup.user(&bob);
    bob_user.drain().await;

    assert_same_epoch_and_leaf(&device_a, &device_b, chat_id, "after the add commit").await;

    send_and_receive(
        &device_b,
        &[bob_user, &device_a],
        chat_id,
        "hello from the sibling",
    )
    .await;
    send_and_receive(
        bob_user,
        &[&device_b, &device_a],
        chat_id,
        "hello from the third party",
    )
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn sibling_follows_a_created_t_group_chat() {
    sibling_follows_a_created_group_chat(false).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn sibling_follows_a_created_apq_group_chat() {
    sibling_follows_a_created_group_chat(true).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn the_acting_devices_own_echo_is_ignored() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;

    let (device_a, device_b, _tmp) = link_sibling(&setup, &alice).await;

    let chat_id = setup.create_non_apq_group(&alice).await;
    let before = device_a.group_epoch_and_own_index(chat_id).await.unwrap();
    let title = device_a
        .expect_chat(chat_id)
        .await
        .attributes()
        .unwrap()
        .title()
        .to_owned();
    let messages_before = device_a.messages(chat_id, 100).await.unwrap();

    let queued = device_a.qs_fetch_messages().await.unwrap();
    assert_eq!(
        queued.len(),
        1,
        "the creating device should receive its own echo and nothing else"
    );
    let processed = device_a.fully_process_qs_messages(queued).await;
    assert!(
        processed.errors.is_empty(),
        "the own echo must be dropped silently: {:?}",
        processed.errors
    );
    assert!(
        processed.new_chats.is_empty() && processed.new_connections.is_empty(),
        "the own echo must not surface as a new chat"
    );

    assert_eq!(
        device_a.group_epoch_and_own_index(chat_id).await.unwrap(),
        before,
        "the own echo must not touch the group state"
    );
    assert_eq!(
        device_a
            .expect_chat(chat_id)
            .await
            .attributes()
            .unwrap()
            .title(),
        &title
    );
    assert_eq!(
        device_a.messages(chat_id, 100).await.unwrap(),
        messages_before,
        "the own echo must not add messages"
    );

    // The sibling, which does not have the group yet, still installs it.
    device_b.drain().await;
    assert_same_epoch_and_leaf(&device_a, &device_b, chat_id, "after the creation echo").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn sibling_follows_an_accepted_connection() {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;

    let (device_a, device_b, _tmp) = link_sibling(&setup, &alice).await;

    // Bob asks for a connection to alice's handle, and the device that
    // listens on the handle queue accepts it.
    let record = setup.add_username(&alice).await;
    let hash = record.username.calculate_hash().unwrap();
    setup
        .user(&bob)
        .add_contact(record.username.clone(), hash, setup.apq_groups)
        .await
        .unwrap()
        .unwrap();

    let chat_id = receive_connection_offer(&device_a, &record).await;
    device_a
        .accept_contact_request(chat_id)
        .await
        .unwrap()
        .unwrap();

    let processed = device_b.drain().await;
    // The user accepted this on another of their own devices, so the echo must
    // not raise a notification here.
    assert!(
        processed.new_chats.is_empty() && processed.new_connections.is_empty(),
        "a join echo must not be reported as a new chat"
    );

    let chat_b = device_b
        .chat(&chat_id)
        .await
        .expect("the sibling should have installed the connection chat");
    assert_eq!(chat_b.chat_type(), &ChatType::Connection(bob.clone()));
    assert_eq!(chat_b.status(), &ChatStatus::Active);

    let contact_a = device_a
        .contact(&bob)
        .await
        .expect("the accepting device should have the contact");
    let contact_b = device_b
        .contact(&bob)
        .await
        .expect("the sibling should have the contact");
    assert_eq!(contact_b.user_id, contact_a.user_id);
    assert_eq!(contact_b.chat_id, contact_a.chat_id);

    // The accept context carries no handle, and this device never saw the
    // pending chat that would have one.
    let messages = device_b.messages(chat_id, 100).await.unwrap();
    assert!(
        messages.iter().any(|message| matches!(
            message.message(),
            Message::Event(EventMessage::System(SystemMessage::AcceptedConnectionRequest {
                contact,
                user_handle: None,
            })) if contact == &bob
        )),
        "the sibling should record the acceptance, got {messages:?}"
    );

    // Under the APQ profile this is an APQ connection group, so the echo carries
    // both legs.
    assert_eq!(
        device_a.chat_is_apq(chat_id).await,
        Some(setup.apq_groups),
        "the connection group should follow the test profile"
    );
    assert_same_epoch_and_leaf(&device_a, &device_b, chat_id, "after the join echo").await;

    let bob_user = setup.user(&bob);
    bob_user.drain().await;
    send_and_receive(
        &device_b,
        &[bob_user, &device_a],
        chat_id,
        "hello bob, from the sibling",
    )
    .await;
    send_and_receive(
        bob_user,
        &[&device_b, &device_a],
        chat_id,
        "hello alice, from bob",
    )
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn sibling_mirrors_a_handle_initiated_connection() {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;

    let (device_a, device_b, _tmp) = link_sibling(&setup, &alice).await;

    let record = setup.add_username(&bob).await;
    let hash = record.username.calculate_hash().unwrap();
    let chat_id = device_a
        .add_contact(record.username.clone(), hash, setup.apq_groups)
        .await
        .unwrap()
        .unwrap();

    device_b.drain().await;

    let chat_b = device_b
        .chat(&chat_id)
        .await
        .expect("the sibling should have mirrored the pending chat");
    assert_eq!(
        chat_b.chat_type(),
        &ChatType::HandleConnection(record.username.clone())
    );

    let contact_of = async |device: &CoreUser| {
        device
            .username_contacts()
            .await
            .unwrap()
            .into_iter()
            .find(|contact| contact.chat_id == chat_id)
            .expect("the device should have a username contact for the pending chat")
    };
    let contact_a = contact_of(&device_a).await;
    let contact_b = contact_of(&device_b).await;
    assert_eq!(contact_b.username, contact_a.username);
    assert_eq!(
        contact_b.connection_offer_hash, contact_a.connection_offer_hash,
        "the sibling needs the offer hash to recognize the peer's PSK"
    );

    assert_same_epoch_and_leaf(&device_a, &device_b, chat_id, "after the creation echo").await;

    // Bob accepts.
    let bob_user = setup.user(&bob);
    let bob_chat_id = receive_connection_offer(bob_user, &record).await;
    bob_user
        .accept_contact_request(bob_chat_id)
        .await
        .unwrap()
        .unwrap();

    device_a.drain().await;
    device_b.drain().await;

    for (label, device) in [("initiating", &device_a), ("sibling", &device_b)] {
        let chat = device.expect_chat(chat_id).await;
        assert_eq!(
            chat.chat_type(),
            &ChatType::Connection(bob.clone()),
            "the {label} device should see a confirmed connection"
        );
        assert!(
            device.contact(&bob).await.is_some(),
            "the {label} device should have the contact"
        );
    }

    assert_same_epoch_and_leaf(&device_a, &device_b, chat_id, "after the peer's accept").await;
    send_and_receive(
        &device_b,
        &[bob_user, &device_a],
        chat_id,
        "hello bob, from the sibling",
    )
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn sibling_mirrors_a_targeted_message_connection() {
    let mut setup = TestBackend::single().await;
    let [alice, bob, charlie] = setup.add_users().await;

    // Bob knows both, so he can put alice and charlie into one group without
    // them being connected.
    setup.connect_users(&bob, &alice).await;
    setup.connect_users(&bob, &charlie).await;
    let group_chat_id = setup.create_group(&bob).await;
    setup
        .invite_to_group(group_chat_id, &bob, &[&alice, &charlie])
        .await;

    let (device_a, device_b, _tmp) = link_sibling(&setup, &alice).await;

    let chat_id = device_a
        .add_contact_from_group(group_chat_id, charlie.clone(), setup.apq_groups)
        .await
        .unwrap();

    device_b.drain().await;

    let chat_b = device_b
        .chat(&chat_id)
        .await
        .expect("the sibling should have mirrored the pending chat");
    assert_eq!(
        chat_b.chat_type(),
        &ChatType::TargetedMessageConnection(charlie.clone())
    );

    let contact_a = device_a
        .try_targeted_message_contact(&charlie)
        .await
        .unwrap()
        .expect("the initiating device should have a targeted message contact");
    let contact_b = device_b
        .try_targeted_message_contact(&charlie)
        .await
        .unwrap()
        .expect("the sibling should have a targeted message contact");
    assert_eq!(contact_b.user_id, contact_a.user_id);
    assert_eq!(contact_b.chat_id, contact_a.chat_id);
    let group_id = device_a.expect_chat(group_chat_id).await.group_id;
    assert_eq!(contact_a.origin_group_id.as_ref(), Some(&group_id));
    assert_eq!(contact_b.origin_group_id.as_ref(), Some(&group_id));
    for (label, device) in [("initiating", &device_a), ("sibling", &device_b)] {
        let messages = device.messages(chat_id, 1).await.unwrap();
        assert!(
            matches!(
                messages[0].message(),
                Message::Event(EventMessage::System(SystemMessage::SentGroupConnectionRequest {
                    recipient,
                    origin_chat_id,
                })) if recipient == &charlie && origin_chat_id == &group_chat_id
            ),
            "the {label} device should name the group chat of the request"
        );
    }

    assert_same_epoch_and_leaf(&device_a, &device_b, chat_id, "after the creation echo").await;

    // Charlie picks the targeted message up and accepts.
    let charlie_user = setup.user(&charlie);
    charlie_user.drain().await;
    charlie_user
        .accept_contact_request(chat_id)
        .await
        .unwrap()
        .unwrap();

    device_a.drain().await;
    device_b.drain().await;

    for (label, device) in [("initiating", &device_a), ("sibling", &device_b)] {
        assert_eq!(
            device.expect_chat(chat_id).await.chat_type(),
            &ChatType::Connection(charlie.clone()),
            "the {label} device should see a confirmed connection"
        );
        assert!(
            device.contact(&charlie).await.is_some(),
            "the {label} device should have the contact"
        );
    }

    assert_same_epoch_and_leaf(&device_a, &device_b, chat_id, "after the peer's accept").await;
    send_and_receive(
        &device_b,
        &[charlie_user, &device_a],
        chat_id,
        "hello charlie, from the sibling",
    )
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn offline_sibling_catches_up_on_a_created_group() {
    const TEXT: &str = "sent while the sibling was away";

    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    setup.connect_users(&alice, &bob).await;

    let (device_a, device_b, _tmp) = link_sibling(&setup, &alice).await;

    // The sibling stops draining its queue from here on.
    let chat_id = setup.create_non_apq_group(&alice).await;
    device_a
        .invite_users(chat_id, std::slice::from_ref(&bob))
        .await
        .unwrap();

    let bob_user = setup.user(&bob);
    bob_user.drain().await;

    // Several commits move the group past the epoch the snapshot holds.
    for _ in 0..3 {
        device_a.update_key(chat_id).await.unwrap();
        bob_user.drain().await;
    }
    bob_user.send_text(chat_id, TEXT).await;
    device_a.drain().await;

    // Coming back, the sibling joins at the creation epoch and walks its queue
    // forward from there without losing anything.
    device_b.drain().await;
    assert_same_epoch_and_leaf(&device_a, &device_b, chat_id, "after catching up").await;
    assert_eq!(
        device_b
            .text_messages(chat_id)
            .await
            .iter()
            .filter(|rendered| *rendered == TEXT)
            .count(),
        1,
        "the sibling should see the message that predates its comeback"
    );

    send_and_receive(
        &device_b,
        &[bob_user, &device_a],
        chat_id,
        "hello from the sibling that caught up",
    )
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn offline_sibling_catches_up_on_an_accepted_connection() {
    const TEXT: &str = "sent while the sibling was away";

    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;

    let (device_a, device_b, _tmp) = link_sibling(&setup, &alice).await;

    let record = setup.add_username(&alice).await;
    let hash = record.username.calculate_hash().unwrap();
    setup
        .user(&bob)
        .add_contact(record.username.clone(), hash, setup.apq_groups)
        .await
        .unwrap()
        .unwrap();

    let chat_id = receive_connection_offer(&device_a, &record).await;
    device_a
        .accept_contact_request(chat_id)
        .await
        .unwrap()
        .unwrap();

    // The sibling stays offline while the group moves on.
    let bob_user = setup.user(&bob);
    bob_user.drain().await;
    for _ in 0..3 {
        bob_user.update_key(chat_id).await.unwrap();
        device_a.drain().await;
    }
    bob_user.send_text(chat_id, TEXT).await;
    device_a.drain().await;

    device_b.drain().await;
    assert_eq!(
        device_b.expect_chat(chat_id).await.chat_type(),
        &ChatType::Connection(bob.clone())
    );
    assert_same_epoch_and_leaf(&device_a, &device_b, chat_id, "after catching up").await;
    assert_eq!(
        device_b
            .text_messages(chat_id)
            .await
            .iter()
            .filter(|rendered| *rendered == TEXT)
            .count(),
        1,
        "the sibling should see the message that predates its comeback"
    );

    send_and_receive(
        &device_b,
        &[bob_user, &device_a],
        chat_id,
        "hello bob, from the sibling that caught up",
    )
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn a_rejected_group_chat_produces_no_echo() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;

    let (device_a, device_b, _tmp) = link_sibling(&setup, &alice).await;

    let chats_before_a = device_a.ordered_chat_ids().await.unwrap();
    let chats_before_b = device_b.ordered_chat_ids().await.unwrap();

    // Creating a chat asks the DS for a group id first, so the rejection has
    // to skip that request to land on the create itself.
    setup.listener_control_handle().set_reject_request_after(1);
    let error = device_a
        .create_chat("rejected by the DS".to_owned(), None, setup.apq_groups)
        .await
        .expect_err("the DS rejection should surface as an error");
    tracing::info!(%error, "the DS rejected the group");

    assert_eq!(
        device_a.ordered_chat_ids().await.unwrap(),
        chats_before_a,
        "the acting device should have discarded the chat of a rejected group"
    );

    let processed = device_b.drain().await;
    assert!(
        processed.new_chats.is_empty() && processed.new_connections.is_empty(),
        "a rejected creation must not reach the sibling"
    );
    assert_eq!(
        device_b.ordered_chat_ids().await.unwrap(),
        chats_before_b,
        "the sibling must not have installed a group the DS rejected"
    );

    // The DS is healthy again, so a second attempt goes through and the
    // sibling picks that one up.
    let chat_id = device_a
        .create_chat("accepted by the DS".to_owned(), None, setup.apq_groups)
        .await
        .unwrap();
    device_b.drain().await;
    assert!(
        device_b.chat(&chat_id).await.is_some(),
        "the sibling should install the group of the accepted retry"
    );
    assert_same_epoch_and_leaf(&device_a, &device_b, chat_id, "after the retried creation").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn a_rejected_creation_produces_no_echo() {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;

    let (device_a, device_b, _tmp) = link_sibling(&setup, &alice).await;

    let record = setup.add_username(&bob).await;

    let chats_before_a = device_a.ordered_chat_ids().await.unwrap();
    let chats_before_b = device_b.ordered_chat_ids().await.unwrap();

    // `add_contact` asks the DS for a group id first, so the rejection has to
    // skip that request to land on the create itself.
    setup.listener_control_handle().set_reject_request_after(1);
    let error = device_a
        .add_contact(
            record.username.clone(),
            record.username.calculate_hash().unwrap(),
            setup.apq_groups,
        )
        .await
        .expect_err("the DS rejection should surface as an error");
    tracing::info!(%error, "the DS rejected the connection group");

    assert_eq!(
        device_a.ordered_chat_ids().await.unwrap(),
        chats_before_a,
        "the acting device should have discarded the chat of a rejected group"
    );
    assert!(
        device_a
            .username_contacts()
            .await
            .unwrap()
            .iter()
            .all(|contact| contact.username != record.username),
        "the acting device should have discarded the partial contact"
    );

    let processed = device_b.drain().await;
    assert!(
        processed.new_chats.is_empty() && processed.new_connections.is_empty(),
        "a rejected creation must not reach the sibling"
    );
    assert_eq!(
        device_b.ordered_chat_ids().await.unwrap(),
        chats_before_b,
        "the sibling must not have installed a group the DS rejected"
    );

    // The DS is healthy again, so a second attempt goes through and the
    // sibling picks that one up.
    let chat_id = device_a
        .add_contact(
            record.username.clone(),
            record.username.calculate_hash().unwrap(),
            setup.apq_groups,
        )
        .await
        .unwrap()
        .unwrap();
    device_b.drain().await;
    assert!(
        device_b.chat(&chat_id).await.is_some(),
        "the sibling should mirror the chat of the accepted retry"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn a_rejected_join_produces_no_echo() {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;

    let (device_a, device_b, _tmp) = link_sibling(&setup, &alice).await;

    let record = setup.add_username(&alice).await;
    let hash = record.username.calculate_hash().unwrap();
    setup
        .user(&bob)
        .add_contact(record.username.clone(), hash, setup.apq_groups)
        .await
        .unwrap()
        .unwrap();
    let chat_id = receive_connection_offer(&device_a, &record).await;

    // The accept fetches the external commit info first, so the rejection has
    // to skip that request to land on the join itself.
    setup.listener_control_handle().set_reject_request_after(1);
    let error = device_a
        .accept_contact_request(chat_id)
        .await
        .expect_err("the DS rejection should surface as an error");
    tracing::info!(%error, "the DS rejected the connection group join");

    let processed = device_b.drain().await;
    assert!(
        processed.new_chats.is_empty() && processed.new_connections.is_empty(),
        "a rejected join must not reach the sibling"
    );
    assert!(
        device_b.chat(&chat_id).await.is_none(),
        "the sibling must not have installed a group the DS rejected"
    );

    // The DS never accepted the commit, so the retry joins at the same epoch.
    device_a
        .accept_contact_request(chat_id)
        .await
        .unwrap()
        .unwrap();
    device_b.drain().await;
    assert_eq!(
        device_b
            .chat(&chat_id)
            .await
            .expect("the sibling should have installed the chat")
            .chat_type(),
        &ChatType::Connection(bob.clone())
    );
    assert_same_epoch_and_leaf(&device_a, &device_b, chat_id, "after the retried join").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn a_failed_offer_send_keeps_the_created_group() {
    let mut setup = TestBackend::single().await;
    let [alice, bob, charlie] = setup.add_users().await;

    setup.connect_users(&bob, &alice).await;
    setup.connect_users(&bob, &charlie).await;
    let group_chat_id = setup.create_group(&bob).await;
    setup
        .invite_to_group(group_chat_id, &bob, &[&alice, &charlie])
        .await;

    let (device_a, device_b, _tmp) = link_sibling(&setup, &alice).await;

    // The group id request and the create go through, the targeted message
    // carrying the connection offer does not.
    setup.listener_control_handle().set_reject_request_after(2);
    let error = device_a
        .add_contact_from_group(group_chat_id, charlie.clone(), setup.apq_groups)
        .await
        .expect_err("the rejected targeted message should surface as an error");
    tracing::info!(%error, "the connection offer could not be sent");

    let contact_a = device_a
        .try_targeted_message_contact(&charlie)
        .await
        .unwrap()
        .expect("the acting device should keep the partial contact of an accepted group");
    let chat_id = contact_a.chat_id;
    assert_eq!(
        device_a
            .chat(&chat_id)
            .await
            .expect("the acting device should keep the chat of an accepted group")
            .chat_type(),
        &ChatType::TargetedMessageConnection(charlie.clone())
    );

    // The DS accepted the creation, so the sibling has the group either way.
    device_b.drain().await;
    assert_eq!(
        device_b
            .chat(&chat_id)
            .await
            .expect("the sibling should have mirrored the chat")
            .chat_type(),
        &ChatType::TargetedMessageConnection(charlie.clone())
    );
    assert_same_epoch_and_leaf(&device_a, &device_b, chat_id, "after the creation echo").await;
}
