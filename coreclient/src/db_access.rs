use sqlx::{
    Connection, Executor, Sqlite, SqliteConnection, SqlitePool, SqliteTransaction,
    pool::PoolConnection,
};

use crate::store::{StoreNotificationsSender, StoreNotifier};

#[derive(Debug, Clone)]
pub(crate) struct DbAccess {
    pool: SqlitePool,
    notifier_tx: StoreNotificationsSender,
}

#[derive(Debug)]
pub(crate) struct ReadDbConnection {
    conn: PoolConnection<Sqlite>,
}

#[derive(Debug)]
pub(crate) struct ReadTransaction<'a> {
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
pub(crate) struct WriteTransaction<'a> {
    txn: SqliteTransaction<'a>,
    notifier: &'a mut StoreNotifier,
}

pub trait ReadConnection: AsMut<SqliteConnection> {}

pub trait WriteConnection: AsMut<SqliteConnection> {
    fn split(&mut self) -> (&mut SqliteConnection, &mut StoreNotifier);
    fn notifier(&mut self) -> &mut StoreNotifier;
}

impl DbAccess {
    #[cfg(test)]
    pub(crate) fn for_tests(pool: SqlitePool) -> Self {
        Self {
            pool,
            notifier_tx: StoreNotificationsSender::new(),
        }
    }

    pub(crate) fn new(pool: SqlitePool, notifier_tx: StoreNotificationsSender) -> Self {
        Self { pool, notifier_tx }
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
            notifier: self.notifier(),
        })
    }

    /// Executes a function with a transaction and a [`StoreNotifier`].
    ///
    /// The transaction is committed if the function returns `Ok`, and rolled
    /// back if the function returns `Err`. The [`StoreNotifier`] is notified
    /// after the transaction is committed successfully.
    pub(crate) async fn with_write_transaction<A, T: Send>(&self, f: A) -> anyhow::Result<T>
    where
        A: for<'a> AsyncFnOnce(&'a mut WriteTransaction<'_>) -> anyhow::Result<T>,
    {
        let mut conn = self.write().await?;
        let mut txn = conn.begin().await?;
        let value = f(&mut txn).await?;
        txn.commit().await?;
        conn.notifier.notify();
        Ok(value)
    }
}

impl ReadDbConnection {
    pub(crate) async fn begin(&mut self) -> sqlx::Result<ReadTransaction<'_>> {
        let txn = self.conn.begin().await?;
        Ok(ReadTransaction { txn })
    }
}

impl WriteDbConnection {
    pub(crate) async fn begin(&mut self) -> sqlx::Result<WriteTransaction<'_>> {
        let txn = self.conn.begin_with("BEGIN IMMEDIATE").await?;
        Ok(WriteTransaction {
            txn,
            notifier: &mut self.notifier,
        })
    }
}

impl WriteTransaction<'_> {
    async fn commit(self) -> sqlx::Result<()> {
        self.txn.commit().await
    }
}

impl AsMut<SqliteConnection> for ReadDbConnection {
    fn as_mut(&mut self) -> &mut SqliteConnection {
        &mut self.conn
    }
}

impl AsMut<SqliteConnection> for ReadTransaction<'_> {
    fn as_mut(&mut self) -> &mut SqliteConnection {
        self.txn.as_mut()
    }
}

impl AsMut<SqliteConnection> for WriteDbConnection {
    fn as_mut(&mut self) -> &mut SqliteConnection {
        &mut self.conn
    }
}

impl AsMut<SqliteConnection> for WriteTransaction<'_> {
    fn as_mut(&mut self) -> &mut SqliteConnection {
        &mut self.txn
    }
}

impl ReadConnection for ReadDbConnection {}
impl ReadConnection for ReadTransaction<'_> {}

// write connections can be also use to read
impl ReadConnection for WriteDbConnection {}
impl ReadConnection for WriteTransaction<'_> {}

impl WriteConnection for WriteDbConnection {
    fn split(&mut self) -> (&mut SqliteConnection, &mut StoreNotifier) {
        (&mut self.conn, &mut self.notifier)
    }

    fn notifier(&mut self) -> &mut StoreNotifier {
        &mut self.notifier
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

impl WriteConnection for WriteTransaction<'_> {
    fn split(&mut self) -> (&mut SqliteConnection, &mut StoreNotifier) {
        (self.txn.as_mut(), self.notifier)
    }

    fn notifier(&mut self) -> &mut StoreNotifier {
        self.notifier
    }
}

impl WriteConnection for &mut WriteTransaction<'_> {
    fn split(&mut self) -> (&mut SqliteConnection, &mut StoreNotifier) {
        (self.txn.as_mut(), self.notifier)
    }

    fn notifier(&mut self) -> &mut StoreNotifier {
        self.notifier
    }
}
