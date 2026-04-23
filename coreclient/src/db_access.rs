use sqlx::{
    Connection, Sqlite, SqliteConnection, SqlitePool, SqliteTransaction, pool::PoolConnection,
};

use crate::store::{StoreNotificationsSender, StoreNotifier};

#[derive(Debug, Clone)]
pub(crate) struct DbAccess {
    notifier_tx: StoreNotificationsSender,
    pool: SqlitePool,
}

impl DbAccess {
    #[cfg(test)]
    pub(crate) fn new(pool: SqlitePool) -> Self {
        Self {
            notifier_tx: StoreNotificationsSender::new(),
            pool,
        }
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
    async fn begin(&'c mut self) -> sqlx::Result<WriteTransaction<'c>> {
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

// impl<'c> ReadExecutor<'c> for &'c mut ReadConnection {}
// impl<'c> Executor<'c> for &'c mut ReadConnection {
//     type Database = Sqlite;

//     fn fetch_many<'e, 'q: 'e, E>(
//         self,
//         query: E,
//     ) -> BoxStream<
//         'e,
//         Result<
//             sqlx::Either<
//                 <Self::Database as sqlx::Database>::QueryResult,
//                 <Self::Database as sqlx::Database>::Row,
//             >,
//             sqlx::Error,
//         >,
//     >
//     where
//         'c: 'e,
//         E: 'q + sqlx::Execute<'q, Self::Database>,
//     {
//         self.conn.fetch_many(query)
//     }

//     fn fetch_optional<'e, 'q: 'e, E>(
//         self,
//         query: E,
//     ) -> BoxFuture<'e, Result<Option<<Self::Database as sqlx::Database>::Row>, sqlx::Error>>
//     where
//         'c: 'e,
//         E: 'q + sqlx::Execute<'q, Self::Database>,
//     {
//         self.conn.fetch_optional(query)
//     }

//     fn prepare_with<'e, 'q: 'e>(
//         self,
//         sql: &'q str,
//         parameters: &'e [<Self::Database as sqlx::Database>::TypeInfo],
//     ) -> BoxFuture<'e, Result<<Self::Database as sqlx::Database>::Statement<'q>, sqlx::Error>>
//     where
//         'c: 'e,
//     {
//         self.conn.prepare_with(sql, parameters)
//     }

//     fn describe<'e, 'q: 'e>(
//         self,
//         sql: &'q str,
//     ) -> BoxFuture<'e, Result<sqlx::Describe<Self::Database>, sqlx::Error>>
//     where
//         'c: 'e,
//     {
//         self.conn.describe(sql)
//     }
// }

// impl<'c> Executor<'c> for &'c mut WriteConnection {
//     type Database = Sqlite;

//     fn fetch_many<'e, 'q: 'e, E>(
//         self,
//         query: E,
//     ) -> BoxStream<
//         'e,
//         Result<
//             sqlx::Either<
//                 <Self::Database as sqlx::Database>::QueryResult,
//                 <Self::Database as sqlx::Database>::Row,
//             >,
//             sqlx::Error,
//         >,
//     >
//     where
//         'c: 'e,
//         E: 'q + sqlx::Execute<'q, Self::Database>,
//     {
//         self.conn.fetch_many(query)
//     }

//     fn fetch_optional<'e, 'q: 'e, E>(
//         self,
//         query: E,
//     ) -> BoxFuture<'e, Result<Option<<Self::Database as sqlx::Database>::Row>, sqlx::Error>>
//     where
//         'c: 'e,
//         E: 'q + sqlx::Execute<'q, Self::Database>,
//     {
//         self.conn.fetch_optional(query)
//     }

//     fn prepare_with<'e, 'q: 'e>(
//         self,
//         sql: &'q str,
//         parameters: &'e [<Self::Database as sqlx::Database>::TypeInfo],
//     ) -> BoxFuture<'e, Result<<Self::Database as sqlx::Database>::Statement<'q>, sqlx::Error>>
//     where
//         'c: 'e,
//     {
//         self.conn.prepare_with(sql, parameters)
//     }

//     fn describe<'e, 'q: 'e>(
//         self,
//         sql: &'q str,
//     ) -> BoxFuture<'e, Result<sqlx::Describe<Self::Database>, sqlx::Error>>
//     where
//         'c: 'e,
//     {
//         self.conn.describe(sql)
//     }
// }

// impl<'c> Executor<'c> for &'c mut WriteTransaction<'_> {
//     type Database = Sqlite;

//     fn fetch_many<'e, 'q: 'e, E>(
//         self,
//         query: E,
//     ) -> BoxStream<
//         'e,
//         Result<
//             sqlx::Either<
//                 <Self::Database as sqlx::Database>::QueryResult,
//                 <Self::Database as sqlx::Database>::Row,
//             >,
//             sqlx::Error,
//         >,
//     >
//     where
//         'c: 'e,
//         E: 'q + sqlx::Execute<'q, Self::Database>,
//     {
//         self.txn.fetch_many(query)
//     }

//     fn fetch_optional<'e, 'q: 'e, E>(
//         self,
//         query: E,
//     ) -> BoxFuture<'e, Result<Option<<Self::Database as sqlx::Database>::Row>, sqlx::Error>>
//     where
//         'c: 'e,
//         E: 'q + sqlx::Execute<'q, Self::Database>,
//     {
//         self.txn.fetch_optional(query)
//     }

//     fn prepare_with<'e, 'q: 'e>(
//         self,
//         sql: &'q str,
//         parameters: &'e [<Self::Database as sqlx::Database>::TypeInfo],
//     ) -> BoxFuture<'e, Result<<Self::Database as sqlx::Database>::Statement<'q>, sqlx::Error>>
//     where
//         'c: 'e,
//     {
//         self.txn.prepare_with(sql, parameters)
//     }

//     fn describe<'e, 'q: 'e>(
//         self,
//         sql: &'q str,
//     ) -> BoxFuture<'e, Result<sqlx::Describe<Self::Database>, sqlx::Error>>
//     where
//         'c: 'e,
//     {
//         self.txn.describe(sql)
//     }
// }
