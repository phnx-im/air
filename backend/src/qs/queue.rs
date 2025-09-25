// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{
    collections::{HashMap, VecDeque},
    sync::Arc,
};

use aircommon::identifiers::QsClientId;
use airprotos::queue_service::v1::{
    QueueEmpty, QueueEvent, QueueEventPayload, QueueMessage, queue_event,
};
use futures_util::{Stream, stream};
use sqlx::{Connection, PgConnection, PgExecutor, PgPool, query_scalar};
use tokio::sync::{Mutex, mpsc};
use tokio_stream::StreamExt;
use tokio_util::sync::CancellationToken;
use tracing::error;
use uuid::Uuid;

use crate::{
    errors::{QueueError, StorageError},
    pg_listen::{PgChannelName, PgListenerTaskHandle, spawn_pg_listener_task},
};

/// Maximum number of messages to fetch at once.
const MAX_BUFFER_SIZE: usize = 32;

#[derive(Debug, Clone)]
pub(crate) struct Queues {
    pool: PgPool,
    listeners: Arc<Mutex<HashMap<QsClientId, ListenerContext>>>,
    pg_listener_task_handle: PgListenerTaskHandle<QsClientId>,
}

/// Context for a queue listener
///
/// Cancels background tasks when dropped.
#[derive(Debug)]
struct ListenerContext {
    cancel: CancellationToken,
    payload_tx: mpsc::Sender<QueueEventPayload>,
}

impl Drop for ListenerContext {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}

impl Queues {
    pub(crate) async fn new(pool: PgPool) -> sqlx::Result<Self> {
        let pg_listener_task_handle = spawn_pg_listener_task(pool.clone()).await?;
        Ok(Self {
            pool,
            listeners: Default::default(),
            pg_listener_task_handle,
        })
    }

    pub(crate) async fn listen(
        &self,
        queue_id: QsClientId,
        sequence_number_start: u64,
    ) -> Result<impl Stream<Item = Option<QueueEvent>> + use<>, QueueError> {
        let notifications = self.pg_listener_task_handle.subscribe(queue_id);
        let (payload_tx, payload_rx) = tokio::sync::mpsc::channel(1024);

        let cancel = self.track_listener(queue_id, payload_tx).await?;
        let context = QueueStreamContext {
            pool: self.pool.clone(),
            notifications,
            queue_id,
            sequence_number: sequence_number_start,
            cancel,
            buffer: VecDeque::with_capacity(MAX_BUFFER_SIZE),
            state: FetchState::Fetch,
        };

        let message_stream = context.into_stream().map(|message| match message {
            Some(message) => Some(QueueEvent {
                event: Some(queue_event::Event::Message(message)),
            }),
            None => Some(QueueEvent {
                event: Some(queue_event::Event::Empty(QueueEmpty {})),
            }),
        });

        let payload_stream =
            tokio_stream::wrappers::ReceiverStream::new(payload_rx).map(|payload| {
                Some(QueueEvent {
                    event: Some(queue_event::Event::Payload(payload)),
                })
            });

        let event_stream = stream::select(message_stream, payload_stream);

        Ok(event_stream)
    }

    pub(crate) async fn enqueue(
        &self,
        connection: &mut PgConnection,
        queue_id: QsClientId,
        message: &QueueMessage,
    ) -> Result<bool, QueueError> {
        let mut txn = connection.begin().await?;

        Queue::enqueue(&mut txn, queue_id, message).await?;
        let query = format!(r#"NOTIFY "{}""#, pg_queue_label(queue_id));
        sqlx::query(&query).execute(txn.as_mut()).await?;

        txn.commit().await?;

        let is_listening = self.listeners.lock().await.contains_key(&queue_id);
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
        let query = queue_id.notify_query();
        sqlx::query(&query).execute(&self.pool).await?;
        Ok(())
    }

    async fn track_listener(
        &self,
        client_id: QsClientId,
        payload_tx: mpsc::Sender<QueueEventPayload>,
    ) -> sqlx::Result<CancellationToken> {
        let mut listeners = self.listeners.lock().await;
        for (id, _) in listeners.extract_if(|_, context| context.cancel.is_cancelled()) {
            self.pg_listener_task_handle.unlisten(id).await;
        }

        let cancel = CancellationToken::new();
        let context = ListenerContext {
            cancel: cancel.clone(),
            payload_tx,
        };

        if listeners.insert(client_id, context).is_none() {
            self.pg_listener_task_handle.listen(client_id).await;
        }

        Ok(cancel)
    }

    pub(crate) async fn stop_listening(&self, queue_id: QsClientId) {
        if let Some(context) = self.listeners.lock().await.remove(&queue_id) {
            context.cancel.cancel();
        }
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

pub trait QueueNotifier {
    /// Returns whether the queue id is currently listened to.
    ///
    /// Takes an additional `connection` parameter to allow to call this function as a part of a
    /// transaction.
    async fn enqueue(
        &self,
        connection: &mut PgConnection,
        queue_id: QsClientId,
        message: &QueueMessage,
    ) -> Result<bool, QueueError>;

    /// Sends a payload to the queue.
    ///
    /// The payload is only sent if the queue is listening.
    async fn send_payload(
        &self,
        queue_id: QsClientId,
        payload: QueueEventPayload,
    ) -> Result<bool, QueueError>;
}

impl QueueNotifier for Queues {
    async fn enqueue(
        &self,
        connection: &mut PgConnection,
        queue_id: QsClientId,
        message: &QueueMessage,
    ) -> Result<bool, QueueError> {
        self.enqueue(connection, queue_id, message).await
    }

    async fn send_payload(
        &self,
        queue_id: QsClientId,
        payload: QueueEventPayload,
    ) -> Result<bool, QueueError> {
        let Some(tx) = self
            .listeners
            .lock()
            .await
            .get(&queue_id)
            .map(|context| context.payload_tx.clone())
        else {
            return Ok(false);
        };
        tx.send(payload).await?;
        Ok(true)
    }
}

struct QueueStreamContext<S> {
    pool: PgPool,
    notifications: S,
    queue_id: QsClientId,
    sequence_number: u64,
    cancel: CancellationToken,
    /// Buffer for already fetched messages
    ///
    /// Invariant: the messages are stored in ascending order by sequence number.
    buffer: VecDeque<QueueMessage>,
    state: FetchState,
}

enum FetchState {
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
            async |mut context| -> Option<(Option<QueueMessage>, Self)> {
                loop {
                    if context.cancel.is_cancelled() {
                        return None;
                    }
                    if let Some(message) = context.buffer.pop_front() {
                        return Some((Some(message), context));
                    }
                    // buffer is empty
                    match context.state {
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
            &self.queue_id,
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

fn pg_queue_label(queue_id: QsClientId) -> String {
    format!("qs_{}", queue_id.as_uuid())
}

pub(super) struct Queue {
    queue_id: QsClientId,
    sequence_number: i64,
}

mod persistence {
    use super::*;

    use airprotos::queue_service::v1::QueueMessage;
    use prost::Message;
    use sqlx::{
        Database, Decode, Encode, Postgres, Type, encode::IsNull, error::BoxDynError, query,
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
            buf: &mut <Postgres as Database>::ArgumentBuffer<'q>,
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
        pub(crate) async fn new_and_store<'a>(
            queue_id: QsClientId,
            connection: impl PgExecutor<'_>,
        ) -> Result<Self, StorageError> {
            let queue_data = Self {
                queue_id,
                sequence_number: 0,
            };
            queue_data.store(connection).await?;
            Ok(queue_data)
        }

        pub(super) async fn store(
            &self,
            connection: impl PgExecutor<'_>,
        ) -> Result<(), StorageError> {
            sqlx::query!(
                "INSERT INTO
                    qs_queue_data
                    (queue_id, sequence_number)
                VALUES
                    ($1, $2)",
                &self.queue_id as &QsClientId,
                self.sequence_number,
            )
            .execute(connection)
            .await?;
            Ok(())
        }

        pub(super) async fn enqueue(
            connection: &mut PgConnection,
            queue_id: QsClientId,
            message: &QueueMessage,
        ) -> Result<(), QueueError> {
            // Begin the transaction
            let mut transaction = connection.begin().await?;

            // Update and get the sequence number, saving one query
            let sequence_number = sqlx::query_scalar!(
                r#"WITH updated_sequence AS (
                    -- Step 1: Update and return the current sequence number.
                    UPDATE qs_queue_data
                    SET sequence_number = sequence_number + 1
                    WHERE queue_id = $1
                    RETURNING sequence_number - 1 as sequence_number
                )
                -- Step 2: Insert the message with the new sequence number.
                INSERT INTO qs_queues (queue_id, sequence_number, message_bytes)
                SELECT $1, sequence_number, $2 FROM updated_sequence
                RETURNING sequence_number
                "#,
                queue_id as QsClientId,
                SqlQueueMessageRef(message) as _,
            )
            .fetch_one(&mut *transaction)
            .await?;

            // Check if the sequence number matches the one we got from the query. If it doesn't,
            // we return an error and automatically rollback the transaction.
            if sequence_number != message.sequence_number as i64 {
                tracing::warn!(
                    "Sequence number mismatch. Message sequence number {}, queue sequence number {}",
                    message.sequence_number,
                    sequence_number
                );
                return Err(QueueError::SequenceNumberMismatch);
            }

            transaction.commit().await?;

            Ok(())
        }

        pub(super) async fn fetch_into(
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
