// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::HashSet;

use aircommon::{credentials::LeafCredential, identifiers::UserId};
use aircoreclient::{
    ChatId, ChatStatus, ChatType, Message, ReadReceiptsSetting, UserProfile,
    clients::{
        CoreUser,
        multi_device::{MultiDeviceLinkClientError, MultiDeviceProvisionStep},
    },
};
use airprotos::relay_service::v1::LinkingSessionId;
use airserver_test_harness::utils::setup::TestBackend;
use chrono::{DateTime, Utc};
use mimi_content::MimiContent;
use tempfile::TempDir;
use uuid::Uuid;

/// Sends `text` from `sender` into the self-group chat and asserts that
/// `receiver` sees it after fetching + processing its queue.
async fn send_and_receive(sender: &CoreUser, devices: &[&CoreUser], chat_id: ChatId, text: &str) {
    // Drain the sender's own queue so it is at the latest epoch.
    let pending = sender.qs_fetch_messages().await.unwrap();
    sender.fully_process_qs_messages(pending).await;

    let content = MimiContent::simple_markdown_message(text.to_owned(), [7u8; 16]);
    sender
        .send_message(chat_id, content.clone(), None)
        .await
        .unwrap();
    sender.outbound_service().run_once().await;

    // check the echoed message on the linked client
    for device in devices {
        let qs_messages = device.qs_fetch_messages().await.unwrap();
        let processed = device.fully_process_qs_messages(qs_messages).await;
        assert!(
            processed.errors.is_empty(),
            "some incoming messages failed to be processed, check logs!"
        );
        let received = processed
            .new_messages
            .last()
            .unwrap_or_else(|| panic!("receiver did not get the message {text:?}"));
        let Message::Content(received_content) = received.message() else {
            panic!("expected a content message, got {:?}", received.message());
        };
        assert_eq!(
            received_content.content(),
            &content,
            "message should round-trip {}",
            device.own_user_profile().await.unwrap().display_name,
        );
    }
}

/// Reads the self-group leaf credentials of `device`, asserts every leaf carries
/// a `SelfGroupCredential`, and returns their client ids in member order.
async fn self_group_client_ids(device: &CoreUser) -> Vec<Uuid> {
    let credentials = device
        .self_group()
        .await
        .unwrap()
        .expect("device should have a self group")
        .credentials()
        .unwrap();
    assert_eq!(
        credentials.len(),
        2,
        "the self group should have two leaf credentials"
    );
    credentials
        .iter()
        .map(|credential| match credential {
            LeafCredential::SelfGroup(self_group) => self_group.client_id(),
            LeafCredential::User(_) => panic!("self-group leaf must carry a SelfGroupCredential"),
        })
        .collect()
}

/// A confirmation receiver that is already fulfilled, so the acceptor proceeds
/// without waiting for user confirmation in tests.
///
/// Carries an empty device name, which leaves the new device's own default in
/// place. Use [`confirm_with_name`] to exercise a user-chosen name.
fn auto_confirm() -> tokio::sync::oneshot::Receiver<String> {
    confirm_with_name("")
}

/// Like [`auto_confirm`], but names the new device the way the confirming user
/// would.
fn confirm_with_name(name: &str) -> tokio::sync::oneshot::Receiver<String> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    tx.send(name.to_owned()).unwrap();
    rx
}

/// A connected-signal sender whose receiver is dropped; the acceptor ignores the
/// send failure, so tests don't need to observe the "connected" signal.
fn ignore_connected() -> tokio::sync::oneshot::Sender<()> {
    tokio::sync::oneshot::channel().0
}

/// Receives the session ID from the first provisioning step. The receiver must
/// stay alive afterwards: the new device later sends a `Linking` step, and
/// dropping the receiver would make that send fail and abort provisioning.
async fn recv_session_id(
    rx: &mut tokio::sync::mpsc::Receiver<MultiDeviceProvisionStep>,
) -> LinkingSessionId {
    match rx
        .recv()
        .await
        .expect("provision channel closed before session id")
    {
        MultiDeviceProvisionStep::SessionId(session_id) => session_id,
        MultiDeviceProvisionStep::Linking => panic!("unexpected Linking step before session id"),
    }
}

/// Provisions a fresh device and links it to `user_id`'s existing device,
/// returning the new device. The [`TempDir`] holds the new device's database
/// and must stay alive as long as the device is used.
pub(crate) async fn link_new_device(setup: &TestBackend, user_id: &UserId) -> (CoreUser, TempDir) {
    link_new_device_named(setup, user_id, "").await
}

/// Like [`link_new_device`], but names the new device the way the confirming
/// user would. An empty `name` leaves the new device's own default in place.
async fn link_new_device_named(
    setup: &TestBackend,
    user_id: &UserId,
    name: &str,
) -> (CoreUser, TempDir) {
    let domain = setup.domain().clone();
    let server_url = setup.server_url();

    let (session_tx, mut session_rx) = tokio::sync::mpsc::channel(1);

    let new_device_task = tokio::spawn(async move {
        let tmp = TempDir::new().unwrap();
        let db_path = tmp.path().to_str().unwrap();
        let new_device =
            CoreUser::multi_device_provision_client(db_path, domain, Some(server_url), session_tx)
                .await
                .unwrap();
        (new_device, tmp)
    });

    let session_id = recv_session_id(&mut session_rx).await;

    setup
        .get_user(user_id)
        .user()
        .multi_device_link_client(session_id, ignore_connected(), confirm_with_name(name))
        .await
        .unwrap()
        .unwrap();

    new_device_task.await.unwrap()
}

/// Fetches and processes all messages in the device's queue.
pub(crate) async fn drain_queue(user: &CoreUser) {
    let messages = user.qs_fetch_messages().await.unwrap();
    user.fully_process_qs_messages(messages).await;
}

/// How many of the chat's stored content messages render as `text`.
pub(crate) async fn count_messages_with_text(
    user: &CoreUser,
    chat_id: ChatId,
    text: &str,
) -> usize {
    user.messages(chat_id, 100)
        .await
        .unwrap()
        .iter()
        .filter(|message| {
            message
                .message()
                .mimi_content()
                .is_some_and(|content| content.string_rendering().is_ok_and(|s| s.contains(text)))
        })
        .count()
}

/// The device's locally stored read-receipts setting. `None` means unset.
async fn read_receipts(user: &CoreUser) -> Option<bool> {
    user.user_setting::<ReadReceiptsSetting>()
        .await
        .map(|ReadReceiptsSetting(value)| value)
}

/// The chat ID of the device's self-group chat.
async fn self_chat_id(user: &CoreUser) -> ChatId {
    let self_group = user
        .self_group()
        .await
        .unwrap()
        .expect("device should have a self group");
    ChatId::try_from(self_group.group_id()).unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[tracing::instrument(name = "Test multi-device linking session", skip_all)]
async fn multi_device_linking_session() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let bob = setup.add_user().await;
    setup.connect_users(&alice, &bob).await;
    let charlie = setup.add_user().await;
    setup.connect_users(&alice, &charlie).await;

    // Alice already has two groups before the new device is linked. The
    // linked device should inherit them, not just the self-group.
    let chat_id_1 = setup.create_group(&alice).await;
    setup.invite_to_group(chat_id_1, &alice, vec![&bob]).await;

    let chat_id_2 = setup.create_group(&alice).await;
    setup.invite_to_group(chat_id_2, &alice, vec![&bob]).await;

    let (new_device, _tmp) = link_new_device(&setup, &alice).await;

    let old_device = setup.get_user(&alice).user();
    assert_eq!(
        new_device.qs_user_id(),
        old_device.qs_user_id(),
        "linked device must share the virtual client (QsUserId)"
    );
    assert_ne!(
        new_device.qs_client_id(),
        old_device.qs_client_id(),
        "linked device must have its own queue (QsClientId)"
    );
    let old_device_self_group = old_device
        .self_group()
        .await
        .unwrap()
        .expect("old device should have a self group");
    let new_device_self_group = new_device
        .self_group()
        .await
        .unwrap()
        .expect("new device should have a self group");
    assert_eq!(
        old_device_self_group.group_id(),
        new_device_self_group.group_id(),
        "linked device must know the shared self group"
    );

    assert_eq!(
        old_device.self_group_member_count().await.unwrap(),
        Some(2),
        "old device should see both emulator clients in the self group"
    );
    assert_eq!(
        new_device.self_group_member_count().await.unwrap(),
        Some(2),
        "new device should see both emulator clients in the self group"
    );

    // The self group carries per-device SelfGroupCredentials, not user
    // credentials. Both devices must see the same pair of distinct client ids,
    // and that pair must match the client id each device stored locally.
    let old_device_ids = self_group_client_ids(old_device).await;
    let new_device_ids = self_group_client_ids(&new_device).await;
    assert_ne!(
        old_device_ids[0], old_device_ids[1],
        "the two self-group leaves must have distinct client ids"
    );
    let old_set: HashSet<Uuid> = old_device_ids.into_iter().collect();
    let new_set: HashSet<Uuid> = new_device_ids.into_iter().collect();
    assert_eq!(
        old_set, new_set,
        "both devices must see the same set of self-group client ids"
    );
    let expected: HashSet<Uuid> = [
        old_device.own_client_id().await.unwrap(),
        new_device.own_client_id().await.unwrap(),
    ]
    .into_iter()
    .collect();
    assert_eq!(
        old_set, expected,
        "self-group client ids must match each device's own client id"
    );

    assert_eq!(
        old_device.self_chat_title().await.unwrap().as_deref(),
        Some("Notes to self"),
        "old device should have a Notes to self chat"
    );
    assert_eq!(
        new_device.self_chat_title().await.unwrap().as_deref(),
        Some("Notes to self"),
        "new device should have a Notes to self chat"
    );

    // Onboarding into the pre-existing groups is queued and processed in the background.
    new_device.outbound_service().run_once().await;

    // The new device must know about all groups from the original client.
    let new_device_chat_ids = new_device.ordered_chat_ids().await.unwrap();
    for (label, chat_id) in [("1", chat_id_1), ("2", chat_id_2)] {
        assert!(
            new_device_chat_ids.contains(&chat_id),
            "linked device should have inherited pre-existing group {label}"
        );
        assert!(
            !new_device.is_resync_pending(chat_id).await.unwrap(),
            "onboarding into group {label} should have completed, not still be queued"
        );

        let members = new_device.mls_chat_participants(chat_id).await;
        assert!(
            members
                .as_ref()
                .is_some_and(|members| members.contains(&alice)),
            "chat {label} on the linked device should be bound to a joined MLS group, got {members:?}"
        );
        assert_eq!(
            members,
            old_device.mls_chat_participants(chat_id).await,
            "linked device should see the same members as the old device in group {label}"
        );
    }

    // Messages sent into the self group are seen by the other device.
    let self_chat_id = ChatId::try_from(old_device_self_group.group_id()).unwrap();
    send_and_receive(
        old_device,
        &[&new_device],
        self_chat_id,
        "hello from the old device in self-group",
    )
    .await;
    send_and_receive(
        &new_device,
        &[old_device],
        self_chat_id,
        "hello back from the new device in self-group",
    )
    .await;

    // The old device has to follow the onboarding external commit onto the virtual client's new leaf.
    let pending = old_device.qs_fetch_messages().await.unwrap();
    old_device.fully_process_qs_messages(pending).await;
    assert_eq!(
        old_device
            .group_epoch_and_own_index(chat_id_1)
            .await
            .unwrap(),
        new_device
            .group_epoch_and_own_index(chat_id_1)
            .await
            .unwrap(),
        "both emulator clients must land on the same epoch and shared leaf"
    );

    // Messages sent into one of the existing groups are seen by both clients.
    for chat_id in [chat_id_1, chat_id_2] {
        send_and_receive(
            old_device,
            &[&new_device],
            chat_id,
            "hello from the old device",
        )
        .await;
        send_and_receive(
            &new_device,
            &[old_device],
            chat_id,
            "hello back from the new device",
        )
        .await;
    }

    // Messages received from a 3rd party are seen by both clients.
    let bob_client = setup.get_user(&bob).user();
    send_and_receive(
        bob_client,
        &[&new_device, old_device],
        chat_id_1,
        "hello from the old device",
    )
    .await;

    // A commit that replaces the shared leaf for some other reason
    // has to derive the new leaf from the emulation epoch too,
    // or the sibling emulator client will break.
    setup
        .invite_to_group(chat_id_1, &alice, vec![&charlie])
        .await;

    let pending = new_device.qs_fetch_messages().await.unwrap();
    let processed = new_device.fully_process_qs_messages(pending).await;
    assert!(
        processed.errors.is_empty(),
        "linked device failed to follow the add commit: {:?}",
        processed.errors
    );
    assert_eq!(
        new_device
            .group_epoch_and_own_index(chat_id_1)
            .await
            .unwrap(),
        setup
            .get_user(&alice)
            .user()
            .group_epoch_and_own_index(chat_id_1)
            .await
            .unwrap(),
        "both emulator clients must stay on the same epoch and shared leaf after an invite+add"
    );

    let bob_client = setup.get_user(&bob).user();
    send_and_receive(
        bob_client,
        &[&new_device],
        chat_id_1,
        "after a member was added",
    )
    .await;

    // Rotating the user profile updates the profile key on the self group. That
    // DS request envelope is now signed with the per-device self-group key, so
    // this confirms the flipped self-group credential still authenticates
    // self-group operations.
    let new_profile = UserProfile {
        user_id: alice.clone(),
        display_name: "New Alice".parse().unwrap(),
        profile_picture: None,
    };
    setup
        .get_user(&alice)
        .user()
        .set_own_user_profile(new_profile)
        .await
        .unwrap();

    // Same for a removal.
    setup
        .remove_from_group(chat_id_1, &alice, vec![&charlie])
        .await
        .unwrap();

    let pending = new_device.qs_fetch_messages().await.unwrap();
    let processed = new_device.fully_process_qs_messages(pending).await;
    assert!(
        processed.errors.is_empty(),
        "linked device failed to follow the remove commit: {:?}",
        processed.errors
    );
    assert_eq!(
        new_device
            .group_epoch_and_own_index(chat_id_1)
            .await
            .unwrap(),
        setup
            .get_user(&alice)
            .user()
            .group_epoch_and_own_index(chat_id_1)
            .await
            .unwrap(),
        "both emulator clients must stay on the same epoch and shared leaf after a removal"
    );

    let bob_client = setup.get_user(&bob).user();
    send_and_receive(
        bob_client,
        &[&new_device],
        chat_id_1,
        "after a member was removed",
    )
    .await;

    // And for a deletion, which replaces the shared leaf as well. Without
    // following it the linked device never learns the group is gone.
    setup.delete_group(chat_id_1, &alice).await;

    let pending = new_device.qs_fetch_messages().await.unwrap();
    let processed = new_device.fully_process_qs_messages(pending).await;
    assert!(
        processed.errors.is_empty(),
        "linked device failed to follow the delete commit: {:?}",
        processed.errors
    );
    let deleted_chat = new_device
        .chat(&chat_id_1)
        .await
        .expect("linked device should still know the deleted chat");
    assert!(
        matches!(deleted_chat.status(), ChatStatus::Inactive(_)),
        "linked device should see the deleted group as inactive, got {:?}",
        deleted_chat.status()
    );
}

// Linking with a session ID that was never registered returns an error.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[tracing::instrument(name = "Test link with nonexistent session ID", skip_all)]
async fn multi_device_link_with_nonexistent_session_id() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;

    let fake_digest =
        hex::decode("68924f1f6f60d5fdb8463881a5945e58c3f1402c65681b1270f5aeccbed17bd1")
            .unwrap()
            .try_into()
            .unwrap();
    let fake_session_id = LinkingSessionId::from_digest(&fake_digest, 8).unwrap();
    let result = setup
        .get_user(&alice)
        .user()
        .multi_device_link_client(fake_session_id, ignore_connected(), auto_confirm())
        .await;

    assert!(matches!(
        result,
        Ok(Err(MultiDeviceLinkClientError::SessionNotFound))
    ));
}

// A session can only be claimed once; a second link attempt on the same session ID
// must fail even when called by the same user.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[tracing::instrument(name = "Test second link attempt returns error", skip_all)]
async fn multi_device_second_link_attempt_returns_error() {
    let mut setup = TestBackend::single().await;
    let domain = setup.domain().clone();
    let server_url = setup.server_url();
    let alice = setup.add_user().await;

    let (session_tx, mut session_rx) = tokio::sync::mpsc::channel(1);

    let new_device_task = tokio::spawn(async move {
        let tmp = TempDir::new().unwrap();
        let db_path = tmp.path().to_str().unwrap();
        let new_device =
            CoreUser::multi_device_provision_client(db_path, domain, Some(server_url), session_tx)
                .await
                .unwrap();
        (new_device, tmp)
    });

    let session_id = recv_session_id(&mut session_rx).await;

    setup
        .get_user(&alice)
        .user()
        .multi_device_link_client(session_id.clone(), ignore_connected(), auto_confirm())
        .await
        .unwrap()
        .unwrap();

    new_device_task.await.unwrap();

    // Session was already consumed — a second attempt must fail.
    let second_result = setup
        .get_user(&alice)
        .user()
        .multi_device_link_client(session_id, ignore_connected(), auto_confirm())
        .await;

    assert!(matches!(
        second_result,
        Ok(Err(MultiDeviceLinkClientError::SessionNotFound))
    ));
}

// Two concurrent linking sessions must not interfere with each other.
// Each new device must be linked to the correct existing device.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[tracing::instrument(name = "Test concurrent linking sessions don't interfere", skip_all)]
async fn multi_device_concurrent_linking_sessions_dont_interfere() {
    let mut setup = TestBackend::single().await;
    let domain = setup.domain().clone();
    let server_url = setup.server_url();
    let alice = setup.add_user().await;
    let bob = setup.add_user().await;

    let (alice_session_tx, mut alice_session_rx) = tokio::sync::mpsc::channel(1);
    let (bob_session_tx, mut bob_session_rx) = tokio::sync::mpsc::channel(1);

    let alice_domain = domain.clone();
    let alice_server_url = server_url.clone();
    let alice_new_device = tokio::spawn(async move {
        let tmp = TempDir::new().unwrap();
        let db_path = tmp.path().to_str().unwrap();
        let new_device = CoreUser::multi_device_provision_client(
            db_path,
            alice_domain,
            Some(alice_server_url),
            alice_session_tx,
        )
        .await
        .unwrap();
        (new_device, tmp)
    });

    let bob_domain = domain.clone();
    let bob_server_url = server_url.clone();
    let bob_new_device = tokio::spawn(async move {
        let tmp = TempDir::new().unwrap();
        let db_path = tmp.path().to_str().unwrap();
        let new_device = CoreUser::multi_device_provision_client(
            db_path,
            bob_domain,
            Some(bob_server_url),
            bob_session_tx,
        )
        .await
        .unwrap();
        (new_device, tmp)
    });

    let alice_session_id = recv_session_id(&mut alice_session_rx).await;
    let bob_session_id = recv_session_id(&mut bob_session_rx).await;

    // Session IDs derived from different key packages must be distinct.
    assert_ne!(alice_session_id, bob_session_id);

    setup
        .get_user(&alice)
        .user()
        .multi_device_link_client(alice_session_id, ignore_connected(), auto_confirm())
        .await
        .unwrap()
        .unwrap();

    setup
        .get_user(&bob)
        .user()
        .multi_device_link_client(bob_session_id, ignore_connected(), auto_confirm())
        .await
        .unwrap()
        .unwrap();

    // Each new device must be linked to the correct existing virtual client.
    let (alice_device, _a_tmp) = alice_new_device.await.unwrap();
    let (bob_device, _b_tmp) = bob_new_device.await.unwrap();
    assert_eq!(
        alice_device.qs_user_id(),
        setup.get_user(&alice).user().qs_user_id()
    );
    assert_eq!(
        bob_device.qs_user_id(),
        setup.get_user(&bob).user().qs_user_id()
    );
    assert_ne!(alice_device.qs_user_id(), bob_device.qs_user_id());
}

// A settings change on one device reaches the linked device through a
// self-group commit, in both directions. This is also the proof that the DS
// passes commits carrying AppEphemeral proposals through.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[tracing::instrument(name = "Test settings sync across linked devices", skip_all)]
async fn multi_device_settings_sync() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let (new_device, _tmp) = link_new_device(&setup, &alice).await;
    let old_device = setup.get_user(&alice).user();

    // Bring both devices to the self-group's latest epoch.
    drain_queue(old_device).await;
    drain_queue(&new_device).await;

    // Initial key package upload
    new_device.outbound_service().run_once().await;
    drain_queue(&new_device).await;
    drain_queue(old_device).await;

    // Nothing was set before linking, so the linking payload carried an empty
    // snapshot and the new device starts unset.
    assert_eq!(read_receipts(&new_device).await, None);

    // Two quick toggles: both are applied locally right away and fold into
    // one pending change. Before the outbound service runs, no commit exists.
    let chat_id = self_chat_id(old_device).await;
    old_device
        .set_synced_user_setting(&ReadReceiptsSetting(true))
        .await
        .unwrap();
    old_device
        .set_synced_user_setting(&ReadReceiptsSetting(false))
        .await
        .unwrap();
    assert_eq!(read_receipts(old_device).await, Some(false));
    assert!(old_device.has_pending_setting_changes().await.unwrap());
    assert!(
        old_device
            .pending_chat_operation_info(chat_id)
            .await
            .unwrap()
            .is_none(),
        "the commit is only staged when the outbound service runs"
    );

    // The outbound service stages a commit from the current state and sends
    // it. Success completes the pending change.
    old_device.outbound_service().run_once().await;
    assert!(!old_device.has_pending_setting_changes().await.unwrap());
    assert!(
        old_device
            .pending_chat_operation_info(chat_id)
            .await
            .unwrap()
            .is_none(),
        "the operation should be gone after a successful send"
    );

    // The linked device applies the folded update when processing its queue.
    drain_queue(&new_device).await;
    assert_eq!(read_receipts(&new_device).await, Some(false));

    // No feedback loop: applying a sibling's update records no pending change
    // and issues no commit on the receiving device.
    assert!(!new_device.has_pending_setting_changes().await.unwrap());
    new_device.outbound_service().run_once().await;
    assert!(
        new_device
            .pending_chat_operation_info(chat_id)
            .await
            .unwrap()
            .is_none(),
        "applying an incoming update must not issue a commit"
    );

    // And back in the other direction, from the new device.
    new_device
        .set_synced_user_setting(&ReadReceiptsSetting(true))
        .await
        .unwrap();
    new_device.outbound_service().run_once().await;
    drain_queue(old_device).await;
    assert_eq!(read_receipts(old_device).await, Some(true));
}

// A setting stored before any linking travels to the new device in the
// provisioning package and is applied during bootstrap.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[tracing::instrument(name = "Test settings travel in the linking payload", skip_all)]
async fn multi_device_settings_in_linking_payload() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;

    // No self-group exists yet, so the setting is only stored locally.
    let old_device = setup.get_user(&alice).user();
    old_device
        .set_synced_user_setting(&ReadReceiptsSetting(false))
        .await
        .unwrap();
    assert_eq!(read_receipts(old_device).await, Some(false));

    let (new_device, _tmp) = link_new_device(&setup, &alice).await;

    // The new device starts from the snapshot in the provisioning package,
    // without processing any queue messages.
    assert_eq!(read_receipts(&new_device).await, Some(false));
}

// The new device's metadata entry rides on the self-group add commit itself, so
// linking costs exactly one commit.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[tracing::instrument(
    name = "Test linking publishes the device entry in one commit",
    skip_all
)]
async fn multi_device_link_publishes_device_entry_without_extra_commit() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let old_device = setup.get_user(&alice).user();

    let old_id = old_device.own_client_id().await.unwrap();
    let (new_device, _tmp) = link_new_device(&setup, &alice).await;
    let new_id = new_device.own_client_id().await.unwrap();

    // No outbound run and no queue drain: both sides already hold both entries.
    // The old device folded the new entry in while staging the add, and the new
    // device wrote its own entry before handing a copy over.
    for (label, device) in [("old", old_device), ("new", &new_device)] {
        let ids: Vec<_> = device
            .linked_devices()
            .await
            .unwrap()
            .into_iter()
            .map(|device| device.client_id)
            .collect();
        assert!(
            ids.contains(&old_id) && ids.contains(&new_id),
            "the {label} device should know both entries right after linking, got {ids:?}"
        );
    }

    // Neither side owes a settings commit, so the self group stays at the epoch
    // the add left it on.
    for (label, device) in [("old", old_device), ("new", &new_device)] {
        assert!(
            !device.has_pending_setting_changes().await.unwrap(),
            "the {label} device must not have queued a settings commit for the entry"
        );
    }
}

// Both devices change the setting before processing each other's commit. The
// DS accepts the first commit and rejects the second with a wrong epoch. The
// loser gives the setting up when it processes the winner's commit, so both
// devices converge on the DS-order winner.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[tracing::instrument(name = "Test settings race converges on the DS order", skip_all)]
async fn multi_device_settings_race_converges() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let (new_device, _tmp) = link_new_device(&setup, &alice).await;
    let old_device = setup.get_user(&alice).user();

    drain_queue(old_device).await;
    drain_queue(&new_device).await;

    // Both devices toggle while at the same epoch. The outbound runs stage
    // conflicting commits against that epoch.
    old_device
        .set_synced_user_setting(&ReadReceiptsSetting(true))
        .await
        .unwrap();
    new_device
        .set_synced_user_setting(&ReadReceiptsSetting(false))
        .await
        .unwrap();

    // The old device reaches the DS first and wins.
    old_device.outbound_service().run_once().await;

    // The new device's commit is rejected with a wrong epoch. The operation
    // parks until the winning commit arrives through the queue, the pending
    // change stays, and the optimistic value stays in place for now.
    new_device.outbound_service().run_once().await;
    let chat_id = self_chat_id(&new_device).await;
    let parked = new_device
        .pending_chat_operation_info(chat_id)
        .await
        .unwrap()
        .expect("losing operation should be parked");
    assert_eq!(parked.request_status, "waiting_for_queue_response");
    assert!(new_device.has_pending_setting_changes().await.unwrap());
    assert_eq!(read_receipts(&new_device).await, Some(false));

    // Processing the winner's commit applies its snapshot, deletes the parked
    // operation, and drops the covered field from the pending changes, so
    // nothing is re-issued.
    drain_queue(&new_device).await;
    assert_eq!(read_receipts(&new_device).await, Some(true));
    assert!(
        new_device
            .pending_chat_operation_info(chat_id)
            .await
            .unwrap()
            .is_none(),
        "losing operation should be deleted"
    );
    assert!(
        !new_device.has_pending_setting_changes().await.unwrap(),
        "covered pending change should be dropped"
    );

    // Nothing left to send: another outbound run stages no new commit.
    new_device.outbound_service().run_once().await;
    assert!(
        new_device
            .pending_chat_operation_info(chat_id)
            .await
            .unwrap()
            .is_none()
    );

    // The winner's own state is unaffected by its echo.
    drain_queue(old_device).await;
    assert_eq!(read_receipts(old_device).await, Some(true));

    // The self group is still usable after the race.
    send_and_receive(old_device, &[&new_device], chat_id, "still in sync").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[tracing::instrument(name = "Test linking a third device", skip_all)]
async fn multi_device_linking_a_third_device() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let bob = setup.add_user().await;
    let connection_chat_id = setup.connect_users(&alice, &bob).await;

    // A higher-level group Alice is already a member of. Every device linked
    // from here on onboards into it with a resync, and unlike the self group its
    // commits are broadcast to all of Alice's emulator queues. The connection
    // group with bob is a second such group.
    let group_chat_id = setup.create_group(&alice).await;
    setup
        .invite_to_group(group_chat_id, &alice, vec![&bob])
        .await;

    let (device_2, _tmp_2) = link_new_device(&setup, &alice).await;
    // Device 2's onboarding replaces Alice's own, not-yet-virtual leaf, and
    // turns it into a virtual-client leaf.
    device_2.outbound_service().run_once().await;

    let (device_3, _tmp_3) = link_new_device(&setup, &alice).await;

    let device_1 = setup.get_user(&alice).user();

    // Empty every queue, so what arrives next is exactly the fan-out of device
    // 3's onboarding commit: the resync is only enqueued at this point and runs
    // when device 3's outbound service is driven below.
    drain_queue(device_1).await;
    drain_queue(&device_2).await;
    drain_queue(&device_3).await;

    for (label, device) in [("1", device_1), ("2", &device_2), ("3", &device_3)] {
        assert_eq!(
            device.self_group_member_count().await.unwrap(),
            Some(3),
            "device {label} should see all three emulator clients in the self group"
        );
    }

    // Device 3 onboards into both higher-level groups. The leaf each onboarding
    // replaces is already a virtual-client leaf, so the sibling queue is covered
    // by the regular destination list; neither commit must be fanned out twice.
    // The same outbound run also stages device 3's key packages via a self-group
    // commit, so the siblings see exactly three commits, each delivered once.
    device_3.outbound_service().run_once().await;
    for (label, device) in [("1", device_1), ("2", &device_2)] {
        let queued = device.qs_fetch_messages().await.unwrap();
        assert_eq!(
            queued.len(),
            3,
            "device {label} should receive device 3's key-package upload and its two \
             onboarding commits exactly once each"
        );
        let processed = device.fully_process_qs_messages(queued).await;
        assert!(
            processed.errors.is_empty(),
            "device {label} failed to process device 3's commits: {:?}",
            processed.errors
        );
    }

    for (chat_label, onboarded_chat_id) in [
        ("higher-level group", group_chat_id),
        ("connection group", connection_chat_id),
    ] {
        assert!(
            !device_3.is_resync_pending(onboarded_chat_id).await.unwrap(),
            "device 3 should have completed onboarding into the {chat_label}"
        );
        let epoch_and_index = device_1
            .group_epoch_and_own_index(onboarded_chat_id)
            .await
            .unwrap();
        for (label, device) in [("2", &device_2), ("3", &device_3)] {
            assert_eq!(
                device
                    .group_epoch_and_own_index(onboarded_chat_id)
                    .await
                    .unwrap(),
                epoch_and_index,
                "device {label} should share the virtual client's leaf and epoch \
                 in the {chat_label}"
            );
        }
    }

    let chat_id = self_chat_id(device_1).await;
    send_and_receive(device_1, &[&device_2, &device_3], chat_id, "from device 1").await;
    send_and_receive(&device_3, &[device_1, &device_2], chat_id, "from device 3").await;

    // The higher-level group is usable from the newest device.
    send_and_receive(
        &device_3,
        &[device_1, &device_2],
        group_chat_id,
        "from device 3 in the group",
    )
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[tracing::instrument(name = "Test onboarding after the self group advanced", skip_all)]
async fn multi_device_onboarding_after_self_group_advanced() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let bob = setup.add_user().await;
    setup.connect_users(&alice, &bob).await;

    let group_chat_id = setup.create_group(&alice).await;
    setup
        .invite_to_group(group_chat_id, &alice, vec![&bob])
        .await;

    let (device_2, _tmp_2) = link_new_device(&setup, &alice).await;
    device_2.outbound_service().run_once().await;

    // Device 3's onboarding into the group is queued here, but not sent yet.
    let (device_3, _tmp_3) = link_new_device(&setup, &alice).await;

    // Linking a fourth device advances the self group, so the emulation epoch
    // current when device 3 was linked is now stale. Device 4 registers
    // emulation epochs only from its own join onwards, so it cannot follow a
    // commit derived from that older epoch.
    let (device_4, _tmp_4) = link_new_device(&setup, &alice).await;

    let device_1 = setup.get_user(&alice).user();
    for device in [device_1, &device_2, &device_3, &device_4] {
        drain_queue(device).await;
    }

    // Device 4 onboards first, so device 3's commit has to be followed by a
    // device that was not in the self group at device 3's linking epoch.
    device_4.outbound_service().run_once().await;
    for device in [device_1, &device_2, &device_3] {
        drain_queue(device).await;
    }

    device_3.outbound_service().run_once().await;
    assert!(
        !device_3.is_resync_pending(group_chat_id).await.unwrap(),
        "device 3 should have completed onboarding into the higher-level group"
    );

    let queued = device_4.qs_fetch_messages().await.unwrap();
    let processed = device_4.fully_process_qs_messages(queued).await;
    assert!(
        processed.errors.is_empty(),
        "device 4 should be able to follow device 3's onboarding commit"
    );
    assert_eq!(
        device_4
            .group_epoch_and_own_index(group_chat_id)
            .await
            .unwrap(),
        device_3
            .group_epoch_and_own_index(group_chat_id)
            .await
            .unwrap(),
        "device 4 should stay on the shared leaf after device 3 onboards"
    );

    send_and_receive(
        &device_3,
        &[device_1, &device_2, &device_4],
        group_chat_id,
        "from device 3 after a late onboarding",
    )
    .await;
}

// Linking a third device fans the add commit out to the existing non-initiating
// device, which must process the self-group add without desynchronizing.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[tracing::instrument(name = "Test third device link keeps siblings in sync", skip_all)]
async fn multi_device_third_device_link() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;

    let (second_device, _tmp2) = link_new_device(&setup, &alice).await;
    // Bring the second device fully up to date before the third link.
    drain_queue(&second_device).await;

    let (third_device, _tmp3) = link_new_device(&setup, &alice).await;

    // The second device processes the add commit for the third device.
    let messages = second_device.qs_fetch_messages().await.unwrap();
    let processed = second_device.fully_process_qs_messages(messages).await;
    assert!(
        processed.errors.is_empty(),
        "second device failed to process the third-device add: {:?}",
        processed.errors
    );

    // All three devices agree on the membership.
    let old_device = setup.get_user(&alice).user();
    drain_queue(old_device).await;
    for (device, name) in [
        (old_device, "old device"),
        (&second_device, "second device"),
        (&third_device, "third device"),
    ] {
        assert_eq!(
            device.self_group_member_count().await.unwrap(),
            Some(3),
            "{name} should see all three emulator clients in the self group"
        );
    }

    // The second device is still in sync: a message from the old device
    // round-trips.
    let chat_id = self_chat_id(&second_device).await;
    send_and_receive(old_device, &[&second_device], chat_id, "three devices").await;
}

/// A periodic self-update in the self group is accepted by the DS and
/// processed by the sibling device.
///
/// Self-group leaves carry a self-group credential and are signed with the
/// per-device self-group key, so both the joint APQ self-update and the pure
/// T self-update must preserve the self-group leaf capabilities and use that
/// key. A leaf built with the regular user credential defaults would be
/// rejected by the DS and by the sibling (RFC 9420 valn0104).
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[tracing::instrument(name = "Test self-update in the self group", skip_all)]
async fn multi_device_self_update_in_self_group() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let (new_device, _tmp) = link_new_device(&setup, &alice).await;
    let old_device = setup.get_user(&alice).user();

    // Bring both devices to the self-group's latest epoch.
    drain_queue(old_device).await;
    drain_queue(&new_device).await;

    let chat_id = self_chat_id(old_device).await;

    // Force the joint APQ self-update (both T and PQ due).
    old_device
        .set_self_updated_at(chat_id, DateTime::UNIX_EPOCH)
        .await
        .unwrap();
    old_device
        .set_pq_self_updated_at(chat_id, DateTime::UNIX_EPOCH)
        .await
        .unwrap();
    let before_apq = Utc::now();
    old_device
        .outbound_service()
        .schedule_self_update(DateTime::UNIX_EPOCH)
        .await
        .unwrap();
    old_device.outbound_service().run_once().await;

    let after_t = old_device.self_updated_at(chat_id).await.unwrap().unwrap();
    let after_pq = old_device
        .pq_self_updated_at(chat_id)
        .await
        .unwrap()
        .unwrap();
    assert!(
        before_apq < after_t && before_apq < after_pq,
        "the DS should have accepted the APQ self-update in the self group"
    );

    // The sibling device processes the update commit.
    let messages = new_device.qs_fetch_messages().await.unwrap();
    let processed = new_device.fully_process_qs_messages(messages).await;
    assert!(
        processed.errors.is_empty(),
        "sibling failed to process the APQ self-update: {:?}",
        processed.errors
    );

    // Force a pure T self-update (only T due).
    drain_queue(old_device).await;
    old_device
        .set_self_updated_at(chat_id, DateTime::UNIX_EPOCH)
        .await
        .unwrap();
    let before_t = Utc::now();
    old_device
        .outbound_service()
        .schedule_self_update(DateTime::UNIX_EPOCH)
        .await
        .unwrap();
    old_device.outbound_service().run_once().await;

    let after_t = old_device.self_updated_at(chat_id).await.unwrap().unwrap();
    assert!(
        before_t < after_t,
        "the DS should have accepted the T self-update in the self group"
    );

    let messages = new_device.qs_fetch_messages().await.unwrap();
    let processed = new_device.fully_process_qs_messages(messages).await;
    assert!(
        processed.errors.is_empty(),
        "sibling failed to process the T self-update: {:?}",
        processed.errors
    );

    // Both devices are still in sync afterwards.
    send_and_receive(old_device, &[&new_device], chat_id, "after self-update").await;
    send_and_receive(&new_device, &[old_device], chat_id, "and back").await;
}

/// After linking, each device advertises itself and learns its sibling, so both
/// see the same two-device list. This is the property the Devices screen shows.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn multi_device_linked_devices_converge_on_both_devices() -> anyhow::Result<()> {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let (new_device, _tmp) = link_new_device(&setup, &alice).await;
    let old_device = setup.get_user(&alice).user();

    drain_queue(old_device).await;
    drain_queue(&new_device).await;

    let a_id = old_device.own_client_id().await?;
    let b_id = new_device.own_client_id().await?;
    assert_ne!(a_id, b_id, "each device mints its own client id");

    // Neither entry needs a round trip of its own. The old device's entry
    // predates the self group, so it never became a pending change and rode
    // along in the provisioning snapshot. The new device's entry travelled the
    // other way in the join request, and the old device folded it into the add
    // commit itself, which is why nothing is left enqueued on either side (see
    // `multi_device_link_publishes_device_entry_without_extra_commit`).
    for user in [old_device, &new_device] {
        let members = user.self_group_client_ids().await?;
        assert_eq!(members.len(), 2);
        assert!(members.contains(&a_id) && members.contains(&b_id));

        let devices = user.linked_devices().await?;
        let ids: Vec<_> = devices.iter().map(|d| d.client_id).collect();
        assert!(
            ids.contains(&a_id) && ids.contains(&b_id),
            "both devices must be in the synced metadata, got {ids:?}"
        );
    }

    Ok(())
}

/// The name the confirming user typed overrides the name the new device chose
/// for itself, and reaches both devices.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn multi_device_link_uses_the_confirmed_device_name() -> anyhow::Result<()> {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let (new_device, _tmp) = link_new_device_named(&setup, &alice, "Work laptop").await;
    let old_device = setup.get_user(&alice).user();

    drain_queue(old_device).await;
    drain_queue(&new_device).await;

    let b_id = new_device.own_client_id().await?;
    for (label, device) in [("old", old_device), ("new", &new_device)] {
        let seen = device
            .linked_devices()
            .await?
            .into_iter()
            .find(|device| device.client_id == b_id)
            .unwrap_or_else(|| panic!("the {label} device should know the new device"));
        assert_eq!(
            seen.name, "Work laptop",
            "the {label} device should use the confirmed name"
        );
    }

    Ok(())
}

/// A blank confirmation name leaves the platform default the new device picked.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn multi_device_link_blank_name_keeps_the_device_default() -> anyhow::Result<()> {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let (new_device, _tmp) = link_new_device_named(&setup, &alice, "   ").await;
    let old_device = setup.get_user(&alice).user();

    drain_queue(old_device).await;
    drain_queue(&new_device).await;

    let b_id = new_device.own_client_id().await?;
    let seen = old_device
        .linked_devices()
        .await?
        .into_iter()
        .find(|device| device.client_id == b_id)
        .expect("the old device should know the new device");
    assert!(
        !seen.name.trim().is_empty(),
        "a blank confirmation must not blank out the device name"
    );

    Ok(())
}

/// Unlinking drops exactly the target's leaf and leaves the remover in place.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn multi_device_unlink_removes_only_the_target_leaf() -> anyhow::Result<()> {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let (new_device, _tmp) = link_new_device(&setup, &alice).await;
    let old_device = setup.get_user(&alice).user();
    drain_queue(old_device).await;
    drain_queue(&new_device).await;

    let a_id = old_device.own_client_id().await?;
    let b_id = new_device.own_client_id().await?;

    old_device.unlink_device(b_id).await?;

    let members = old_device.self_group_client_ids().await?;
    assert_eq!(members, vec![a_id], "only A must remain in the self group");

    // The removed device must be able to follow the commit that removed it,
    // which is what lets it notice and tear itself down.
    let messages = new_device.qs_fetch_messages().await?;
    let processed = new_device.fully_process_qs_messages(messages).await;
    assert!(
        processed.errors.is_empty(),
        "the unlinked device failed to process its own removal: {:?}",
        processed.errors
    );

    Ok(())
}

/// The unlinked device notices on its next queue drain and flags itself. The
/// flag is what the app watches to delete its local data and return to the
/// welcome screen.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn multi_device_unlinked_device_flags_itself() -> anyhow::Result<()> {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let (new_device, _tmp) = link_new_device(&setup, &alice).await;
    let old_device = setup.get_user(&alice).user();
    drain_queue(old_device).await;
    drain_queue(&new_device).await;

    let b_id = new_device.own_client_id().await?;
    assert!(!new_device.is_account_unlinked().await?);

    old_device.unlink_device(b_id).await?;
    drain_queue(&new_device).await;

    assert!(
        new_device.is_account_unlinked().await?,
        "the removed device must know it was unlinked"
    );
    assert!(
        !old_device.is_account_unlinked().await?,
        "the remover must not flag itself"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn multi_device_unlink_unknown_client_id_is_an_error() -> anyhow::Result<()> {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let (new_device, _tmp) = link_new_device(&setup, &alice).await;
    let old_device = setup.get_user(&alice).user();
    drain_queue(old_device).await;
    drain_queue(&new_device).await;

    let error = old_device
        .unlink_device(Uuid::from_u128(0xbeef))
        .await
        .expect_err("unlinking an unknown client id must fail");
    assert!(
        error.to_string().contains("not in the self group"),
        "unexpected error: {error}"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn multi_device_unlink_self_is_an_error() -> anyhow::Result<()> {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let (new_device, _tmp) = link_new_device(&setup, &alice).await;
    let old_device = setup.get_user(&alice).user();
    drain_queue(old_device).await;
    drain_queue(&new_device).await;

    let a_id = old_device.own_client_id().await?;
    let error = old_device
        .unlink_device(a_id)
        .await
        .expect_err("a device cannot unlink itself");
    assert!(
        error.to_string().contains("itself"),
        "unexpected error: {error}"
    );

    Ok(())
}

/// A rename on one device reaches the other through the self group.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn multi_device_rename_propagates_to_the_sibling() -> anyhow::Result<()> {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let (new_device, _tmp) = link_new_device(&setup, &alice).await;
    let old_device = setup.get_user(&alice).user();
    drain_queue(old_device).await;
    drain_queue(&new_device).await;

    let b_id = new_device.own_client_id().await?;
    new_device.rename_device(b_id, "Phone".to_owned()).await?;
    new_device.outbound_service().run_once().await;
    drain_queue(old_device).await;

    let seen = old_device
        .linked_devices()
        .await?
        .into_iter()
        .find(|device| device.client_id == b_id)
        .expect("A must know about B");
    assert_eq!(seen.name, "Phone");

    Ok(())
}

/// A linked device removes a member from a group it was onboarded into. The
/// commit replaces the shared virtual-client leaf, so it has to be derived from
/// the self group's emulation epoch.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[tracing::instrument(name = "Remove a member from a linked device", skip_all)]
async fn multi_device_remove_member_from_linked_device() -> anyhow::Result<()> {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let bob = setup.add_user().await;
    setup.connect_users(&alice, &bob).await;
    let charlie = setup.add_user().await;
    setup.connect_users(&alice, &charlie).await;

    let chat_id = setup.create_group(&alice).await;
    setup
        .invite_to_group(chat_id, &alice, vec![&bob, &charlie])
        .await;

    let (new_device, _tmp) = link_new_device(&setup, &alice).await;
    // Onboarding into the pre-existing group runs in the background.
    new_device.outbound_service().run_once().await;

    let old_device = setup.get_user(&alice).user();
    drain_queue(old_device).await;
    drain_queue(&new_device).await;

    new_device
        .remove_users(chat_id, vec![charlie.clone()])
        .await?;

    let members = new_device.mls_chat_participants(chat_id).await.unwrap();
    assert!(
        !members.contains(&charlie),
        "the linked device should no longer see the removed member, got {members:?}"
    );

    // The sibling emulator client follows the removal on the shared leaf.
    let messages = old_device.qs_fetch_messages().await?;
    let processed = old_device.fully_process_qs_messages(messages).await;
    assert!(
        processed.errors.is_empty(),
        "the old device failed to follow the removal: {:?}",
        processed.errors
    );
    assert_eq!(
        old_device.group_epoch_and_own_index(chat_id).await?,
        new_device.group_epoch_and_own_index(chat_id).await?,
        "both emulator clients must stay on the same epoch and shared leaf"
    );

    Ok(())
}

/// After the sibling rotates the user profile, the linked device must still be
/// able to run a group operation: those encrypt the own user profile key for
/// the commit, so the device has to keep knowing which key is its own.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[tracing::instrument(name = "Remove a member after a profile rotation", skip_all)]
async fn multi_device_remove_member_after_sibling_rotated_the_profile() -> anyhow::Result<()> {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let bob = setup.add_user().await;
    setup.connect_users(&alice, &bob).await;
    let charlie = setup.add_user().await;
    setup.connect_users(&alice, &charlie).await;

    let chat_id = setup.create_group(&alice).await;
    setup
        .invite_to_group(chat_id, &alice, vec![&bob, &charlie])
        .await;

    let (new_device, _tmp) = link_new_device(&setup, &alice).await;
    new_device.outbound_service().run_once().await;

    let old_device = setup.get_user(&alice).user();
    drain_queue(old_device).await;
    drain_queue(&new_device).await;

    // The sibling rotates the user profile key and announces it to every group.
    old_device
        .set_own_user_profile(UserProfile {
            user_id: alice.clone(),
            display_name: "New Alice".parse().unwrap(),
            profile_picture: None,
        })
        .await?;

    // The linked device picks the new key up and fetches the profile.
    drain_queue(&new_device).await;
    new_device.outbound_service().run_once().await;

    new_device
        .remove_users(chat_id, vec![charlie.clone()])
        .await?;

    let members = new_device.mls_chat_participants(chat_id).await.unwrap();
    assert!(
        !members.contains(&charlie),
        "the linked device should no longer see the removed member, got {members:?}"
    );

    Ok(())
}

/// A resync of the self group rejoins with the per-device self-group
/// credential.
///
/// The external commit must carry a self-group credential and self-group leaf
/// capabilities. Rejoining with the user credential would be rejected, since
/// the self group accepts only self-group credentials.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[tracing::instrument(name = "Test resync of the self group", skip_all)]
async fn multi_device_self_group_resync() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let (new_device, _tmp) = link_new_device(&setup, &alice).await;
    let old_device = setup.get_user(&alice).user();

    // Bring both devices to the self-group's latest epoch.
    drain_queue(old_device).await;
    drain_queue(&new_device).await;

    let chat_id = self_chat_id(old_device).await;
    let (epoch_before, _) = old_device
        .group_epoch_and_own_index(chat_id)
        .await
        .unwrap()
        .unwrap();

    // The old device resyncs into the self group.
    old_device.enqueue_group_resync(chat_id).await.unwrap();
    assert!(
        old_device.is_resync_pending(chat_id).await.unwrap(),
        "resync should be queued"
    );
    old_device.outbound_service().run_once().await;
    assert!(
        !old_device.is_resync_pending(chat_id).await.unwrap(),
        "resync should have completed"
    );

    // The external commit advanced the epoch, so the rejoin actually happened.
    let (epoch_after, _) = old_device
        .group_epoch_and_own_index(chat_id)
        .await
        .unwrap()
        .unwrap();
    assert!(
        epoch_before < epoch_after,
        "the resync commit should have advanced the self-group epoch"
    );

    // The sibling device processes the rejoin commit.
    let messages = new_device.qs_fetch_messages().await.unwrap();
    let processed = new_device.fully_process_qs_messages(messages).await;
    assert!(
        processed.errors.is_empty(),
        "sibling failed to process the self-group rejoin: {:?}",
        processed.errors
    );

    // Both devices still see two distinct self-group clients and stay in sync.
    self_group_client_ids(old_device).await;
    self_group_client_ids(&new_device).await;
    send_and_receive(old_device, &[&new_device], chat_id, "after resync").await;
    send_and_receive(&new_device, &[old_device], chat_id, "and back").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[tracing::instrument(name = "Test a linked device inherits connection chats", skip_all)]
async fn multi_device_inherits_connection_chats() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let bob = setup.add_user().await;
    let connection_chat_id = setup.connect_users(&alice, &bob).await;

    let (new_device, _tmp) = link_new_device(&setup, &alice).await;
    new_device.outbound_service().run_once().await;

    assert!(
        !new_device
            .is_resync_pending(connection_chat_id)
            .await
            .unwrap(),
        "onboarding into the connection group should have completed"
    );

    let chat = new_device
        .chat(&connection_chat_id)
        .await
        .expect("linked device should have inherited the connection chat");
    assert!(
        matches!(chat.chat_type(), ChatType::Connection(user_id) if user_id == &bob),
        "inherited chat should be a confirmed connection chat, got {:?}",
        chat.chat_type()
    );
    assert_eq!(chat.status(), &ChatStatus::Active);

    // The contact carries the friendship token and WAI key, which are not
    // recoverable from the group and therefore have to ride the linking channel.
    let contact = new_device
        .contact(&bob)
        .await
        .expect("linked device should have inherited the contact");
    assert_eq!(contact.chat_id, connection_chat_id);

    // The old device has to follow the onboarding external commit onto the
    // virtual client's new leaf.
    let old_device = setup.get_user(&alice).user();
    drain_queue(old_device).await;
    assert_eq!(
        old_device
            .group_epoch_and_own_index(connection_chat_id)
            .await
            .unwrap(),
        new_device
            .group_epoch_and_own_index(connection_chat_id)
            .await
            .unwrap(),
        "both emulator clients must land on the same epoch and shared leaf"
    );

    send_and_receive(
        &new_device,
        &[old_device],
        connection_chat_id,
        "hello bob, from the linked device",
    )
    .await;
    send_and_receive(
        old_device,
        &[&new_device],
        connection_chat_id,
        "hello bob, from the old device",
    )
    .await;

    let bob_device = setup.get_user(&bob).user();
    send_and_receive(
        bob_device,
        &[&new_device, old_device],
        connection_chat_id,
        "hello alice, from bob",
    )
    .await;

    // Inviting bob into a fresh group is what actually consumes the transferred key
    // material.
    let group_chat_id = new_device
        .create_chat(
            "group from the linked device".to_owned(),
            None,
            setup.apq_groups,
        )
        .await
        .unwrap();
    new_device
        .invite_users(group_chat_id, std::slice::from_ref(&bob))
        .await
        .expect("fatal error inviting bob")
        .expect("failed to invite bob");

    let qs_messages = bob_device.qs_fetch_messages().await.unwrap();
    let processed = bob_device.fully_process_qs_messages(qs_messages).await;
    assert!(
        processed.errors.is_empty(),
        "bob failed to process the welcome, check logs!"
    );
    assert!(
        bob_device.chat(&group_chat_id).await.is_some(),
        "bob should have joined the group created by the linked device"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[tracing::instrument(name = "Test blocked connection chats are not inherited", skip_all)]
async fn multi_device_skips_blocked_connection_chats() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let bob = setup.add_user().await;
    let connection_chat_id = setup.connect_users(&alice, &bob).await;

    setup
        .get_user(&alice)
        .user()
        .block_contact(bob.clone())
        .await
        .unwrap();

    let (new_device, _tmp) = link_new_device(&setup, &alice).await;
    new_device.outbound_service().run_once().await;

    assert!(
        new_device.chat(&connection_chat_id).await.is_none(),
        "a blocked connection chat should not be conveyed to a linked device"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[tracing::instrument(name = "Test unconfirmed connections are not inherited", skip_all)]
async fn multi_device_skips_unconfirmed_connection_chats() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let bob = setup.add_user().await;

    // Alice requests a connection to bob's username, which bob never accepts.
    let bob_username = setup
        .get_user_mut(&bob)
        .add_username()
        .await
        .unwrap()
        .username;
    let username_hash = bob_username.calculate_hash().unwrap();
    let pending_chat_id = setup
        .get_user(&alice)
        .user()
        .add_contact(bob_username, username_hash)
        .await
        .unwrap()
        .unwrap();

    let (new_device, _tmp) = link_new_device(&setup, &alice).await;
    new_device.outbound_service().run_once().await;

    assert!(
        new_device.chat(&pending_chat_id).await.is_none(),
        "an unconfirmed connection chat should not be conveyed to a linked device"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[tracing::instrument(name = "Test an own echo confirms an unconfirmed send", skip_all)]
async fn multi_device_own_echo_confirms_unconfirmed_send() {
    const TEXT: &str = "the DS response to this one got lost";

    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let bob = setup.add_user().await;
    let chat_id = setup.connect_users(&alice, &bob).await;

    // Linking turns alice's leaf into a virtual-client leaf shared by both
    // devices, which is what makes the DS echo decryptable for the sender.
    let (second_device, _tmp) = link_new_device(&setup, &alice).await;
    second_device.outbound_service().run_once().await;

    let first_device = setup.get_user(&alice).user();
    let bob_device = setup.get_user(&bob).user();
    drain_queue(first_device).await;
    drain_queue(&second_device).await;
    drain_queue(bob_device).await;

    let content = MimiContent::simple_markdown_message(TEXT.to_owned(), [11u8; 16]);
    let message = first_device
        .send_message(chat_id, content, None)
        .await
        .unwrap();
    let message_id = message.id();
    assert!(
        !message.is_sent(),
        "the message should still be waiting for the outbound service"
    );

    // The DS accepts the message and fans it out, but the sending device never
    // sees the response and therefore never confirms the send.
    first_device
        .send_chat_message_without_confirmation(message_id)
        .await
        .unwrap();

    drain_queue(bob_device).await;
    assert_eq!(
        count_messages_with_text(bob_device, chat_id, TEXT).await,
        1,
        "bob should have received the message once"
    );

    // The echo of our own message arrives.
    drain_queue(first_device).await;

    let stored = first_device
        .message(message_id)
        .await
        .unwrap()
        .expect("the sent message should still be stored");
    assert_eq!(
        count_messages_with_text(first_device, chat_id, TEXT).await,
        1,
        "the echo must not be stored as a second message"
    );
    assert!(
        stored.is_sent(),
        "the echo should have confirmed the unconfirmed send"
    );
    assert_eq!(
        first_device
            .last_message(chat_id)
            .await
            .unwrap()
            .unwrap()
            .id(),
        message_id,
        "the echo must not become a newer message"
    );

    // The message left the queue with the confirmation, so there is nothing
    // left to send.
    first_device.outbound_service().run_once().await;

    drain_queue(bob_device).await;
    assert_eq!(
        count_messages_with_text(bob_device, chat_id, TEXT).await,
        1,
        "bob must not receive the message a second time"
    );

    // For the sibling this is a plain incoming message.
    drain_queue(&second_device).await;
    assert_eq!(
        count_messages_with_text(&second_device, chat_id, TEXT).await,
        1,
        "the sibling device should have received the message once"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[tracing::instrument(name = "Test an own echo confirms an unconfirmed edit", skip_all)]
async fn multi_device_own_echo_confirms_unconfirmed_edit() {
    const TEXT: &str = "original text";
    const EDITED_TEXT: &str = "edited, the DS response to this one got lost";
    const FINAL_TEXT: &str = "edited a second time";

    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let bob = setup.add_user().await;
    let chat_id = setup.connect_users(&alice, &bob).await;

    // Linking turns alice's leaf into a virtual-client leaf shared by both
    // devices, which is what makes the DS echo decryptable for the sender.
    let (second_device, _tmp) = link_new_device(&setup, &alice).await;
    second_device.outbound_service().run_once().await;

    let first_device = setup.get_user(&alice).user();
    let bob_device = setup.get_user(&bob).user();
    drain_queue(first_device).await;
    drain_queue(&second_device).await;
    drain_queue(bob_device).await;

    // A regular, confirmed send of the original message.
    let content = MimiContent::simple_markdown_message(TEXT.to_owned(), [21u8; 16]);
    let message = first_device
        .send_message(chat_id, content, None)
        .await
        .unwrap();
    let message_id = message.id();
    first_device.outbound_service().run_once().await;
    drain_queue(first_device).await;
    drain_queue(&second_device).await;
    drain_queue(bob_device).await;

    // The edit is stored and reaches the DS, but the sending device never
    // sees the response and therefore never confirms the send.
    let original = first_device.message(message_id).await.unwrap().unwrap();
    let edit = MimiContent::simple_markdown_message(EDITED_TEXT.to_owned(), [22u8; 16]);
    let edited = first_device
        .send_message(chat_id, edit, Some(original))
        .await
        .unwrap();
    assert!(
        !edited.is_sent(),
        "the edit should still be waiting for the outbound service"
    );
    first_device
        .send_chat_message_without_confirmation(message_id)
        .await
        .unwrap();

    // The echo of the edit arrives.
    drain_queue(first_device).await;

    let stored = first_device.message(message_id).await.unwrap().unwrap();
    assert!(
        stored.is_sent(),
        "the echo should have confirmed the unconfirmed edit"
    );
    assert_eq!(
        count_messages_with_text(first_device, chat_id, EDITED_TEXT).await,
        1,
        "the echo must not be stored as a second message"
    );

    // The edit left the queue with the confirmation, so there is nothing left
    // to send.
    first_device.outbound_service().run_once().await;

    drain_queue(bob_device).await;
    assert_eq!(
        count_messages_with_text(bob_device, chat_id, EDITED_TEXT).await,
        1,
        "bob should see the edit applied once"
    );
    drain_queue(&second_device).await;
    assert_eq!(
        count_messages_with_text(&second_device, chat_id, EDITED_TEXT).await,
        1,
        "the sibling device should see the edit applied once"
    );

    // Editing the message again still works. An echo that lands in the edit
    // history would block this edit with a conflict.
    let original = first_device.message(message_id).await.unwrap().unwrap();
    let second_edit = MimiContent::simple_markdown_message(FINAL_TEXT.to_owned(), [23u8; 16]);
    first_device
        .send_message(chat_id, second_edit, Some(original))
        .await
        .unwrap();
    first_device.outbound_service().run_once().await;

    drain_queue(bob_device).await;
    assert_eq!(
        count_messages_with_text(bob_device, chat_id, FINAL_TEXT).await,
        1,
        "bob should see the second edit"
    );
}
