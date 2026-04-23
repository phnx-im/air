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
}

pub trait ReadExecutor<'c>: AsMut<SqliteConnection> {}

pub trait WriteExecutor<'c>: AsMut<SqliteConnection> {
    fn split(self) -> (&'c mut SqliteConnection, &'c mut StoreNotifier);
}

impl DbAccess {
    fn notifier(&self) -> StoreNotifier {
        StoreNotifier::new(self.notifier_tx.clone())
    }

    pub(crate) async fn read(&self) -> sqlx::Result<ReadConnection> {
        let conn = self.pool.acquire().await?;
        Ok(ReadConnection { conn })
    }

    pub(crate) async fn write(&self) -> sqlx::Result<WriteConnection> {
        let conn = self.pool.acquire().await?;
        Ok(WriteConnection {
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

#[derive(Debug)]
pub(crate) struct ReadConnection {
    conn: PoolConnection<Sqlite>,
}

#[derive(Debug)]
pub(crate) struct WriteConnection {
    conn: PoolConnection<Sqlite>,
    notifier: StoreNotifier,
}

impl<'c> WriteConnection {
    pub(crate) async fn begin(&'c mut self) -> sqlx::Result<WriteTransaction<'c>> {
        let txn = self.conn.begin_with("BEGIN IMMEDIATE").await?;
        Ok(WriteTransaction {
            txn,
            notifier: &mut self.notifier,
        })
    }
}

#[derive(Debug)]
pub(crate) struct WriteTransaction<'a> {
    txn: SqliteTransaction<'a>,
    notifier: &'a mut StoreNotifier,
}

impl WriteTransaction<'_> {
    async fn commit(self) -> sqlx::Result<()> {
        self.txn.commit().await
    }
}

impl AsMut<SqliteConnection> for &mut ReadConnection {
    fn as_mut(&mut self) -> &mut SqliteConnection {
        &mut self.conn
    }
}

impl AsMut<SqliteConnection> for &mut WriteConnection {
    fn as_mut(&mut self) -> &mut SqliteConnection {
        &mut self.conn
    }
}

impl AsMut<SqliteConnection> for &mut WriteTransaction<'_> {
    fn as_mut(&mut self) -> &mut SqliteConnection {
        &mut self.txn
    }
}

impl<'c> ReadExecutor<'c> for &'c mut ReadConnection {}
impl<'c> ReadExecutor<'c> for &'c mut WriteConnection {}
impl<'c> ReadExecutor<'c> for &'c mut WriteTransaction<'_> {}

impl<'c> WriteExecutor<'c> for &'c mut WriteConnection {
    fn split(self) -> (&'c mut SqliteConnection, &'c mut StoreNotifier) {
        (self.conn.as_mut(), &mut self.notifier)
    }
}

impl<'c> WriteExecutor<'c> for &'c mut WriteTransaction<'_> {
    fn split(self) -> (&'c mut SqliteConnection, &'c mut StoreNotifier) {
        (self.txn.as_mut(), &mut self.notifier)
    }
}
