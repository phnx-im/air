// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use sqlx::{Sqlite, SqliteConnection, pool::PoolConnection};

use crate::{
    clients::{CoreUser, api_clients::ApiClients},
    key_stores::MemoryUserKeyStore,
    store::StoreNotifier,
};

pub(crate) mod chat_operation;
mod pending_chat_operation;

pub(crate) struct JobContext<'a> {
    pub api_clients: &'a ApiClients,
    pub connection: PoolConnection<Sqlite>,
    pub notifier: StoreNotifier,
    pub key_store: &'a MemoryUserKeyStore,
}

pub(crate) trait Job<T> {
    async fn execute(mut self, context: &mut JobContext<'_>) -> anyhow::Result<T>
    where
        Self: Sized,
    {
        self.execute_dependencies(context).await?;
        self.execute_logic(context).await
    }

    async fn execute_logic(self, context: &mut JobContext<'_>) -> anyhow::Result<T>;

    async fn execute_dependencies(&mut self, context: &mut JobContext<'_>) -> anyhow::Result<()>;
}
