use std::{future::Future, ops::AsyncFnOnce};

use sqlx::{
    Connection, Sqlite, SqliteConnection, SqlitePool, SqliteTransaction, TransactionManager,
    pool::PoolConnection, sqlite::SqliteTransactionManager,
};
use tracing::debug;

use crate::store::{StoreNotificationsSender, StoreNotifier};

#[derive(Debug, Clone)]
pub struct DbAccess {
    pool: SqlitePool,
    pub(crate) notifier_tx: StoreNotificationsSender,
}

#[derive(Debug)]
pub(crate) struct ReadDbConnection {
    conn: PoolConnection<Sqlite>,
}

#[derive(Debug)]
#[must_use = "transactions must be committed or rolled back"]
pub(crate) struct ReadDbTransaction<'a> {
    pub(crate) txn: SqliteTransaction<'a>,
}

/// Open connection for writing/reading incl. a [`StoreNotifier`].
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

#[derive(Debug)]
#[must_use = "transactions must be committed or rolled back"]
pub(crate) struct WriteDbTransaction<'a> {
    txn: SqliteTransaction<'a>,
    notifier: &'a mut StoreNotifier,
}

pub(crate) trait ReadConnection: AsMut<SqliteConnection> + Send {}

pub(crate) trait ReadTransaction: ReadConnection {}
pub(crate) trait WriteTransaction: WriteConnection {}


pub(crate) trait WriteConnection: ReadConnection + AsMut<SqliteConnection> + Send {
    fn split(&mut self) -> (&mut SqliteConnection, &mut StoreNotifier);
    fn notifier(&mut self) -> &mut StoreNotifier;

    fn begin<'a>(&'a mut self) -> impl Future<Output = sqlx::Result<WriteDbTransaction<'a>>> {
        async {
            let (connection, notifier) = self.split();
            Ok(WriteDbTransaction {
                txn: begin_txn(connection, true).await?,
                notifier,
            })
        }
    }

    /// Executes a function with a transaction and a [`StoreNotifier`].
    ///
    /// The transaction is committed if the function returns `Ok`, and rolled
    /// back if the function returns `Err`. The [`StoreNotifier`] is notified
    /// after the transaction is committed successfully.
    async fn with_transaction<T, E>(
        &mut self,
        f: impl AsyncFnOnce(&mut WriteDbTransaction<'_>) -> Result<T, E>,
    ) -> Result<T, E>
    where
        T: Send,
        E: From<sqlx::Error>;
}

impl DbAccess {
    pub(crate) fn new(pool: SqlitePool, notifier_tx: StoreNotificationsSender) -> Self {
        Self { pool, notifier_tx }
    }

    #[cfg(test)]
    pub(crate) fn for_tests(pool: SqlitePool) -> Self {
        Self {
            pool,
            notifier_tx: StoreNotificationsSender::new(),
        }
    }

    fn notifier(&self) -> StoreNotifier {
        StoreNotifier::new(self.notifier_tx.clone())
    }

    pub(crate) async fn read(&self) -> sqlx::Result<ReadDbConnection> {
        let conn = self.pool.acquire().await?;
        Ok(ReadDbConnection { conn })
    }

    pub(crate) async fn write(&self) -> sqlx::Result<WriteDbConnection> {
        let conn = self.pool.acquire().await?;
        Ok(WriteDbConnection {
            conn,
            notifier_tx: self.notifier_tx.clone(),
            notifier: self.notifier(),
        })
    }

    /// Executes a function with a transaction and a [`StoreNotifier`].
    ///
    /// The transaction is committed if the function returns `Ok`, and rolled
    /// back if the function returns `Err`. The [`StoreNotifier`] is notified
    /// after the transaction is committed successfully.
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
    pub(crate) async fn begin(&mut self) -> sqlx::Result<ReadDbTransaction<'_>> {
        Ok(ReadDbTransaction {
            txn: begin_txn(self.as_mut(), false).await?,
        })
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
            txn: begin_txn(connection, true).await?,
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

pub(crate) async fn begin_txn(
    connection: &mut SqliteConnection,
    immediate: bool,
) -> sqlx::Result<SqliteTransaction<'_>> {
    if immediate && SqliteTransactionManager::get_transaction_depth(connection) == 0 {
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
        txn: begin_txn(connection, true).await?,
        notifier,
    };
    let value = f(&mut txn).await?;

    match txn.commit().await {
        Ok(_) => Ok(value),
        Err(error) => {
            notifier.clear(); // don't notify on commit failure (rollback)
            Err(error.into())
        }
    }
}
