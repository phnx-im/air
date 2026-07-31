// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::identifiers::UserId;
use aircoreclient::{
    ChatId, Message, ReadReceiptsSetting,
    clients::{
        CoreUser,
        multi_device::{MultiDeviceLinkClientError, MultiDeviceProvisionStep},
    },
};
use airprotos::relay_service::v1::LinkingSessionId;
use airserver_test_harness::utils::setup::TestBackend;
use mimi_content::MimiContent;
use tempfile::TempDir;

/// Sends `text` from `sender` into the self-group chat and asserts that
/// `receiver` sees it after fetching + processing its queue.
async fn send_and_receive(sender: &CoreUser, linked: &CoreUser, chat_id: ChatId, text: &str) {
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
    let qs_messages = linked.qs_fetch_messages().await.unwrap();
    let processed = linked.fully_process_qs_messages(qs_messages).await;
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
        "self-group message should round-trip"
    );
}

/// A confirmation receiver that is already fulfilled, so the acceptor proceeds
/// without waiting for user confirmation in tests.
fn auto_confirm() -> tokio::sync::oneshot::Receiver<()> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    tx.send(()).unwrap();
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

/// Provisions a fresh device and links it to `user_id`'s existing device, returning the new device.
/// The [`TempDir`] holds the new device's database and must stay alive as long as the device is
/// used.
pub(crate) async fn link_new_device(setup: &TestBackend, user_id: &UserId) -> (CoreUser, TempDir) {
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
        .multi_device_link_client(session_id, ignore_connected(), auto_confirm())
        .await
        .unwrap()
        .unwrap();

    new_device_task.await.unwrap()
}

/// Fetches and processes all messages in the device's queue.
async fn drain_queue(user: &CoreUser) {
    let messages = user.qs_fetch_messages().await.unwrap();
    user.fully_process_qs_messages(messages).await;
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

    let (new_device, _tmp) = link_new_device(&setup, &alice).await;

    // The new device is bootstrapped as a second emulator of the same virtual
    // client: it shares the QsUserId and self-group, but has its own queue.
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

    // Both devices are now members of the self group.
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

    // Both devices surface the self group as a "Notes to self" chat.
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

    // Messages sent into the self group are seen by the other device, in both
    // directions.
    let self_chat_id = ChatId::try_from(old_device_self_group.group_id()).unwrap();
    send_and_receive(
        old_device,
        &new_device,
        self_chat_id,
        "hello from the old device",
    )
    .await;
    send_and_receive(
        &new_device,
        old_device,
        self_chat_id,
        "hello back from the new device",
    )
    .await;
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
    send_and_receive(old_device, &new_device, chat_id, "still in sync").await;
}
