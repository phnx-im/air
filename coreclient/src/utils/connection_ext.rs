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
        self,
        f: impl AsyncFnOnce(&mut SqliteTransaction<'_>) -> anyhow::Result<T>,
    ) -> anyhow::Result<T>;

    /// Executes a function with a connection.
    ///
    /// The connection is dropped at the end of the closure.
    async fn with_connection<T: Send, E: From<sqlx::Error>>(
        self,
        f: impl AsyncFnOnce(&mut SqliteConnection) -> Result<T, E>,
    ) -> Result<T, E>;
}

impl ConnectionExt for &mut SqliteConnection {
    async fn with_transaction<T: Send>(
        self,
        f: impl AsyncFnOnce(&mut SqliteTransaction<'_>) -> anyhow::Result<T>,
    ) -> anyhow::Result<T> {
        let mut txn = self.begin_with("BEGIN IMMEDIATE").await?;
        let value = f(&mut txn).await?;
        txn.commit().await?;
        Ok(value)
    }

    async fn with_connection<T: Send, E: From<sqlx::Error>>(
        self,
        f: impl AsyncFnOnce(&mut SqliteConnection) -> Result<T, E>,
    ) -> Result<T, E> {
        f(self).await
    }
}

impl ConnectionExt for &SqlitePool {
    async fn with_transaction<T: Send>(
        self,
        f: impl AsyncFnOnce(&mut SqliteTransaction<'_>) -> anyhow::Result<T>,
    ) -> anyhow::Result<T> {
        let mut txn = self.begin_with("BEGIN IMMEDIATE").await?;
        let value = f(&mut txn).await?;
        txn.commit().await?;
        Ok(value)
    }

    async fn with_connection<T: Send, E: From<sqlx::Error>>(
        self,
        f: impl AsyncFnOnce(&mut SqliteConnection) -> Result<T, E>,
    ) -> Result<T, E> {
        let mut connection = self.acquire().await?;
        f(&mut connection).await
    }
}

pub(crate) trait DatabaseAccess {
    async fn acquire(&self) -> sqlx::Result<impl AsMut<SqliteConnection>>;
    async fn begin_immediate(&self) -> sqlx::Result<SqliteTransaction<'_>>;

    fn notifier(&self) -> StoreNotifier;

    /// Executes a function with a connection from the pool.
    ///
    /// Connection is dropped (back to the pool) at the end of the lambda.
    async fn with_connection<U: Send, E>(
        &self,
        f: impl AsyncFnOnce(&mut SqliteConnection) -> Result<U, E>,
    ) -> Result<U, E>
    where
        E: From<sqlx::Error>,
    {
        let mut connection = self.acquire().await?;
        let value = f(&mut connection).await?;
        Ok(value)
    }

    /// Executes a function with a transaction.
    ///
    /// The transaction is committed if the function returns `Ok`, and rolled
    /// back if the function returns `Err`.
    async fn with_transaction<U: Send, E>(
        &self,
        f: impl AsyncFnOnce(&mut SqliteTransaction<'_>) -> Result<U, E>,
    ) -> Result<U, E>
    where
        E: From<sqlx::Error>,
    {
        let mut txn = self.begin_immediate().await?;
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
        let mut txn = self.begin_immediate().await?;
        let mut notifier = self.notifier();
        let value = f(&mut txn, &mut notifier).await?;
        txn.commit().await?;
        notifier.notify();
        Ok(value)
    }
}
