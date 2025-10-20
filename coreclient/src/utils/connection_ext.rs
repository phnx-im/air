// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use sqlx::{Connection, SqliteConnection, SqlitePool, SqliteTransaction};

use crate::store::StoreNotifier;

pub(crate) trait ConnectionExt {
    /// Executes a function with a transaction.
    ///
    /// The transaction is committed if the function returns `Ok`, and rolled
    /// back if the function returns `Err`.
    async fn with_transaction<T: Send>(
        &mut self,
        f: impl AsyncFnOnce(&mut SqliteTransaction<'_>) -> anyhow::Result<T>,
    ) -> anyhow::Result<T>;
}

impl ConnectionExt for SqliteConnection {
    async fn with_transaction<T: Send>(
        &mut self,
        f: impl AsyncFnOnce(&mut SqliteTransaction<'_>) -> anyhow::Result<T>,
    ) -> anyhow::Result<T> {
        let mut txn = self.begin_with("BEGIN IMMEDIATE").await?;
        let value = f(&mut txn).await?;
        txn.commit().await?;
        Ok(value)
    }
}

pub(crate) trait StoreExt {
    fn pool(&self) -> &SqlitePool;

    fn notifier(&self) -> StoreNotifier;

    /// Executes a function with a transaction.
    ///
    /// The transaction is committed if the function returns `Ok`, and rolled
    /// back if the function returns `Err`.
    async fn with_transaction<U: Send>(
        &self,
        f: impl AsyncFnOnce(&mut SqliteTransaction<'_>) -> anyhow::Result<U>,
    ) -> anyhow::Result<U> {
        let mut txn = self.pool().begin_with("BEGIN IMMEDIATE").await?;
        let value = f(&mut txn).await?;
        txn.commit().await?;
        Ok(value)
    }

    /// Executes a function with a [`StoreNotifier`].
    ///
    /// The [`StoreNotifier`] is notified if the function returns `Ok`.
    async fn with_notifier<T: Send>(
        &self,
        f: impl AsyncFnOnce(&mut StoreNotifier) -> anyhow::Result<T>,
    ) -> anyhow::Result<T> {
        let mut notifier = self.notifier();
        let value = f(&mut notifier).await?;
        notifier.notify();
        Ok(value)
    }

    /// Executes a function with a transaction and a [`StoreNotifier`].
    ///
    /// The transaction is committed if the function returns `Ok`, and rolled
    /// back if the function returns `Err`. The [`StoreNotifier`] is notified
    /// after the transaction is committed successfully.
    async fn with_transaction_and_notifier<T: Send>(
        &self,
        f: impl AsyncFnOnce(&mut SqliteTransaction<'_>, &mut StoreNotifier) -> anyhow::Result<T>,
    ) -> anyhow::Result<T> {
        let mut txn = self.pool().begin_with("BEGIN IMMEDIATE").await?;
        let mut notifier = self.notifier();
        let value = f(&mut txn, &mut notifier).await?;
        txn.commit().await?;
        notifier.notify();
        Ok(value)
    }
}
