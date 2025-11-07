// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Facilities for listening to Postgres notifications and managing the multiplexed notifications
//! and lifetimes of the listener.

use futures_util::Stream;
use sqlx::postgres::{PgListener, PgNotification};
use thiserror::Error;
use tokio::sync::{broadcast, mpsc};
use tokio_stream::StreamExt;
use tracing::{error, info};

/// A handle to a running [`PgListener`] task.
///
/// When the last handle is dropped, the task is stopped.
#[derive(Debug, Clone)]
pub(crate) struct PgListenerTaskHandle<C> {
    broadcast: broadcast::Sender<C>,
    listener_tx: mpsc::Sender<Command<C>>,
}

impl<C: PgChannelName> PgListenerTaskHandle<C> {
    pub(crate) async fn listen(&self, channel: C) {
        if let Err(error) = self.listener_tx.send(Command::Listen(channel)).await {
            error!(%error, "Error sending listen command to pg listener task");
        }
    }

    pub(crate) async fn unlisten(&self, channel: C) {
        if let Err(error) = self.listener_tx.send(Command::Unlisten(channel)).await {
            error!(%error, "Error sending unlisten command to pg listener task");
        }
    }

    pub(crate) fn subscribe(&self, channel: C) -> impl Stream<Item = ()> + Send + use<C> {
        let rx = self.broadcast.subscribe();
        tokio_stream::wrappers::BroadcastStream::new(rx)
            .filter_map(move |recv_channel| {
                recv_channel
                    .inspect_err(|error| {
                        error!(%error, "Receiving channel lagged");
                    })
                    .ok()
                    .filter(|recv_channel| recv_channel == &channel)
            })
            .map(|_| ())
            .fuse()
    }
}

/// Spawns a new task that listens to Postgres notifications.
///
/// A connection is held open for the duration of the task. The task will stop when the last
/// [`PgListenerTaskHandle`] is dropped.
pub(crate) async fn spawn_pg_listener_task<C: PgChannelName>(
    pool: sqlx::PgPool,
) -> sqlx::Result<PgListenerTaskHandle<C>> {
    let (broadcast, _) = broadcast::channel(1024);
    let mut listener = PgListener::connect_with(&pool).await?;
    let (listener_tx, mut listener_rx) = mpsc::channel(1024);

    // Cancelled when listener_tx is dropped
    let broadcast_inner = broadcast.clone();
    tokio::spawn(async move {
        info!("Starting pg listener task");
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
            if let Err(error) = handle_loop_event(&mut listener, &broadcast_inner, event).await {
                error!(%error, "Error handling listener loop event");
            }
        }
    });

    Ok(PgListenerTaskHandle {
        broadcast,
        listener_tx,
    })
}

async fn handle_loop_event<C: PgChannelName>(
    listener: &mut PgListener,
    broadcast: &broadcast::Sender<C>,
    event: LoopEvent<C>,
) -> Result<(), LoopError> {
    match event {
        LoopEvent::Notification(notification) => {
            let notification = notification?;
            let channel = notification.channel();
            let channel = C::from_pg_channel(channel)
                .ok_or_else(|| LoopError::InvalidChannel(channel.to_string()))?;
            broadcast.send(channel).ok();
        }
        LoopEvent::Command(Command::Listen(channel)) => {
            listener.listen(&channel.pg_channel()).await?;
        }
        LoopEvent::Command(Command::Unlisten(channel)) => {
            listener.unlisten(&channel.pg_channel()).await?;
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
pub(crate) trait PgChannelName: PartialEq + Eq + Send + Clone + 'static {
    fn pg_channel(&self) -> String;

    fn from_pg_channel(channel: &str) -> Option<Self>;

    fn notify_query(&self) -> String {
        format!(r#"NOTIFY "{}""#, self.pg_channel())
    }
}

enum Command<C> {
    Listen(C),
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
