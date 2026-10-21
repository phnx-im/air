// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{assert_matches, collections::HashSet};

use aircommon::{
    credentials::LeafCredential,
    identifiers::{UserId, Username},
};
use aircoreclient::{
    ChatId, ChatStatus, ChatType, EventMessage, Message, ReadReceiptsSetting, SystemMessage,
    UserProfile,
    clients::{
        CoreUser, ListenResponse, MarkChatAsRead, listen_response,
        multi_device::{
            MultiDeviceLinkClientError, MultiDeviceProvisionClientError, MultiDeviceProvisionStep,
        },
        store::ClientRecord,
    },
};
use airprotos::{
    auth_service::v1::OperationType,
    queue_service::v1::{SiblingClientState, sibling_client_state},
    relay_service::v1::LinkingSessionId,
};
use airserver_test_harness::utils::{
    core_user_ext::CoreUserExt,
    setup::{TestBackend, TestBackendParams},
};
use chrono::{DateTime, Utc};
use mimi_content::{MessageStatus, MimiContent};
use std::time::Duration;
use tempfile::TempDir;
use tokio::time::sleep;
use tokio_stream::{Stream, StreamExt};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

/// Sends `text` from `sender` into the self-group chat and asserts that
/// `receiver` sees it after fetching + processing its queue.
pub(crate) async fn send_and_receive(
    sender: &CoreUser,
    devices: &[&CoreUser],
    chat_id: ChatId,
    text: &str,
) {
    // Drain the sender's own queue so it is at the latest epoch.
    sender.drain().await;

    let content = MimiContent::simple_markdown_message(text.to_owned(), [7u8; 16]);
    sender
        .send_message(chat_id, content.clone(), None, MarkChatAsRead::Yes)
        .await
        .unwrap();
    sender.outbound_service().run_once().await;

    // check the echoed message on the linked client
    for device in devices {
        let processed = device.drain().await;
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
async fn multi_device_linking_session() {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    setup.connect_users(&alice, &bob).await;
    let charlie = setup.add_user().await;
    setup.connect_users(&alice, &charlie).await;

    // Alice already has two groups before the new device is linked. The
    // linked device should inherit them, not just the self-group.
    let chat_id_1 = setup.create_group(&alice).await;
    setup.invite_to_group(chat_id_1, &alice, &[&bob]).await;

    let chat_id_2 = setup.create_group(&alice).await;
    setup.invite_to_group(chat_id_2, &alice, &[&bob]).await;

    let alice2 = alice.add_linked_device().await;

    assert_eq!(
        alice2.qs_user_id(),
        alice.qs_user_id(),
        "linked device must share the virtual client (QsUserId)"
    );
    assert_ne!(
        alice2.qs_client_id(),
        alice.qs_client_id(),
        "linked device must have its own queue (QsClientId)"
    );
    let old_device_self_group = alice
        .self_group()
        .await
        .unwrap()
        .expect("old device should have a self group");
    let new_device_self_group = alice2
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
        alice.self_group_member_count().await.unwrap(),
        Some(2),
        "old device should see both emulator clients in the self group"
    );
    assert_eq!(
        alice2.self_group_member_count().await.unwrap(),
        Some(2),
        "new device should see both emulator clients in the self group"
    );

    // The self group carries per-device SelfGroupCredentials, not user
    // credentials. Both devices must see the same pair of distinct client ids,
    // and that pair must match the client id each device stored locally.
    let old_device_ids = self_group_client_ids(&alice).await;
    let new_device_ids = self_group_client_ids(&alice2).await;
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
        alice.own_client_id().await.unwrap(),
        alice2.own_client_id().await.unwrap(),
    ]
    .into_iter()
    .collect();
    assert_eq!(
        old_set, expected,
        "self-group client ids must match each device's own client id"
    );

    assert!(
        alice.self_chat_id().await.unwrap().is_some(),
        "old device should have a self-chat"
    );
    assert!(
        alice2.self_chat_id().await.unwrap().is_some(),
        "new device should have a Notes to self chat"
    );

    // Onboarding into the pre-existing groups is queued and processed in the background.
    alice2.outbound_service().run_once().await;

    // The new device must know about all groups from the original client.
    let new_device_chat_ids = alice2.ordered_chat_ids().await.unwrap();
    for (label, chat_id) in [("1", chat_id_1), ("2", chat_id_2)] {
        assert!(
            new_device_chat_ids.contains(&chat_id),
            "linked device should have inherited pre-existing group {label}"
        );
        assert!(
            alice2.resync_status(chat_id).await.unwrap().is_none(),
            "onboarding into group {label} should have completed, not still be queued"
        );

        let members = alice2.mls_chat_participants(chat_id).await;
        assert!(
            members
                .as_ref()
                .is_some_and(|members| members.contains(alice.user_id())),
            "chat {label} on the linked device should be bound to a joined MLS group, got {members:?}"
        );
        assert_eq!(
            members,
            alice.mls_chat_participants(chat_id).await,
            "linked device should see the same members as the old device in group {label}"
        );
    }

    // Linking the new device is recorded in the self group, not in each
    // pre-existing chat: the old device's commit adding the new device's
    // self-group leaf must produce a DeviceLinked system message identifying
    // it, not a generic Add (which would render as e.g. "Alice added Alice").
    let self_group_chat_id = ChatId::try_from(old_device_self_group.group_id()).unwrap();
    assert_eq!(
        device_linked_client_ids(&alice, self_group_chat_id).await,
        vec![alice2.own_client_id().await.unwrap()],
        "linking the new device should record a DeviceLinked system message in the self group"
    );

    // Messages sent into the self group are seen by the other device.
    let self_chat_id = ChatId::try_from(old_device_self_group.group_id()).unwrap();
    send_and_receive(
        &alice,
        &[&alice2],
        self_chat_id,
        "hello from the old device in self-group",
    )
    .await;
    send_and_receive(
        &alice2,
        &[&alice],
        self_chat_id,
        "hello back from the new device in self-group",
    )
    .await;

    // The old device has to follow the onboarding external commit onto the virtual client's new leaf.
    alice.drain().await;
    assert_eq!(
        alice.group_epoch_and_own_index(chat_id_1).await.unwrap(),
        alice2.group_epoch_and_own_index(chat_id_1).await.unwrap(),
        "both emulator clients must land on the same epoch and shared leaf"
    );

    // Messages sent into one of the existing groups are seen by both clients.
    for chat_id in [chat_id_1, chat_id_2] {
        send_and_receive(&alice, &[&alice2], chat_id, "hello from the old device").await;
        send_and_receive(
            &alice2,
            &[&alice],
            chat_id,
            "hello back from the new device",
        )
        .await;
    }

    // Messages received from a 3rd party are seen by both clients.
    send_and_receive(
        &bob,
        &[&alice2, &alice],
        chat_id_1,
        "hello from the old device",
    )
    .await;

    // A commit that replaces the shared leaf for some other reason
    // has to derive the new leaf from the emulation epoch too,
    // or the sibling emulator client will break.
    setup.invite_to_group(chat_id_1, &alice, &[&charlie]).await;

    alice2.drain().await;
    assert_eq!(
        alice2.group_epoch_and_own_index(chat_id_1).await.unwrap(),
        alice.group_epoch_and_own_index(chat_id_1).await.unwrap(),
        "both emulator clients must stay on the same epoch and shared leaf after an invite+add"
    );

    send_and_receive(&bob, &[&alice2], chat_id_1, "after a member was added").await;

    // Rotating the user profile updates the profile key on the self group. That
    // DS request envelope is now signed with the per-device self-group key, so
    // this confirms the flipped self-group credential still authenticates
    // self-group operations.
    let new_profile = UserProfile {
        user_id: alice.user_id().clone(),
        display_name: "New Alice".parse().unwrap(),
        profile_picture: None,
    };
    alice.set_own_user_profile(new_profile).await.unwrap();

    // Same for a removal.
    setup
        .remove_from_group(chat_id_1, &alice, &[&charlie])
        .await
        .unwrap();

    alice2.drain().await;
    assert_eq!(
        alice2.group_epoch_and_own_index(chat_id_1).await.unwrap(),
        alice.group_epoch_and_own_index(chat_id_1).await.unwrap(),
        "both emulator clients must stay on the same epoch and shared leaf after a removal"
    );

    send_and_receive(&bob, &[&alice2], chat_id_1, "after a member was removed").await;

    // And for a deletion, which replaces the shared leaf as well. Without
    // following it the linked device never learns the group is gone.
    setup.delete_group(chat_id_1, &alice).await;

    alice2.drain().await;
    let deleted_chat = alice2
        .chat(&chat_id_1)
        .await
        .expect("linked device should still know the deleted chat");
    assert!(
        matches!(deleted_chat.status(), ChatStatus::Inactive(_)),
        "linked device should see the deleted group as inactive, got {:?}",
        deleted_chat.status()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn multi_device_self_group_attachment() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let alice2 = alice.add_linked_device().await;
    alice.drain().await;
    alice2.drain().await;

    let chat_id = self_chat_id(&alice).await;
    let attachment = vec![0x00, 0x01, 0x02, 0x03];
    let tmp_dir = TempDir::new().unwrap();
    let path = tmp_dir.path().join("test.bin");
    std::fs::write(&path, &attachment).unwrap();

    let (_attachment_id, _progress, upload_task) = alice
        .upload_chat_attachment(chat_id, &path, MarkChatAsRead::Yes)
        .await
        .unwrap()
        .unwrap();
    let message = upload_task
        .await
        .unwrap_or_else(|error| panic!("upload in the self-group failed: {error:?}"));
    alice
        .outbound_service()
        .enqueue_chat_message(message.id())
        .await
        .unwrap();
    alice.outbound_service().run_once().await;

    alice2.drain().await;
    let pending = alice2.pending_attachments().await.unwrap();
    assert_eq!(pending.len(), 1);
    let attachment_id = pending[0];

    let (_progress, download_task) = alice2.download_attachment(attachment_id);
    download_task
        .await
        .expect("download in the self-group failed");

    let content = alice2
        .load_attachment(attachment_id)
        .await
        .unwrap()
        .into_bytes()
        .unwrap();
    assert_eq!(content, attachment);
}

// Linking with a session ID that was never registered returns an error.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn multi_device_link_with_nonexistent_session_id() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;

    let fake_digest =
        hex::decode("68924f1f6f60d5fdb8463881a5945e58c3f1402c65681b1270f5aeccbed17bd1")
            .unwrap()
            .try_into()
            .unwrap();
    let fake_session_id = LinkingSessionId::from_digest(&fake_digest, 8).unwrap();
    let result = alice
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
async fn multi_device_second_link_attempt_returns_error() {
    let mut setup = TestBackend::single().await;
    let domain = setup.domain().clone();
    let server_url = setup.server_url();
    let alice = setup.add_user().await;

    let (session_tx, mut session_rx) = tokio::sync::mpsc::channel(1);

    let new_device_task = tokio::spawn(async move {
        CoreUser::new_linked_ephemeral(domain, server_url, session_tx)
            .await
            .unwrap()
            .unwrap()
    });

    let session_id = recv_session_id(&mut session_rx).await;

    alice
        .multi_device_link_client(session_id.clone(), ignore_connected(), auto_confirm())
        .await
        .unwrap()
        .unwrap();

    new_device_task.await.unwrap();

    // Session was already consumed — a second attempt must fail.
    let second_result = alice
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
async fn multi_device_concurrent_linking_sessions_dont_interfere() {
    let mut setup = TestBackend::single().await;
    let domain = setup.domain().clone();
    let server_url = setup.server_url();
    let [alice, bob] = setup.add_users().await;

    let (alice_session_tx, mut alice_session_rx) = tokio::sync::mpsc::channel(1);
    let (bob_session_tx, mut bob_session_rx) = tokio::sync::mpsc::channel(1);

    let alice_domain = domain.clone();
    let alice_server_url = server_url.clone();
    let alice2_task = tokio::spawn(async move {
        CoreUser::new_linked_ephemeral(alice_domain, alice_server_url, alice_session_tx)
            .await
            .unwrap()
            .unwrap()
    });

    let bob_domain = domain.clone();
    let bob_server_url = server_url.clone();
    let bob2_task = tokio::spawn(async move {
        CoreUser::new_linked_ephemeral(bob_domain, bob_server_url, bob_session_tx)
            .await
            .unwrap()
            .unwrap()
    });

    let alice_session_id = recv_session_id(&mut alice_session_rx).await;
    let bob_session_id = recv_session_id(&mut bob_session_rx).await;

    // Session IDs derived from different key packages must be distinct.
    assert_ne!(alice_session_id, bob_session_id);

    alice
        .multi_device_link_client(alice_session_id, ignore_connected(), auto_confirm())
        .await
        .unwrap()
        .unwrap();

    bob.multi_device_link_client(bob_session_id, ignore_connected(), auto_confirm())
        .await
        .unwrap()
        .unwrap();

    // Each new device must be linked to the correct existing virtual client.
    let alice2 = alice2_task.await.unwrap();
    let bob2 = bob2_task.await.unwrap();
    assert_eq!(alice2.qs_user_id(), alice.qs_user_id());
    assert_eq!(bob2.qs_user_id(), bob.qs_user_id());
    assert_ne!(alice2.qs_user_id(), bob2.qs_user_id());
}

// A settings change on one device reaches the linked device through a
// self-group commit, in both directions. This is also the proof that the DS
// passes commits carrying AppEphemeral proposals through.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn multi_device_settings_sync() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let alice2 = alice.add_linked_device().await;

    // Bring both devices to the self-group's latest epoch.
    alice.drain().await;
    alice2.drain().await;

    // Initial key package uploads. Each upload completes only once the
    // uploading device fetches its own commit.
    alice2.outbound_service().run_once().await;
    alice2.drain().await;
    alice.drain().await;
    alice.outbound_service().run_once().await;
    alice.drain().await;
    alice2.drain().await;

    // Nothing was set before linking, so the linking payload carried an empty
    // snapshot and the new device starts unset.
    assert_eq!(read_receipts(&alice2).await, None);

    // Two quick toggles: both are applied locally right away and fold into
    // one pending change. Before the outbound service runs, no commit exists.
    let chat_id = self_chat_id(&alice).await;
    alice
        .set_synced_user_setting(&ReadReceiptsSetting(true))
        .await
        .unwrap();
    alice
        .set_synced_user_setting(&ReadReceiptsSetting(false))
        .await
        .unwrap();
    assert_eq!(read_receipts(&alice).await, Some(false));
    assert!(alice.has_pending_setting_changes().await.unwrap());
    assert!(
        alice
            .pending_chat_operation_info(chat_id)
            .await
            .unwrap()
            .is_none(),
        "the commit is only staged when the outbound service runs"
    );

    // The outbound service stages a commit from the current state and sends
    // it. Success completes the pending change.
    alice.outbound_service().run_once().await;
    assert!(!alice.has_pending_setting_changes().await.unwrap());
    assert!(
        alice
            .pending_chat_operation_info(chat_id)
            .await
            .unwrap()
            .is_none(),
        "the operation should be gone after a successful send"
    );

    // The linked device applies the folded update when processing its queue.
    alice2.drain().await;
    assert_eq!(read_receipts(&alice2).await, Some(false));

    // No feedback loop: applying a sibling's update records no pending change
    // and issues no commit on the receiving device.
    assert!(!alice2.has_pending_setting_changes().await.unwrap());
    alice2.outbound_service().run_once().await;
    assert!(
        alice2
            .pending_chat_operation_info(chat_id)
            .await
            .unwrap()
            .is_none(),
        "applying an incoming update must not issue a commit"
    );

    // And back in the other direction, from the new device.
    alice2
        .set_synced_user_setting(&ReadReceiptsSetting(true))
        .await
        .unwrap();
    alice2.outbound_service().run_once().await;
    alice.drain().await;
    assert_eq!(read_receipts(&alice).await, Some(true));
}

// A setting stored before any linking travels to the new device in the
// provisioning package and is applied during bootstrap.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn multi_device_settings_in_linking_payload() {
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

// The new device's metadata entry rides on the self-group add commit itself, so
// linking costs exactly one commit.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn multi_device_link_publishes_device_entry_without_extra_commit() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;

    let old_id = alice.own_client_id().await.unwrap();
    let alice2 = alice.add_linked_device().await;
    let new_id = alice2.own_client_id().await.unwrap();

    // No outbound run and no queue drain: both sides already hold both entries.
    // The old device folded the new entry in while staging the add, and the new
    // device wrote its own entry before handing a copy over.
    for (label, device) in [("old", &alice), ("new", &alice2)] {
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
    for (label, device) in [("old", &alice), ("new", &alice2)] {
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
async fn multi_device_settings_race_converges() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let alice2 = alice.add_linked_device().await;

    alice.drain().await;
    alice2.drain().await;

    // Both devices toggle while at the same epoch. The outbound runs stage
    // conflicting commits against that epoch.
    alice
        .set_synced_user_setting(&ReadReceiptsSetting(true))
        .await
        .unwrap();
    alice2
        .set_synced_user_setting(&ReadReceiptsSetting(false))
        .await
        .unwrap();

    // The old device reaches the DS first and wins.
    alice.outbound_service().run_once().await;

    // The new device's commit is rejected with a wrong epoch. The operation
    // parks until the winning commit arrives through the queue, the pending
    // change stays, and the optimistic value stays in place for now.
    alice2.outbound_service().run_once().await;
    let chat_id = self_chat_id(&alice2).await;
    let parked = alice2
        .pending_chat_operation_info(chat_id)
        .await
        .unwrap()
        .expect("losing operation should be parked");
    assert_eq!(parked.request_status, "waiting_for_queue_response");
    assert!(alice2.has_pending_setting_changes().await.unwrap());
    assert_eq!(read_receipts(&alice2).await, Some(false));

    // Processing the winner's commit applies its snapshot, deletes the parked
    // operation, and drops the covered field from the pending changes, so
    // nothing is re-issued.
    alice2.drain().await;
    assert_eq!(read_receipts(&alice2).await, Some(true));
    assert!(
        alice2
            .pending_chat_operation_info(chat_id)
            .await
            .unwrap()
            .is_none(),
        "losing operation should be deleted"
    );
    assert!(
        !alice2.has_pending_setting_changes().await.unwrap(),
        "covered pending change should be dropped"
    );

    // Nothing left to send: another outbound run stages no new commit.
    alice2.outbound_service().run_once().await;
    assert!(
        alice2
            .pending_chat_operation_info(chat_id)
            .await
            .unwrap()
            .is_none()
    );

    // The winner's own state is unaffected by its echo.
    alice.drain().await;
    assert_eq!(read_receipts(&alice).await, Some(true));

    // The self group is still usable after the race.
    send_and_receive(&alice, &[&alice2], chat_id, "still in sync").await;
}

// Idempotent issuance derives every token from a seed the devices agree on, so
// two devices that replenish independently end up holding byte-identical tokens
// with no token synchronization protocol. The seed reaches the second device in
// its provisioning package, which is what keeps it from proposing a seed of its
// own and losing the race for an allowance epoch the first device has already
// locked.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn multi_device_token_issuance_is_idempotent() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;

    // A lone device agrees with itself, so it can issue right away.
    alice.outbound_service().run_once().await;
    let seeds = alice.committed_token_seeds().await.unwrap();
    assert!(
        !seeds.is_empty(),
        "a lone device must agree on its own token seeds"
    );
    let first_tokens = alice
        .cached_privacy_pass_tokens(OperationType::AddUsername)
        .await
        .unwrap();
    assert!(
        !first_tokens.is_empty(),
        "the first device must hold tokens"
    );

    let alice2 = alice.add_linked_device().await;

    // The linking payload carried the agreed seeds, so the new device starts out
    // agreeing rather than proposing.
    assert_eq!(
        alice2.committed_token_seeds().await.unwrap(),
        seeds,
        "the new device must adopt its sibling's agreed seeds"
    );

    // The second device replenishes on its own. The AS answers the repeated
    // request for free, and the tokens finalize to the same bytes.
    alice2.outbound_service().run_once().await;
    assert_eq!(
        alice2
            .cached_privacy_pass_tokens(OperationType::AddUsername)
            .await
            .unwrap(),
        first_tokens,
        "both devices must hold byte-identical tokens"
    );

    // Both devices still see each other, so neither switched to a private seed.
    for (label, device) in [("first", &alice), ("second", &alice2)] {
        assert_eq!(
            device.self_group_client_ids().await.unwrap().len(),
            2,
            "the {label} device should see both leaves"
        );
        assert_eq!(
            device.committed_token_seeds().await.unwrap(),
            seeds,
            "the {label} device must keep the agreed seeds"
        );
    }
}

// A seed that has to be invented while two devices are already linked is agreed
// over the self group: a device proposes, a commit carries the proposal, and DS
// commit order decides which proposal becomes the seed. This is the path a key
// rotation takes, where neither device has a seed for the new key and the
// provisioning snapshot cannot help, so the test puts both devices into exactly
// the state a rotation leaves behind.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn multi_device_token_seed_agreement() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let alice2 = alice.add_linked_device().await;

    for device in [&alice, &alice2] {
        device.reset_privacy_pass_for_key_rotation().await.unwrap();
        assert!(
            device.committed_token_seeds().await.unwrap().is_empty(),
            "the rotation left no agreed seed behind"
        );
    }

    // Replenishment proposes, the next outbound run stages and sends the
    // proposal commit, and the sibling has to process it. Once a seed is agreed,
    // a further run fetches the batch under it.
    for _ in 0..4 {
        for device in [&alice, &alice2] {
            device.outbound_service().run_once().await;
            device.drain().await;
        }
    }

    let first_seeds = alice.committed_token_seeds().await.unwrap();
    assert!(
        !first_seeds.is_empty(),
        "the devices must have agreed on a seed"
    );
    assert_eq!(
        alice2.committed_token_seeds().await.unwrap(),
        first_seeds,
        "both devices must agree on the same seed"
    );

    // Agreeing on the seed is what makes the tokens identical.
    assert_eq!(
        alice2
            .cached_privacy_pass_tokens(OperationType::AddUsername)
            .await
            .unwrap(),
        alice
            .cached_privacy_pass_tokens(OperationType::AddUsername)
            .await
            .unwrap(),
        "both devices must hold byte-identical tokens"
    );
}

/// The device's cached AddUsername tokens, in consumption order.
async fn cached_tokens(user: &CoreUser) -> Vec<Vec<u8>> {
    user.cached_privacy_pass_tokens(OperationType::AddUsername)
        .await
        .unwrap()
}

/// A username no other test can have taken.
fn fresh_username() -> Username {
    Username::new(format!("alice-{}", Uuid::new_v4().simple())).unwrap()
}

// Redemption is local to the spending device. Without a broadcast a sibling
// keeps holding a token the AS has already seen and loses one of its own
// requests to it. The spending device therefore publishes the position it
// spent, which names the same token on every device.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn multi_device_redeemed_token_is_removed_from_sibling() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;

    alice.outbound_service().run_once().await;
    let alice2 = alice.add_linked_device().await;
    alice2.outbound_service().run_once().await;
    // Catch up on the echo of our own add commit and on the new device's own
    // self-group commits, so the broadcast below is built at the DS epoch.
    alice.drain().await;

    let full_batch = cached_tokens(&alice).await;
    assert!(!full_batch.is_empty(), "the first device must hold tokens");
    assert_eq!(
        cached_tokens(&alice2).await,
        full_batch,
        "both devices must start from the same batch"
    );

    assert!(
        alice
            .add_username(fresh_username())
            .await
            .unwrap()
            .is_some(),
        "the username must be created"
    );

    let remaining = cached_tokens(&alice).await;
    assert_eq!(
        remaining.len(),
        full_batch.len() - 1,
        "creating the username spent one token"
    );
    assert_eq!(
        cached_tokens(&alice2).await,
        full_batch,
        "the sibling still holds the spent token"
    );

    // The broadcast is held back to keep it apart from the redemption. The
    // test skips that wait.
    alice.expedite_redeemed_token_broadcast().await.unwrap();
    let epochs_before = alice.self_group_epochs().await.unwrap();
    alice.outbound_service().run_once().await;

    assert!(
        alice
            .pending_redeemed_token_broadcasts()
            .await
            .unwrap()
            .is_empty(),
        "the DS accepting the message retires the broadcast"
    );
    assert_eq!(
        alice.self_group_epochs().await.unwrap(),
        epochs_before,
        "the broadcast is an application message, not a commit"
    );

    alice2.drain().await;
    assert_eq!(
        cached_tokens(&alice2).await,
        remaining,
        "the sibling must drop the redeemed token"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn multi_device_deleted_chat_is_erased_on_sibling() -> anyhow::Result<()> {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    let chat_id = setup.connected_group(&alice, &[&bob]).await;

    let alice2 = alice.add_synced_device().await;
    assert!(alice2.chat(&chat_id).await.is_some());

    alice.delete_chat(chat_id).await?;
    alice2.drain().await;
    assert!(
        alice2.chat(&chat_id).await.is_some(),
        "deleting the group leaves the chat in place until it is erased"
    );

    let self_chat_id = self_chat_id(&alice).await;
    let epochs_before = alice.self_group_epochs().await?;
    alice.erase_chat(chat_id).await?;
    assert!(alice.chat(&chat_id).await.is_none());
    alice.outbound_service().run_once().await;
    assert_ne!(
        alice.self_group_epochs().await?,
        epochs_before,
        "the deletion travels in a self-group commit"
    );
    assert!(
        alice
            .pending_chat_operation_info(self_chat_id)
            .await?
            .is_none(),
        "the commit should be gone after a successful send"
    );

    alice2.drain().await;
    assert!(
        alice2.chat(&chat_id).await.is_none(),
        "the sibling must erase the chat"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn multi_device_sole_member_group_deletion_reaches_the_sibling() -> anyhow::Result<()> {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let chat_id = setup.create_group(&alice).await;

    let alice2 = alice.add_synced_device().await;
    assert!(alice2.chat(&chat_id).await.is_some());
    assert!(alice.ds_has_group(chat_id).await?);

    let deletion = SystemMessage::DeleteGroup(alice.user_id().clone());
    alice.delete_chat(chat_id).await?;
    let messages = alice.system_messages(chat_id).await;
    assert_eq!(
        messages
            .iter()
            .filter(|message| **message == deletion)
            .count(),
        1,
        "the deleter stores one deletion message: {messages:?}"
    );
    assert!(
        !alice.ds_has_group(chat_id).await?,
        "the DS deletes the group of a sole member"
    );

    alice2.drain().await;
    assert!(alice2.expect_chat(chat_id).await.status().is_inactive());
    let sibling_messages = alice2.system_messages(chat_id).await;
    assert_eq!(
        sibling_messages
            .iter()
            .filter(|message| **message == deletion)
            .count(),
        1,
        "got {sibling_messages:?}"
    );
    assert!(
        !sibling_messages
            .iter()
            .any(|message| matches!(message, SystemMessage::Remove(_remover, _removed))),
        "got {sibling_messages:?}"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn multi_device_missed_group_deletion_is_found_by_resync() -> anyhow::Result<()> {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    let chat_id = setup.connected_group(&alice, &[&bob]).await;

    let alice2 = alice.add_synced_device().await;
    assert!(alice2.chat(&chat_id).await.is_some());

    alice.delete_chat(chat_id).await?;
    // The sibling misses the delete commit, so its leaf is still in the group
    // on the DS and a resync would rejoin the one-member group.
    alice2.discard_queue().await;
    assert_eq!(
        alice2.expect_chat(chat_id).await.status(),
        &ChatStatus::Active
    );
    let participants = alice2.chat_participants(chat_id).await;

    alice2.enqueue_group_resync(chat_id).await?;
    alice2.outbound_service().run_once().await;

    assert!(alice2.expect_chat(chat_id).await.status().is_inactive());
    let sibling_messages = alice2.system_messages(chat_id).await;
    assert_eq!(
        sibling_messages
            .iter()
            .filter(|message| **message == SystemMessage::DeleteGroup(alice.user_id().clone()))
            .count(),
        1,
        "got {sibling_messages:?}"
    );
    assert!(
        !sibling_messages
            .iter()
            .any(|message| matches!(message, SystemMessage::Remove(_remover, _removed))),
        "got {sibling_messages:?}"
    );
    assert!(
        alice2.resync_status(chat_id).await?.is_none(),
        "the resync entry is gone"
    );
    assert!(
        alice2.mls_members(chat_id).await?.is_none(),
        "the MLS group is gone"
    );
    assert_eq!(
        alice2.chat_participants(chat_id).await,
        participants,
        "the chat still lists its members"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn multi_device_deleted_group_takes_no_further_operations() -> anyhow::Result<()> {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    let chat_id = setup.connected_group(&alice, &[&bob]).await;

    let alice2 = alice.add_synced_device().await;

    alice.delete_chat(chat_id).await?;
    // The sibling's leaf is still in the group, so only the deletion stops
    // the DS from accepting its commit.
    alice2.discard_queue().await;

    let result = alice2.update_key(chat_id).await;
    assert!(
        result.is_err(),
        "the DS accepts a commit on a deleted group"
    );
    // The rejection schedules a resync, which finds the deletion.
    alice2.outbound_service().run_once().await;
    assert_deletion_announced(&alice2, chat_id, alice.user_id()).await;

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn multi_device_message_to_deleted_group_announces_the_deletion() -> anyhow::Result<()> {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    let chat_id = setup.connected_group(&alice, &[&bob]).await;

    let alice2 = alice.add_synced_device().await;

    alice.delete_chat(chat_id).await?;
    alice2.discard_queue().await;

    let content = MimiContent::simple_markdown_message("too late".to_owned(), [5; 16]);
    alice2
        .send_message(chat_id, content, None, MarkChatAsRead::Yes)
        .await?;
    // The first run has the DS reject the message, the second one resyncs.
    alice2.outbound_service().run_once().await;
    alice2.outbound_service().run_once().await;
    assert_deletion_announced(&alice2, chat_id, alice.user_id()).await;

    Ok(())
}

/// Asserts that `device` ended the chat with exactly one message saying that
/// `deleter` deleted it, and dropped the group's MLS state.
async fn assert_deletion_announced(device: &CoreUser, chat_id: ChatId, deleter: &UserId) {
    assert!(device.expect_chat(chat_id).await.status().is_inactive());
    let messages = device.system_messages(chat_id).await;
    assert_eq!(
        messages
            .iter()
            .filter(|message| **message == SystemMessage::DeleteGroup(deleter.clone()))
            .count(),
        1,
        "got {messages:?}"
    );
    assert!(
        device.resync_status(chat_id).await.unwrap().is_none(),
        "the resync entry is gone"
    );
    assert!(
        device.mls_members(chat_id).await.unwrap().is_none(),
        "the MLS group is gone"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn multi_device_deleted_message_is_removed_from_sibling() -> anyhow::Result<()> {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    let chat_id = setup.connected_group(&alice, &[&bob]).await;

    let alice2 = alice.add_synced_device().await;

    bob.drain().await;
    let text = "delete me on every device";
    bob.send_text(chat_id, text).await;

    alice.drain().await;
    alice2.drain().await;
    assert_eq!(
        alice
            .text_messages(chat_id)
            .await
            .iter()
            .filter(|rendered| *rendered == text)
            .count(),
        1
    );
    assert_eq!(
        alice2
            .text_messages(chat_id)
            .await
            .iter()
            .filter(|rendered| *rendered == text)
            .count(),
        1
    );

    let message_id = alice
        .messages(chat_id, 100)
        .await?
        .iter()
        .find(|message| {
            message
                .message()
                .mimi_content()
                .is_some_and(|content| content.string_rendering().is_ok_and(|s| s.contains(text)))
        })
        .map(|message| message.id())
        .expect("the first device holds the message");

    let epochs_before = alice.self_group_epochs().await?;
    alice.delete_message(message_id).await?;
    alice.outbound_service().run_once().await;
    assert_eq!(
        alice
            .text_messages(chat_id)
            .await
            .iter()
            .filter(|rendered| *rendered == text)
            .count(),
        0
    );
    assert_eq!(
        alice.self_group_epochs().await?,
        epochs_before,
        "the deletion is an application message, not a commit"
    );

    alice2.drain().await;
    assert_eq!(
        alice2
            .text_messages(chat_id)
            .await
            .iter()
            .filter(|rendered| *rendered == text)
            .count(),
        0,
        "the sibling must delete the message"
    );

    Ok(())
}

// A device that joins via Welcome cannot read the messages that carried past
// redemptions, and its first batch fetch is answered from the request its
// sibling registered. The provisioning package therefore carries the redeemed
// positions, pending or not.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn multi_device_new_device_skips_redeemed_tokens() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;

    alice.outbound_service().run_once().await;
    assert!(
        alice
            .add_username(fresh_username())
            .await
            .unwrap()
            .is_some(),
        "the username must be created"
    );
    let remaining = cached_tokens(&alice).await;

    let alice2 = alice.add_linked_device().await;
    alice2.outbound_service().run_once().await;

    assert_eq!(
        cached_tokens(&alice2).await,
        remaining,
        "the new device must never hold the redeemed token"
    );
    assert!(
        alice2
            .pending_redeemed_token_broadcasts()
            .await
            .unwrap()
            .is_empty(),
        "the new device has nothing of its own to tell"
    );
}

/// Links a fresh device to `user_id` and asserts that both sides fail with the
/// device limit error and that the rejected device removed its local client.
async fn assert_link_rejected_at_limit(
    setup: &TestBackend,
    user: &CoreUser,
    max_devices: u32,
) -> anyhow::Result<()> {
    let domain = setup.domain().clone();
    let server_url = setup.server_url();
    let (session_tx, mut session_rx) = tokio::sync::mpsc::channel(1);
    let tmp = TempDir::new()?;
    let db_path = tmp.path().to_str().unwrap().to_owned();
    let new_device_task = tokio::spawn({
        let db_path = db_path.clone();
        async move {
            CoreUser::new_linked(
                &db_path,
                domain,
                Some(server_url),
                session_tx,
                CancellationToken::new(),
            )
            .await
        }
    });
    let session_id = recv_session_id(&mut session_rx).await;

    let linked = user
        .multi_device_link_client(session_id, ignore_connected(), auto_confirm())
        .await?;
    assert_matches!(
        linked,
        Err(MultiDeviceLinkClientError::DeviceLimitReached { max_devices: max })
            if max == max_devices
    );

    let provisioned = new_device_task.await??;
    assert_matches!(
        provisioned,
        Err(MultiDeviceProvisionClientError::DeviceLimitReached { max_devices: max })
            if max == max_devices
    );

    assert!(
        ClientRecord::load_all_from_air_db(&db_path)
            .await?
            .is_empty(),
        "the rejected device must not keep a client record"
    );
    assert!(
        client_db_files(tmp.path()).is_empty(),
        "no client DB files may remain: {:?}",
        client_db_files(tmp.path())
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn multi_device_link_rejected_at_device_limit() -> anyhow::Result<()> {
    let mut setup = TestBackend::single_with_params(TestBackendParams {
        max_devices: Some(2),
        ..Default::default()
    })
    .await;
    let alice = setup.add_user().await;
    let alice2 = alice.add_linked_device().await;
    alice.drain().await;
    alice2.drain().await;

    assert_link_rejected_at_limit(&setup, &alice, 2).await?;

    // The self group is unchanged.
    let a_id = alice.own_client_id().await?;
    let b_id = alice2.own_client_id().await?;
    let members: HashSet<_> = alice.self_group_client_ids().await?.into_iter().collect();
    assert_eq!(members, HashSet::from([a_id, b_id]));

    // After unlinking a device, linking works again.
    alice.unlink_device(b_id).await?;
    let alice3 = alice.add_linked_device().await;
    let c_id = alice3.own_client_id().await?;
    let members: HashSet<_> = alice.self_group_client_ids().await?.into_iter().collect();
    assert_eq!(members, HashSet::from([a_id, c_id]));

    Ok(())
}

/// A self group above the device limit (e.g. after the limit was lowered)
/// still accepts commits that don't grow it, so the user can sync and unlink.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn multi_device_self_group_above_limit_can_shrink() -> anyhow::Result<()> {
    let mut setup = TestBackend::single_with_params(TestBackendParams {
        max_devices: Some(3),
        ..Default::default()
    })
    .await;
    let alice = setup.add_user().await;
    let alice2 = alice.add_linked_device().await;
    alice2.drain().await;
    let alice3 = alice.add_linked_device().await;
    alice.drain().await;
    alice2.drain().await;
    alice3.drain().await;

    // Linking makes device 1 upload key packages, which completes only once
    // it fetches its own commit.
    alice.outbound_service().run_once().await;
    alice.drain().await;
    alice2.drain().await;
    alice3.drain().await;

    setup.set_max_devices(1);

    // Settings sync
    alice
        .set_synced_user_setting(&ReadReceiptsSetting(true))
        .await?;
    alice.outbound_service().run_once().await;
    assert!(!alice.has_pending_setting_changes().await?);
    alice2.drain().await;
    alice3.drain().await;
    assert_eq!(read_receipts(&alice2).await, Some(true));

    // Self update
    let chat_id = self_chat_id(&alice).await;
    alice
        .set_self_updated_at(chat_id, DateTime::UNIX_EPOCH)
        .await?;
    let before = Utc::now();
    alice
        .outbound_service()
        .schedule_self_update(DateTime::UNIX_EPOCH)
        .await?;
    alice.outbound_service().run_once().await;
    let after = alice.self_updated_at(chat_id).await?.unwrap();
    assert!(
        before < after,
        "the DS should accept a self-update above the limit"
    );
    alice2.drain().await;
    alice3.drain().await;

    // Growing is still rejected.
    assert_link_rejected_at_limit(&setup, &alice, 1).await?;

    // Unlinking shrinks the group while it stays above the limit.
    let a_id = alice.own_client_id().await?;
    let b_id = alice2.own_client_id().await?;
    let c_id = alice3.own_client_id().await?;
    alice.unlink_device(c_id).await?;
    alice2.drain().await;
    let members: HashSet<_> = alice.self_group_client_ids().await?.into_iter().collect();
    assert_eq!(members, HashSet::from([a_id, b_id]));

    send_and_receive(&alice, &[&alice2], chat_id, "after shrinking").await;

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn multi_device_linking_a_third_device() {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    let connection_chat_id = setup.connect_users(&alice, &bob).await;

    // A higher-level group Alice is already a member of. Every device linked
    // from here on onboards into it with a resync, and unlike the self group its
    // commits are broadcast to all of Alice's emulator queues. The connection
    // group with bob is a second such group.
    let group_chat_id = setup.create_group(&alice).await;
    setup.invite_to_group(group_chat_id, &alice, &[&bob]).await;

    let alice2 = alice.add_linked_device().await;
    // Device 2's onboarding replaces Alice's own, not-yet-virtual leaf, and
    // turns it into a virtual-client leaf.
    alice2.outbound_service().run_once().await;

    let alice3 = alice.add_linked_device().await;

    // Empty every queue, so what arrives next is exactly the fan-out of device
    // 3's onboarding commit: the resync is only enqueued at this point and runs
    // when device 3's outbound service is driven below.
    alice.drain().await;
    alice2.drain().await;
    alice3.drain().await;

    for (label, device) in [("1", &alice), ("2", &alice2), ("3", &alice3)] {
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
    alice3.outbound_service().run_once().await;
    for (label, device) in [("1", &alice), ("2", &alice2)] {
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
            alice3
                .resync_status(onboarded_chat_id)
                .await
                .unwrap()
                .is_none(),
            "device 3 should have completed onboarding into the {chat_label}"
        );
        let epoch_and_index = alice
            .group_epoch_and_own_index(onboarded_chat_id)
            .await
            .unwrap();
        for (label, device) in [("2", &alice2), ("3", &alice3)] {
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

    let chat_id = self_chat_id(&alice).await;
    send_and_receive(&alice, &[&alice2, &alice3], chat_id, "from device 1").await;
    send_and_receive(&alice3, &[&alice, &alice2], chat_id, "from device 3").await;

    // The higher-level group is usable from the newest device.
    send_and_receive(
        &alice3,
        &[&alice, &alice2],
        group_chat_id,
        "from device 3 in the group",
    )
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn multi_device_onboarding_after_self_group_advanced() {
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

// Linking a third device fans the add commit out to the existing non-initiating
// device, which must process the self-group add without desynchronizing.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn multi_device_third_device_link() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;

    let alice2 = alice.add_linked_device().await;
    // Bring the second device fully up to date before the third link.
    alice2.drain().await;

    let alice3 = alice.add_linked_device().await;

    // The second device processes the add commit for the third device.
    alice2.drain().await;

    // All three devices agree on the membership.
    alice.drain().await;
    for (device, name) in [
        (&alice, "old device"),
        (&alice2, "second device"),
        (&alice3, "third device"),
    ] {
        assert_eq!(
            device.self_group_member_count().await.unwrap(),
            Some(3),
            "{name} should see all three emulator clients in the self group"
        );
    }

    // The second device is still in sync: a message from the old device
    // round-trips.
    let chat_id = self_chat_id(&alice2).await;
    send_and_receive(&alice, &[&alice2], chat_id, "three devices").await;
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
async fn multi_device_self_update_in_self_group() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let alice2 = alice.add_linked_device().await;

    // Bring both devices to the self-group's latest epoch.
    alice.drain().await;
    alice2.drain().await;

    let chat_id = self_chat_id(&alice).await;

    // Force the joint APQ self-update (both T and PQ due).
    alice
        .set_self_updated_at(chat_id, DateTime::UNIX_EPOCH)
        .await
        .unwrap();
    alice
        .set_pq_self_updated_at(chat_id, DateTime::UNIX_EPOCH)
        .await
        .unwrap();
    let before_apq = Utc::now();
    alice
        .outbound_service()
        .schedule_self_update(DateTime::UNIX_EPOCH)
        .await
        .unwrap();
    alice.outbound_service().run_once().await;

    let after_t = alice.self_updated_at(chat_id).await.unwrap().unwrap();
    let after_pq = alice.pq_self_updated_at(chat_id).await.unwrap().unwrap();
    assert!(
        before_apq < after_t && before_apq < after_pq,
        "the DS should have accepted the APQ self-update in the self group"
    );

    // The sibling device processes the update commit.
    alice2.drain().await;

    // Force a pure T self-update (only T due).
    alice.drain().await;
    alice
        .set_self_updated_at(chat_id, DateTime::UNIX_EPOCH)
        .await
        .unwrap();
    let before_t = Utc::now();
    alice
        .outbound_service()
        .schedule_self_update(DateTime::UNIX_EPOCH)
        .await
        .unwrap();
    alice.outbound_service().run_once().await;

    let after_t = alice.self_updated_at(chat_id).await.unwrap().unwrap();
    assert!(
        before_t < after_t,
        "the DS should have accepted the T self-update in the self group"
    );

    alice2.drain().await;

    // Both devices are still in sync afterwards.
    send_and_receive(&alice, &[&alice2], chat_id, "after self-update").await;
    send_and_receive(&alice2, &[&alice], chat_id, "and back").await;
}

/// After linking, each device advertises itself and learns its sibling, so both
/// see the same two-device list. This is the property the Devices screen shows.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn multi_device_linked_devices_converge_on_both_devices() -> anyhow::Result<()> {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let alice2 = alice.add_linked_device().await;

    alice.drain().await;
    alice2.drain().await;

    let a_id = alice.own_client_id().await?;
    let b_id = alice2.own_client_id().await?;
    assert_ne!(a_id, b_id, "each device mints its own client id");

    // Neither entry needs a round trip of its own. The old device's entry
    // predates the self group, so it never became a pending change and rode
    // along in the provisioning snapshot. The new device's entry travelled the
    // other way in the join request, and the old device folded it into the add
    // commit itself, which is why nothing is left enqueued on either side (see
    // `multi_device_link_publishes_device_entry_without_extra_commit`).
    for user in [&alice, &alice2] {
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
    let alice2 = alice
        .link_ephemeral_device_named("Work laptop")
        .await
        .unwrap();

    alice.drain().await;
    alice2.drain().await;

    let b_id = alice2.own_client_id().await?;
    for (label, device) in [("old", &alice), ("new", &alice2)] {
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
    let alice2 = alice.link_ephemeral_device_named("   ").await.unwrap();

    alice.drain().await;
    alice2.drain().await;

    let b_id = alice2.own_client_id().await?;
    let seen = alice
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
    let alice2 = alice.add_linked_device().await;
    alice.drain().await;
    alice2.drain().await;

    let a_id = alice.own_client_id().await?;
    let b_id = alice2.own_client_id().await?;

    alice.unlink_device(b_id).await?;

    let members = alice.self_group_client_ids().await?;
    assert_eq!(members, vec![a_id], "only A must remain in the self group");

    // The removed device must be able to follow the commit that removed it,
    // which is what lets it notice and tear itself down.
    alice2.drain().await;

    Ok(())
}

/// The unlinked device notices on its next queue drain and flags itself. The
/// flag is what the app watches to delete its local data and return to the
/// welcome screen.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn multi_device_unlinked_device_flags_itself() -> anyhow::Result<()> {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let alice2 = alice.add_linked_device().await;
    alice.drain().await;
    alice2.drain().await;

    let b_id = alice2.own_client_id().await?;
    assert!(!alice2.is_account_unlinked().await?);

    alice.unlink_device(b_id).await?;
    alice2.drain().await;

    assert!(
        alice2.is_account_unlinked().await?,
        "the removed device must know it was unlinked"
    );
    assert!(
        !alice.is_account_unlinked().await?,
        "the remover must not flag itself"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn multi_device_unlink_unknown_client_id_is_an_error() -> anyhow::Result<()> {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let alice2 = alice.add_linked_device().await;
    alice.drain().await;
    alice2.drain().await;

    let error = alice
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
    let alice2 = alice.add_linked_device().await;
    alice.drain().await;
    alice2.drain().await;

    let a_id = alice.own_client_id().await?;
    let error = alice
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
    let alice2 = alice.add_linked_device().await;
    alice.drain().await;
    alice2.drain().await;

    let b_id = alice2.own_client_id().await?;
    alice2.rename_device(b_id, "Phone".to_owned()).await?;
    alice2.outbound_service().run_once().await;
    alice.drain().await;

    let seen = alice
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
async fn multi_device_remove_member_from_linked_device() -> anyhow::Result<()> {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    setup.connect_users(&alice, &bob).await;
    let charlie = setup.add_user().await;
    setup.connect_users(&alice, &charlie).await;

    let chat_id = setup.create_group(&alice).await;
    setup
        .invite_to_group(chat_id, &alice, &[&bob, &charlie])
        .await;

    let alice2 = alice.add_synced_device().await;

    alice2
        .remove_users(chat_id, vec![charlie.user_id().clone()])
        .await?;

    let members = alice2.mls_chat_participants(chat_id).await.unwrap();
    assert!(
        !members.contains(charlie.user_id()),
        "the linked device should no longer see the removed member, got {members:?}"
    );

    // The sibling emulator client follows the removal on the shared leaf.
    alice.drain().await;
    assert_eq!(
        alice.group_epoch_and_own_index(chat_id).await?,
        alice2.group_epoch_and_own_index(chat_id).await?,
        "both emulator clients must stay on the same epoch and shared leaf"
    );

    Ok(())
}

/// After the sibling rotates the user profile, the linked device must still be
/// able to run a group operation: those encrypt the own user profile key for
/// the commit, so the device has to keep knowing which key is its own.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn multi_device_remove_member_after_sibling_rotated_the_profile() -> anyhow::Result<()> {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    setup.connect_users(&alice, &bob).await;
    let charlie = setup.add_user().await;
    setup.connect_users(&alice, &charlie).await;

    let chat_id = setup.create_group(&alice).await;
    setup
        .invite_to_group(chat_id, &alice, &[&bob, &charlie])
        .await;

    let alice2 = alice.add_synced_device().await;

    // The sibling rotates the user profile key and announces it to every group.
    alice
        .set_own_user_profile(UserProfile {
            user_id: alice.user_id().clone(),
            display_name: "New Alice".parse().unwrap(),
            profile_picture: None,
        })
        .await?;

    // The linked device picks the new key up and fetches the profile.
    alice2.drain().await;
    alice2.outbound_service().run_once().await;

    alice2
        .remove_users(chat_id, vec![charlie.user_id().clone()])
        .await?;

    let members = alice2.mls_chat_participants(chat_id).await.unwrap();
    assert!(
        !members.contains(charlie.user_id()),
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
async fn multi_device_self_group_resync() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let alice2 = alice.add_linked_device().await;

    // Bring both devices to the self-group's latest epoch.
    alice.drain().await;
    alice2.drain().await;

    let chat_id = self_chat_id(&alice).await;
    let (epoch_before, _) = alice
        .group_epoch_and_own_index(chat_id)
        .await
        .unwrap()
        .unwrap();

    // The old device resyncs into the self group.
    alice.enqueue_group_resync(chat_id).await.unwrap();
    assert!(
        alice.resync_status(chat_id).await.unwrap().is_some(),
        "resync should be queued"
    );
    alice.outbound_service().run_once().await;
    assert!(
        alice.resync_status(chat_id).await.unwrap().is_none(),
        "resync should have completed"
    );

    // The external commit advanced the epoch, so the rejoin actually happened.
    let (epoch_after, _) = alice
        .group_epoch_and_own_index(chat_id)
        .await
        .unwrap()
        .unwrap();
    assert!(
        epoch_before < epoch_after,
        "the resync commit should have advanced the self-group epoch"
    );

    // The sibling device processes the rejoin commit.
    alice2.drain().await;

    // Both devices still see two distinct self-group clients and stay in sync.
    self_group_client_ids(&alice).await;
    self_group_client_ids(&alice2).await;
    send_and_receive(&alice, &[&alice2], chat_id, "after resync").await;
    send_and_receive(&alice2, &[&alice], chat_id, "and back").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn multi_device_inherits_connection_chats() {
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
async fn multi_device_skips_blocked_connection_chats() {
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

async fn one_unread_message_on_both_devices(
    alice: &CoreUser,
    bob: &CoreUser,
    chat_id: ChatId,
) -> CoreUser {
    let alice2 = alice.add_linked_device().await;
    // Device 2's onboarding turns Alice's leaf into a virtual-client leaf, which
    // is what fans a message out to both of her devices in the first place.
    alice2.outbound_service().run_once().await;

    alice.drain().await;
    alice2.drain().await;

    // Bob drains first, so he sends at the epoch device 2's onboarding left behind.
    bob.drain().await;
    bob.send_text(chat_id, "from bob").await;

    alice.drain().await;
    alice2.drain().await;
    assert_eq!(alice.unread_messages_count(chat_id).await, 1);
    assert_eq!(alice2.unread_messages_count(chat_id).await, 1);

    alice2
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn multi_device_read_markers_follow_a_sibling() {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    let chat_id = setup.connect_users(&alice, &bob).await;

    let alice2 = one_unread_message_on_both_devices(&alice, &bob, chat_id).await;

    // Device 1 reads the message and sends the receipt for it.
    let message = alice.expect_last_message(chat_id).await;
    let (marked, _) = alice
        .mark_chat_as_read(chat_id, message.id())
        .await
        .unwrap();
    assert!(marked);
    alice
        .outbound_service()
        .enqueue_receipts(
            chat_id,
            [(
                message.id(),
                message.message().mimi_id().unwrap(),
                MessageStatus::Read,
            )]
            .into_iter(),
        )
        .await
        .unwrap();
    alice.outbound_service().run_once().await;

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
async fn multi_device_read_markers_via_the_self_group() {
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

    // Device 1 reads the message. With read receipts off, the report goes to the
    // self chat instead of the chat with Bob (see `applogic::mark_as_read`).
    let message = alice.expect_last_message(chat_id).await;
    let (marked, _) = alice
        .mark_chat_as_read(chat_id, message.id())
        .await
        .unwrap();
    assert!(marked);
    alice
        .outbound_service()
        .enqueue_receipts(
            self_chat_id(&alice).await,
            [(
                message.id(),
                message.message().mimi_id().unwrap(),
                MessageStatus::Read,
            )]
            .into_iter(),
        )
        .await
        .unwrap();
    alice.outbound_service().run_once().await;

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
async fn multi_device_own_echo_confirms_unconfirmed_send() {
    const TEXT: &str = "the DS response to this one got lost";

    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    let chat_id = setup.connect_users(&alice, &bob).await;

    // Linking turns alice's leaf into a virtual-client leaf shared by both
    // devices, which is what makes the DS echo decryptable for the sender.
    let alice2 = alice.add_linked_device().await;
    alice2.outbound_service().run_once().await;

    alice.drain().await;
    alice2.drain().await;
    bob.drain().await;

    let content = MimiContent::simple_markdown_message(TEXT.to_owned(), [11u8; 16]);
    let message = alice
        .send_message(chat_id, content, None, MarkChatAsRead::Yes)
        .await
        .unwrap();
    let message_id = message.id();
    assert!(
        !message.is_sent(),
        "the message should still be waiting for the outbound service"
    );

    // The DS accepts the message and fans it out, but the sending device never
    // sees the response and therefore never confirms the send.
    alice
        .send_chat_message_without_confirmation(message_id)
        .await
        .unwrap();

    bob.drain().await;
    assert_eq!(
        bob.text_messages(chat_id)
            .await
            .iter()
            .filter(|rendered| *rendered == TEXT)
            .count(),
        1,
        "bob should have received the message once"
    );

    // The echo of our own message arrives.
    alice.drain().await;

    let stored = alice
        .message(message_id)
        .await
        .unwrap()
        .expect("the sent message should still be stored");
    assert_eq!(
        alice
            .text_messages(chat_id)
            .await
            .iter()
            .filter(|rendered| *rendered == TEXT)
            .count(),
        1,
        "the echo must not be stored as a second message"
    );
    assert!(
        stored.is_sent(),
        "the echo should have confirmed the unconfirmed send"
    );
    assert_eq!(
        alice.expect_last_message(chat_id).await.id(),
        message_id,
        "the echo must not become a newer message"
    );

    // The message left the queue with the confirmation, so there is nothing
    // left to send.
    alice.outbound_service().run_once().await;

    bob.drain().await;
    assert_eq!(
        bob.text_messages(chat_id)
            .await
            .iter()
            .filter(|rendered| *rendered == TEXT)
            .count(),
        1,
        "bob must not receive the message a second time"
    );

    // For the sibling this is a plain incoming message.
    alice2.drain().await;
    assert_eq!(
        alice2
            .text_messages(chat_id)
            .await
            .iter()
            .filter(|rendered| *rendered == TEXT)
            .count(),
        1,
        "the sibling device should have received the message once"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn multi_device_own_echo_confirms_unconfirmed_edit() {
    const TEXT: &str = "original text";
    const EDITED_TEXT: &str = "edited, the DS response to this one got lost";
    const FINAL_TEXT: &str = "edited a second time";

    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    let chat_id = setup.connect_users(&alice, &bob).await;

    // Linking turns alice's leaf into a virtual-client leaf shared by both
    // devices, which is what makes the DS echo decryptable for the sender.
    let alice2 = alice.add_linked_device().await;
    alice2.outbound_service().run_once().await;

    alice.drain().await;
    alice2.drain().await;
    bob.drain().await;

    // A regular, confirmed send of the original message.
    let message = alice.send_text(chat_id, TEXT).await;
    let message_id = message.id();
    alice.drain().await;
    alice2.drain().await;
    bob.drain().await;

    // The edit is stored and reaches the DS, but the sending device never
    // sees the response and therefore never confirms the send.
    let original = alice.expect_message(message_id).await;
    let edit = MimiContent::simple_markdown_message(EDITED_TEXT.to_owned(), [22u8; 16]);
    let edited = alice
        .send_message(chat_id, edit, Some(original), MarkChatAsRead::Yes)
        .await
        .unwrap();
    assert!(
        !edited.is_sent(),
        "the edit should still be waiting for the outbound service"
    );
    alice
        .send_chat_message_without_confirmation(message_id)
        .await
        .unwrap();

    // The echo of the edit arrives.
    alice.drain().await;

    let stored = alice.expect_message(message_id).await;
    assert!(
        stored.is_sent(),
        "the echo should have confirmed the unconfirmed edit"
    );
    assert_eq!(
        alice
            .text_messages(chat_id)
            .await
            .iter()
            .filter(|rendered| *rendered == EDITED_TEXT)
            .count(),
        1,
        "the echo must not be stored as a second message"
    );

    // The edit left the queue with the confirmation, so there is nothing left
    // to send.
    alice.outbound_service().run_once().await;

    bob.drain().await;
    assert_eq!(
        bob.text_messages(chat_id)
            .await
            .iter()
            .filter(|rendered| *rendered == EDITED_TEXT)
            .count(),
        1,
        "bob should see the edit applied once"
    );
    alice2.drain().await;
    assert_eq!(
        alice2
            .text_messages(chat_id)
            .await
            .iter()
            .filter(|rendered| *rendered == EDITED_TEXT)
            .count(),
        1,
        "the sibling device should see the edit applied once"
    );

    // Editing the message again still works. An echo that lands in the edit
    // history would block this edit with a conflict.
    let original = alice.expect_message(message_id).await;
    alice.send_edit(&original, FINAL_TEXT).await;

    bob.drain().await;
    assert_eq!(
        bob.text_messages(chat_id)
            .await
            .iter()
            .filter(|rendered| *rendered == FINAL_TEXT)
            .count(),
        1,
        "bob should see the second edit"
    );
}

/// Alice in a group with Bob and Charlie, plus a second linked device of Alice
/// that has onboarded itself into the group. Both of Alice's devices are
/// drained, so they sit on the same epoch of the shared leaf.
async fn setup_group_with_linked_device()
-> (TestBackend, CoreUser, CoreUser, CoreUser, ChatId, CoreUser) {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    setup.connect_users(&alice, &bob).await;
    let charlie = setup.add_user().await;
    setup.connect_users(&alice, &charlie).await;

    let chat_id = setup.create_group(&alice).await;
    setup
        .invite_to_group(chat_id, &alice, &[&bob, &charlie])
        .await;

    let alice2 = alice.add_synced_device().await;

    (setup, alice, bob, charlie, chat_id, alice2)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn multi_device_leave_before_own_creation_echo() {
    let (setup, alice, bob, _charlie, _chat_id, alice2) = setup_group_with_linked_device().await;

    // The linked device has no profile of bob, so the old device creates.
    let chat_id = alice
        .create_chat("left early".to_owned(), None, setup.apq_groups)
        .await
        .unwrap();
    alice
        .invite_users(chat_id, std::slice::from_ref(bob.user_id()))
        .await
        .unwrap();
    alice.leave_chat(chat_id).await.unwrap();

    // The creator still has its own creation echo queued.
    alice.drain().await;
    let past_members: HashSet<UserId> = [alice.user_id(), bob.user_id()]
        .into_iter()
        .cloned()
        .collect();
    assert_inactive(
        &alice,
        chat_id,
        Some(&past_members),
        "the echo must not revive the chat",
    )
    .await;
    assert!(alice.mls_chat_participants(chat_id).await.is_none());

    alice2.drain().await;
}

/// The client ids of every "device was linked" system message the device
/// stores for the chat.
async fn device_linked_client_ids(user: &CoreUser, chat_id: ChatId) -> Vec<Uuid> {
    user.messages(chat_id, 100)
        .await
        .unwrap()
        .iter()
        .filter_map(|message| match message.message() {
            Message::Event(EventMessage::System(SystemMessage::DeviceLinked(client_id))) => {
                Some(*client_id)
            }
            _ => None,
        })
        .collect()
}

/// How many "`user_id` left" system messages the device stores for the chat.
async fn count_self_removes(user: &CoreUser, chat_id: ChatId, user_id: &UserId) -> usize {
    user.messages(chat_id, 100)
        .await
        .unwrap()
        .iter()
        .filter(|message| match message.message() {
            Message::Event(EventMessage::System(SystemMessage::Remove(Some(remover), removed))) => {
                remover == user_id && removed == user_id
            }
            _ => false,
        })
        .count()
}

/// Asserts that the device's chat is inactive. With `past_members` given,
/// also asserts whom the chat remembers.
pub(crate) async fn assert_inactive(
    user: &CoreUser,
    chat_id: ChatId,
    past_members: Option<&HashSet<UserId>>,
    what: &str,
) {
    let chat = user.expect_chat(chat_id).await;
    match chat.status() {
        ChatStatus::Inactive(inactive) => {
            if let Some(past_members) = past_members {
                let remembered: HashSet<_> = inactive.past_members().iter().cloned().collect();
                assert_eq!(&remembered, past_members, "{what}: wrong past members");
            }
        }
        status => panic!("{what}: expected an inactive chat, got {status:?}"),
    }
}

/// `leaver` leaves the group while `sibling` shares the leaf. The sibling sees
/// the pending self-remove on the shared leaf, the leaver copes with the echo of
/// its own proposal, and both devices turn the chat inactive once bob commits.
async fn leave_and_follow(
    alice: &CoreUser,
    bob: &CoreUser,
    charlie: &CoreUser,
    chat_id: ChatId,
    leaver: &CoreUser,
    sibling: &CoreUser,
) {
    let all_members: HashSet<UserId> = [alice.user_id(), bob.user_id(), charlie.user_id()]
        .into_iter()
        .cloned()
        .collect();
    let remaining: HashSet<UserId> = [bob.user_id(), charlie.user_id()]
        .into_iter()
        .cloned()
        .collect();

    leaver.leave_chat(chat_id).await.unwrap();
    assert_inactive(
        leaver,
        chat_id,
        Some(&all_members),
        "the leaver turns the chat inactive right away",
    )
    .await;
    assert_eq!(
        count_self_removes(leaver, chat_id, alice.user_id()).await,
        1
    );
    assert!(
        leaver.mls_chat_participants(chat_id).await.is_none(),
        "the leaver drops the group with the leave"
    );

    // The DS fans the self-remove back to the shared leaf, so the sibling learns
    // about it from the queue, like any other member does.
    sibling.drain().await;
    assert_eq!(
        sibling.pending_removes(chat_id).await.unwrap(),
        vec![alice.user_id().clone()],
        "the sibling should hold the pending removal of its own leaf"
    );
    assert_eq!(
        count_self_removes(sibling, chat_id, alice.user_id()).await,
        1
    );

    // The leaver already holds the proposal, so the echo must be a no-op.
    leaver.drain().await;
    assert_eq!(
        count_self_removes(leaver, chat_id, alice.user_id()).await,
        1,
        "the echo of the own self-remove must not add a second system message"
    );

    // Another member commits the pending self-remove.
    bob.drain().await;
    assert_eq!(
        bob.pending_removes(chat_id).await.unwrap(),
        vec![alice.user_id().clone()]
    );
    bob.update_key(chat_id).await.unwrap();
    assert_eq!(bob.mls_chat_participants(chat_id).await.unwrap(), remaining);

    // Both of alice's devices follow the commit that removes their shared leaf.
    sibling.drain().await;
    assert_inactive(
        sibling,
        chat_id,
        Some(&all_members),
        "the sibling turns the chat inactive with the commit",
    )
    .await;
    leaver.drain().await;
    assert_inactive(
        leaver,
        chat_id,
        Some(&all_members),
        "the leaver keeps the chat inactive",
    )
    .await;
    for (device, what) in [(leaver, "leaver"), (sibling, "sibling")] {
        assert_eq!(
            count_self_removes(device, chat_id, alice.user_id()).await,
            1,
            "the {what} must record the leave exactly once"
        );
        assert!(
            device
                .pending_chat_operation_info(chat_id)
                .await
                .unwrap()
                .is_none(),
            "the {what} must not keep a pending operation for the left group"
        );
    }

    charlie.drain().await;
    assert_eq!(
        charlie.mls_chat_participants(chat_id).await.unwrap(),
        remaining
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn multi_device_leave_from_linked_device_is_followed_by_the_sibling() {
    let (_setup, alice, bob, charlie, chat_id, alice2) = setup_group_with_linked_device().await;

    leave_and_follow(&alice, &bob, &charlie, chat_id, &alice2, &alice).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn multi_device_leave_from_old_device_is_followed_by_the_linked_device() {
    let (_setup, alice, bob, charlie, chat_id, alice2) = setup_group_with_linked_device().await;

    leave_and_follow(&alice, &bob, &charlie, chat_id, &alice, &alice2).await;
}

/// A leave whose DS response got lost is retried with the same proposal. The
/// DS fans the retry out to the shared leaf again, and the sibling must treat
/// it as the duplicate it is.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn multi_device_retried_leave_is_ignored_by_the_sibling() {
    let (setup, alice, bob, charlie, chat_id, alice2) = setup_group_with_linked_device().await;
    let all_members: HashSet<UserId> = [alice.user_id(), bob.user_id(), charlie.user_id()]
        .into_iter()
        .cloned()
        .collect();

    setup.listener_control_handle().set_drop_next_response();
    alice.leave_chat(chat_id).await.unwrap();
    let pending = alice
        .pending_chat_operation_info(chat_id)
        .await
        .unwrap()
        .expect("the leave must stay pending after the dropped response");
    assert_eq!(pending.operation_type, "leave");
    assert_inactive(&alice, chat_id, Some(&all_members), "the leaver").await;
    assert!(
        alice.mls_chat_participants(chat_id).await.is_some(),
        "the leaver keeps the group for the retry"
    );

    // The DS did accept the proposal, so the sibling sees it once.
    alice2.drain().await;
    assert_eq!(
        count_self_removes(&alice2, chat_id, alice.user_id()).await,
        1
    );

    // The retry re-sends the same proposal.
    sleep(Duration::from_secs(2)).await;
    alice.drain().await;
    alice.outbound_service().run_once().await;
    assert!(
        alice
            .pending_chat_operation_info(chat_id)
            .await
            .unwrap()
            .is_none(),
        "the retry must complete the pending leave"
    );
    assert!(
        alice.mls_chat_participants(chat_id).await.is_none(),
        "the completed leave drops the group"
    );

    alice2.drain().await;
    assert_eq!(
        count_self_removes(&alice2, chat_id, alice.user_id()).await,
        1,
        "the retried self-remove must not be recorded twice"
    );
    assert_eq!(
        alice2.pending_removes(chat_id).await.unwrap(),
        vec![alice.user_id().clone()]
    );

    // Bob commits and both devices turn the chat inactive.
    bob.drain().await;
    bob.update_key(chat_id).await.unwrap();

    alice2.drain().await;
    assert_inactive(&alice2, chat_id, Some(&all_members), "the sibling").await;
    alice.drain().await;
    assert_inactive(&alice, chat_id, Some(&all_members), "the leaver").await;
    assert_eq!(
        count_self_removes(&alice2, chat_id, alice.user_id()).await,
        1
    );
    assert_eq!(
        count_self_removes(&alice, chat_id, alice.user_id()).await,
        1
    );
}

/// Both devices leave before anyone commits: the second leave proposes the
/// removal of a leaf that is already pending removal. Neither device may end up
/// with a duplicate record or a stuck pending operation, and the group must
/// still converge on the commit.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn multi_device_both_devices_leave_before_the_commit() {
    let (_setup, alice, bob, charlie, chat_id, alice2) = setup_group_with_linked_device().await;
    let all_members: HashSet<UserId> = [alice.user_id(), bob.user_id(), charlie.user_id()]
        .into_iter()
        .cloned()
        .collect();
    let remaining: HashSet<UserId> = [bob.user_id(), charlie.user_id()]
        .into_iter()
        .cloned()
        .collect();

    alice2.leave_chat(chat_id).await.unwrap();
    alice.drain().await;

    // The user leaves on the other device as well. The DS already holds the
    // leave of the shared leaf, so nothing is sent.
    alice.leave_chat(chat_id).await.unwrap();
    assert_inactive(&alice, chat_id, Some(&all_members), "the second leaver").await;

    alice2.drain().await;
    alice.drain().await;
    for (device, what) in [(&alice2, "first leaver"), (&alice, "second leaver")] {
        assert_eq!(
            count_self_removes(device, chat_id, alice.user_id()).await,
            1,
            "the {what} must record the leave exactly once"
        );
        assert!(
            device.mls_chat_participants(chat_id).await.is_none(),
            "the {what} must drop the group"
        );
    }

    bob.drain().await;
    assert_eq!(
        bob.pending_removes(chat_id).await.unwrap(),
        vec![alice.user_id().clone()],
        "bob must see the shared leaf pending removal once"
    );
    bob.update_key(chat_id).await.unwrap();
    assert_eq!(bob.mls_chat_participants(chat_id).await.unwrap(), remaining);

    for (device, what) in [(&alice2, "first leaver"), (&alice, "second leaver")] {
        device.drain().await;
        assert_inactive(device, chat_id, Some(&all_members), what).await;
        assert_eq!(
            count_self_removes(device, chat_id, alice.user_id()).await,
            1
        );
        assert!(
            device
                .pending_chat_operation_info(chat_id)
                .await
                .unwrap()
                .is_none(),
            "the {what} must not keep a pending operation"
        );
    }

    charlie.drain().await;
    assert_eq!(
        charlie.mls_chat_participants(chat_id).await.unwrap(),
        remaining
    );
}

/// Returns the next sibling client state change, skipping other events.
async fn next_sibling_client_state(
    stream: &mut (impl Stream<Item = Result<ListenResponse, tonic::Status>> + Unpin),
) -> sibling_client_state::Change {
    loop {
        let response = tokio::time::timeout(Duration::from_secs(5), stream.next())
            .await
            .expect("timeout waiting for sibling client state")
            .expect("stream ended")
            .expect("stream failed");
        if let Some(listen_response::Event::SiblingClientState(SiblingClientState {
            change: Some(change),
        })) = response.event
        {
            return change;
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn multi_device_sibling_client_state() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let alice2 = alice.add_linked_device().await;

    let (mut old_stream, old_responder) = alice.listen_queue().await.unwrap();
    let (mut new_stream, _new_responder) = alice2.listen_queue().await.unwrap();

    old_responder.report_client_state(b"state".to_vec()).await;
    let sibling_client_state::Change::Updated(updated) =
        next_sibling_client_state(&mut new_stream).await
    else {
        panic!("expected updated");
    };
    assert_eq!(updated.blob.as_ref().unwrap().encrypted_blob, b"state");
    let old_client_id = updated.client_id.unwrap();

    // A new session gets the current state right after the version status.
    let (mut new_stream, _new_responder) = alice2.listen_queue().await.unwrap();
    assert_matches!(
        new_stream.next().await,
        Some(Ok(ListenResponse {
            event: Some(listen_response::Event::VersionStatus(_)),
        }))
    );
    assert_matches!(
        new_stream.next().await,
        Some(Ok(ListenResponse {
            event: Some(listen_response::Event::SiblingClientState(SiblingClientState {
                change: Some(sibling_client_state::Change::Updated(snapshot)),
            })),
        })) if snapshot.client_id == updated.client_id
            && snapshot.epoch == updated.epoch
            && snapshot.blob == updated.blob
    );

    // Ending the session clears the state.
    old_responder.close(&mut old_stream).await;
    let sibling_client_state::Change::Removed(removed) =
        next_sibling_client_state(&mut new_stream).await
    else {
        panic!("expected removed");
    };
    assert_eq!(removed.client_id, Some(old_client_id));
    assert!(removed.epoch > updated.epoch);

    // Oversized states are ignored, the session goes on.
    let (_old_stream, old_responder) = alice.listen_queue().await.unwrap();
    old_responder.report_client_state(vec![0; 1024]).await;
    old_responder.report_client_state(b"after".to_vec()).await;
    let sibling_client_state::Change::Updated(updated) =
        next_sibling_client_state(&mut new_stream).await
    else {
        panic!("expected updated");
    };
    assert_eq!(updated.blob.unwrap().encrypted_blob, b"after");
}

fn client_db_files(db_path: &std::path::Path) -> Vec<String> {
    std::fs::read_dir(db_path)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .filter(|name| name.contains(".db") && !name.starts_with("air.db"))
        .collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn multi_device_cancelled_provisioning_leaves_no_client() -> anyhow::Result<()> {
    let setup = TestBackend::single().await;
    let tmp = TempDir::new()?;
    let db_path = tmp.path().to_str().unwrap().to_owned();

    let (session_tx, mut session_rx) = tokio::sync::mpsc::channel(1);
    let cancel = CancellationToken::new();
    let task = tokio::spawn({
        let domain = setup.domain().clone();
        let server_url = setup.server_url();
        let db_path = db_path.clone();
        let cancel = cancel.clone();
        async move { CoreUser::new_linked(&db_path, domain, Some(server_url), session_tx, cancel).await }
    });
    recv_session_id(&mut session_rx).await;

    cancel.cancel();
    assert!(task.await?.is_err(), "cancelled provisioning must fail");

    assert!(
        ClientRecord::load_all_from_air_db(&db_path)
            .await?
            .is_empty()
    );
    assert!(
        client_db_files(tmp.path()).is_empty(),
        "no client DB files may remain: {:?}",
        client_db_files(tmp.path())
    );
    Ok(())
}
