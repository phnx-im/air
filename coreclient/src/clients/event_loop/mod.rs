// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! The event loop of the [`CoreUser`].
//!
//! Drives message processing and internal state machines. Implements operations for the
//! [`CoreUser`] via message passing. In particular, the execution of operations and processing of
//! events is linearized.

use std::sync::Weak;

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing::info;

use crate::clients::{
    CoreUserInner,
    event_loop::{
        event::{ClientOperation, RemoteQueueEvent},
        response::{ResponderError, responder},
    },
    process::process_qs::QsStreamProcessor,
};

mod api;
mod event;
mod response;

pub(crate) struct EventLoop {
    remote_queue_event_rx: mpsc::Receiver<RemoteQueueEvent>,
    client_operation_rx: mpsc::Receiver<ClientOperation>,
    cancel: CancellationToken,
}

impl EventLoop {
    /// Creates a new [`EventLoop`].
    ///
    /// Returns the event loop, event loop sender for passing messages to the event loop and a
    /// cancellation token for stopping the event loop.
    pub(crate) fn new() -> (Self, EventLoopSender, CancellationToken) {
        let (remote_queue_event_tx, remote_queue_event_rx) = mpsc::channel(1024);
        let (client_operation_tx, client_operation_rx) = mpsc::channel(1024);

        let cancel = CancellationToken::new();
        let event_loop_sender = EventLoopSender {
            remote_queue_event_tx,
            client_operation_tx,
        };
        let event_loop = Self {
            remote_queue_event_rx,
            client_operation_rx,
            cancel: cancel.clone(),
        };
        (event_loop, event_loop_sender, cancel)
    }

    /// Spawns a taks running the event loop.
    ///
    /// The task stops when one of the following conditions is met:
    /// * the cancellation token from the creation of the event loop is cancelled
    /// * the last instance of the `CoreUser` is dropped
    /// * the event loop sender channels are closed
    pub(crate) fn spawn(self, core_user: Weak<CoreUserInner>) {
        let task = self
            .cancel
            .clone()
            .run_until_cancelled_owned(self.run(core_user));
        tokio::spawn(task);
    }

    async fn run(mut self, core_user: Weak<CoreUserInner>) {
        enum Incoming {
            Remote(RemoteQueueEvent),
            Client(ClientOperation),
        }

        let mut qs_stream_processor = QsStreamProcessor::new(None);

        loop {
            let incoming = tokio::select! {
                biased; // prefer remote queue polling first
                message = self.remote_queue_event_rx.recv() => {
                    match message {
                        Some(message) => Incoming::Remote(message),
                        None => return, // channel closed
                    }
                }
                message = self.client_operation_rx.recv() => {
                    match message {
                        Some(message) => Incoming::Client(message),
                        None => return, // channel closed
                    }
                }
            };

            match incoming {
                Incoming::Remote(RemoteQueueEvent::Qs { event, responder }) => {
                    let Some(core_user) = CoreUserInner::upgrade(&core_user) else {
                        info!("Core user dropped; exit event loop");
                        return;
                    };
                    let result = qs_stream_processor.process_event(&core_user, event).await;
                    responder.send(Ok(result));
                }

                Incoming::Remote(RemoteQueueEvent::Handle {
                    handle,
                    message,
                    responder,
                }) => {
                    let Some(core_user) = CoreUserInner::upgrade(&core_user) else {
                        info!("Core user dropped; exit event loop");
                        return;
                    };
                    let chat_id = core_user
                        .process_handle_queue_message_event_loop(handle, message)
                        .await;
                    responder.send(chat_id.map_err(ResponderError::Fatal));
                }

                Incoming::Client(ClientOperation::ReplaceQsListenResponder(responder)) => {
                    qs_stream_processor.replace_responder(responder);
                }
            }
        }
    }
}

/// Passes messages to the event loop.
#[derive(Debug)]
pub(crate) struct EventLoopSender {
    remote_queue_event_tx: mpsc::Sender<RemoteQueueEvent>,
    client_operation_tx: mpsc::Sender<ClientOperation>,
}

impl EventLoopSender {
    async fn send_remote_queue_event(&self, message: RemoteQueueEvent) {
        let _ = self.remote_queue_event_tx.send(message).await;
    }

    async fn send_client_operation(&self, message: ClientOperation) {
        let _ = self.client_operation_tx.send(message).await;
    }
}
