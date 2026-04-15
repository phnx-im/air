// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::fmt;

use tokio::sync::{mpsc, watch};
use tokio_util::sync::CancellationToken;
use tracing::trace;

use crate::{SseEncode, StreamSink};

use super::spawn_from_sync;

/// A Cubit is a stateful stream of states
///
/// This trait corresponds to the [`StateStreamableSource`] interface in the Flutter [BLoC] library.
///
/// [`StateStreamableSource`]: https://pub.dev/documentation/bloc/latest/bloc/StateStreamableSource-class.html
/// [BLoC]: https://pub.dev/packages/bloc
///
/// Note: Currently used only internally, until Flutter Rust Bridge supports traits bridging to
/// Dart.
pub(crate) trait Cubit {
    type State;

    /// Closes the stream and cancels all pending and background operations
    fn close(&self);

    /// Returns `true` if the stream is closed
    fn is_closed(&self) -> bool;

    /// Returns the current state
    fn state(&self) -> Self::State;

    /// Streams new states
    async fn stream(&self, sink: StreamSink<Self::State>);
}

/// Building block for cubits
///
/// Bundles a state, a set of sinks listening to state changes, a cancellation token and a
/// background emitter task [`CubitCore::emitter_loop`]. The latter is spawned in the background on
/// construction.
///
/// The cancellation token is used to cancel all pending and background operations. It can be
/// cancelled by calling [`Cubit::close`] or by dropping the [`CubitCore`].
pub(crate) struct CubitCore<S> {
    state_tx: watch::Sender<S>,
    sinks_tx: mpsc::Sender<StreamSink<S>>,
    cancel: CancellationToken,
}

impl<S> Drop for CubitCore<S> {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}

impl<S: Clone> Cubit for CubitCore<S> {
    type State = S;

    fn is_closed(&self) -> bool {
        self.cancel.is_cancelled()
    }

    fn close(&self) {
        self.cancel.cancel();
    }

    fn state(&self) -> S {
        self.state_tx.borrow().clone()
    }

    async fn stream(&self, sink: StreamSink<S>) {
        if self.is_closed() {
            return;
        }
        // Ignoring the error: send fails only when the emitter loop has
        // already exited (receiver dropped), so there is nothing to close.
        let _ = self.sinks_tx.send(sink).await;
    }
}

impl<S> CubitCore<S>
where
    S: SseEncode + Default + Clone + Send + Sync + fmt::Debug + 'static,
{
    /// Creates a new [`CubitCore`] with default initial state and spawns the emitter task
    pub(crate) fn new() -> Self {
        Self::with_initial_state(S::default())
    }
}

impl<S> CubitCore<S>
where
    S: SseEncode + Clone + Send + Sync + fmt::Debug + 'static,
{
    /// Creates a new [`CubitCore`] with provided initial state and spawns the emitter task
    pub(crate) fn with_initial_state(state: S) -> Self {
        let (state_tx, state_rx) = watch::channel(state);
        let (sinks_tx, sinks_rx) = mpsc::channel(16);
        let cancel = CancellationToken::new();

        spawn_from_sync(Self::emitter_loop(state_rx, sinks_rx, cancel.clone()));

        Self {
            state_tx,
            sinks_tx,
            cancel,
        }
    }

    pub(crate) fn state_tx(&self) -> &watch::Sender<S> {
        &self.state_tx
    }

    pub(crate) fn cancellation_token(&self) -> &CancellationToken {
        &self.cancel
    }

    /// The asynchronous tasks that emits new states to all sinks
    ///
    /// It manages the set of currently active sinks. New sinks are added by calling
    /// [`Cubit::stream`].
    ///
    /// State is emitted via the [`CubitCore::state_tx`] watch channel.
    ///
    /// The task is stopped when the [`CubitCore::cancel`] or [`CubitCore`] is dropped.
    async fn emitter_loop(
        mut state_rx: watch::Receiver<S>,
        mut sinks_rx: mpsc::Receiver<StreamSink<S>>,
        stop: CancellationToken,
    ) {
        let mut sinks = Vec::new();
        loop {
            tokio::select! {
                sink = sinks_rx.recv() => {
                    let Some(sink) = sink else { return };
                    sinks.push(sink);
                },
                changed = state_rx.changed() => {
                    if changed.is_err() {
                        return;
                    }
                },
                _ = stop.cancelled() => {
                    return;
                }
            }

            let state = state_rx.borrow_and_update().clone();
            trace!(num_sinks = sinks.len(), ?state, "emitting new state");
            sinks.retain(|sink| {
                if stop.is_cancelled() {
                    false
                } else {
                    sink.add(state.clone()).is_ok()
                }
            });
        }
    }
}
