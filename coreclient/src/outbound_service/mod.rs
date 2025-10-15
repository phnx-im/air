// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::{credentials::keys::ClientSigningKey, identifiers::UserId};
use sqlx::SqlitePool;
use tokio::sync::mpsc;
use tracing::error;

use crate::{
    clients::api_clients::ApiClients,
    store::{StoreNotificationsSender, StoreNotifier},
};

mod receipts;

#[derive(Debug)]
pub(crate) struct OutboundService {
    pool: SqlitePool,
    tx: mpsc::Sender<OutboundServiceOp>,
}

impl OutboundService {
    pub(crate) fn new(
        pool: SqlitePool,
        api_clients: ApiClients,
        client_signing_key: ClientSigningKey,
        store_notifications_tx: StoreNotificationsSender,
    ) -> Self {
        let (tx, rx) = mpsc::channel(1024);
        tokio::spawn(
            OutboundServiceTask {
                pool: pool.clone(),
                rx,
                api_clients,
                signing_key: client_signing_key,
                store_notifications_tx,
            }
            .run(),
        );
        Self { pool, tx }
    }

    pub(crate) async fn start(&self) {
        self.tx.send(OutboundServiceOp::Start).await.ok();
    }

    pub(crate) async fn stop(&self) {
        self.tx.send(OutboundServiceOp::Stop).await.ok();
    }
}

#[derive(Debug, Copy, Clone)]
enum OutboundServiceOp {
    Start,
    Stop,
    Work,
}

struct OutboundServiceTask {
    pool: SqlitePool,
    rx: mpsc::Receiver<OutboundServiceOp>,
    api_clients: ApiClients,
    signing_key: ClientSigningKey,
    store_notifications_tx: StoreNotificationsSender,
}

impl OutboundServiceTask {
    async fn run(mut self) {
        let mut is_stopped = true; // initial state is being stopped
        while let Some(op) = self.rx.recv().await {
            match op {
                OutboundServiceOp::Start => is_stopped = false,
                OutboundServiceOp::Stop => {
                    is_stopped = true;
                    continue;
                }
                OutboundServiceOp::Work if is_stopped => continue,
                OutboundServiceOp::Work => {}
            }

            // do work

            if let Err(error) = self.send_queued_receipts().await {
                error!(%error, "Failed to send queued receipts");
            }
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
