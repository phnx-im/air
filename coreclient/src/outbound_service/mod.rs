// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{
    pin::Pin,
    task::{Context, Poll},
};

use aircommon::{
    credentials::keys::ClientSigningKey,
    identifiers::{QsClientId, UserId},
};
use pin_project::pin_project;
use sqlx::SqlitePool;
use tokio::sync::watch;
use tokio_util::sync::{CancellationToken, WaitForCancellationFutureOwned};
use tracing::{debug, error};

use crate::{
    clients::api_clients::ApiClients,
    key_stores::MemoryUserKeyStore,
    store::{StoreNotificationsSender, StoreNotifier},
    utils::{connection_ext::StoreExt, global_lock::GlobalLock},
};

pub use timed_tasks::KEY_PACKAGES;

mod chat_message_queue;
mod chat_messages;
mod error;
mod receipt_queue;
mod receipts;
pub(crate) mod resync;
mod timed_tasks;
pub(crate) mod timed_tasks_queue;

/// A service which is responsible for processing outbound messages.
///
/// The service starts a background task which dequeues messages from the correspoding work queues.
/// The initial state of the service is `Stopped`, that is, the background task is not running. The
/// background task only runs when the service is started, and when there is a notification to run.
/// After doing the work once, it wait for the next notification, or stops if it is stopped.
#[derive(Debug)]
pub struct OutboundService<C: OutboundServiceWork = OutboundServiceContext> {
    context: C,
    run_token_tx: watch::Sender<RunToken>,
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
        key_store: MemoryUserKeyStore,
        qs_client_id: QsClientId,
        store_notifications_tx: StoreNotificationsSender,
        global_lock: GlobalLock,
    ) -> Self {
        let context = OutboundServiceContext {
            pool,
            api_clients,
            key_store,
            qs_client_id,
            store_notifications_tx,
        };
        Self::with_context(context, global_lock)
    }
}

impl<C: OutboundServiceWork> OutboundService<C> {
    fn with_context(context: C, global_lock: GlobalLock) -> Self {
        let (run_token_tx, run_token_rx) = watch::channel(RunToken::new_cancelled());
        let task = OutboundServiceTask {
            context: context.clone(),
        };
        tokio::spawn(task.run(run_token_rx, global_lock));
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
        self.run_token_tx.send_if_modified(|run_token| {
            if !run_token.rotate() {
                run_token.rotate_done();
            }
            done_token = Some(run_token.done.clone());
            true // notify the background task
        });
        debug!("starting background task");
        WaitForDoneFuture::new(done_token)
    }

    /// Notifies the background task to stop.
    ///
    /// Returns a futures which resolves when the background task fully stops.
    pub(crate) fn stop(&self) -> WaitForDoneFuture {
        let mut done_token = None;
        self.run_token_tx.send_if_modified(|run_token| {
            run_token.cancel();
            done_token = Some(run_token.done.clone());
            false // no more work => no need to wake up the background task
        });
        debug!("stopping background task");
        WaitForDoneFuture::new(done_token)
    }

    /// Notifies the background task about new work.
    fn notify_work(&self) -> WaitForDoneFuture {
        let mut done_token = None;
        let notified = self.run_token_tx.send_if_modified(|run_token| {
            if run_token.is_cancelled() {
                false
            } else {
                run_token.rotate_done();
                done_token = Some(run_token.done.clone());
                true
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
    async fn run(self, mut run_token_rx: watch::Receiver<RunToken>, mut global_lock: GlobalLock) {
        loop {
            if run_token_rx.changed().await.is_err() {
                break;
            }

            let run_token = {
                let run_token = run_token_rx.borrow_and_update().clone();
                debug!(?run_token, "incoming work notification");

                if run_token.is_cancelled() {
                    run_token.mark_as_done();
                    continue;
                }

                run_token
            };

            {
                let _guard = global_lock
                    .lock()
                    .await
                    .expect("fatal: failed to acquire global lock");
                debug!("starting doing work in background task");
                self.context.work(run_token.cancel.clone()).await;
                debug!("finished work in background task");
            }

            run_token.mark_as_done();
        }
    }
}

#[derive(Debug, Clone)]
pub struct OutboundServiceContext {
    pool: SqlitePool,
    api_clients: ApiClients,
    key_store: MemoryUserKeyStore,
    qs_client_id: QsClientId,
    store_notifications_tx: StoreNotificationsSender,
}

impl OutboundServiceContext {
    async fn work(&self, run_token: CancellationToken) {
        if let Err(error) = self.perform_queued_resyncs(&run_token).await {
            error!(%error, "Failed to perform queued resyncs");
        }
        if let Err(error) = self.send_queued_receipts(&run_token).await {
            error!(%error, "Failed to send queued receipts");
        }
        if let Err(error) = self.send_queued_messages(&run_token).await {
            error!(%error, "Failed to send queued messages");
        }
        if let Err(error) = self.execute_timed_tasks(&run_token).await {
            error!(%error, "Failed to execute timed tasks");
        }
    }

    fn signing_key(&self) -> &ClientSigningKey {
        &self.key_store.signing_key
    }

    fn user_id(&self) -> &UserId {
        self.signing_key().credential().identity()
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
/// the token is cancelled, the the background work (if any) is cancelled. There is no need to wake
/// up the background task in this case.
///
/// The token also contains a `done` token which is *shared* between the callers and the background
/// task. The background task uses it to mark the work as done. In case the run token is created
/// but the work is immediately cancelled such that the background task never receives the token,
/// the done token is cancelled too.
#[derive(Debug, Default, Clone)]
struct RunToken {
    cancel: CancellationToken,
    done: CancellationToken,
}

impl RunToken {
    fn new() -> Self {
        Default::default()
    }

    fn new_cancelled() -> Self {
        let run_token = RunToken::new();
        run_token.cancel();
        run_token.mark_as_done();
        run_token
    }

    fn is_cancelled(&self) -> bool {
        self.cancel.is_cancelled()
    }

    fn rotate(&mut self) -> bool {
        if self.is_cancelled() {
            *self = RunToken::new();
            true
        } else {
            false
        }
    }

    fn rotate_done(&mut self) -> bool {
        if self.done.is_cancelled() {
            self.done = CancellationToken::new();
            true
        } else {
            false
        }
    }

    fn cancel(&self) {
        self.cancel.cancel();
    }

    fn mark_as_done(&self) {
        self.done.cancel();
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

    use tokio::{
        sync::Notify,
        time::{sleep, timeout},
    };

    use crate::utils::init_test_tracing;

    use super::*;

    fn global_lock() -> GlobalLock {
        let lock_path = std::env::temp_dir().join(format!(
            "air_lock_outbound_test_{}.sqlite",
            uuid::Uuid::new_v4()
        ));
        GlobalLock::from_path(lock_path).unwrap()
    }

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
        let service = OutboundService::with_context(context.clone(), global_lock());

        service.start().await;

        assert_eq!(1, context.counter.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn stop_cancels_work() {
        init_test_tracing();

        let context = DelayedCounterContext::default();
        let service = OutboundService::with_context(context.clone(), global_lock());

        service.start();
        service.stop().await;

        assert_eq!(0, context.counter.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn stop_and_wait() {
        init_test_tracing();

        let context = DelayedCounterContext::default();
        let service = OutboundService::with_context(context.clone(), global_lock());

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
        let service = OutboundService::with_context(context.clone(), global_lock());

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
        let service = OutboundService::with_context(context.clone(), global_lock());

        service.start().await;
        service.notify_work().await;

        assert_eq!(2, context.counter.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn start_is_idempotent() {
        init_test_tracing();

        let context = DelayedCounterContext::default();
        let service = OutboundService::with_context(context.clone(), global_lock());

        service.start();
        service.start();
        service.start();
        service.start().await;
        debug!("done waiting for the last start to finish");
        service.start();
        service.start();
        service.start();
        service.start().await;
        debug!("done waiting for the last start to finish");

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
        let service = OutboundService::with_context(context.clone(), global_lock());

        service.run_once().await;
        assert_eq!(3, context.counter.load(Ordering::SeqCst));

        service.run_once().await;
        assert_eq!(6, context.counter.load(Ordering::SeqCst));

        service.run_once().await;
        assert_eq!(9, context.counter.load(Ordering::SeqCst));

        assert!(service.run_token_tx.subscribe().borrow().is_cancelled());
    }

    #[derive(Clone)]
    struct BlockingWork {
        gate: Arc<Notify>,
        started: Arc<Notify>,
    }

    impl OutboundServiceWork for BlockingWork {
        async fn work(&self, _run_token: CancellationToken) {
            self.started.notify_waiters();
            // Wait until the test explicitly releases the gate.
            self.gate.notified().await;
        }
    }

    #[tokio::test]
    async fn stop_reuses_done_token_for_multiple_waiters() {
        let gate = Arc::new(Notify::new());
        let started = Arc::new(Notify::new());
        let context = BlockingWork {
            gate: gate.clone(),
            started: started.clone(),
        };
        let service = OutboundService::with_context(context, global_lock());

        // Start the worker; do not await the done future.
        service.start();

        // Wait until the background task started work.
        started.notified().await;

        let mut stop1 = Box::pin(service.stop());
        let mut stop2 = Box::pin(service.stop());
        // Both futures should remain pending until the gate is released.
        assert!(
            timeout(Duration::from_millis(10), &mut stop1)
                .await
                .is_err()
        );
        assert!(
            timeout(Duration::from_millis(10), &mut stop2)
                .await
                .is_err()
        );

        gate.notify_waiters();
        tokio::join!(stop1, stop2);
    }

    #[tokio::test]
    async fn stop_ready_after_previous_stop_completed() {
        let gate = Arc::new(Notify::new());
        let started = Arc::new(Notify::new());
        let context = BlockingWork {
            gate: gate.clone(),
            started: started.clone(),
        };
        let service = OutboundService::with_context(context, global_lock());

        service.start();
        started.notified().await;
        let stop_fut = service.stop();
        gate.notify_waiters();
        stop_fut.await;

        // Subsequent stop should resolve immediately using cached done token.
        assert!(
            timeout(Duration::from_millis(10), service.stop())
                .await
                .is_ok()
        );
    }
}
