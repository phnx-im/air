// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Facilities for listening to Postgres notifications and managing the multiplexed notifications
//! and lifetimes of the listener.

use std::{
    collections::{HashMap, hash_map::Entry},
    hash::Hash,
};

use futures_util::Stream;
use sqlx::{
    PgPool,
    postgres::{PgListener, PgNotification},
};
use thiserror::Error;
use tokio::sync::{broadcast, mpsc, oneshot};
use tokio_stream::{StreamExt, wrappers::BroadcastStream};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info};

/// A handle to a running [`PgListener`] task.
///
/// When the last handle is dropped, the task is stopped.
#[derive(Debug, Clone)]
pub(crate) struct PgListenerTaskHandle<C> {
    broadcast: broadcast::Sender<C>,
    listener_tx: mpsc::UnboundedSender<Command<C>>,
}

impl<C: PgChannelName> PgListenerTaskHandle<C> {
    /// Returns a stream that yields whenever a notification is received on `channel`.
    ///
    /// Returns once the channel is listened to in Postgres. Fails if listening in Postgres fails.
    ///
    /// Many streams can be subscribed to the same channel. It is listened to in Postgres only once,
    /// and unlistened when the last stream is dropped.
    pub(crate) async fn subscribe(
        &self,
        channel: C,
    ) -> sqlx::Result<impl Stream<Item = ()> + Send + use<C>> {
        let rx = self.broadcast.subscribe();
        let (resp_tx, resp) = oneshot::channel();
        let subscription = Subscription::new(channel.clone(), self.listener_tx.clone(), resp_tx);
        resp.await.map_err(|_| sqlx::Error::WorkerCrashed)??;
        Ok(BroadcastStream::new(rx)
            .filter_map(move |recv_channel| {
                // Keeps the subscription alive until the stream is dropped.
                let _subscription = &subscription;
                recv_channel
                    .inspect_err(|error| {
                        error!(%error, "Receiving channel lagged");
                    })
                    .ok()
                    .filter(|recv_channel| recv_channel == &channel)
            })
            .map(|_| ())
            .fuse())
    }
}

/// Unlistens from the channel on drop.
struct Subscription<C: Clone> {
    channel: C,
    listener_tx: mpsc::UnboundedSender<Command<C>>,
}

impl<C: Clone> Subscription<C> {
    fn new(
        channel: C,
        listener_tx: mpsc::UnboundedSender<Command<C>>,
        resp_tx: oneshot::Sender<sqlx::Result<()>>,
    ) -> Self {
        // Sent after the broadcast receiver exists, so no notification is missed.
        let _ = listener_tx.send(Command::Listen(channel.clone(), resp_tx));
        Self {
            channel,
            listener_tx,
        }
    }
}

impl<C: Clone> Drop for Subscription<C> {
    fn drop(&mut self) {
        let _ = self
            .listener_tx
            .send(Command::Unlisten(self.channel.clone()));
    }
}

/// Tracks how many streams are listening to a channel.
#[derive(Default)]
struct Subscribers {
    /// Number of streams listening to the channel.
    count: usize,
    /// Whether the channel is listened to in Postgres.
    listening: bool,
}

/// Spawns a new task that listens to Postgres notifications.
///
/// A connection is held open for the duration of the task. The task will stop when the last
/// [`PgListenerTaskHandle`] is dropped.
pub(crate) async fn spawn_pg_listener_task<C: PgChannelName>(
    pool: PgPool,
    stop: CancellationToken,
) -> sqlx::Result<PgListenerTaskHandle<C>> {
    let (broadcast, _) = broadcast::channel(1024);
    let mut listener = PgListener::connect_with(&pool).await?;
    let (listener_tx, mut listener_rx) = mpsc::unbounded_channel();

    // Cancelled when listener_tx is dropped
    let broadcast_inner = broadcast.clone();
    tokio::spawn(stop.run_until_cancelled_owned(async move {
        info!("Starting pg listener task");
        let mut subscribers: HashMap<C, Subscribers> = HashMap::new();
        loop {
            let event = tokio::select! {
                notification = listener.recv() => notification.into(),
                command = listener_rx.recv() => {
                    let Some(command) = command else {
                        return; // stop the task
                    };
                    LoopEvent::from(command)
                }
            };
            if let Err(error) =
                handle_loop_event(&mut listener, &broadcast_inner, &mut subscribers, event).await
            {
                error!(%error, "Error handling listener loop event");
            }
        }
    }));

    Ok(PgListenerTaskHandle {
        broadcast,
        listener_tx,
    })
}

async fn handle_loop_event<C: PgChannelName>(
    listener: &mut PgListener,
    broadcast: &broadcast::Sender<C>,
    subscribers: &mut HashMap<C, Subscribers>,
    event: LoopEvent<C>,
) -> Result<(), LoopError> {
    match event {
        LoopEvent::Notification(notification) => {
            let notification = notification?;
            let channel = notification.channel();
            debug!(channel, "received notification");
            let channel = C::from_pg_channel(channel)
                .ok_or_else(|| LoopError::InvalidChannel(channel.to_string()))?;
            broadcast.send(channel).ok();
        }
        LoopEvent::Command(Command::Listen(channel, resp_tx)) => {
            let subs = subscribers.entry(channel.clone()).or_default();
            subs.count += 1;
            if subs.listening {
                let _ = resp_tx.send(Ok(()));
            } else {
                let pg_channel = channel.pg_channel();
                debug!(pg_channel, "listen");
                let res = listener.listen(&pg_channel).await;
                subs.listening = res.is_ok();
                let _ = resp_tx.send(res);
            }
        }
        LoopEvent::Command(Command::Unlisten(channel)) => {
            let Entry::Occupied(mut entry) = subscribers.entry(channel) else {
                return Ok(());
            };
            let subs = entry.get_mut();
            subs.count = subs.count.saturating_sub(1);
            if subs.count == 0 {
                let (channel, subs) = entry.remove_entry();
                if subs.listening {
                    let pg_channel = channel.pg_channel();
                    debug!(pg_channel, "unlisten");
                    listener.unlisten(&pg_channel).await?;
                }
            }
        }
    }
    Ok(())
}

#[derive(Debug, Error)]
enum LoopError {
    #[error(transparent)]
    Broadcast(#[from] broadcast::error::RecvError),
    #[error(transparent)]
    Listen(#[from] sqlx::Error),
    #[error("Invalid channel: {0}")]
    InvalidChannel(String),
}

/// A type that can be converted to a Postgres channel name, and back.
pub(crate) trait PgChannelName: PartialEq + Eq + Hash + Send + Clone + 'static {
    fn pg_channel(&self) -> String;

    fn from_pg_channel(channel: &str) -> Option<Self>;
}

enum Command<C> {
    Listen(C, oneshot::Sender<sqlx::Result<()>>),
    Unlisten(C),
}

enum LoopEvent<C> {
    Notification(sqlx::Result<PgNotification>),
    Command(Command<C>),
}

impl<C> From<Command<C>> for LoopEvent<C> {
    fn from(command: Command<C>) -> Self {
        Self::Command(command)
    }
}

impl<C> From<sqlx::Result<PgNotification>> for LoopEvent<C> {
    fn from(notification: sqlx::Result<PgNotification>) -> Self {
        Self::Notification(notification)
    }
}
