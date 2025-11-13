// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{
    collections::{BTreeMap, HashMap, HashSet},
    net::SocketAddr,
    path::Path,
    time::Duration,
};

use aircommon::{
    OpenMlsRand, RustCrypto,
    identifiers::{Fqdn, UserHandle, UserId},
};
use aircoreclient::{ChatId, ChatStatus, ChatType, clients::CoreUser, store::Store, *};
use airserver::{RateLimitsConfig, network_provider::MockNetworkProvider};
use anyhow::Context;
use mimi_content::{
    ByteBuf, Disposition, MimiContent, NestedPart,
    content_container::{EncryptionAlgorithm, HashAlgorithm, NestedPartContent},
};
use rand::{Rng, RngCore, distributions::Alphanumeric, seq::IteratorRandom};
use rand_chacha::rand_core::OsRng;
use tempfile::TempDir;
use tokio::{
    task::{LocalEnterGuard, LocalSet},
    time::timeout,
};
use tokio_stream::StreamExt;
use tracing::info;

use crate::utils::spawn_app_with_rate_limits;

use super::TEST_RATE_LIMITS;

#[derive(Debug)]
pub struct TestUser {
    pub user: CoreUser,
    /// If this is an ephemeral user, this is None.
    pub db_dir: Option<String>,
    /// The user handle record of the user if a handle was added.
    pub user_handle_record: Option<UserHandleRecord>,
}

impl AsRef<CoreUser> for TestUser {
    fn as_ref(&self) -> &CoreUser {
        &self.user
    }
}

impl AsMut<CoreUser> for TestUser {
    fn as_mut(&mut self) -> &mut CoreUser {
        &mut self.user
    }
}

impl TestUser {
    pub async fn new(user_id: &UserId, listen: SocketAddr) -> Self {
        let user = Self::try_new(user_id, listen).await.unwrap();
        // Run outbound service to upload KeyPackages
        user.user.outbound_service().run_once().await;
        user
    }

    pub async fn try_new(user_id: &UserId, listen: SocketAddr) -> anyhow::Result<Self> {
        let server_url = format!("http://{listen}").parse().unwrap();

        let user = CoreUser::new_ephemeral(user_id.clone(), server_url, None).await?;

        Ok(Self {
            user,
            db_dir: None,
            user_handle_record: None,
        })
    }

    pub async fn new_persisted(user_id: &UserId, listen_addr: SocketAddr, db_dir: &str) -> Self {
        let server_url = format!("http://{listen_addr}").parse().unwrap();
        let user = CoreUser::new(user_id.clone(), server_url, db_dir, None)
            .await
            .unwrap();
        Self {
            user,
            db_dir: Some(db_dir.to_owned()),
            user_handle_record: None,
        }
    }

    pub fn user(&self) -> &CoreUser {
        &self.user
    }

    pub async fn add_user_handle(&mut self) -> anyhow::Result<UserHandleRecord> {
        if let Some(record) = self.user_handle_record.clone() {
            info!(user_id = ?self.user.user_id(), "User handle already exists");
            return Ok(record);
        }

        let user_id_str = format!("uuid-{:?}", self.user.user_id()).replace(['@', '.'], "-");
        let handle = UserHandle::new(user_id_str)?;
        info!(
            user_id = ?self.user.user_id(),
            handle = handle.plaintext(),
            "Adding handle to user"
        );
        let record = self
            .user
            .add_user_handle(handle)
            .await?
            .context("user handle is already in use")?;
        self.user_handle_record = Some(record.clone());

        Ok(record)
    }

    pub async fn fetch_and_process_qs_messages(&self) -> usize {
        let qs_messages = self.user.qs_fetch_messages().await.unwrap();
        let n = qs_messages.len();
        let processed_messages = self.user.fully_process_qs_messages(qs_messages).await;
        assert_eq!(processed_messages.processed, n);
        n
    }
}

enum TestKind {
    SingleBackend(String), // url of the single backend
}

pub struct TestBackend {
    pub users: HashMap<UserId, TestUser>,
    pub groups: HashMap<ChatId, HashSet<UserId>>,
    // This is what we feed to the test clients.
    listen_addr: SocketAddr,
    temp_dir: TempDir,
    _guard: Option<LocalEnterGuard>,
}

impl TestBackend {
    pub async fn single() -> Self {
        Self::single_with_rate_limits(TEST_RATE_LIMITS).await
    }

    pub async fn single_with_rate_limits(rate_limits: RateLimitsConfig) -> Self {
        let network_provider = MockNetworkProvider::new();
        let domain: Fqdn = "example.com".parse().unwrap();
        let local = LocalSet::new();
        let _guard = local.enter();
        let listen_addr =
            spawn_app_with_rate_limits(domain.clone(), network_provider, rate_limits).await;
        info!(%listen_addr, "spawned server");
        Self {
            users: HashMap::new(),
            groups: HashMap::new(),
            listen_addr,
            temp_dir: tempfile::tempdir().unwrap(),
            _guard: Some(_guard),
        }
    }

    pub fn listen_addr(&self) -> SocketAddr {
        self.listen_addr
    }

    pub fn temp_dir(&self) -> &Path {
        self.temp_dir.path()
    }

    pub async fn add_persisted_user(&mut self, user_id: &UserId) {
        let path = self.temp_dir.path().to_str().unwrap();
        info!(%path, ?user_id, "Creating persisted user");
        let user = TestUser::new_persisted(user_id, self.listen_addr, path).await;
        self.users.insert(user_id.clone(), user);
    }

    pub async fn add_user(&mut self, user_id: &UserId) {
        info!(?user_id, "Creating user");
        let user = TestUser::new(user_id, self.listen_addr).await;
        self.users.insert(user_id.clone(), user);
    }

    pub fn get_user(&self, user_id: &UserId) -> &TestUser {
        self.users.get(user_id).unwrap()
    }

    pub fn get_user_mut(&mut self, user_id: &UserId) -> &mut TestUser {
        self.users.get_mut(user_id).unwrap()
    }

    pub fn take_user(&mut self, user_id: &UserId) -> TestUser {
        self.users.remove(user_id).unwrap()
    }

    pub async fn delete_user(&mut self, user_id: &UserId) {
        let test_user = self.take_user(user_id);
        match test_user.db_dir {
            Some(db_dir) => test_user.user.delete(db_dir.as_str()).await.unwrap(),
            None => test_user.user.delete_ephemeral().await.unwrap(),
        }
    }

    /// This has the updater commit an update, but without the checks ensuring
    /// that the group state remains unchanged.
    pub async fn commit_to_proposals(&mut self, chat_id: ChatId, updater_id: UserId) {
        info!(
            "{updater_id:?} performs an update in group {}",
            chat_id.uuid()
        );

        let test_updater = self.users.get_mut(&updater_id).unwrap();
        let updater = &mut test_updater.user;

        let pending_removes = HashSet::from_iter(updater.pending_removes(chat_id).await.unwrap());
        let group_members_before = updater.mls_chat_participants(chat_id).await.unwrap();

        updater.update_key(chat_id).await.unwrap();

        let group_members_after = updater.mls_chat_participants(chat_id).await.unwrap();
        let difference: HashSet<UserId> = group_members_before
            .difference(&group_members_after)
            .map(|s| s.to_owned())
            .collect();
        assert_eq!(difference, pending_removes);

        let group_members = self.groups.get(&chat_id).unwrap();
        // Have all group members fetch and process messages.
        for group_member_id in group_members.iter() {
            // skip the sender
            if group_member_id == &updater_id {
                continue;
            }
            let test_group_member = self.users.get_mut(group_member_id).unwrap();
            let group_member = &mut test_group_member.user;
            let qs_messages = group_member.qs_fetch_messages().await.unwrap();

            let pending_removes =
                HashSet::from_iter(group_member.pending_removes(chat_id).await.unwrap());
            let group_members_before = group_member.mls_chat_participants(chat_id).await.unwrap();

            group_member.fully_process_qs_messages(qs_messages).await;

            // If the group member in question is removed with this commit,
            // it should turn its chat inactive ...
            if pending_removes.contains(group_member_id) {
                let chat_after = group_member.chat(&chat_id).await.unwrap();
                assert!(matches!(&chat_after.status(),
                ChatStatus::Inactive(ic)
                if HashSet::from_iter(ic.past_members().to_vec()) ==
                    group_members_before
                ));
            } else {
                // ... if not, it should remove the members to be removed.
                let group_members_after =
                    group_member.mls_chat_participants(chat_id).await.unwrap();
                let difference: HashSet<UserId> = group_members_before
                    .difference(&group_members_after)
                    .cloned()
                    .collect();
                assert_eq!(difference, pending_removes);
            }
        }
    }

    pub async fn update_group(&mut self, chat_id: ChatId, updater_id: &UserId) {
        info!(
            "{updater_id:?} performs an update in group {}",
            chat_id.uuid()
        );

        let test_updater = self.users.get_mut(updater_id).unwrap();
        let updater = &mut test_updater.user;

        updater.update_key(chat_id).await.unwrap();

        let group_members = self.groups.get(&chat_id).unwrap();
        // Have all group members fetch and process messages.
        for group_member_id in group_members.iter() {
            // skip the sender
            if group_member_id == updater_id {
                continue;
            }
            let test_group_member = self.users.get_mut(group_member_id).unwrap();
            let group_member = &mut test_group_member.user;
            let group_members_before = group_member.chat_participants(chat_id).await.unwrap();

            let qs_messages = group_member.qs_fetch_messages().await.unwrap();

            group_member.fully_process_qs_messages(qs_messages).await;

            let group_members_after = group_member.chat_participants(chat_id).await.unwrap();
            assert_eq!(group_members_after, group_members_before);
        }
    }

    pub async fn connect_users(&mut self, user1_id: &UserId, user2_id: &UserId) -> ChatId {
        info!("Connecting users {user1_id:?} and {user2_id:?}");

        let test_user2 = self.users.get_mut(user2_id).unwrap();
        let user2_handle_record = test_user2.add_user_handle().await.unwrap();
        let user2_handle = &user2_handle_record.handle;

        let test_user1 = self.users.get_mut(user1_id).unwrap();
        let user1 = &mut test_user1.user;
        let user1_profile = user1.own_user_profile().await.unwrap();
        let user1_handle_contacts_before = user1.handle_contacts().await.unwrap();
        let user1_chats_before = user1.chats().await.unwrap();
        user1.add_contact(user2_handle.clone()).await.unwrap();
        let mut user1_handle_contacts_after = user1.handle_contacts().await.unwrap();
        let error_msg = format!(
            "User 2 should be in the handle contacts list of user 1. List: {user1_handle_contacts_after:?}",
        );
        let new_user_position = user1_handle_contacts_after
            .iter()
            .position(|c| &c.handle == user2_handle)
            .expect(&error_msg);
        // If we remove the new user, the handle contact lists should be the same.
        user1_handle_contacts_after.remove(new_user_position);
        user1_handle_contacts_before
            .into_iter()
            .zip(user1_handle_contacts_after)
            .for_each(|(before, after)| {
                assert_eq!(before.handle, after.handle);
            });
        let mut user1_chats_after = user1.chats().await.unwrap();
        let test_title = format!("Connection group: {}", user2_handle.plaintext());
        let new_chat_position = user1_chats_after
            .iter()
            .position(|c| c.attributes().title() == test_title)
            .expect("User 1 should have created a new chat");
        let chat = user1_chats_after.remove(new_chat_position);
        assert!(chat.status() == &ChatStatus::Active);
        assert!(chat.chat_type() == &ChatType::HandleConnection(user2_handle.clone()));
        user1_chats_before
            .into_iter()
            .zip(user1_chats_after)
            .for_each(|(before, after)| {
                assert_eq!(before.id(), after.id());
            });
        let user1_chat_id = chat.id();

        let test_user2 = self.users.get_mut(user2_id).unwrap();
        let user2 = &mut test_user2.user;
        let user2_contacts_before = user2.contacts().await.unwrap();
        let user2_chats_before = user2.chats().await.unwrap();
        info!("{user2_id:?} fetches and process AS handle messages");
        let (mut stream, responder) = user2.listen_handle(&user2_handle_record).await.unwrap();
        while let Some(Some(message)) = timeout(Duration::from_millis(500), stream.next())
            .await
            .unwrap()
        {
            let message_id = message.message_id.unwrap();
            user2
                .process_handle_queue_message(&user2_handle_record.handle, message)
                .await
                .unwrap();
            responder.ack(message_id.into()).await;
        }

        // User 2 should have auto-accepted (for now at least) the connection request.
        let mut user2_contacts_after = user2.contacts().await.unwrap();
        info!("User 2 contacts after: {:?}", user2_contacts_after);
        let user2_handle_contacts_before = user2.handle_contacts().await.unwrap();
        info!(
            "User 2 handle contacts after: {:?}",
            user2_handle_contacts_before
        );
        let new_contact_position = user2_contacts_after
            .iter()
            .position(|c| &c.user_id == user1_id)
            .expect("User 1 should be in the handle contacts list of user 2");
        // If we remove the new user, the handle contact lists should be the same.
        user2_contacts_after.remove(new_contact_position);
        user2_contacts_before
            .into_iter()
            .zip(user2_contacts_after)
            .for_each(|(before, after)| {
                assert_eq!(before.user_id, after.user_id);
            });
        // User 2 should have created a connection group.
        let mut user2_chats_after = user2.chats().await.unwrap();
        info!("User 2 chats after: {:?}", user2_chats_after);
        let new_chat_position = user2_chats_after
            .iter()
            .position(|c| {
                c.attributes().title() == user1_profile.display_name.clone().into_string()
            })
            .expect("User 2 should have created a new chat");
        let chat = user2_chats_after.remove(new_chat_position);
        assert!(chat.status() == &ChatStatus::Active);
        assert!(chat.chat_type() == &ChatType::Connection(user1_id.clone()));
        user2_chats_before
            .into_iter()
            .zip(user2_chats_after)
            .for_each(|(before, after)| {
                assert_eq!(before.id(), after.id());
            });
        let user2_chat_id = chat.id();

        let user2_id = user2.user_id().clone();
        let test_user1 = self.users.get_mut(user1_id).unwrap();
        let user1 = &mut test_user1.user;
        let user1_contacts_before: HashSet<_> = user1
            .contacts()
            .await
            .unwrap()
            .into_iter()
            .map(|contact| contact.user_id.clone())
            .collect();
        let user1_chats_before = user1.chats().await.unwrap();
        info!("{user1_id:?} fetches QS messages");
        let qs_messages = user1.qs_fetch_messages().await.unwrap();
        info!("{user1_id:?} processes QS messages");
        user1.fully_process_qs_messages(qs_messages).await;

        // User 1 should have added user 2 to its contacts now and a connection
        // group should have been created.
        let user1_contacts_after: HashSet<_> = user1
            .contacts()
            .await
            .unwrap()
            .into_iter()
            .map(|contact| contact.user_id.clone())
            .collect();
        let new_user_vec: Vec<_> = user1_contacts_after
            .difference(&user1_contacts_before)
            .collect();
        assert_eq!(new_user_vec, vec![&user2_id]);
        // User 2 should have created a connection group.
        let mut user1_chats_after = user1.chats().await.unwrap();
        let new_chat_position = user1_chats_after
            .iter()
            .position(|c| c.attributes().title() == test_title)
            .expect("User 1 should have created a new chat");
        let chat = user1_chats_after.remove(new_chat_position);
        assert!(chat.status() == &ChatStatus::Active);
        assert!(chat.chat_type() == &ChatType::Connection(user2_id.clone()));
        let ids_before: HashSet<_> = user1_chats_before.iter().map(|c| c.id()).collect();
        let ids_after: HashSet<_> = user1_chats_after.iter().map(|c| c.id()).collect();
        assert!(ids_before.is_superset(&ids_after));
        debug_assert_eq!(user1_chat_id, user2_chat_id);

        let user1_unread_messages = self
            .users
            .get_mut(user1_id)
            .unwrap()
            .user
            .unread_messages_count(user1_chat_id)
            .await;
        assert_eq!(user1_unread_messages, 0);

        // Send messages both ways to ensure it works.
        self.send_message(user1_chat_id, user1_id, vec![&user2_id])
            .await;

        let user1_unread_messages = self
            .users
            .get_mut(user1_id)
            .unwrap()
            .user
            .unread_messages_count(user1_chat_id)
            .await;
        assert_eq!(user1_unread_messages, 0);

        self.send_message(user1_chat_id, &user2_id, vec![user1_id])
            .await;

        let user1_unread_messages = self
            .users
            .get_mut(user1_id)
            .unwrap()
            .user
            .unread_messages_count(user1_chat_id)
            .await;
        assert_eq!(user1_unread_messages, 1);

        // Fetch the last message and mark it as read.
        let test_user1 = self.users.get_mut(user1_id).unwrap();
        let user1 = &mut test_user1.user;
        let user1_messages = user1.messages(user1_chat_id, 1).await.unwrap();

        assert_eq!(user1_messages.len(), 1);
        let user1_unread_messages = user1.unread_messages_count(user1_chat_id).await;
        assert_eq!(user1_unread_messages, 1);

        let last_message = user1_messages.last().unwrap();

        user1
            .mark_as_read([(user1_chat_id, last_message.timestamp())].into_iter())
            .await
            .unwrap();

        let user1_unread_messages = user1.unread_messages_count(user1_chat_id).await;
        assert_eq!(user1_unread_messages, 0);

        let test_user2 = self.users.get_mut(&user2_id).unwrap();
        let user2 = &mut test_user2.user;
        let user2_messages = user2.messages(user2_chat_id, 1).await.unwrap();

        assert_eq!(user2_messages.len(), 1);
        let last_message = user2_messages.last().unwrap();
        user2
            .mark_as_read([(user2_chat_id, last_message.timestamp())].into_iter())
            .await
            .unwrap();

        let user2_unread_messages = user2.unread_messages_count(user2_chat_id).await;
        assert_eq!(user2_unread_messages, 0);

        let member_set: HashSet<UserId> = [user1_id.clone(), user2_id.clone()].into();
        assert_eq!(member_set.len(), 2);
        self.groups.insert(user1_chat_id, member_set);
        user1_chat_id
    }

    /// Sends a message from the given sender to the given recipients. Before
    /// sending a message, the sender picks up its QS messages to make sure it's
    /// up to date.
    pub async fn send_message(
        &mut self,
        chat_id: ChatId,
        sender_id: &UserId,
        recipients: Vec<&UserId>,
    ) -> MessageId {
        let recipient_strings = recipients
            .iter()
            .map(|n| format!("{n:?}"))
            .collect::<Vec<_>>();
        info!(
            "{sender_id:?} sends a message to {}",
            recipient_strings.join(", ")
        );
        let message: String = OsRng
            .sample_iter(&Alphanumeric)
            .take(32)
            .map(char::from)
            .collect();
        let salt: [u8; 16] = RustCrypto::default().random_array().unwrap();
        let orig_message = MimiContent::simple_markdown_message(message, salt);
        let test_sender = self.users.get_mut(sender_id).unwrap();
        let sender = &mut test_sender.user;

        // Before sending a message, the sender must first fetch and process its QS messages.

        let sender_qs_messages = sender.qs_fetch_messages().await.unwrap();

        sender.fully_process_qs_messages(sender_qs_messages).await;

        sender
            .send_message(chat_id, orig_message.clone(), None)
            .await
            .unwrap();
        sender.outbound_service().run_once().await;
        let sender_user_id = test_sender.user.user_id().clone();

        let chat = test_sender.user.chat(&chat_id).await.unwrap();
        let group_id = chat.group_id();

        // Reload message, because we sent it in the background.
        let message = test_sender
            .user
            .last_message(chat_id)
            .await
            .unwrap()
            .unwrap();

        assert_eq!(
            message.message(),
            &Message::Content(Box::new(ContentMessage::new(
                test_sender.user.user_id().clone(),
                true,
                orig_message.clone(),
                group_id,
            )))
        );

        for recipient_id in &recipients {
            let recipient = self.users.get_mut(recipient_id).unwrap();
            let recipient_user = &mut recipient.user;

            let recipient_qs_messages = recipient_user.qs_fetch_messages().await.unwrap();

            let messages = recipient_user
                .fully_process_qs_messages(recipient_qs_messages)
                .await;
            // Send out delivery receipts
            recipient_user.outbound_service().run_once().await;

            let message = messages.new_messages.last().unwrap();
            let chat = recipient_user.chat(&message.chat_id()).await.unwrap();
            let group_id = chat.group_id();

            assert_eq!(
                message.message(),
                &Message::Content(Box::new(ContentMessage::new(
                    sender_user_id.clone(),
                    true,
                    orig_message.clone(),
                    group_id
                )))
            );
        }

        // Fetch and process delivery receipts
        let sender = self.users.get_mut(sender_id).unwrap().user.clone();
        let delivery_receipts = sender.qs_fetch_messages().await.unwrap();
        sender.fully_process_qs_messages(delivery_receipts).await;

        message.id()
    }

    pub async fn edit_message(
        &mut self,
        chat_id: ChatId,
        sender_id: &UserId,
        recipients: Vec<&UserId>,
    ) -> MessageId {
        let test_sender = self.users.get_mut(sender_id).unwrap();
        let sender = &mut test_sender.user;

        let last_messages = sender.messages(chat_id, 1).await.unwrap();
        assert_eq!(last_messages.len(), 1);
        let last_message = &last_messages[0];

        let salt: [u8; 16] = RustCrypto::default().random_array().unwrap();
        let mut orig_message = MimiContent::simple_markdown_message("edited".to_owned(), salt);
        orig_message.replaces = Some(ByteBuf::from(
            last_message.message().mimi_id().unwrap().as_slice(),
        ));

        // Before sending a message, the sender must first fetch and process its QS messages.

        let sender_qs_messages = sender.qs_fetch_messages().await.unwrap();

        sender.fully_process_qs_messages(sender_qs_messages).await;

        test_sender
            .user
            .send_message(chat_id, orig_message.clone(), Some(last_message.id()))
            .await
            .unwrap();
        test_sender.user.outbound_service().run_once().await;
        let message = test_sender
            .user
            .last_message(chat_id)
            .await
            .unwrap()
            .unwrap();
        let sender_user_id = test_sender.user.user_id().clone();

        let chat = test_sender.user.chat(&chat_id).await.unwrap();
        let group_id = chat.group_id();

        let mut target_message = ChatMessage::new_for_test(
            chat_id,
            last_message.id(),
            last_message.timestamp().into(),
            Message::Content(Box::new(ContentMessage::new(
                sender_user_id.clone(),
                true,
                orig_message.clone(),
                group_id,
            ))),
        );
        target_message.set_edited_at(message.edited_at().unwrap());

        for recipient_id in &recipients {
            let recipient = self.users.get_mut(recipient_id).unwrap();
            let recipient_user = &mut recipient.user;

            let recipient_qs_messages = recipient_user.qs_fetch_messages().await.unwrap();

            let messages = recipient_user
                .fully_process_qs_messages(recipient_qs_messages)
                .await;

            let message = messages.new_messages.last().unwrap();
            let chat = recipient_user.chat(&message.chat_id()).await.unwrap();
            let _group_id = chat.group_id();

            assert_eq!(message.message(), target_message.message());
        }
        message.id()
    }

    pub async fn delete_message(
        &mut self,
        chat_id: ChatId,
        sender_id: &UserId,
        recipients: Vec<&UserId>,
    ) -> MessageId {
        let test_sender = self.users.get_mut(sender_id).unwrap();
        let sender = &mut test_sender.user;

        let last_messages = sender.messages(chat_id, 1).await.unwrap();
        assert_eq!(last_messages.len(), 1);
        let last_message = &last_messages[0];

        let salt: [u8; 16] = RustCrypto::default().random_array().unwrap();
        let orig_message = MimiContent {
            salt: ByteBuf::from(salt),
            replaces: Some(ByteBuf::from(
                last_message.message().mimi_id().unwrap().as_slice(),
            )),
            topic_id: ByteBuf::from(b""),
            expires: None,
            in_reply_to: None,
            extensions: BTreeMap::new(),
            nested_part: NestedPart {
                disposition: Disposition::Render,
                language: "".to_owned(),
                part: NestedPartContent::NullPart,
            },
        };

        // Before sending a message, the sender must first fetch and process its QS messages.

        let sender_qs_messages = sender.qs_fetch_messages().await.unwrap();

        sender.fully_process_qs_messages(sender_qs_messages).await;

        test_sender
            .user
            .send_message(chat_id, orig_message.clone(), Some(last_message.id()))
            .await
            .unwrap();
        test_sender.user.outbound_service().run_once().await;
        let message = test_sender
            .user
            .last_message(chat_id)
            .await
            .unwrap()
            .unwrap();

        let sender_user_id = test_sender.user.user_id().clone();

        let chat = test_sender.user.chat(&chat_id).await.unwrap();

        let group_id = chat.group_id();

        let mut target_message = ChatMessage::new_for_test(
            chat_id,
            last_message.id(),
            last_message.timestamp().into(),
            Message::Content(Box::new(ContentMessage::new(
                sender_user_id.clone(),
                true,
                orig_message.clone(),
                group_id,
            ))),
        );
        target_message.set_edited_at(message.edited_at().unwrap());

        for recipient_id in &recipients {
            let recipient = self.users.get_mut(recipient_id).unwrap();
            let recipient_user = &mut recipient.user;

            let recipient_qs_messages = recipient_user.qs_fetch_messages().await.unwrap();

            let messages = recipient_user
                .fully_process_qs_messages(recipient_qs_messages)
                .await;

            let message = messages.new_messages.last().unwrap();
            let chat = recipient_user.chat(&message.chat_id()).await.unwrap();
            let _group_id = chat.group_id();

            assert_eq!(message.message(), target_message.message());
        }
        message.id()
    }

    pub async fn send_attachment(
        &mut self,
        chat_id: ChatId,
        sender_id: &UserId,
        recipients: Vec<&UserId>,
        attachment: &[u8],
        filename: &str,
    ) -> (MessageId, NestedPartContent) {
        let recipient_strings = recipients
            .iter()
            .map(|n| format!("{n:?}"))
            .collect::<Vec<_>>();
        info!(
            "{sender_id:?} sends a message to {}",
            recipient_strings.join(", ")
        );

        let test_sender = self.users.get_mut(sender_id).unwrap();
        let sender = &mut test_sender.user;

        // Before sending a message, the sender must first fetch and process its QS messages.
        let sender_qs_messages = sender.qs_fetch_messages().await.unwrap();
        sender.fully_process_qs_messages(sender_qs_messages).await;

        let tmp_dir = TempDir::new().unwrap();
        let path = tmp_dir.path().join(filename);
        std::fs::write(&path, attachment).unwrap();

        let message = sender.upload_attachment(chat_id, &path).await.unwrap();
        sender.outbound_service().run_once().await;

        let mut external_part = None;
        message
            .message()
            .mimi_content()
            .unwrap()
            .visit_attachments(|part| {
                assert!(external_part.replace(part.clone()).is_none());
                Ok(())
            })
            .unwrap();
        let external_part = external_part.unwrap();
        match &external_part {
            NestedPartContent::ExternalPart {
                enc_alg,
                key,
                nonce,
                hash_alg,
                ..
            } => {
                assert_eq!(*enc_alg, EncryptionAlgorithm::Aes256Gcm12);
                assert_eq!(nonce.len(), 12);
                assert_eq!(key.len(), 32);
                assert_eq!(*hash_alg, HashAlgorithm::Sha256);
            }
            _ => panic!("unexpected attachment type"),
        };

        for recipient_id in &recipients {
            let recipient = self.users.get_mut(recipient_id).unwrap();
            let recipient_user = &mut recipient.user;

            let recipient_qs_messages = recipient_user.qs_fetch_messages().await.unwrap();
            let messages = recipient_user
                .fully_process_qs_messages(recipient_qs_messages)
                .await;

            let mut attachment_found_once = false;
            messages
                .new_messages
                .last()
                .unwrap()
                .message()
                .mimi_content()
                .unwrap()
                .visit_attachments(|part| {
                    assert!(!attachment_found_once);

                    // Removed cleared fields from the expected attachment.
                    let mut expected = external_part.clone();
                    if let NestedPartContent::ExternalPart {
                        key,
                        nonce,
                        aad,
                        content_hash,
                        ..
                    } = &mut expected
                    {
                        key.clear();
                        nonce.clear();
                        aad.clear();
                        content_hash.clear();
                    } else {
                        panic!("Unexpected attachment type")
                    }

                    assert_eq!(part, &expected);
                    attachment_found_once = true;
                    Ok(())
                })
                .unwrap();
        }

        (message.id(), external_part)
    }

    /// This function goes through all tables of the database and returns all columns that contain the query.
    pub async fn scan_database(
        &mut self,
        query: &str,
        strict: bool,
        users: Vec<&UserId>,
    ) -> Vec<String> {
        let mut result = Vec::new();
        for user_id in &users {
            let user = self.users.get_mut(user_id).unwrap();
            let user = &mut user.user;

            result.append(&mut user.scan_database(query, strict).await.unwrap());
        }

        result
    }

    pub async fn create_group(&mut self, user_id: &UserId) -> ChatId {
        let test_user = self.users.get_mut(user_id).unwrap();
        let user = &mut test_user.user;
        let user_chats_before = user.chats().await.unwrap();

        let group_name = format!("{:?}", OsRng.r#gen::<[u8; 32]>());
        let group_picture_bytes_option = Some(OsRng.r#gen::<[u8; 32]>().to_vec());
        let chat_id = user
            .create_chat(group_name.clone(), group_picture_bytes_option.clone())
            .await
            .unwrap();
        let mut user_chats_after = user.chats().await.unwrap();
        let new_chat_position = user_chats_after
            .iter()
            .position(|c| c.attributes().title() == group_name)
            .expect("User 1 should have created a new chat");
        let chat = user_chats_after.remove(new_chat_position);
        assert!(chat.id() == chat_id);
        assert!(chat.status() == &ChatStatus::Active);
        assert!(chat.chat_type() == &ChatType::Group);
        assert_eq!(chat.attributes().title(), &group_name);
        assert_eq!(
            chat.attributes().picture(),
            group_picture_bytes_option.as_deref()
        );
        user_chats_before
            .into_iter()
            .zip(user_chats_after)
            .for_each(|(before, after)| {
                assert_eq!(before.id(), after.id());
            });
        let member_set: HashSet<UserId> = [user_id.clone()].into();
        assert_eq!(member_set.len(), 1);
        self.groups.insert(chat_id, member_set);

        chat_id
    }

    /// Has the inviter invite the invitees to the given group and has everyone
    /// send and process their messages.
    pub async fn invite_to_group(
        &mut self,
        chat_id: ChatId,
        inviter_id: &UserId,
        invitees: Vec<&UserId>,
    ) {
        let invitee_strings = invitees
            .iter()
            .map(|n| format!("{n:?}"))
            .collect::<Vec<_>>();
        let test_inviter = self.users.get_mut(inviter_id).unwrap();
        let inviter = &mut test_inviter.user;

        // Before inviting anyone to a group, the inviter must first fetch and
        // process its QS messages.
        let qs_messages = inviter.qs_fetch_messages().await.unwrap();

        inviter.fully_process_qs_messages(qs_messages).await;
        let inviter_chat = inviter.chat(&chat_id).await.unwrap();

        info!(
            "{inviter_id:?} invites {} to the group with id {}",
            invitee_strings.join(", "),
            chat_id.uuid()
        );

        // Perform the invite operation and check that the invitees are now in the group.
        let inviter_group_members_before = inviter
            .chat_participants(chat_id)
            .await
            .expect("Error getting group members.");

        let invite_messages = inviter
            .invite_users(
                chat_id,
                &invitees.iter().cloned().cloned().collect::<Vec<_>>(),
            )
            .await
            .expect("Error inviting users.");

        let mut expected_messages = HashSet::new();
        for invitee_id in &invitees {
            let expected_message = format!("{inviter_id:?} added {invitee_id:?} to the chat");
            expected_messages.insert(expected_message);
        }

        let invite_messages = display_messages_to_string_map(invite_messages);

        assert_eq!(invite_messages, expected_messages);

        let inviter_group_members_after = inviter
            .chat_participants(chat_id)
            .await
            .expect("Error getting group members.");
        let new_members = inviter_group_members_after
            .difference(&inviter_group_members_before)
            .collect::<HashSet<_>>();
        let invitee_set = invitees.iter().copied().collect::<HashSet<_>>();
        assert_eq!(new_members, invitee_set);

        // Now that the invitation is out, have the invitees and all other group
        // members fetch and process QS messages.
        for invitee_id in &invitees {
            let test_invitee = self.users.get_mut(invitee_id).unwrap();
            let invitee = &mut test_invitee.user;
            let mut invitee_chats_before = invitee.chats().await.unwrap();

            let qs_messages = invitee.qs_fetch_messages().await.unwrap();

            let res = invitee.fully_process_qs_messages(qs_messages).await;
            assert!(
                res.errors.is_empty(),
                "Errors processing QS messages for {invitee_id:?}: {:?}",
                res.errors
            );

            let mut invitee_chats_after = invitee.chats().await.unwrap();
            let chat_uuid = chat_id.uuid();
            let new_chat_position = invitee_chats_after
                .iter()
                .position(|c| c.id() == chat_id)
                .unwrap_or_else(|| {
                    panic!("{invitee_id:?} should have created a new chat {chat_uuid}")
                });
            let chat = invitee_chats_after.remove(new_chat_position);
            assert!(chat.id() == chat_id);
            assert!(chat.status() == &ChatStatus::Active);
            assert!(chat.chat_type() == &ChatType::Group);
            assert_eq!(chat.attributes().title(), inviter_chat.attributes().title());
            assert_eq!(
                chat.attributes().picture(),
                inviter_chat.attributes().picture()
            );
            // In case it was a re-join, we remove it from the chat list before as well.
            if let Some(inactive_chat_position) =
                invitee_chats_before.iter().position(|c| c.id() == chat_id)
            {
                invitee_chats_before.remove(inactive_chat_position);
            }
            // Now that we've removed the new chat, it should be the same set of chats
            info!("chats_before: {:?}", invitee_chats_before);
            info!("chats_after: {:?}", invitee_chats_after);
            let different_chats = invitee_chats_before
                .into_iter()
                .collect::<HashSet<_>>()
                .symmetric_difference(&invitee_chats_after.into_iter().collect::<HashSet<_>>())
                .count();
            assert_eq!(different_chats, 0);
        }
        let group_members = self.groups.get_mut(&chat_id).unwrap();
        for group_member_id in group_members.iter() {
            // Skip the sender
            if group_member_id == inviter_id {
                continue;
            }
            let test_group_member = self.users.get_mut(group_member_id).unwrap();
            let group_member = &mut test_group_member.user;
            let group_members_before = group_member.chat_participants(chat_id).await.unwrap();
            let qs_messages = group_member.qs_fetch_messages().await.unwrap();

            let invite_messages = group_member.fully_process_qs_messages(qs_messages).await;
            assert!(
                invite_messages.errors.is_empty(),
                "Errors processing QS messages for {group_member_id:?}: {:?}",
                invite_messages.errors
            );

            let invite_messages = display_messages_to_string_map(invite_messages.new_messages);

            assert_eq!(invite_messages, expected_messages);

            let group_members_after = group_member.chat_participants(chat_id).await.unwrap();
            let new_members = group_members_after
                .difference(&group_members_before)
                .collect::<HashSet<_>>();
            let invitee_set = invitees.iter().copied().collect::<HashSet<_>>();
            assert_eq!(new_members, invitee_set)
        }

        for invitee_id in &invitees {
            let unique_member = group_members.insert((*invitee_id).clone());
            assert!(unique_member);
        }

        // Now send messages to check that the group works properly. This also
        // ensures that everyone involved has picked up their messages from the
        // QS and that notifications are flushed.
        self.send_message(chat_id, inviter_id, invitees.clone())
            .await;
        for invitee_id in &invitees {
            let recipients: Vec<_> = invitees
                .iter()
                .filter(|&name| name != invitee_id)
                .chain([&inviter_id].into_iter())
                .cloned()
                .collect();
            self.send_message(chat_id, invitee_id, recipients).await;
        }
    }

    /// Has the remover remove the removed from the given group and has everyone
    /// send and process their messages.
    pub async fn remove_from_group(
        &mut self,
        chat_id: ChatId,
        remover_id: &UserId,
        removed_ids: Vec<&UserId>,
    ) -> anyhow::Result<()> {
        let removed_strings = removed_ids
            .iter()
            .map(|n| format!("{n:?}"))
            .collect::<Vec<_>>();
        let test_remover = self.users.get_mut(remover_id).unwrap();
        let remover = &mut test_remover.user;

        // Before removing anyone from a group, the remover must first fetch and
        // process its QS messages.
        let qs_messages = remover.qs_fetch_messages().await.unwrap();

        remover.fully_process_qs_messages(qs_messages).await;

        info!(
            "{remover_id:?} removes {} from the group with id {}",
            removed_strings.join(", "),
            chat_id.uuid()
        );

        // Perform the remove operation and check that the removed are not in
        // the group anymore.
        let remover_group_members_before = remover
            .chat_participants(chat_id)
            .await
            .expect("Error getting group members.");

        let remove_messages = remover
            .remove_users(
                chat_id,
                removed_ids.iter().copied().cloned().collect::<Vec<_>>(),
            )
            .await?;

        let mut expected_messages = HashSet::new();

        for removed_id in &removed_ids {
            let expected_message = format!("{remover_id:?} removed {removed_id:?} from the chat");
            expected_messages.insert(expected_message);
        }

        let remove_messages = display_messages_to_string_map(remove_messages);
        assert_eq!(remove_messages, expected_messages);

        let remover_group_members_after = remover
            .chat_participants(chat_id)
            .await
            .expect("Error getting group members.");
        let removed_members: HashSet<_> = remover_group_members_before
            .difference(&remover_group_members_after)
            .cloned()
            .collect();
        let removed_set: HashSet<_> = removed_ids.iter().map(|&user_id| user_id.clone()).collect();
        assert_eq!(removed_members, removed_set);

        for removed_id in &removed_ids {
            let test_removed = self.users.get_mut(removed_id).unwrap();
            let removed = &mut test_removed.user;
            let removed_chats_before = removed
                .chats()
                .await
                .unwrap()
                .into_iter()
                .collect::<HashSet<_>>();
            let past_members = removed.chat_participants(chat_id).await.unwrap();

            let qs_messages = removed.qs_fetch_messages().await.unwrap();

            removed.fully_process_qs_messages(qs_messages).await;

            let removed_chats_after = removed
                .chats()
                .await
                .unwrap()
                .into_iter()
                .collect::<HashSet<_>>();
            let chat = removed_chats_after
                .iter()
                .find(|c| c.id() == chat_id)
                .unwrap_or_else(|| {
                    panic!(
                        "{removed_id:?} should have the chat with id {}",
                        chat_id.uuid()
                    )
                });
            assert!(chat.id() == chat_id);
            if let ChatStatus::Inactive(inactive_status) = &chat.status() {
                let inactive_status_members =
                    HashSet::from_iter(inactive_status.past_members().to_vec());
                assert_eq!(inactive_status_members, past_members);
            } else {
                panic!("chat should be inactive.")
            }
            assert!(chat.chat_type() == &ChatType::Group);
            for chat in removed_chats_after {
                assert!(removed_chats_before.iter().any(|c| c.id() == chat.id()))
            }
        }
        let group_members = self.groups.get_mut(&chat_id).unwrap();
        for removed_id in &removed_ids {
            let remove_successful = group_members.remove(removed_id);
            assert!(remove_successful);
        }
        // Now have the rest of the group pick up and process their messages.
        for group_member_id in group_members.iter() {
            // Skip the remover
            if group_member_id == remover_id {
                continue;
            }
            let test_group_member = self.users.get_mut(group_member_id).unwrap();
            let group_member = &mut test_group_member.user;
            let group_members_before = group_member.chat_participants(chat_id).await.unwrap();
            let qs_messages = group_member.qs_fetch_messages().await.unwrap();

            let remove_messages = group_member.fully_process_qs_messages(qs_messages).await;

            let remove_messages = display_messages_to_string_map(remove_messages.new_messages);
            assert_eq!(remove_messages, expected_messages);

            let group_members_after = group_member.chat_participants(chat_id).await.unwrap();
            let removed_members: HashSet<_> = group_members_before
                .difference(&group_members_after)
                .cloned()
                .collect();
            let removed_set: HashSet<_> = removed_ids.iter().cloned().cloned().collect();
            assert_eq!(removed_members, removed_set)
        }

        Ok(())
    }

    /// Has the leaver leave the given group.
    pub async fn leave_group(&mut self, chat_id: ChatId, leaver_id: &UserId) -> anyhow::Result<()> {
        info!("{leaver_id:?} leaves the group with id {}", chat_id.uuid());

        // Get random member that observes the proposal and can commit it later
        let group_members = self.groups.get(&chat_id).unwrap().clone();
        let mut random_member_iter = group_members.iter();
        let mut random_member_id = random_member_iter.next().unwrap();
        // Ensure that the random member isn't the leaver.
        if random_member_id == leaver_id {
            random_member_id = random_member_iter.next().unwrap()
        }

        // Perform the leave operation.
        let test_random_member = self.users.get_mut(random_member_id).unwrap();
        let random_member = &mut test_random_member.user;
        let mimi_members_before = random_member.chat_participants(chat_id).await.unwrap();
        let test_leaver = self.users.get_mut(leaver_id).unwrap();
        let leaver = &mut test_leaver.user;
        leaver.leave_chat(chat_id).await?;

        // Make sure everyone else processes their QS messages so that the
        // leaver's proposal is observed. This is relevant mostly s.t. we can
        // compare the pending removes with the actual members removed after the
        // commit.
        for member_id in &group_members {
            if member_id != leaver_id {
                let test_member = self.users.get_mut(member_id).unwrap();
                let member = &mut test_member.user;
                let qs_messages = member.qs_fetch_messages().await.unwrap();
                member.fully_process_qs_messages(qs_messages).await;
            }
        }

        // Fetch and process the QS messages to make sure the random member has the proposal.
        let test_random_member = self.users.get_mut(random_member_id).unwrap();
        let random_member = &mut test_random_member.user;
        let qs_messages = random_member.qs_fetch_messages().await.unwrap();

        random_member.fully_process_qs_messages(qs_messages).await;

        let mimi_members_after = random_member.chat_participants(chat_id).await.unwrap();
        let difference: HashSet<UserId> = mimi_members_before
            .difference(&mimi_members_after)
            .map(|s| s.to_owned())
            .collect();
        let pending_removes =
            HashSet::from_iter(random_member.pending_removes(chat_id).await.unwrap());
        assert_eq!(difference, pending_removes);

        // Now have a random group member perform an update, thus committing the leave operation.
        // TODO: This is not really random. We should do better here. But also,
        // we probably want a way to track the randomness s.t. we can reproduce
        // tests.

        // Now commit to the pending proposal. This also makes everyone else
        // pick up and process their messages. This also tests that group
        // members were removed correctly from the local group and that the
        // leaver has turned its chat inactive.
        self.commit_to_proposals(chat_id, random_member_id.clone())
            .await;

        let group_members = self.groups.get_mut(&chat_id).unwrap();

        group_members.remove(leaver_id);

        Ok(())
    }

    pub async fn delete_group(&mut self, chat_id: ChatId, deleter_id: &UserId) {
        info!(
            "{deleter_id:?} deletes the group with id {}",
            chat_id.uuid()
        );
        let test_deleter = self.users.get_mut(deleter_id).unwrap();
        let deleter = &mut test_deleter.user;

        // Before removing anyone from a group, the remover must first fetch and
        // process its QS messages.
        let qs_messages = deleter.qs_fetch_messages().await.unwrap();

        deleter.fully_process_qs_messages(qs_messages).await;

        // Perform the remove operation and check that the removed are not in
        // the group anymore.
        let deleter_chat_before = deleter.chat(&chat_id).await.unwrap().clone();
        assert_eq!(deleter_chat_before.status(), &ChatStatus::Active);
        let past_members = deleter.chat_participants(chat_id).await.unwrap();

        deleter.delete_chat(chat_id).await.unwrap();

        let deleter_chat_after = deleter.chat(&chat_id).await.unwrap();
        if let ChatStatus::Inactive(inactive_status) = &deleter_chat_after.status() {
            let inactive_status_members =
                HashSet::from_iter(inactive_status.past_members().to_vec());
            assert_eq!(inactive_status_members, past_members);
        } else {
            panic!("chat should be inactive.")
        }

        for group_member_id in self.groups.get(&chat_id).unwrap().iter() {
            // Skip the deleter
            if group_member_id == deleter_id {
                continue;
            }
            let test_group_member = self.users.get_mut(group_member_id).unwrap();
            let group_member = &mut test_group_member.user;

            let group_member_chat_before = group_member.chat(&chat_id).await.unwrap();
            assert_eq!(group_member_chat_before.status(), &ChatStatus::Active);
            let past_members = group_member.chat_participants(chat_id).await.unwrap();

            let qs_messages = group_member.qs_fetch_messages().await.unwrap();

            group_member.fully_process_qs_messages(qs_messages).await;

            let group_member_chat_after = group_member.chat(&chat_id).await.unwrap();
            if let ChatStatus::Inactive(inactive_status) = &group_member_chat_after.status() {
                let inactive_status_members =
                    HashSet::from_iter(inactive_status.past_members().to_vec());
                assert_eq!(inactive_status_members, past_members);
            } else {
                panic!("chat should be inactive.")
            }
        }
        self.groups.remove(&chat_id);
    }

    pub fn random_user(&self, rng: &mut impl RngCore) -> UserId {
        self.users
            .keys()
            .choose(rng)
            .expect("There should be at least one user")
            .clone()
    }

    pub async fn perform_random_operation(&mut self, rng: &mut impl RngCore) {
        // Get a user to perform the operation
        let random_user = self.random_user(rng);
        // Possible actions:
        // 0: Establish a connection
        // 1: Create a group and invite one or more users
        // 2: Invite up to 5 users to a group
        // 3: Remove up to 5 users from a group
        // 4: Leave a group
        // Message sending is covered, as it's done as part of all of those
        // actions. If one of the actions is not possible, it is skipped.
        // TODO: Breaking up of connections
        let action = rng.gen_range(0..=3);
        match action {
            // Establish a connection
            0 => {
                let mut other_users = Vec::new();
                for user in self.users.keys() {
                    let is_contact = self
                        .users
                        .get(user)
                        .unwrap()
                        .user()
                        .contacts()
                        .await
                        .unwrap()
                        .into_iter()
                        .any(|contact| contact.user_id == random_user);
                    if user != &random_user && !is_contact {
                        other_users.push(user.clone());
                    }
                }
                if let Some(other_user) = other_users.into_iter().choose(rng) {
                    info!(
                        random_operation = true,
                        "Random operation: Connecting {random_user:?} and {other_user:?}"
                    );
                    self.connect_users(&random_user, &other_user).await;
                }
            }
            1 => {
                let chat_id = self.create_group(&random_user).await;
                info!(
                    random_operation = true,
                    "Random operation: Created group {}",
                    chat_id.uuid()
                );
                // TODO: Invite user(s)
            }
            2 => {
                // Pick a group
                let user = self.users.get(&random_user).unwrap();
                // Let's exclude connection groups for now.
                if let Some(chat) = user
                    .user()
                    .chats()
                    .await
                    .unwrap()
                    .into_iter()
                    .filter(|chat| {
                        chat.chat_type() == &ChatType::Group && chat.status() == &ChatStatus::Active
                    })
                    .choose(rng)
                {
                    let number_of_invitees = rng.gen_range(1..=5);
                    let mut invitees = Vec::new();
                    for invitee in self.users.keys() {
                        let is_group_member =
                            self.groups.get(&chat.id()).unwrap().contains(invitee);
                        let is_connected = user
                            .user()
                            .contacts()
                            .await
                            .unwrap()
                            .into_iter()
                            .any(|contact| &contact.user_id == invitee);
                        if !is_group_member && is_connected && invitee != &random_user {
                            invitees.push(invitee);
                        }
                    }
                    let invitees = invitees
                        .into_iter()
                        .cloned()
                        .choose_multiple(rng, number_of_invitees);
                    // It can happen that there are no suitable users to invite
                    if !invitees.is_empty() {
                        let invitee_strings = invitees
                            .iter()
                            .map(|invitee| format!("{invitee:?}"))
                            .collect::<Vec<_>>();
                        info!(
                            random_operation = true,
                            "Random operation: {random_user:?} invites {} to group {}",
                            invitee_strings.join(", "),
                            chat.id().uuid()
                        );
                        self.invite_to_group(chat.id(), &random_user, invitees.iter().collect())
                            .await;
                    }
                }
            }
            3 => {
                let user = self.users.get(&random_user).unwrap();
                if let Some(chat) = user
                    .user()
                    .chats()
                    .await
                    .unwrap()
                    .into_iter()
                    .filter(|chat| {
                        chat.chat_type() == &ChatType::Group && chat.status() == &ChatStatus::Active
                    })
                    .choose(rng)
                {
                    let number_of_removals = rng.gen_range(1..=5);
                    let members_to_remove = self
                        .groups
                        .get(&chat.id())
                        .unwrap()
                        .iter()
                        .filter(|&member| member != &random_user)
                        .cloned()
                        .choose_multiple(rng, number_of_removals);
                    if !members_to_remove.is_empty() {
                        let removed_strings = members_to_remove
                            .iter()
                            .map(|removed| format!("{removed:?}"))
                            .collect::<Vec<_>>();
                        info!(
                            random_operation = true,
                            "Random operation: {random_user:?} removes {} from group {}",
                            removed_strings.join(", "),
                            chat.id().uuid()
                        );
                        let members_to_remove = members_to_remove.iter().collect();
                        self.remove_from_group(chat.id(), &random_user, members_to_remove)
                            .await
                            .unwrap();
                    }
                }
            }
            4 => {
                let user = self.users.get(&random_user).unwrap();
                if let Some(chat) = user
                    .user()
                    .chats()
                    .await
                    .unwrap()
                    .into_iter()
                    .filter(|chat| {
                        chat.chat_type() == &ChatType::Group && chat.status() == &ChatStatus::Active
                    })
                    .choose(rng)
                {
                    info!(
                        random_operation = true,
                        "Random operation: {random_user:?} leaves group {}",
                        chat.id().uuid()
                    );
                    self.leave_group(chat.id(), &random_user).await.unwrap();
                }
            }
            _ => panic!("Invalid action"),
        }
    }
}

fn display_messages_to_string_map(display_messages: Vec<ChatMessage>) -> HashSet<String> {
    display_messages
        .into_iter()
        .filter_map(|m| {
            if let Message::Event(EventMessage::System(system_message)) = m.message() {
                match system_message {
                    SystemMessage::Add(adder, added) => {
                        Some(format!("{adder:?} added {added:?} to the chat"))
                    }
                    SystemMessage::Remove(remover, removed) => {
                        Some(format!("{remover:?} removed {removed:?} from the chat"))
                    }
                }
            } else {
                None
            }
        })
        .collect()
}
