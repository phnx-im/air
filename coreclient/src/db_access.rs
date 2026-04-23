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
pub(crate) struct ReadPoolConnection {
    conn: PoolConnection<Sqlite>,
}

#[derive(Debug)]
pub(crate) struct WritePoolConnection {
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

    pub(crate) async fn read(&self) -> sqlx::Result<ReadPoolConnection> {
        let conn = self.pool.acquire().await?;
        Ok(ReadPoolConnection { conn })
    }

    pub(crate) async fn write(&self) -> sqlx::Result<WritePoolConnection> {
        let conn = self.pool.acquire().await?;
        Ok(WritePoolConnection {
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

impl WritePoolConnection {
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

impl AsMut<SqliteConnection> for &mut ReadPoolConnection {
    fn as_mut(&mut self) -> &mut SqliteConnection {
        &mut self.conn
    }
}

impl AsMut<SqliteConnection> for &mut WritePoolConnection {
    fn as_mut(&mut self) -> &mut SqliteConnection {
        &mut self.conn
    }
}

impl AsMut<SqliteConnection> for &mut WriteTransaction<'_> {
    fn as_mut(&mut self) -> &mut SqliteConnection {
        &mut self.txn
    }
}

// write connections can be also use to read
impl ReadConnection for &mut ReadPoolConnection {}
impl ReadConnection for &mut WritePoolConnection {}
impl ReadConnection for &mut WriteTransaction<'_> {}

impl WriteConnection for &mut WritePoolConnection {
    fn notifier(&mut self) -> &mut StoreNotifier {
        &mut self.notifier
    }
}

impl WriteConnection for &mut WriteTransaction<'_> {
    fn notifier(&mut self) -> &mut StoreNotifier {
        &mut self.notifier
    }
}
