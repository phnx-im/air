// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{
    pin::Pin,
    sync::{Arc, Mutex},
    task::{Context, Poll},
};

use aircommon::{credentials::keys::ClientSigningKey, identifiers::UserId};
use pin_project::pin_project;
use sqlx::SqlitePool;
use tokio::sync::watch;
use tokio_util::sync::{CancellationToken, DropGuard, WaitForCancellationFutureOwned};
use tracing::{debug, error};

use crate::{
    clients::api_clients::ApiClients,
    store::{StoreNotificationsSender, StoreNotifier},
    utils::connection_ext::StoreExt,
};

mod receipt_queue;
mod receipts;
pub(crate) mod resync;

/// A service which is responsible for processing outbound messages.
///
/// The service starts a background task which dequeues messages from the correspoding work queues.
/// The initial state of the service is `Stopped`, that is, the background task is not running. The
/// background task only runs when the service is started, and when there is a notification to run.
/// After doing the work once, it wait for the next notification, or stops if it is stopped.
#[derive(Debug)]
pub struct OutboundService<C: OutboundServiceWork = OutboundServiceContext> {
    context: C,
    run_token_tx: watch::Sender<Option<RunToken>>,
}

pub trait OutboundServiceWork: Clone + Send + 'static {
    fn work(&self, run_token: CancellationToken) -> impl Future<Output = ()> + Send;
}

impl OutboundServiceWork for OutboundServiceContext {
    async fn work(&self, run_token: CancellationToken) {
        OutboundServiceContext::work(self, run_token).await;
    }
}

impl OutboundService<OutboundServiceContext> {
    pub(crate) fn new(
        pool: SqlitePool,
        api_clients: ApiClients,
        client_signing_key: ClientSigningKey,
        store_notifications_tx: StoreNotificationsSender,
    ) -> Self {
        let context = OutboundServiceContext {
            pool,
            api_clients,
            signing_key: client_signing_key,
            store_notifications_tx,
        };
        Self::with_context(context)
    }
}

impl<C: OutboundServiceWork> OutboundService<C> {
    fn with_context(context: C) -> Self {
        let (run_token_tx, run_token_rx) = watch::channel(None);
        let task = OutboundServiceTask {
            context: context.clone(),
        };
        tokio::spawn(task.run(run_token_rx));
        Self {
            context,
            run_token_tx,
        }
    }

    /// Starts the background task.
    ///
    /// Returns a future which finishes when the background task is done.
    pub(crate) fn start(&self) -> WaitForDoneFuture {
        let mut done_token = None;
        self.run_token_tx.send_if_modified(|token| match token {
            Some(run_token) => {
                done_token = Some(run_token.done_token());
                true // already running
            }
            None => {
                debug!("starting background task");
                let run_token = RunToken::new();
                done_token = Some(run_token.done_token());
                token.replace(run_token);
                true // start running
            }
        });
        WaitForDoneFuture::new(done_token)
    }

    /// Notifies the background task to stop.
    ///
    /// Returns a futures which resolves when the background task fully stops.
    pub(crate) fn stop(&self) -> WaitForDoneFuture {
        let mut done_token = None;
        let stopped = self.run_token_tx.send_if_modified(|token| {
            if let Some(run_token) = token.take() {
                run_token.cancel();
                done_token = Some(run_token.done_token());
                false // no more work => no need to wake up the background task
            } else {
                false // already stopped
            }
        });
        debug!(stopped, "stopping background task");
        WaitForDoneFuture::new(done_token)
    }

    /// Notifies the background task about new work.
    fn notify_work(&self) -> WaitForDoneFuture {
        let mut done_token = None;
        let notified = self.run_token_tx.send_if_modified(|run_token| {
            if let Some(run_token) = run_token {
                done_token = Some(run_token.done_token());
                true
            } else {
                false
            }
        });
        debug!(?notified, "notifying background task about new work");
        WaitForDoneFuture::new(done_token)
    }

    /// Runs the background task and waits until it is done.
    ///
    /// If the background is already running, just waits until it is done.
    ///
    /// The task is stopped in any case.
    pub async fn run_once(&self) {
        self.start().await;
        self.stop().await;
    }
}

struct OutboundServiceTask<C> {
    context: C,
}

impl<C: OutboundServiceWork> OutboundServiceTask<C> {
    async fn run(self, mut run_token_rx: watch::Receiver<Option<RunToken>>) {
        loop {
            if run_token_rx.changed().await.is_err() {
                break;
            }

            let (cancel, done_cell) = {
                let run_token = run_token_rx.borrow_and_update();
                debug!(?run_token, "incoming work notification");

                let Some(run_token) = run_token.as_ref() else {
                    continue;
                };

                (run_token.cancel.clone(), run_token.done_cell.clone())
            };

            debug!("starting doing work in background task");
            self.context.work(cancel).await;
            debug!("finished work in background task");

            // Rotate done token
            *done_cell.lock().expect("poisoned") = DoneToken::new();
        }
    }
}

#[derive(Debug, Clone)]
pub struct OutboundServiceContext {
    pool: SqlitePool,
    api_clients: ApiClients,
    signing_key: ClientSigningKey,
    store_notifications_tx: StoreNotificationsSender,
}

impl OutboundServiceContext {
    async fn work(&self, run_token: CancellationToken) {
        if let Err(error) = self.send_queued_receipts(&run_token).await {
            error!(%error, "Failed to send queued receipts");
        }
        if let Err(error) = self.perform_queued_resyncs(&run_token).await {
            error!(%error, "Failed to perform queued resyncs");
        }
    }

    fn user_id(&self) -> &UserId {
        self.signing_key.credential().identity()
    }
}

impl StoreExt for OutboundServiceContext {
    fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    fn notifier(&self) -> StoreNotifier {
        StoreNotifier::new(self.store_notifications_tx.clone())
    }
}

/// A token send to the background task as work permit.
///
/// The token is stored in a [`tokio::sync::watch`] cell. Whenever, the token is updated, the
/// background task is woken up and uses the token to start work (if it is not running yet). When
/// the token is removed, the the background work (if any) is cancelled. There is no need to wake
/// up the background task in this case.
///
/// The token also contains a `done_cell` which is *shared* between the callers and the background
/// task. The background task uses it to mark the work as done. In case the run token is created
/// but the work is immediately cancelled such that the background task never receives the token,
/// the done cell is dropped (it has only a single reference), which marks the work as done.
///
/// Note: Even though is is possible to make this type `Clone`, it is not implemented, because it
/// makes it easier to argue about how many references of `done_cell` exist. Indeed, at any point
/// in time, there are at most two `done_cell` references: one as part of the `RunToken` in a
/// `watch` cell, and another in the background task.
#[derive(Debug, Default)]
struct RunToken {
    cancel: CancellationToken,
    done_cell: Arc<Mutex<DoneToken>>,
}

impl RunToken {
    fn new() -> Self {
        Default::default()
    }

    fn cancel(&self) {
        self.cancel.cancel();
    }

    fn done_token(&self) -> CancellationToken {
        self.done_cell.lock().expect("poisoned").token.clone()
    }
}

/// A token for notifying or observing that the work in the background task is done.
///
/// It is important that this token also contains the drop guard, so the work is marked as done,
/// even though it never arrived at the background task.
#[derive(Debug)]
struct DoneToken {
    token: CancellationToken,
    _guard: Option<DropGuard>,
}

impl DoneToken {
    fn new() -> Self {
        let token = CancellationToken::new();
        Self {
            token: token.clone(),
            _guard: Some(token.drop_guard()),
        }
    }
}

impl Default for DoneToken {
    fn default() -> Self {
        Self::new()
    }
}

/// A future that resolves when the background task is done.
///
/// This future is not marked as `must_use`, because the default usage of the apis returning this
/// futures is not wait for its completion.
#[pin_project]
pub struct WaitForDoneFuture {
    #[pin]
    done_fut: Option<WaitForCancellationFutureOwned>,
}

impl WaitForDoneFuture {
    fn new(done: Option<CancellationToken>) -> Self {
        Self {
            done_fut: done.map(|done| done.cancelled_owned()),
        }
    }
}

impl Future for WaitForDoneFuture {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match self.project().done_fut.as_pin_mut() {
            Some(fut) => fut.poll(cx),
            None => Poll::Ready(()),
        }
    }
}

#[cfg(test)]
mod test {
    use std::{
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
        time::Duration,
    };

    use tokio::time::{sleep, timeout};

    use crate::utils::init_test_tracing;

    use super::*;

    #[derive(Default, Clone)]
    struct DelayedCounterContext {
        counter: Arc<AtomicUsize>,
    }

    impl OutboundServiceWork for DelayedCounterContext {
        async fn work(&self, run_token: CancellationToken) {
            debug!("starting work in delayed counter");
            sleep(Duration::from_millis(50)).await;
            if !run_token.is_cancelled() {
                debug!("+1 in delayed counter");
                self.counter.fetch_add(1, Ordering::SeqCst);
            } else {
                debug!("work cancelled");
            }
        }
    }

    #[tokio::test]
    async fn start_triggers_work() {
        init_test_tracing();

        let context = DelayedCounterContext::default();
        let service = OutboundService::with_context(context.clone());

        service.start().await;

        assert_eq!(1, context.counter.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn stop_cancels_work() {
        init_test_tracing();

        let context = DelayedCounterContext::default();
        let service = OutboundService::with_context(context.clone());

        service.start();
        service.stop().await;

        assert_eq!(0, context.counter.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn stop_and_wait() {
        init_test_tracing();

        let context = DelayedCounterContext::default();
        let service = OutboundService::with_context(context.clone());

        service.start().await;
        sleep(Duration::from_millis(100)).await; // +1
        service.notify_work();
        sleep(Duration::from_millis(100)).await; // +1
        service.notify_work();
        timeout(Duration::from_millis(100), service.stop())
            .await
            .unwrap(); // cancelled
        assert_eq!(2, context.counter.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn wait_for_idle() {
        let context = DelayedCounterContext::default();
        let service = OutboundService::with_context(context.clone());

        service.start().await;
        sleep(Duration::from_millis(100)).await; // +1
        service.notify_work();
        sleep(Duration::from_millis(100)).await; // +1
        service.notify_work().await; // +1
        assert_eq!(3, context.counter.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn notify_work_triggers_another_run() {
        let context = DelayedCounterContext::default();
        let service = OutboundService::with_context(context.clone());

        service.start().await;
        service.notify_work().await;

        assert_eq!(2, context.counter.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn start_is_idempotent() {
        init_test_tracing();

        let context = DelayedCounterContext::default();
        let service = OutboundService::with_context(context.clone());

        service.start();
        service.start();
        service.start();
        service.start().await;
        service.start();
        service.start();
        service.start();
        service.start().await;

        assert_eq!(2, context.counter.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn run_once() {
        init_test_tracing();

        #[derive(Debug, Clone, Default)]
        struct MultiCounterContext {
            counter: Arc<AtomicUsize>,
        }

        impl OutboundServiceWork for MultiCounterContext {
            async fn work(&self, run_token: CancellationToken) {
                sleep(Duration::from_millis(30)).await;
                if !run_token.is_cancelled() {
                    self.counter.fetch_add(1, Ordering::SeqCst);
                }
                sleep(Duration::from_millis(30)).await;
                if !run_token.is_cancelled() {
                    self.counter.fetch_add(1, Ordering::SeqCst);
                }
                sleep(Duration::from_millis(30)).await;
                if !run_token.is_cancelled() {
                    self.counter.fetch_add(1, Ordering::SeqCst);
                }
            }
        }

        let context = MultiCounterContext::default();
        let service = OutboundService::with_context(context.clone());

        service.run_once().await;
        assert_eq!(3, context.counter.load(Ordering::SeqCst));

        service.run_once().await;
        assert_eq!(6, context.counter.load(Ordering::SeqCst));

        service.run_once().await;
        assert_eq!(9, context.counter.load(Ordering::SeqCst));

        assert!(service.run_token_tx.subscribe().borrow().is_none());
    }
}
