// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{collections::HashSet, sync::Arc};

pub use airapiclient::as_api::ListenHandleResponder;
use airapiclient::{
    ApiClient, ApiClientInitError,
    qs_api::{ListenResponder, ListenResponderClosedError},
};
use aircommon::{
    DEFAULT_PORT_GRPC,
    credentials::{
        ClientCredential, ClientCredentialCsr, ClientCredentialPayload, keys::ClientSigningKey,
    },
    crypto::{
        RatchetDecryptionKey,
        ear::{
            EarEncryptable,
            keys::{PushTokenEarKey, WelcomeAttributionInfoEarKey},
        },
        hpke::HpkeEncryptable,
        kdf::keys::RatchetSecret,
        signatures::keys::{QsClientSigningKey, QsUserSigningKey},
    },
    identifiers::{ClientConfig, QsClientId, QsReference, QsUserId, UserId},
    messages::{FriendshipToken, QueueMessage, push_token::PushToken},
};
pub use airprotos::auth_service::v1::{HandleQueueMessage, handle_queue_message};
pub use airprotos::queue_service::v1::{
    QueueEvent, QueueEventPayload, QueueEventUpdate, queue_event,
};
use anyhow::{Context, Result, anyhow, ensure};
use chrono::{DateTime, Utc};
use openmls::prelude::Ciphersuite;
use own_client_info::OwnClientInfo;

use serde::{Deserialize, Serialize};
use sqlx::{Row, SqliteConnection, SqlitePool, query};
use store::ClientRecord;
use thiserror::Error;
use tls_codec::DeserializeBytes;
use tokio_stream::{Stream, StreamExt};
use tracing::{error, info, warn};
use url::Url;

use crate::{
    Asset, UserHandleRecord,
    contacts::HandleContact,
    groups::Group,
    key_stores::queue_ratchets::StorableQsQueueRatchet,
    outbound_service::OutboundService,
    store::Store,
    utils::{
        connection_ext::StoreExt,
        file_lock::FileLock,
        image::resize_profile_image,
        persistence::{delete_client_database, open_lock_file},
    },
};
use crate::{ChatId, key_stores::as_credentials::AsCredentials};
use crate::{
    MessageId,
    chats::{
        Chat, ChatAttributes,
        messages::{ChatMessage, TimestampedMessage},
    },
    clients::connection_offer::FriendshipPackage,
    contacts::Contact,
    groups::openmls_provider::AirOpenMlsProvider,
    key_stores::MemoryUserKeyStore,
    store::{StoreNotification, StoreNotifier},
    user_profiles::IndexedUserProfile,
    utils::persistence::{open_air_db, open_client_db},
};
use crate::{store::StoreNotificationsSender, user_profiles::UserProfile};

use self::{api_clients::ApiClients, create_user::InitialUserState, store::UserCreationState};

mod add_contact;
pub(crate) mod api_clients;
pub(crate) mod attachment;
pub(crate) mod block_contact;
pub mod chats;
pub(crate) mod connection_offer;
mod create_user;
mod delete_account;
mod invite_users;
mod message;
pub(crate) mod own_client_info;
mod persistence;
pub mod process;
mod remove_users;
pub mod store;
#[cfg(test)]
mod tests;
mod update_key;
mod user_profile;
pub(crate) mod user_settings;

pub(crate) const CIPHERSUITE: Ciphersuite =
    Ciphersuite::MLS_128_DHKEMX25519_AES128GCM_SHA256_Ed25519;

pub(crate) const CONNECTION_PACKAGES: usize = 50;
pub(crate) const KEY_PACKAGES: usize = 50;

#[derive(Debug, Clone)]
pub struct CoreUser {
    inner: Arc<CoreUserInner>,
}

#[derive(Debug)]
struct CoreUserInner {
    pool: SqlitePool,
    api_clients: ApiClients,
    http_client: reqwest::Client,
    qs_user_id: QsUserId,
    qs_client_id: QsClientId,
    key_store: MemoryUserKeyStore,
    store_notifications_tx: StoreNotificationsSender,
    outbound_service: OutboundService,
}

impl CoreUser {
    /// Create a new user with the given `user_id`.
    ///
    /// If a user with this name already exists, this will overwrite that user.
    pub async fn new(
        user_id: UserId,
        server_url: Url,
        grpc_port: u16,
        db_path: &str,
        push_token: Option<PushToken>,
    ) -> Result<Self> {
        info!(?user_id, "creating new user");

        // Open the air db to store the client record
        let air_db = open_air_db(db_path).await?;

        // Open client specific db
        let client_db = open_client_db(&user_id, db_path).await?;

        let global_lock = open_lock_file(db_path)?;

        Self::new_with_connections(
            user_id,
            server_url,
            grpc_port,
            push_token,
            air_db,
            client_db,
            global_lock,
        )
        .await
    }

    async fn new_with_connections(
        user_id: UserId,
        server_url: Url,
        grpc_port: u16,
        push_token: Option<PushToken>,
        air_db: SqlitePool,
        client_db: SqlitePool,
        global_lock: FileLock,
    ) -> Result<Self> {
        let server_url = server_url.to_string();
        let api_clients = ApiClients::new(user_id.domain().clone(), server_url.clone(), grpc_port);

        let user_creation_state =
            UserCreationState::new(&client_db, &air_db, user_id, server_url.clone(), push_token)
                .await?;

        let final_state = user_creation_state
            .complete_user_creation(&air_db, &client_db, &api_clients)
            .await?;

        OwnClientInfo {
            server_url,
            qs_user_id: *final_state.qs_user_id(),
            qs_client_id: *final_state.qs_client_id(),
            user_id: final_state.user_id().clone(),
        }
        .store(&client_db)
        .await?;

        let self_user = final_state.into_self_user(client_db, api_clients, global_lock);

        Ok(self_user)
    }

    /// The same as [`Self::new()`], except that databases are ephemeral and are
    /// dropped together with this instance of [`CoreUser`].
    #[cfg(feature = "test_utils")]
    pub async fn new_ephemeral(
        user_id: UserId,
        server_url: Url,
        grpc_port: u16,
        push_token: Option<PushToken>,
    ) -> Result<Self> {
        use crate::utils::persistence::open_db_in_memory;

        info!(?user_id, "creating new ephemeral user");

        // Open the air db to store the client record
        let air_db = open_db_in_memory().await?;

        // Open client specific db
        let client_db = open_db_in_memory().await?;

        let global_lock = FileLock::from_file(tempfile::tempfile()?)?;

        Self::new_with_connections(
            user_id,
            server_url,
            grpc_port,
            push_token,
            air_db,
            client_db,
            global_lock,
        )
        .await
    }

    /// Load a user from the database.
    ///
    /// If a user creation process with a matching `UserId` was interrupted before, this will
    /// resume that process.
    pub async fn load(user_id: UserId, db_path: &str) -> Result<CoreUser> {
        let client_db = open_client_db(&user_id, db_path).await?;

        let user_creation_state = UserCreationState::load(&client_db, &user_id)
            .await?
            .context("missing user creation state")?;

        let air_db = open_air_db(db_path).await?;
        let api_clients = ApiClients::new(
            user_id.domain().clone(),
            user_creation_state.server_url(),
            DEFAULT_PORT_GRPC,
        );
        let final_state = user_creation_state
            .complete_user_creation(&air_db, &client_db, &api_clients)
            .await?;
        ClientRecord::set_default(&air_db, &user_id).await?;

        let global_lock = open_lock_file(db_path)?;

        Ok(final_state.into_self_user(client_db, api_clients, global_lock))
    }

    /// Delete this user on the server and locally.
    ///
    /// The user database is also deleted. The client record is removed from the air database.
    pub async fn delete(self, db_path: &str) -> anyhow::Result<()> {
        let user_id = self.user_id().clone();
        self.delete_ephemeral().await?;
        delete_client_database(db_path, &user_id).await?;
        Ok(())
    }

    /// Delete this user on the server.
    ///
    /// The local database and client record are not touched.
    pub async fn delete_ephemeral(self) -> anyhow::Result<()> {
        self.inner
            .api_clients
            .default_client()?
            .as_delete_user(self.user_id().clone(), &self.inner.key_store.signing_key)
            .await?;
        Ok(())
    }

    pub(crate) fn pool(&self) -> &SqlitePool {
        &self.inner.pool
    }

    pub(crate) fn signing_key(&self) -> &ClientSigningKey {
        &self.inner.key_store.signing_key
    }

    pub(crate) fn api_client(&self) -> anyhow::Result<ApiClient> {
        Ok(self.inner.api_clients.default_client()?)
    }

    pub(crate) fn http_client(&self) -> reqwest::Client {
        self.inner.http_client.clone()
    }

    pub fn outbound_service(&self) -> &OutboundService {
        &self.inner.outbound_service
    }

    pub(crate) fn send_store_notification(&self, notification: StoreNotification) {
        if !notification.is_empty() {
            self.inner.store_notifications_tx.notify(notification);
        }
    }

    /// Subscribes to store notifications.
    ///
    /// All notifications sent after this function was called are observed as items of the returned
    /// stream.
    pub(crate) fn subscribe_to_store_notifications(
        &self,
    ) -> impl Stream<Item = Arc<StoreNotification>> + Send + 'static {
        self.inner.store_notifications_tx.subscribe()
    }

    /// Subcribes to pending store notifications.
    ///
    /// Unlike `subscribe_to_store_notifications`, this function does not remove stored
    /// notifications from the persisted queue.
    pub(crate) fn subscribe_iter_to_store_notifications(
        &self,
    ) -> impl Iterator<Item = Arc<StoreNotification>> + Send + 'static {
        self.inner.store_notifications_tx.subscribe_iter()
    }

    pub(crate) fn store_notifier(&self) -> StoreNotifier {
        StoreNotifier::new(self.inner.store_notifications_tx.clone())
    }

    pub(crate) async fn enqueue_store_notification(
        &self,
        notification: &StoreNotification,
    ) -> Result<()> {
        notification
            .enqueue(self.pool().acquire().await?.as_mut())
            .await?;
        Ok(())
    }

    pub(crate) async fn dequeue_store_notification(&self) -> Result<StoreNotification> {
        Ok(StoreNotification::dequeue(self.pool()).await?)
    }

    pub async fn set_own_user_profile(&self, mut user_profile: UserProfile) -> Result<UserProfile> {
        ensure!(
            &user_profile.user_id == self.user_id(),
            "Can't set user profile for users other than the current user"
        );
        if let Some(profile_picture) = user_profile.profile_picture {
            let new_image = match profile_picture {
                Asset::Value(image_bytes) => resize_profile_image(&image_bytes)?,
            };
            user_profile.profile_picture = Some(Asset::Value(new_image));
        }
        self.update_user_profile(user_profile.clone()).await?;
        Ok(user_profile)
    }

    /// Get the user profile of the user with the given [`AsClientId`].
    ///
    /// In case of an error, or if the user profile is not found, the client id is used as a
    /// fallback.
    pub async fn user_profile(&self, user_id: &UserId) -> UserProfile {
        match self.pool().acquire().await {
            Ok(mut connection) => self.user_profile_internal(&mut connection, user_id).await,
            Err(error) => {
                error!(%error, "Error loading user profile; fallback to user_id");
                UserProfile::from_user_id(user_id)
            }
        }
    }

    // Helper to use when we already hold a connection
    async fn user_profile_internal(
        &self,
        connection: &mut SqliteConnection,
        user_id: &UserId,
    ) -> UserProfile {
        IndexedUserProfile::load(connection, user_id)
            .await
            .inspect_err(|error| {
                error!(%error, "Error loading user profile; fallback to user_id");
            })
            .ok()
            .flatten()
            .map(UserProfile::from)
            .unwrap_or_else(|| UserProfile::from_user_id(user_id))
    }

    /// Fetch and process messages from all user handle queues.
    ///
    /// Returns the list of [`ChatId`]s of any newly created chats.
    pub async fn fetch_and_process_handle_messages(&self) -> Result<Vec<ChatId>> {
        let records = self.user_handle_records().await?;
        let api_client = self.api_client()?;
        let mut chat_ids = Vec::new();
        for record in records {
            let (mut stream, responder) = api_client
                .as_listen_handle(record.hash, &record.signing_key)
                .await?;
            while let Some(Some(message)) = stream.next().await {
                let Some(message_id) = message.message_id else {
                    error!("no message id in handle queue message");
                    continue;
                };
                match self
                    .process_handle_queue_message(&record.handle, message)
                    .await
                {
                    Ok(chat_id) => {
                        chat_ids.push(chat_id);
                    }
                    Err(error) => {
                        error!(%error, "failed to process handle queue message");
                    }
                }
                // ack the message independently of the result of processing the message
                responder.ack(message_id.into()).await;
            }
        }
        Ok(chat_ids)
    }

    /// Fetches all messages from all user handle queues and returns them.
    ///
    /// Used in integration tests
    pub async fn fetch_handle_messages(&self) -> Result<Vec<HandleQueueMessage>> {
        let records = self.user_handle_records().await?;
        let api_client = self.api_client()?;
        let mut messages = Vec::new();
        for record in records {
            let (mut stream, responder) = api_client
                .as_listen_handle(record.hash, &record.signing_key)
                .await?;
            while let Some(Some(message)) = stream.next().await {
                let Some(message_id) = message.message_id else {
                    error!("no message id in handle queue message");
                    continue;
                };
                // ack the message independently of the result of processing the message
                responder.ack(message_id.into()).await;
                messages.push(message);
            }
        }
        Ok(messages)
    }

    /// Fetches all messages from the QS queue.
    ///
    /// Must *not* be used outside of integration tests, because the messages are not acked.
    pub async fn qs_fetch_messages(&self) -> Result<Vec<QueueMessage>> {
        let (stream, _responder) = self.listen_queue().await?;
        let messages = stream
            .take_while(|message| !matches!(message.event, Some(queue_event::Event::Empty(_))))
            .filter_map(|message| match message.event? {
                queue_event::Event::Empty(_) => unreachable!(),
                queue_event::Event::Message(queue_message) => queue_message.try_into().ok(),
                queue_event::Event::Payload(_) => None,
            })
            .collect()
            .await;
        Ok(messages)
    }

    pub async fn contacts(&self) -> sqlx::Result<Vec<Contact>> {
        let contacts = Contact::load_all(self.pool()).await?;
        Ok(contacts)
    }

    pub async fn contact(&self, user_id: &UserId) -> Option<Contact> {
        self.try_contact(user_id).await.ok().flatten()
    }

    pub async fn try_contact(&self, user_id: &UserId) -> sqlx::Result<Option<Contact>> {
        Contact::load(self.pool(), user_id).await
    }

    pub async fn handle_contacts(&self) -> sqlx::Result<Vec<HandleContact>> {
        HandleContact::load_all(self.pool()).await
    }

    fn create_own_client_reference(&self) -> QsReference {
        let sealed_reference = ClientConfig {
            client_id: self.inner.qs_client_id,
            push_token_ear_key: Some(self.inner.key_store.push_token_ear_key.clone()),
        }
        .encrypt(&self.inner.key_store.qs_client_id_encryption_key, &[], &[]);
        QsReference {
            client_homeserver_domain: self.user_id().domain().clone(),
            sealed_reference,
        }
    }

    /// Returns None if there is no chat with the given id.
    pub async fn mls_chat_participants(&self, chat_id: ChatId) -> Option<HashSet<UserId>> {
        self.try_mls_chat_participants(chat_id).await.ok()?
    }

    pub(crate) async fn try_mls_chat_participants(
        &self,
        chat_id: ChatId,
    ) -> Result<Option<HashSet<UserId>>> {
        let mut connection = self.pool().acquire().await?;
        let Some(chat_id) = Chat::load(&mut connection, &chat_id).await? else {
            return Ok(None);
        };
        let Some(group) = Group::load(&mut connection, chat_id.group_id()).await? else {
            return Ok(None);
        };
        Ok(Some(group.members(&mut *connection).await))
    }

    /// Returns None if there is no chat with the given id.
    pub async fn chat_participants(&self, chat_id: ChatId) -> Option<HashSet<UserId>> {
        self.try_chat_participants(chat_id).await.ok()?
    }

    pub(crate) async fn try_chat_participants(
        &self,
        chat_id: ChatId,
    ) -> Result<Option<HashSet<UserId>>> {
        let mut connection = self.pool().acquire().await?;
        let Some(chat) = Chat::load(&mut connection, &chat_id).await? else {
            return Ok(None);
        };
        let Some(group) = Group::load(&mut connection, chat.group_id()).await? else {
            return Ok(None);
        };
        let users = group
            .room_state
            .users()
            .keys()
            .map(|bytes| Ok(UserId::tls_deserialize_exact_bytes(bytes)?))
            .collect::<Result<HashSet<_>>>()?;
        Ok(Some(users))
    }

    pub async fn pending_removes(&self, chat_id: ChatId) -> Option<Vec<UserId>> {
        let mut connection = self.pool().acquire().await.ok()?;
        let chat = Chat::load(&mut connection, &chat_id).await.ok()??;
        let group = Group::load(&mut connection, chat.group_id()).await.ok()??;
        Some(group.pending_removes(&mut connection).await)
    }

    pub async fn listen_queue(
        &self,
    ) -> Result<(impl Stream<Item = QueueEvent> + use<>, QsListenResponder)> {
        let queue_ratchet = StorableQsQueueRatchet::load(self.pool()).await?;
        let sequence_number_start = queue_ratchet.sequence_number();
        let api_client = self.inner.api_clients.default_client()?;
        let (stream, responder) = api_client
            .listen_queue(self.inner.qs_client_id, sequence_number_start)
            .await?;
        let responder = QsListenResponder { responder };
        Ok((stream, responder))
    }

    pub async fn listen_handle(
        &self,
        handle_record: &UserHandleRecord,
    ) -> Result<(
        impl Stream<Item = Option<HandleQueueMessage>> + Send + 'static,
        ListenHandleResponder,
    )> {
        let api_client = self.inner.api_clients.default_client()?;
        match api_client
            .as_listen_handle(handle_record.hash, &handle_record.signing_key)
            .await
        {
            Ok(ok) => Ok(ok),
            Err(error) => {
                // We remove the user handle locally if it is not found
                if error.is_not_found() {
                    warn!(
                        "User handle {} not found on the server, removing locally",
                        &handle_record.handle.plaintext()
                    );
                    let _ = self.remove_user_handle_locally(&handle_record.handle).await;
                }
                Err(error.into())
            }
        }
    }

    /// Mark all messages in the chat with the given chat id and
    /// with a timestamp older than the given timestamp as read.
    pub async fn mark_as_read<T: IntoIterator<Item = (ChatId, DateTime<Utc>)>>(
        &self,
        mark_as_read_data: T,
    ) -> anyhow::Result<()> {
        let mut notifier = self.store_notifier();
        Chat::mark_as_read(
            self.pool().acquire().await?.as_mut(),
            &mut notifier,
            mark_as_read_data,
        )
        .await?;
        notifier.notify();
        Ok(())
    }

    /// Returns how many messages are marked as unread across all chats.
    pub async fn global_unread_messages_count(&self) -> sqlx::Result<usize> {
        Chat::global_unread_message_count(self.pool()).await
    }

    /// Returns how many messages in the chat with the given ID are
    /// marked as unread.
    pub async fn unread_messages_count(&self, chat_id: ChatId) -> usize {
        Chat::unread_messages_count(self.pool(), chat_id)
            .await
            .inspect_err(|error| error!(%error, "Error while fetching unread messages count"))
            .unwrap_or(0)
    }

    pub(crate) async fn try_messages_count(&self, chat_id: ChatId) -> sqlx::Result<usize> {
        Chat::messages_count(self.pool(), chat_id).await
    }

    pub(crate) async fn try_unread_messages_count(&self, chat_id: ChatId) -> sqlx::Result<usize> {
        Chat::unread_messages_count(self.pool(), chat_id).await
    }

    /// Updates the client's push token on the QS.
    pub async fn update_push_token(&self, push_token: Option<PushToken>) -> Result<()> {
        match &push_token {
            Some(_) => info!("Updating push token on QS"),
            None => info!("Clearing push token on QS"),
        }

        let client_id = self.inner.qs_client_id;
        // Ratchet encryption key
        let queue_encryption_key = self
            .inner
            .key_store
            .qs_queue_decryption_key
            .encryption_key();
        // Signung key
        let signing_key = self.inner.key_store.qs_client_signing_key.clone();

        // Encrypt the push token, if there is one.
        let encrypted_push_token = match push_token {
            Some(push_token) => {
                let encrypted_push_token =
                    push_token.encrypt(&self.inner.key_store.push_token_ear_key)?;
                Some(encrypted_push_token)
            }
            None => None,
        };

        self.inner
            .api_clients
            .default_client()?
            .qs_update_client(
                client_id,
                queue_encryption_key.clone(),
                encrypted_push_token,
                &signing_key,
            )
            .await?;
        Ok(())
    }

    pub fn user_id(&self) -> &UserId {
        self.inner.key_store.signing_key.credential().identity()
    }

    async fn store_new_messages(
        connection: &mut sqlx::SqliteConnection,
        notifier: &mut StoreNotifier,
        chat_id: ChatId,
        group_messages: Vec<TimestampedMessage>,
    ) -> Result<Vec<ChatMessage>> {
        let mut stored_messages = Vec::with_capacity(group_messages.len());
        for timestamped_message in group_messages.into_iter() {
            let message_id = MessageId::random();
            let mut message = ChatMessage::new(chat_id, message_id, timestamped_message);
            let attachment_records = Self::extract_attachments(&mut message);
            message.store(&mut *connection, notifier).await?;
            for (record, pending_record) in attachment_records {
                if let Err(error) = record.store(&mut *connection, notifier, None).await {
                    error!(%error, "Failed to store attachment");
                    continue;
                }
                if let Err(error) = pending_record.store(&mut *connection, notifier).await {
                    error!(%error, "Failed to store pending attachment");
                }
            }
            stored_messages.push(message);
        }
        Ok(stored_messages)
    }

    /// Returns the user profile of this [`CoreUser`].
    pub async fn own_user_profile(&self) -> sqlx::Result<UserProfile> {
        IndexedUserProfile::load(self.pool(), self.user_id())
            .await
            // We unwrap here, because we know that the user exists.
            .map(|user_option| user_option.unwrap().into())
    }

    pub async fn report_spam(&self, spammer_id: UserId) -> anyhow::Result<()> {
        self.inner
            .api_clients
            .default_client()?
            .as_report_spam(
                self.user_id().clone(),
                spammer_id,
                &self.inner.key_store.signing_key,
            )
            .await?;
        Ok(())
    }

    /// This function goes through all tables of the database and returns all columns that contain the query.
    pub async fn scan_database(&self, query: &str, strict: bool) -> anyhow::Result<Vec<String>> {
        self.with_transaction(async |txn| {
            let tables = query!("SELECT name FROM sqlite_schema WHERE type='table'")
                .fetch_all(&mut **txn)
                .await?;

            let mut result = Vec::new();

            for table in tables {
                for row in sqlx::query(&format!("SELECT * FROM '{}'", table.name.unwrap()))
                    .fetch_all(&mut **txn)
                    .await?
                {
                    for i in 0..row.len() {
                        let string = if let Ok(column) = row.try_get::<String, _>(i) {
                            column
                        } else if let Ok(column) = row.try_get::<Vec<u8>, _>(i) {
                            String::from_utf8_lossy(&column).to_string()
                        } else {
                            // Unable to decode this type
                            continue;
                        };

                        if string.contains(query) {
                            result.push(string.to_string());
                            continue;
                        }

                        if !strict {
                            // Try again without 0x18, because that's the CBOR unsigned byte indicator for Vec<u8>
                            let string2 = string.replace('\x18', "");
                            if string2.contains(query) {
                                result.push(string.to_string());
                                continue;
                            }
                        }
                    }
                }
            }

            Ok(result)
        })
        .await
    }
}

impl StoreExt for CoreUser {
    fn pool(&self) -> &SqlitePool {
        &self.inner.pool
    }

    fn notifier(&self) -> StoreNotifier {
        StoreNotifier::new(self.inner.store_notifications_tx.clone())
    }
}

#[derive(Debug, Clone)]
pub struct QsListenResponder {
    responder: ListenResponder,
}

#[derive(Debug, thiserror::Error)]
pub enum QsListenResponderError {
    #[error(transparent)]
    Closed(#[from] ListenResponderClosedError),
}

impl QsListenResponder {
    pub async fn ack(&self, up_to_sequence_number: u64) -> Result<(), QsListenResponderError> {
        self.responder.ack(up_to_sequence_number).await?;
        Ok(())
    }

    pub async fn fetch(&self) -> Result<(), QsListenResponderError> {
        self.responder.fetch().await?;
        Ok(())
    }
}
