// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

pub(crate) mod client_state;

use std::{
    borrow::Cow,
    collections::{HashMap, VecDeque},
    sync::{Arc, Mutex, MutexGuard, PoisonError, Weak},
};

use aircommon::identifiers::{QsClientId, QsUserId};
use airprotos::queue_service::v1::{
    ListenResponse, QueueEmpty, QueueEventPayload, QueueMessage, listen_response,
};
use dashmap::{DashMap, mapref::entry::Entry};
use futures_util::{Stream, stream};
use metrics::gauge;
use semver::Version;
use sqlx::{PgExecutor, PgPool, PgTransaction};
use tokio::sync::mpsc;
use tokio_stream::StreamExt;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error};
use uuid::Uuid;

use self::client_state::ClientState;
use crate::{
    errors::QueueError,
    pg_listen::{PgChannelName, PgListenerTaskHandle, spawn_pg_listener_task},
    qs::{METRIC_AIR_ACTIVE_USERS, client_record::QsClientRecord},
};

/// Maximum number of messages to fetch at once.
const MAX_BUFFER_SIZE: usize = 32;

#[derive(Debug, Clone)]
pub(crate) struct Queues {
    pool: PgPool,
    listeners: Arc<DashMap<QsClientId, ListenerContext>>,
    pg_listener_task_handle: PgListenerTaskHandle<QsClientId>,
    /// Client states per user, owned by the listeners
    user_clients: Arc<DashMap<QsUserId, Weak<UserClients>>>,
}

/// Context for a queue listener
///
/// Cancels background tasks when dropped.
#[derive(Debug)]
struct ListenerContext {
    cancel: CancellationToken,
    payload_tx: mpsc::Sender<ListenResponse>,
    /// Clients of the same user, kept alive while one of them listens
    user_clients: Arc<UserClients>,
}

impl ListenerContext {
    fn new(
        cancel: CancellationToken,
        client_version: Option<&Version>,
        payload_tx: mpsc::Sender<ListenResponse>,
        user_clients: Arc<UserClients>,
    ) -> Self {
        let client_version_label = client_version_label(client_version);
        gauge!(
            METRIC_AIR_ACTIVE_USERS,
            "client_version" => client_version_label,
        )
        .increment(1);
        Self {
            cancel,
            payload_tx,
            user_clients,
        }
    }
}

impl Drop for ListenerContext {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}

#[derive(Debug, Default)]
struct UserClients {
    /// Last client state change per client
    ///
    /// Kept apart from the listeners, since it outlives them.
    clients: Mutex<HashMap<QsClientId, ClientEntry>>,
}

impl UserClients {
    fn lock(&self) -> MutexGuard<'_, HashMap<QsClientId, ClientEntry>> {
        self.clients.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

#[derive(Debug, Default)]
struct ClientEntry {
    /// Epoch of the last state change, kept across sessions (listeners)
    epoch: u64,
    /// Counter for the ids of the client's sessions
    last_session_id: u64,
    /// Current listen session, if any
    session: Option<ClientSession>,
}

impl ClientEntry {
    /// Returns the epoch of the next state change, starting at 1.
    fn next_epoch(&mut self) -> u64 {
        self.epoch += 1;
        self.epoch
    }
}

#[derive(Debug)]
struct ClientSession {
    session_id: u64,
    /// Clone of the `ListenerContext` sender for fan-out
    payload_tx: mpsc::Sender<ListenResponse>,
    /// Last state reported in this session
    state: Option<ClientState>,
}

impl Queues {
    pub(crate) async fn new(pool: PgPool, stop: CancellationToken) -> sqlx::Result<Self> {
        let pg_listener_task_handle = spawn_pg_listener_task(pool.clone(), stop).await?;
        Ok(Self {
            pool,
            listeners: Default::default(),
            pg_listener_task_handle,
            user_clients: Default::default(),
        })
    }

    /// Starts a listen session of `client_id`, replacing a previous one.
    ///
    /// Returns the id of the session and its events.
    pub(crate) async fn listen(
        &self,
        client_id: QsClientId,
        client_version: Option<Version>,
        sequence_number_start: u64,
    ) -> Result<(u64, impl Stream<Item = Option<ListenResponse>> + use<>), QueueError> {
        let user_id = QsClientRecord::load_user_id(&self.pool, &client_id)
            .await?
            .ok_or(QueueError::ClientNotFound)?;
        let notifications = self.pg_listener_task_handle.subscribe(client_id);
        let (payload_tx, payload_rx) = mpsc::channel(1024);

        let (session_id, cancel) =
            self.track_listener(user_id, client_id, client_version.as_ref(), payload_tx);
        let context = QueueStreamContext {
            pool: self.pool.clone(),
            notifications,
            client_id,
            client_version,
            sequence_number: sequence_number_start,
            cancel,
            buffer: VecDeque::with_capacity(MAX_BUFFER_SIZE),
            state: FetchState::Init,
        };

        let message_stream = context.into_stream().map(|message| match message {
            Some(message) => Some(ListenResponse {
                event: Some(listen_response::Event::Message(message)),
            }),
            None => Some(ListenResponse {
                event: Some(listen_response::Event::Empty(QueueEmpty {})),
            }),
        });

        let payload_stream = tokio_stream::wrappers::ReceiverStream::new(payload_rx).map(Some);

        let event_stream = stream::select(message_stream, payload_stream);

        Ok((session_id, event_stream))
    }

    pub(crate) async fn enqueue(
        &self,
        txn: &mut PgTransaction<'_>,
        queue_id: QsClientId,
        message: &QueueMessage,
    ) -> Result<bool, QueueError> {
        Queue::enqueue(txn.as_mut(), queue_id, message).await?;
        sqlx::query("SELECT pg_notify($1, '')")
            .bind(queue_id.pg_channel())
            .execute(txn.as_mut())
            .await?;

        let is_listening = self
            .listeners
            .get(&queue_id)
            .map(|context| !context.cancel.is_cancelled())
            .unwrap_or(false);
        Ok(is_listening)
    }

    pub(crate) async fn ack(
        &self,
        queue_id: QsClientId,
        up_to_sequence_number: u64,
    ) -> Result<(), QueueError> {
        Queue::delete(&self.pool, queue_id, up_to_sequence_number).await?;
        Ok(())
    }

    pub(crate) async fn trigger_fetch(&self, queue_id: QsClientId) -> Result<(), QueueError> {
        sqlx::query("SELECT pg_notify($1, '')")
            .bind(queue_id.pg_channel())
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub(crate) async fn send_payload(
        &self,
        queue_id: QsClientId,
        payload: QueueEventPayload,
    ) -> Result<bool, QueueError> {
        let Some(tx) = self
            .listeners
            .get(&queue_id)
            .map(|context| context.payload_tx.clone())
        else {
            return Ok(false);
        };
        tx.send(ListenResponse {
            event: Some(listen_response::Event::Payload(payload)),
        })
        .await?;
        Ok(true)
    }

    /// Registers the listener of `client_id`, replacing a previous one.
    ///
    /// Returns the id of the new session and the cancellation token of the
    /// listener.
    fn track_listener(
        &self,
        user_id: QsUserId,
        client_id: QsClientId,
        client_version: Option<&Version>,
        payload_tx: mpsc::Sender<ListenResponse>,
    ) -> (u64, CancellationToken) {
        // Clean up cancelled listeners, and then the clients they kept alive
        self.listeners.retain(|id, context| {
            if context.cancel.is_cancelled() {
                self.pg_listener_task_handle.unlisten(*id);
                false
            } else {
                true
            }
        });
        self.user_clients
            .retain(|_, user_clients| user_clients.strong_count() > 0);

        let user_clients = self.user_clients_of(user_id);
        let cancel = CancellationToken::new();
        let context = ListenerContext::new(
            cancel.clone(),
            client_version,
            payload_tx.clone(),
            user_clients.clone(),
        );

        // Holding the entry keeps the session in line with the listener when
        // the same client listens concurrently.
        let entry = self.listeners.entry(client_id);
        let session_id = user_clients.start_session(client_id, payload_tx);
        let is_new = match entry {
            Entry::Occupied(mut entry) => {
                entry.insert(context).cancel.cancel();
                false
            }
            Entry::Vacant(entry) => {
                entry.insert(context);
                true
            }
        };
        if is_new {
            self.pg_listener_task_handle.listen(client_id);
        }

        (session_id, cancel)
    }

    /// Returns the clients of `user_id`, creating them if no listener holds
    /// them.
    fn user_clients_of(&self, user_id: QsUserId) -> Arc<UserClients> {
        let mut entry = self.user_clients.entry(user_id).or_default();
        entry.upgrade().unwrap_or_else(|| {
            let user_clients = Arc::default();
            *entry = Arc::downgrade(&user_clients);
            user_clients
        })
    }

    /// Returns the clients of the user of `client_id`, if it is listening.
    fn listening_user_clients(&self, client_id: QsClientId) -> Option<Arc<UserClients>> {
        Some(self.listeners.get(&client_id)?.user_clients.clone())
    }
}

impl PgChannelName for QsClientId {
    fn pg_channel(&self) -> String {
        format!("qs_{}", self.as_uuid())
    }

    fn from_pg_channel(channel: &str) -> Option<Self> {
        let uuid: Uuid = channel.strip_prefix("qs_")?.parse().ok()?;
        Some(uuid.into())
    }
}

struct QueueStreamContext<S> {
    pool: PgPool,
    notifications: S,
    client_id: QsClientId,
    client_version: Option<Version>,
    sequence_number: u64,
    cancel: CancellationToken,
    /// Buffer for already fetched messages
    ///
    /// Invariant: the messages are stored in ascending order by sequence number.
    buffer: VecDeque<QueueMessage>,
    state: FetchState,
}

impl<S> Drop for QueueStreamContext<S> {
    fn drop(&mut self) {
        self.cancel.cancel();
        let client_version_label = client_version_label(self.client_version.as_ref());
        gauge!(
            METRIC_AIR_ACTIVE_USERS,
            "client_version" => client_version_label
        )
        .decrement(1);
        debug!(queue_id =? self.client_id, "QS queue stream stopped");
    }
}

#[derive(Debug, PartialEq, Eq)]
enum FetchState {
    /// Update the activity time of the client record.
    Init,
    /// Fetch the next message.
    Fetch,
    /// Wait for a notification to fetch the next message.
    ///
    /// This state is used when the queue is empty.
    Wait,
}

impl<S: Stream<Item = ()> + Send + Unpin> QueueStreamContext<S> {
    fn into_stream(self) -> impl Stream<Item = Option<QueueMessage>> + Send {
        stream::unfold(
            self,
            // Note: This function must be cancellation safe, because the stream can be dropped any
            // time, when the client disconnects.
            async |mut context| -> Option<(Option<QueueMessage>, Self)> {
                loop {
                    if let Some(message) = context.buffer.pop_front() {
                        return Some((Some(message), context));
                    }

                    // Check if the task is cancelled, but do it *after* fetching messages at least
                    // once. Otherwise, concurrent streams might cancel each other before any
                    // progress is done.
                    if context.cancel.is_cancelled() && context.state != FetchState::Init {
                        return None;
                    }

                    // buffer is empty
                    match context.state {
                        FetchState::Init => {
                            context.state = FetchState::Fetch;
                        }
                        FetchState::Fetch => {
                            context.fetch_next_messages().await?;
                            if context.buffer.is_empty() {
                                // return sentinel value to indicate that the queue is empty
                                context.state = FetchState::Wait;
                                return Some((None, context));
                            }
                        }
                        FetchState::Wait => {
                            context.wait_for_notification().await?;
                            context.state = FetchState::Fetch;
                        }
                    }
                }
            },
        )
    }

    /// Fetches the next batch of messages into the internal buffer.
    async fn fetch_next_messages(&mut self) -> Option<()> {
        debug_assert!(self.buffer.is_empty());
        Queue::fetch_into(
            &self.pool,
            &self.client_id,
            self.sequence_number,
            MAX_BUFFER_SIZE,
            &mut self.buffer,
        )
        .await
        .inspect_err(|error| {
            error!(%error, "failed to fetch next messages");
        })
        .ok()?;
        if let Some(new_sequence_number) = self.buffer.back().map(|m| m.sequence_number) {
            self.sequence_number = new_sequence_number + 1;
        }
        Some(())
    }

    /// Waits for either a new message or for the listener to be cancelled.
    ///
    /// Returns `None` if the listener was cancelled and should stop.
    async fn wait_for_notification(&mut self) -> Option<()> {
        tokio::select! {
            _ = self.notifications.next() => Some(()),
            _ = self.cancel.cancelled() => None,
        }
    }
}

fn client_version_label(client_version: Option<&Version>) -> Cow<'static, str> {
    client_version
        .as_ref()
        .map(|v| v.to_string().into())
        .unwrap_or("unknown".into())
}

pub(super) struct Queue {}

pub(crate) mod persistence {
    use super::*;

    use airprotos::queue_service::v1::QueueMessage;
    use prost::Message;
    use sqlx::{
        Database, Decode, Encode, Postgres, Type, encode::IsNull, error::BoxDynError, query,
        query_scalar,
    };

    #[derive(Debug)]
    pub(super) struct SqlQueueMessage(pub(super) QueueMessage);

    #[derive(Debug)]
    pub(super) struct SqlQueueMessageRef<'a>(pub(super) &'a QueueMessage);

    impl Type<Postgres> for SqlQueueMessageRef<'_> {
        fn type_info() -> <Postgres as Database>::TypeInfo {
            <Vec<u8> as Type<Postgres>>::type_info()
        }
    }

    impl<'q> Encode<'q, Postgres> for SqlQueueMessageRef<'_> {
        fn encode_by_ref(
            &self,
            buf: &mut <Postgres as Database>::ArgumentBuffer,
        ) -> Result<IsNull, BoxDynError> {
            let buf: &mut Vec<u8> = buf.as_mut();
            self.0.encode(buf)?;
            Ok(IsNull::No)
        }
    }

    impl Type<Postgres> for SqlQueueMessage {
        fn type_info() -> <Postgres as Database>::TypeInfo {
            <Vec<u8> as Type<Postgres>>::type_info()
        }
    }

    impl<'r> Decode<'r, Postgres> for SqlQueueMessage {
        fn decode(value: <Postgres as Database>::ValueRef<'r>) -> Result<Self, BoxDynError> {
            let bytes: &[u8] = Decode::<Postgres>::decode(value)?;
            let value = QueueMessage::decode(bytes)?;
            Ok(SqlQueueMessage(value))
        }
    }

    impl Queue {
        pub(super) async fn enqueue(
            executor: impl PgExecutor<'_>,
            queue_id: QsClientId,
            message: &QueueMessage,
        ) -> Result<(), QueueError> {
            query!(
                "INSERT INTO qs_queues (queue_id, sequence_number, message_bytes)
                VALUES ($1, $2, $3)",
                queue_id as QsClientId,
                message.sequence_number as i64,
                SqlQueueMessageRef(message) as _,
            )
            .execute(executor)
            .await?;
            Ok(())
        }

        pub(crate) async fn fetch_into(
            executor: impl PgExecutor<'_>,
            queue_id: &QsClientId,
            sequence_number: u64,
            limit: usize,
            buffer: &mut VecDeque<QueueMessage>,
        ) -> sqlx::Result<()> {
            let mut messages = query_scalar!(
                r#"SELECT message_bytes AS "message: SqlQueueMessage"
                FROM qs_queues
                WHERE queue_id = $1 AND sequence_number >= $2
                ORDER BY sequence_number ASC
                LIMIT $3
                "#,
                queue_id as &QsClientId,
                sequence_number as i64,
                limit as i64,
            )
            .fetch(executor);
            while let Some(SqlQueueMessage(message)) = messages.next().await.transpose()? {
                buffer.push_back(message);
            }
            debug_assert!(
                buffer
                    .iter()
                    .zip(buffer.iter().skip(1))
                    .all(|(a, b)| a.sequence_number + 1 == b.sequence_number),
                "sequence numbers are not consecutive"
            );
            Ok(())
        }

        pub(super) async fn delete(
            executor: impl PgExecutor<'_>,
            queue_id: QsClientId,
            up_to_sequence_number: u64,
        ) -> sqlx::Result<()> {
            query!(
                "DELETE FROM qs_queues WHERE queue_id = $1 AND sequence_number < $2",
                queue_id as QsClientId,
                up_to_sequence_number as i64,
            )
            .execute(executor)
            .await?;
            Ok(())
        }
    }
}
