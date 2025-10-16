// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::{credentials::keys::ClientSigningKey, identifiers::UserId};
use sqlx::SqlitePool;
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;
use tracing::error;

use crate::{
    clients::api_clients::ApiClients,
    store::{StoreNotificationsSender, StoreNotifier},
};

mod receipts;

#[derive(Debug)]
pub struct OutboundService {
    context: OutboundServiceContext,
    run_token_tx: watch::Sender<Option<CancellationToken>>,
}

impl OutboundService {
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

        let (run_token_tx, run_token_rx) = watch::channel(None);
        let task = OutboundServiceTask {
            context: context.clone(),
            run_token_rx,
        };
        tokio::spawn(task.run());

        Self {
            run_token_tx,
            context,
        }
    }

    pub(crate) fn start(&self) {
        self.run_token_tx.send_if_modified(|token| match token {
            Some(_) => false, // already running
            None => {
                token.replace(CancellationToken::new());
                true // start running
            }
        });
    }

    pub(crate) fn stop(&self) {
        self.run_token_tx
            .send_if_modified(|token| token.take().is_some());
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

struct OutboundServiceTask {
    context: OutboundServiceContext,
    run_token_rx: watch::Receiver<Option<CancellationToken>>,
}

impl OutboundServiceTask {
    async fn run(mut self) {
        loop {
            let work_token = match self.run_token_rx.wait_for(|token| token.is_some()).await {
                Ok(work_token) => work_token
                    .clone()
                    .expect("logic error: work token is some and locked"),
                Err(_) => return, // The task is being stopped, so we can return
            };
            self.context.work(work_token).await;
        }
    }
}

#[derive(Debug, Clone)]
struct OutboundServiceContext {
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

    /// Executes a function with a transaction.
    ///
    /// The transaction is committed if the function returns `Ok`, and rolled
    /// back if the function returns `Err`.
    pub(crate) async fn with_transaction<T: Send, E: From<sqlx::Error>>(
        &self,
        f: impl AsyncFnOnce(&mut sqlx::SqliteTransaction<'_>) -> Result<T, E>,
    ) -> Result<T, E> {
        let mut txn = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let value = f(&mut txn).await?;
        txn.commit().await?;
        Ok(value)
    }

    /// Executes a function with a transaction and a [`StoreNotifier`].
    ///
    /// The transaction is committed if the function returns `Ok`, and rolled
    /// back if the function returns `Err`. The [`StoreNotifier`] is notified
    /// after the transaction is committed successfully.
    pub(crate) async fn with_transaction_and_notifier<T: Send, E: From<sqlx::Error>>(
        &self,
        f: impl AsyncFnOnce(&mut sqlx::SqliteTransaction<'_>, &mut StoreNotifier) -> Result<T, E>,
    ) -> Result<T, E> {
        let mut txn = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let mut notifier = StoreNotifier::new(self.store_notifications_tx.clone());
        let value = f(&mut txn, &mut notifier).await?;
        txn.commit().await?;
        notifier.notify();
        Ok(value)
    }
}
