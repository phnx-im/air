// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Multi-device: linking, sibling sync via the self group, and devices
//! following each other in groups.

mod account_deletion;
mod bootstrap;
mod deletion;
mod device_limit;
mod devices;
mod groups;
mod key_packages;
mod leave;
mod linking;
mod onboarding;
mod push_tokens;
mod read_state;
mod requests;
mod self_group;
mod settings;
mod tokens;
mod usernames;

use std::{assert_matches, collections::HashSet, time::Duration};

use aircommon::{credentials::LeafCredential, identifiers::UserId};
use aircoreclient::{
    ChatId, ChatStatus, EventMessage, Message, ReadReceiptsSetting, SystemMessage,
    clients::{
        CoreUser, ListenResponse, MarkChatAsRead, listen_response,
        multi_device::{
            MultiDeviceLinkClientError, MultiDeviceProvisionClientError, MultiDeviceProvisionStep,
        },
        store::ClientRecord,
    },
};
use airprotos::queue_service::v1::{SiblingClientState, sibling_client_state};
use airserver_test_harness::utils::{core_user_ext::CoreUserExt, setup::TestBackend};
use mimi_content::MimiContent;
use tempfile::TempDir;
use tokio_stream::{Stream, StreamExt};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

/// Links a second device to `user`, so that key packages are uploaded via
/// the self group, and brings both devices past the linking commit.
async fn link_sibling(user: &CoreUser) -> CoreUser {
    let sibling = user.add_linked_device().await;
    user.drain().await;
    sibling.drain().await;
    sibling
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

/// Receives the linking code from the first provisioning step. The receiver
/// must stay alive afterwards: the new device later sends a `Linking` step,
/// and dropping the receiver would make that send fail and abort
/// provisioning.
async fn recv_linking_code(
    rx: &mut tokio::sync::mpsc::Receiver<MultiDeviceProvisionStep>,
) -> String {
    match rx
        .recv()
        .await
        .expect("provision channel closed before the linking code")
    {
        MultiDeviceProvisionStep::Code(code) => code,
        MultiDeviceProvisionStep::Linking => panic!("unexpected Linking step before the code"),
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

fn client_db_files(db_path: &std::path::Path) -> Vec<String> {
    std::fs::read_dir(db_path)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .filter(|name| name.contains(".db") && !name.starts_with("air.db"))
        .collect()
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
    let session_id = recv_linking_code(&mut session_rx).await;

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
