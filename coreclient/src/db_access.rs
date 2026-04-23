use sqlx::{
    Connection, Sqlite, SqliteConnection, SqlitePool, SqliteTransaction, pool::PoolConnection,
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

    async fn begin(&mut self) -> sqlx::Result<WriteDbTransaction<'_>> {
        let txn = self.as_mut().begin_with("BEGIN IMMEDIATE").await?;
        Ok(WriteDbTransaction {
            txn,
            notifier: &mut self.notifier,
        })
    }
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
    pub(crate) async fn with_write_transaction<U: Send, E>(
        &self,
        f: impl AsyncFnOnce(&mut WriteDbTransaction<'_>) -> Result<U, E>,
    ) -> Result<U, E>
    where
        E: From<sqlx::Error>,
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
    pub(crate) async fn begin(&mut self) -> sqlx::Result<ReadDbTransaction<'_>> {
        let txn = self.conn.begin().await?;
        Ok(ReadDbTransaction { txn })
    }
}

impl WriteDbTransaction<'_> {
    pub(crate) async fn commit(self) -> sqlx::Result<()> {
        self.txn.commit().await
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
impl ReadConnection for ReadDbTransaction<'_> {}
impl ReadConnection for &mut ReadDbTransaction<'_> {}

// write connections can be also use to read
impl ReadConnection for WriteDbConnection {}
impl ReadConnection for &mut WriteDbConnection {}
impl ReadConnection for WriteDbTransaction<'_> {}
impl ReadConnection for &mut WriteDbTransaction<'_> {}

impl WriteConnection for WriteDbConnection {
    fn split(&mut self) -> (&mut SqliteConnection, &mut StoreNotifier) {
        (&mut self.conn, &mut self.notifier)
    }

    fn notifier(&mut self) -> &mut StoreNotifier {
        &mut self.notifier
    }

    async fn begin(&mut self) -> sqlx::Result<WriteDbTransaction<'_>> {
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
