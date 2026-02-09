// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use sqlx::SqlitePool;

use crate::{
    clients::api_clients::ApiClients, key_stores::MemoryUserKeyStore, store::StoreNotifier,
};

pub(crate) mod chat_operation;
pub(crate) mod create_chat;
mod pending_chat_operation;

pub(crate) struct JobContext<'a> {
    pub api_clients: &'a ApiClients,
    pub pool: SqlitePool,
    pub notifier: &'a mut StoreNotifier,
    pub key_store: &'a MemoryUserKeyStore,
}

pub(crate) trait Job {
    type Output;

    async fn execute(mut self, context: &mut JobContext<'_>) -> anyhow::Result<Self::Output>
    where
        Self: Sized,
    {
        Box::pin(self.execute_dependencies(context)).await?;
        Box::pin(self.execute_logic(context)).await
    }

    async fn execute_logic(self, context: &mut JobContext<'_>) -> anyhow::Result<Self::Output>;

    async fn execute_dependencies(&mut self, _context: &mut JobContext<'_>) -> anyhow::Result<()> {
        Ok(())
    }
}
