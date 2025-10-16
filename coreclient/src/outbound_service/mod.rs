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
    utils::connection_ext::StoreExt,
};

mod receipt_queue;
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
}

impl StoreExt for OutboundServiceContext {
    fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    fn notifier(&self) -> StoreNotifier {
        StoreNotifier::new(self.store_notifications_tx.clone())
    }
}
