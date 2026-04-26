use sqlx::{
    Connection, Sqlite, SqliteConnection, SqlitePool, SqliteTransaction, TransactionManager,
    pool::PoolConnection, sqlite::SqliteTransactionManager,
};
use tracing::debug;

use crate::store::{StoreNotificationsSender, StoreNotifier};

#[derive(Debug, Clone)]
pub struct DbAccess {
    pool: SqlitePool,
    notifier_tx: Option<StoreNotificationsSender>,
}

#[derive(Debug)]
pub(crate) struct ReadDbConnection {
    conn: PoolConnection<Sqlite>,
}

#[derive(Debug)]
pub(crate) struct ReadDbTransaction<'a> {
    txn: SqliteTransaction<'a>,
}

/// Open connection for writing/reading incl. a [`StoreNotifier`].
///
/// On drop,
/// * the connection is returned to the db pool, and
/// * the [`StoreNotifier`] is notified.
#[derive(Debug)]
pub(crate) struct WriteDbConnection {
    conn: PoolConnection<Sqlite>,
    notifier: StoreNotifier,
}

#[derive(Debug)]
pub(crate) struct WriteDbTransaction<'a> {
    txn: SqliteTransaction<'a>,
    notifier: &'a mut StoreNotifier,
}

pub trait ReadConnection: AsMut<SqliteConnection> {
    async fn begin(&mut self) -> sqlx::Result<ReadDbTransaction<'_>> {
        let txn = self.as_mut().begin().await?;
        Ok(ReadDbTransaction { txn })
    }
}

pub trait WriteConnection: ReadConnection + AsMut<SqliteConnection> {
    fn split(&mut self) -> (&mut SqliteConnection, &mut StoreNotifier);
    fn notifier(&mut self) -> &mut StoreNotifier;

    #[deprecated]
    async fn begin_immediate(&mut self) -> sqlx::Result<WriteDbTransaction<'_>> {
        let (connection, notifier) = self.split();
        let txn = connection.begin_with("BEGIN IMMEDIATE").await?;
        Ok(WriteDbTransaction { txn, notifier })
    }

    /// Executes a function with a transaction and a [`StoreNotifier`].
    ///
    /// The transaction is committed if the function returns `Ok`, and rolled
    /// back if the function returns `Err`. The [`StoreNotifier`] is notified
    /// after the transaction is committed successfully.
    async fn with_transaction<U: Send, E>(
        mut self,
        f: impl AsyncFnOnce(&mut WriteDbTransaction<'_>) -> Result<U, E>,
    ) -> Result<U, E>
    where
        Self: Sized,
        E: From<sqlx::Error>,
    {
        let (conn, notifier) = self.split();
        let txn_depth = SqliteTransactionManager::get_transaction_depth(conn);
        let mut txn = WriteDbTransaction {
            txn: if txn_depth == 0 {
                conn.begin_with("BEGIN IMMEDIATE").await?
            } else {
                debug!("Nested transaction detected; making a savepoint inside");
                conn.begin().await?
            },
            notifier,
        };
        let value = f(&mut txn).await?;
        txn.commit().await?;
        self.notifier().notify();
        Ok(value)
    }
}

impl DbAccess {
    #[cfg(test)]
    pub(crate) fn for_tests(pool: SqlitePool) -> Self {
        Self {
            pool,
            notifier_tx: None,
        }
    }

    pub(crate) fn new(pool: SqlitePool, notifier_tx: StoreNotificationsSender) -> Self {
        Self {
            pool,
            notifier_tx: Some(notifier_tx),
        }
    }

    fn notifier(&self) -> StoreNotifier {
        StoreNotifier::new(self.notifier_tx.clone().unwrap_or_default())
    }

    pub(crate) async fn read(&self) -> sqlx::Result<ReadDbConnection> {
        let conn = self.pool.acquire().await?;
        Ok(ReadDbConnection { conn })
    }

    pub(crate) async fn write(&self) -> sqlx::Result<WriteDbConnection> {
        let conn = self.pool.acquire().await?;
        Ok(WriteDbConnection {
            conn,
            notifier: self.notifier(),
        })
    }

    /// Executes a function with a transaction and a [`StoreNotifier`].
    ///
    /// The transaction is committed if the function returns `Ok`, and rolled
    /// back if the function returns `Err`. The [`StoreNotifier`] is notified
    /// after the transaction is committed successfully.
    pub(crate) async fn with_write_transaction<U: Send, E>(
        &self,
        f: impl AsyncFnOnce(&mut WriteDbTransaction<'_>) -> Result<U, E>,
    ) -> Result<U, E>
    where
        E: From<sqlx::Error>,
    {
        self.write().await?.with_transaction(f).await
    }
}

impl WriteDbConnection {
    pub(crate) fn notify(mut self) {
        self.notifier.notify();
    }
}

impl WriteDbTransaction<'_> {
    pub(crate) async fn commit(self) -> sqlx::Result<()> {
        self.txn.commit().await?;
        Ok(())
    }
}

impl ReadDbTransaction<'_> {
    pub(crate) async fn commit(self) -> sqlx::Result<()> {
        self.txn.commit().await
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

// write connections can be also use to read
impl<T> ReadConnection for &mut T where T: WriteConnection {}
impl ReadConnection for WriteDbConnection {}
impl ReadConnection for WriteDbTransaction<'_> {}

impl WriteConnection for WriteDbConnection {
    fn split(&mut self) -> (&mut SqliteConnection, &mut StoreNotifier) {
        (&mut self.conn, &mut self.notifier)
    }

    fn notifier(&mut self) -> &mut StoreNotifier {
        &mut self.notifier
    }

    async fn begin_immediate(&mut self) -> sqlx::Result<WriteDbTransaction<'_>> {
        let txn = self.conn.begin_with("BEGIN IMMEDIATE").await?;
        Ok(WriteDbTransaction {
            txn,
            notifier: &mut self.notifier,
        })
    }
}

impl WriteConnection for &mut WriteDbConnection {
    fn split(&mut self) -> (&mut SqliteConnection, &mut StoreNotifier) {
        (&mut self.conn, &mut self.notifier)
    }

    fn notifier(&mut self) -> &mut StoreNotifier {
        &mut self.notifier
    }
}

impl WriteConnection for WriteDbTransaction<'_> {
    fn split(&mut self) -> (&mut SqliteConnection, &mut StoreNotifier) {
        (self.txn.as_mut(), self.notifier)
    }

    fn notifier(&mut self) -> &mut StoreNotifier {
        self.notifier
    }
}

impl WriteConnection for &mut WriteDbTransaction<'_> {
    fn split(&mut self) -> (&mut SqliteConnection, &mut StoreNotifier) {
        (self.txn.as_mut(), self.notifier)
    }

    fn notifier(&mut self) -> &mut StoreNotifier {
        self.notifier
    }
}
