// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use sqlx::{Sqlite, pool::PoolConnection};

use crate::{
    clients::api_clients::ApiClients, key_stores::MemoryUserKeyStore, store::StoreNotifier,
};

pub(crate) mod create_chat;
pub(crate) mod chat_operation;
mod pending_chat_operation;

pub(crate) struct JobContext<'a> {
    pub api_clients: &'a ApiClients,
    pub connection: &'a mut PoolConnection<Sqlite>,
    pub notifier: &'a mut StoreNotifier,
    pub key_store: &'a MemoryUserKeyStore,
}

pub(crate) trait Job<T> {
    async fn execute(mut self, context: &mut JobContext<'_>) -> anyhow::Result<T>
    where
        Self: Sized,
    {
        self.execute_dependencies(context).await?;
        let result = self.execute_logic(context).await?;
        //context.notifier.notify();
        Ok(result)
    }

    async fn execute_logic(self, context: &mut JobContext<'_>) -> anyhow::Result<T>;

    async fn execute_dependencies(&mut self, context: &mut JobContext<'_>) -> anyhow::Result<()>;
}
