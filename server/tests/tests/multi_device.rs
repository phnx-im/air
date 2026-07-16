// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircoreclient::{
    ChatId, Message,
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

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[tracing::instrument(name = "Test multi-device linking session", skip_all)]
async fn multi_device_linking_session() {
    let mut setup = TestBackend::single().await;
    let domain = setup.domain().clone();
    let server_url = setup.server_url();
    let alice = setup.add_user().await;

    let (session_tx, mut session_rx) = tokio::sync::mpsc::channel(1);

    let new_device_task = tokio::spawn(async move {
        // Fresh device: its own (temporary) database location.
        let tmp = TempDir::new().unwrap();
        let db_path = tmp.path().to_str().unwrap();
        let new_device =
            CoreUser::multi_device_provision_client(db_path, domain, Some(server_url), session_tx)
                .await
                .unwrap();
        // Keep `tmp` alive until the CoreUser is returned.
        (new_device, tmp)
    });

    let session_id = recv_session_id(&mut session_rx).await;

    // The old device scans/types the session ID and drives linking.
    setup
        .get_user(&alice)
        .user()
        .multi_device_link_client(session_id, ignore_connected(), auto_confirm())
        .await
        .unwrap()
        .unwrap();

    let (new_device, _tmp) = new_device_task.await.unwrap();

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
