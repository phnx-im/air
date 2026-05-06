// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Database read-only and write access utilities.

use std::{future::Future, ops::AsyncFnOnce};

use sqlx::{
    Connection, Sqlite, SqliteConnection, SqlitePool, SqliteTransaction, TransactionManager,
    pool::PoolConnection, sqlite::SqliteTransactionManager,
};
use tracing::debug;

use crate::store::{StoreNotificationsSender, StoreNotifier};

/// Abstraction over a database connection pool providing read and write
/// access, and a [`StoreNotifier`] for tracking database changes.
#[derive(Debug, Clone)]
pub struct DbAccess {
    pool: SqlitePool,
    pub(crate) notifier_tx: StoreNotificationsSender,
}

/// A read-only database connection.
///
/// The connection is acquired via [`DbAccess::read`].
///
/// On drop, the connection is returned to the db pool.
#[derive(Debug)]
pub(crate) struct ReadDbConnection {
    conn: PoolConnection<Sqlite>,
}

/// A read-only database transaction.
///
/// The transaction is acquired via [`ReadDbConnection::begin`] or via
/// [`WriteDbTransaction::begin_read`].
#[derive(Debug)]
#[must_use = "transactions must be committed or rolled back"]
pub(crate) struct ReadDbTransaction<'a> {
    txn: SqliteTransaction<'a>,
}

/// A write database connection incl. a [`StoreNotifier`].
///
/// The connection is acquired via [`DbAccess::write`].
///
/// On drop,
/// * the connection is returned to the db pool, and
/// * the [`StoreNotifier`] is notified.
#[derive(Debug)]
pub(crate) struct WriteDbConnection {
    conn: PoolConnection<Sqlite>,
    notifier_tx: StoreNotificationsSender,
    notifier: StoreNotifier,
}

/// A write database transaction.
///
/// The transaction is acquired via [`WriteDbConnection::begin`] or via
/// [`WriteDbConnection::with_transaction`].
///
/// The transaction must be committed manually via [`WriteDbTransaction::commit`]. On drop, it is
/// automatically rolled back.
#[derive(Debug)]
#[must_use = "transactions must be committed or rolled back"]
pub(crate) struct WriteDbTransaction<'a> {
    txn: SqliteTransaction<'a>,
    notifier: &'a mut StoreNotifier,
}

/// A read-only database connection or transaction.
pub(crate) trait ReadConnection: AsMut<SqliteConnection> + Send {}

pub(crate) trait ReadTransaction: ReadConnection {}
pub(crate) trait WriteTransaction: WriteConnection {}

/// A write database connection or transaction.
pub(crate) trait WriteConnection: ReadConnection + AsMut<SqliteConnection> + Send {
    /// Split the connection into a connection and a [`StoreNotifier`].
    ///
    /// Useful when notifier needs to be accessed after the connection was used by value.
    fn split(&mut self) -> (&mut SqliteConnection, &mut StoreNotifier);

    /// Get a reference to the [`StoreNotifier`].
    fn notifier(&mut self) -> &mut StoreNotifier;

    /// Begin a new write transaction.
    fn begin<'a>(&'a mut self) -> impl Future<Output = sqlx::Result<WriteDbTransaction<'a>>> {
        async {
            let (connection, notifier) = self.split();
            Ok(WriteDbTransaction {
                txn: begin_write_txn(connection).await?,
                notifier,
            })
        }
    }

    /// Executes a function with a write transaction.
    ///
    /// The transaction is committed if the function returns `Ok`, and rolled
    /// back if the function returns `Err`. The [`StoreNotifier`] is notified
    /// after the transaction is committed successfully.
    //
    // Note: Even though, this method can be default implemented, in this case, Rust cannot reason
    // about the bounds of the returned future. In particular, the returned future is not Send
    // anymore. Currently, there is no way to express this in Rust in trait bounds. Instead, this
    // method is implemented directly on the corresponding types, where Rust can prove Send.
    async fn with_transaction<T, E>(
        &mut self,
        f: impl AsyncFnOnce(&mut WriteDbTransaction<'_>) -> Result<T, E>,
    ) -> Result<T, E>
    where
        T: Send,
        E: From<sqlx::Error>;
}

impl DbAccess {
    /// Create a new [`DbAccess`] from a database connection pool.
    pub(crate) fn new(pool: SqlitePool, notifier_tx: StoreNotificationsSender) -> Self {
        Self { pool, notifier_tx }
    }

    /// Create a new [`DbAccess`] for testing with a local store notifier.
    #[cfg(test)]
    pub(crate) fn for_tests(pool: SqlitePool) -> Self {
        Self {
            pool,
            notifier_tx: StoreNotificationsSender::new(),
        }
    }

    /// Create a new [`StoreNotifier`] for this [`DbAccess`].
    fn notifier(&self) -> StoreNotifier {
        StoreNotifier::new(self.notifier_tx.clone())
    }

    /// Acquire a read-only database connection.
    pub(crate) async fn read(&self) -> sqlx::Result<ReadDbConnection> {
        let conn = self.pool.acquire().await?;
        Ok(ReadDbConnection { conn })
    }

    /// Acquire a write database connection.
    pub(crate) async fn write(&self) -> sqlx::Result<WriteDbConnection> {
        let conn = self.pool.acquire().await?;
        Ok(WriteDbConnection {
            conn,
            notifier_tx: self.notifier_tx.clone(),
            notifier: self.notifier(),
        })
    }

    /// Executes a function within a read transaction.
    pub(crate) async fn with_read_transaction<T, E>(
        &self,
        f: impl AsyncFnOnce(&mut ReadDbTransaction<'_>) -> Result<T, E> + Send,
    ) -> Result<T, E>
    where
        T: Send,
        E: From<sqlx::Error>,
    {
        let mut read = self.read().await?;
        let mut txn = read.begin().await?;
        f(&mut txn).await // No need to commit a read transaction
    }

    /// Executes a function within a write transaction.
    ///
    /// This is a shortcut for `db.write().await?.with_transaction(f).await`.
    pub(crate) async fn with_write_transaction<T, E>(
        &self,
        f: impl AsyncFnOnce(&mut WriteDbTransaction<'_>) -> Result<T, E> + Send,
    ) -> Result<T, E>
    where
        T: Send,
        E: From<sqlx::Error>,
    {
        self.write().await?.with_transaction(f).await
    }
}

impl ReadDbConnection {
    /// Begin a read transaction.
    pub(crate) async fn begin(&mut self) -> sqlx::Result<ReadDbTransaction<'_>> {
        let txn = self.conn.begin().await?;
        Ok(ReadDbTransaction { txn })
    }
}

impl WriteDbConnection {
    /// Send all accumulated notifications until this point manually.
    pub(crate) fn notify(&mut self) {
        let notifier = std::mem::replace(
            &mut self.notifier,
            StoreNotifier::new(self.notifier_tx.clone()),
        );
        notifier.notify();
    }

    async fn with_transaction_impl<T, E>(
        &mut self,
        f: impl AsyncFnOnce(&mut WriteDbTransaction<'_>) -> Result<T, E>,
    ) -> Result<T, E>
    where
        T: Send,
        E: From<sqlx::Error>,
    {
        let (connection, notifier) = self.split();
        with_write_transaction_impl(f, connection, notifier).await
    }
}

impl Drop for WriteDbConnection {
    fn drop(&mut self) {
        self.notify();
    }
}

impl WriteDbTransaction<'_> {
    /// Begin a read transaction within the current write transaction.
    pub(crate) async fn begin_read(&mut self) -> sqlx::Result<ReadDbTransaction<'_>> {
        Ok(ReadDbTransaction {
            txn: self.txn.begin().await?,
        })
    }

    /// Commit the current write transaction.
    pub(crate) async fn commit(self) -> sqlx::Result<()> {
        if let Err(error) = self.txn.commit().await {
            self.notifier.clear(); // don't notify on commit failure (rollback)
            Err(error)
        } else {
            Ok(())
        }
    }
}

impl AsMut<SqliteConnection> for ReadDbConnection {
    fn as_mut(&mut self) -> &mut SqliteConnection {
        &mut self.conn
    }
}

impl AsMut<SqliteConnection> for ReadDbTransaction<'_> {
    fn as_mut(&mut self) -> &mut SqliteConnection {
        self.txn.as_mut()
    }
}

impl AsMut<SqliteConnection> for WriteDbConnection {
    fn as_mut(&mut self) -> &mut SqliteConnection {
        &mut self.conn
    }
}

impl AsMut<SqliteConnection> for WriteDbTransaction<'_> {
    fn as_mut(&mut self) -> &mut SqliteConnection {
        &mut self.txn
    }
}

impl ReadConnection for ReadDbConnection {}
impl ReadConnection for &mut ReadDbConnection {}
impl ReadConnection for ReadDbTransaction<'_> {}
impl ReadConnection for &mut ReadDbTransaction<'_> {}

impl ReadTransaction for ReadDbTransaction<'_> {}
impl ReadTransaction for &mut ReadDbTransaction<'_> {}
impl ReadTransaction for WriteDbTransaction<'_> {}
impl ReadTransaction for &mut WriteDbTransaction<'_> {}
impl WriteTransaction for WriteDbTransaction<'_> {}
impl WriteTransaction for &mut WriteDbTransaction<'_> {}

// write connections can be also use to read
impl<C> ReadConnection for &mut C where C: WriteConnection {}

impl<C> WriteConnection for &mut C
where
    C: WriteConnection,
{
    fn split(&mut self) -> (&mut SqliteConnection, &mut StoreNotifier) {
        (*self).split()
    }

    fn notifier(&mut self) -> &mut StoreNotifier {
        (*self).notifier()
    }

    async fn with_transaction<T, E>(
        &mut self,
        f: impl AsyncFnOnce(&mut WriteDbTransaction<'_>) -> Result<T, E>,
    ) -> Result<T, E>
    where
        T: Send,
        E: From<sqlx::Error>,
    {
        (*self).with_transaction(f).await
    }
}

impl ReadConnection for WriteDbConnection {}

impl ReadConnection for WriteDbTransaction<'_> {}

impl WriteConnection for WriteDbConnection {
    fn split(&mut self) -> (&mut SqliteConnection, &mut StoreNotifier) {
        (&mut self.conn, &mut self.notifier)
    }

    fn notifier(&mut self) -> &mut StoreNotifier {
        &mut self.notifier
    }

    #[cfg(test)]
    async fn begin(&mut self) -> sqlx::Result<WriteDbTransaction<'_>> {
        let (connection, notifier) = self.split();
        Ok(WriteDbTransaction {
            txn: begin_write_txn(connection).await?,
            notifier,
        })
    }

    async fn with_transaction<T, E>(
        &mut self,
        f: impl AsyncFnOnce(&mut WriteDbTransaction<'_>) -> Result<T, E>,
    ) -> Result<T, E>
    where
        T: Send,
        E: From<sqlx::Error>,
    {
        self.with_transaction_impl(f).await
    }
}

impl WriteConnection for WriteDbTransaction<'_> {
    fn split(&mut self) -> (&mut SqliteConnection, &mut StoreNotifier) {
        (self.txn.as_mut(), self.notifier)
    }

    fn notifier(&mut self) -> &mut StoreNotifier {
        self.notifier
    }

    async fn with_transaction<T, E>(
        &mut self,
        f: impl AsyncFnOnce(&mut WriteDbTransaction<'_>) -> Result<T, E>,
    ) -> Result<T, E>
    where
        T: Send,
        E: From<sqlx::Error>,
    {
        let (connection, notifier) = self.split();
        with_write_transaction_impl(f, connection, notifier).await
    }
}

async fn begin_write_txn(connection: &mut SqliteConnection) -> sqlx::Result<SqliteTransaction<'_>> {
    if SqliteTransactionManager::get_transaction_depth(connection) == 0 {
        connection.begin_with("BEGIN IMMEDIATE").await
    } else {
        debug!("Nested transaction detected; making a savepoint inside");
        connection.begin().await
    }
}

async fn with_write_transaction_impl<T, E>(
    f: impl AsyncFnOnce(&mut WriteDbTransaction<'_>) -> Result<T, E>,
    connection: &mut SqliteConnection,
    notifier: &mut StoreNotifier,
) -> Result<T, E>
where
    E: From<sqlx::Error>,
{
    let mut txn = WriteDbTransaction {
        txn: begin_write_txn(connection).await?,
        notifier,
    };
    
    // if we fail in the closure, we avoid propagating notifications
    let value = match f(&mut txn).await {
        Ok(value) => value,
        Err(error) => {
            txn.notifier.clear();
            return Err(error.into())
        }
    };

    // don't notify on commit failure (rollback)
    match txn.commit().await {
        Ok(_) => Ok(value),
        Err(error) => {
            notifier.clear();
            Err(error.into())
        }
    }
}
