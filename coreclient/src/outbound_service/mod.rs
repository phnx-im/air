// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::{credentials::keys::ClientSigningKey, identifiers::UserId};
use sqlx::SqlitePool;
use tokio::sync::watch;
use tokio_stream::{StreamExt, wrappers::WatchStream};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error};

use crate::{
    clients::api_clients::ApiClients,
    store::{StoreNotificationsSender, StoreNotifier},
    utils::connection_ext::StoreExt,
};

mod receipt_queue;
mod receipts;

/// A service which is responsible for processing outbound messages.
///
/// The service starts a background task which dequeues messages from the correspoding work queues.
/// The initial state of the service is `Stopped`, that is, the background task is not running. The
/// background task only runs when the service is started, and when there is a notification to run.
/// After doing the work once, it wait for the next notification, or stops if it is stopped.
#[derive(Debug)]
pub struct OutboundService<C: OutboundServiceWork = OutboundServiceContext> {
    context: C,
    run_token_tx: watch::Sender<Option<CancellationToken>>,
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
            run_token_rx,
        };
        tokio::spawn(task.run());
        Self {
            context,
            run_token_tx,
        }
    }

    pub(crate) fn start(&self) {
        self.run_token_tx.send_if_modified(|token| match token {
            Some(_) => false, // already running
            None => {
                debug!("starting background task");
                token.replace(CancellationToken::new());
                true // start running
            }
        });
    }

    pub(crate) fn stop(&self) {
        let stopped = self
            .run_token_tx
            .send_if_modified(|token| token.take().is_some());
        debug!(stopped, "stopping background task");
    }

    /// Notify the background task about new work.
    fn notify_task(&self) {
        self.run_token_tx.send_if_modified(|token| token.is_some());
    }

    /// Run the background task immediately.
    ///
    /// This method must *must not* be called when the background task is running.
    pub async fn run_now(&self) {
        let run_token = CancellationToken::new();
        self.context.work(run_token).await;
    }
}

struct OutboundServiceTask<C> {
    context: C,
    run_token_rx: watch::Receiver<Option<CancellationToken>>,
}

impl<C: OutboundServiceWork> OutboundServiceTask<C> {
    async fn run(self) {
        let mut stream = WatchStream::new(self.run_token_rx.clone());
        while let Some(work_token) = stream.next().await {
            if let Some(work_token) = work_token {
                debug!("starting doing work in background task");
                self.context.work(work_token).await;
                debug!("finished work in background task");
            }
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
        if let Err(error) = self.send_queued_receipts(run_token).await {
            error!(%error, "Failed to send queued receipts");
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

#[cfg(test)]
mod test {
    use std::{future, time::Duration};

    use tokio::time::{sleep, timeout};

    use super::*;

    #[derive(Debug, Clone, Default)]
    struct CounterContext {
        tx: watch::Sender<usize>,
    }

    impl OutboundServiceWork for CounterContext {
        async fn work(&self, _run_token: CancellationToken) {
            self.tx.send_modify(|v| *v += 1);
        }
    }

    #[tokio::test]
    async fn start_triggers_work() {
        let (tx, mut rx) = watch::channel(0);
        let context = CounterContext { tx };
        let service = OutboundService::with_context(context);

        service.start();
        sleep(Duration::from_millis(50)).await;

        timeout(Duration::from_millis(100), rx.wait_for(|v| *v == 1))
            .await
            .unwrap()
            .unwrap();
    }

    #[tokio::test]
    async fn stop_cancels_work() {
        #[derive(Clone)]
        struct TestContext {
            tx: watch::Sender<bool>,
        }

        impl OutboundServiceWork for TestContext {
            async fn work(&self, run_token: CancellationToken) {
                run_token.cancelled_owned().await;
                self.tx.send(true).unwrap();
            }
        }

        let (tx, mut rx) = watch::channel(false);
        let context = TestContext { tx };
        let service = OutboundService::with_context(context);

        service.start();
        sleep(Duration::from_millis(50)).await;
        service.stop();

        timeout(Duration::from_millis(100), rx.wait_for(|v| !*v))
            .await
            .unwrap()
            .unwrap();
    }

    #[tokio::test]
    async fn notify_triggers_another_run() {
        let (tx, mut rx) = watch::channel(0);
        let context = CounterContext { tx };
        let service = OutboundService::with_context(context);

        service.start();
        sleep(Duration::from_millis(100)).await;

        service.notify_task();
        sleep(Duration::from_millis(100)).await;

        timeout(Duration::from_millis(100), rx.wait_for(|v| *v == 2))
            .await
            .unwrap()
            .unwrap();
    }

    #[tokio::test]
    async fn start_is_idempotent() {
        let (tx, mut rx) = watch::channel(0);
        let context = CounterContext { tx };
        let service = OutboundService::with_context(context);

        service.start();
        service.start();
        sleep(Duration::from_millis(100)).await;

        timeout(Duration::from_millis(100), rx.wait_for(|v| *v == 1))
            .await
            .unwrap()
            .unwrap();
    }
}
