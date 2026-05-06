// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{fmt, str::FromStr};

use anyhow::bail;
use chrono::{DateTime, Utc};
use uuid::Uuid;

/// A type which can be persisted as an operation
///
/// Operations with the same kind form a queue. They can be enqueued, dequeued, retried and
/// deleted.
pub(crate) trait OperationData {
    /// Unique kind of the associated operation
    fn kind() -> OperationKind;

    /// Generates an identifier for the operation
    ///
    /// It can be random or determined by the operation data.
    fn generate_id(&self) -> OperationId;

    /// Converts the operation data into an [`Operation`]
    fn into_operation(self) -> Operation<Self>
    where
        Self: Sized,
    {
        Operation::new(self)
    }
}

/// Identifier of an operation
#[derive(PartialEq, Eq)]
pub(crate) struct OperationId(pub(crate) Vec<u8>);

impl fmt::Debug for OperationId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        hex::encode(&self.0).fmt(f)
    }
}

/// Persisted operation
///
/// Enqueued operations are stored in the database and are uniquely identified by their
/// [`OperationId`]. In case of a conflict, a scheduled operation is overwritten.
///
/// When `scheduled_at` is `None`, operations will executed as fast as possible. The order is then
/// determined by `created_at`.
#[derive(Debug)]
pub(crate) struct Operation<T> {
    pub(crate) operation_id: OperationId,
    pub(crate) data: T,
    pub(crate) created_at: DateTime<Utc>,
    pub(crate) scheduled_at: DateTime<Utc>,
    pub(crate) retries: usize,
}

/// Warning: Do not reorder the variants. The order is used for operation id generation.
#[derive(Debug)]
pub(crate) enum OperationKind {
    FetchUserProfile,
    TimedTask,
    FetchGroupProfile,
}

impl<T: OperationData> Operation<T> {
    pub(crate) fn new(data: T) -> Self {
        let now = Utc::now();
        Self {
            operation_id: data.generate_id(),
            data,
            created_at: now,
            scheduled_at: now,
            retries: 0,
        }
    }

    #[cfg(any(feature = "test_utils", test))]
    pub(crate) fn schedule_at(mut self, due_at: DateTime<Utc>) -> Self {
        self.scheduled_at = due_at;
        self
    }

    pub(crate) fn take_data(self) -> (Operation<()>, T) {
        let op = Operation {
            operation_id: self.operation_id,
            data: (),
            created_at: self.created_at,
            scheduled_at: self.scheduled_at,
            retries: self.retries,
        };
        (op, self.data)
    }
}

impl OperationKind {
    fn as_str(&self) -> &'static str {
        match self {
            Self::FetchUserProfile => "fetch_profile",
            Self::FetchGroupProfile => "fetch_group_profile",
            Self::TimedTask => "timed_task",
        }
    }
}

impl FromStr for OperationKind {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "fetch_profile" => Self::FetchUserProfile,
            "fetch_group_profile" => Self::FetchGroupProfile,
            "timed_task" => Self::TimedTask,
            _ => bail!("Invalid operation type: {s}"),
        })
    }
}

mod persistence {
    use aircommon::codec::{BlobEncoded, PersistenceCodec};
    use serde::{Serialize, de::DeserializeOwned};
    use sqlx::{
        Database, Decode, Encode, Sqlite, Type, encode::IsNull, error::BoxDynError, query,
        query_as, query_scalar,
    };
    use tracing::warn;

    use crate::db_access::{WriteConnection, WriteDbTransaction};

    use super::*;

    impl<T> Operation<T> {
        /// Enqueue an operation
        ///
        /// If an operation with the same id is already enqueued, it is overwritten.
        pub(crate) async fn enqueue(&self, mut connection: impl WriteConnection) -> sqlx::Result<()>
        where
            T: OperationData + Serialize,
        {
            let kind = T::kind();
            let data = BlobEncoded(&self.data);
            let retries = self.retries as i64;
            query!(
                "INSERT INTO operation (
                    operation_id,
                    kind,
                    data,
                    created_at,
                    scheduled_at,
                    retries
                )
                VALUES (?, ?, ?, ?, ?, ?)
                ON CONFLICT (operation_id) DO UPDATE SET
                    kind = excluded.kind,
                    data = excluded.data,
                    created_at = excluded.created_at,
                    scheduled_at = excluded.scheduled_at,
                    retries = excluded.retries
                ",
                self.operation_id.0,
                kind,
                data,
                self.created_at,
                self.scheduled_at,
                retries,
            )
            .execute(connection.as_mut())
            .await?;
            Ok(())
        }

        /// Enqueue an operation if it doesn't exist
        pub(crate) async fn enqueue_if_not_exists(
            &self,
            mut connection: impl WriteConnection,
        ) -> sqlx::Result<()>
        where
            T: OperationData + Serialize,
        {
            let kind = T::kind();
            let data = BlobEncoded(&self.data);
            let retries = self.retries as i64;
            query!(
                "INSERT INTO operation (
                    operation_id,
                    kind,
                    data,
                    created_at,
                    scheduled_at,
                    retries
                )
                VALUES (?, ?, ?, ?, ?, ?)
                ON CONFLICT (operation_id) DO NOTHING
                ",
                self.operation_id.0,
                kind,
                data,
                self.created_at,
                self.scheduled_at,
                retries,
            )
            .execute(connection.as_mut())
            .await?;
            Ok(())
        }

        /// Dequeue an operation for retry
        pub(crate) async fn dequeue(
            txn: &mut WriteDbTransaction<'_>,
            task_id: Uuid,
            now: DateTime<Utc>,
        ) -> sqlx::Result<Option<Self>>
        where
            T: OperationData + DeserializeOwned + Unpin + Send + 'static,
        {
            let kind = T::kind();
            let Some(operation_id) = query_scalar!(
                r#"
                SELECT operation_id
                FROM operation
                WHERE kind = ?1
                    AND scheduled_at <= ?2
                    AND locked_by != ?3
                ORDER BY scheduled_at ASC, created_at ASC
                LIMIT 1
                "#,
                kind,
                now,
                task_id,
            )
            .fetch_optional(txn.as_mut())
            .await?
            else {
                return Ok(None);
            };

            struct SqlOperationData {
                data: Vec<u8>,
                created_at: DateTime<Utc>,
                scheduled_at: DateTime<Utc>,
                retries: u16,
            }

            let op_data = query_as!(
                SqlOperationData,
                r#"
                UPDATE operation
                SET locked_by = ?2
                WHERE operation_id = ?1
                RETURNING
                    data AS "data: _",
                    created_at AS "created_at: _",
                    scheduled_at AS "scheduled_at: _",
                    retries AS "retries: _"
                "#,
                operation_id,
                task_id,
            )
            .fetch_optional(txn.as_mut())
            .await?;
            let Some(op_data) = op_data else {
                return Ok(None);
            };

            let SqlOperationData {
                data,
                created_at,
                scheduled_at,
                retries,
            } = op_data;

            let data: T = match PersistenceCodec::from_slice(&data) {
                Ok(data) => data,
                Err(error) => {
                    // Delete the operation from the database if it cannot be deserialized
                    warn!(
                        ?kind,
                        ?operation_id, %error, "Failed to deserialize operation; deleting"
                    );
                    query!("DELETE FROM operation WHERE operation_id = ?", operation_id)
                        .execute(txn.as_mut())
                        .await?;
                    return Ok(None);
                }
            };

            Ok(Some(Operation {
                operation_id: OperationId(operation_id),
                data,
                created_at,
                scheduled_at,
                retries: retries.into(),
            }))
        }

        /// Delete an operation
        pub(crate) async fn delete(self, mut connection: impl WriteConnection) -> sqlx::Result<()> {
            query!(
                "DELETE FROM operation WHERE operation_id = ?",
                self.operation_id.0,
            )
            .execute(connection.as_mut())
            .await?;
            Ok(())
        }

        /// Increase the number of retries and set the retry due at
        pub(crate) async fn reschedule(
            &mut self,
            mut connection: impl WriteConnection,
            schedule_at: DateTime<Utc>,
        ) -> sqlx::Result<()> {
            self.scheduled_at = schedule_at;
            self.retries += 1;
            let retries = self.retries as i64;
            query!(
                "UPDATE operation SET
                    scheduled_at = ?,
                    retries = ?
                WHERE operation_id = ?",
                self.scheduled_at,
                retries,
                self.operation_id.0,
            )
            .execute(connection.as_mut())
            .await?;
            Ok(())
        }
    }

    impl Type<Sqlite> for OperationKind {
        fn type_info() -> <Sqlite as Database>::TypeInfo {
            <String as Type<Sqlite>>::type_info()
        }
    }

    impl Encode<'_, Sqlite> for OperationKind {
        fn encode_by_ref(
            &self,
            buf: &mut <Sqlite as Database>::ArgumentBuffer<'_>,
        ) -> Result<IsNull, BoxDynError> {
            let s = self.as_str();
            Encode::<Sqlite>::encode(s, buf)
        }
    }

    impl Decode<'_, Sqlite> for OperationKind {
        fn decode(value: <Sqlite as Database>::ValueRef<'_>) -> Result<Self, BoxDynError> {
            let s: &str = Decode::<Sqlite>::decode(value)?;
            Ok(Self::from_str(s)?)
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::db_access::{DbAccess, WriteConnection};

    use super::*;
    use serde::{Deserialize, Serialize};
    use sqlx::SqlitePool;

    // 1. Define a mock data structure for testing
    #[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
    struct MockData {
        payload: String,
    }

    impl OperationData for MockData {
        fn kind() -> OperationKind {
            OperationKind::FetchUserProfile
        }
        fn generate_id(&self) -> OperationId {
            // Consistent ID based on payload for testing "replace" logic
            OperationId(self.payload.as_bytes().to_vec())
        }
    }

    #[sqlx::test]
    async fn test_dequeue_locking(pool: SqlitePool) {
        let pool = DbAccess::for_tests(pool);

        let mut connection = pool.write().await.unwrap();
        let mut txn = connection.begin().await.unwrap();
        let data = MockData {
            payload: "lock_test".to_string(),
        };
        let op = Operation::new(data);
        op.enqueue(&mut txn).await.unwrap();

        let worker_a = Uuid::new_v4();
        let worker_b = Uuid::new_v4();
        let now = Utc::now();

        // Worker A grabs the task
        let op = Operation::<MockData>::dequeue(&mut txn, worker_a, now)
            .await
            .unwrap();
        assert!(op.is_some(), "Worker A should have claimed the task");

        // Worker B tries to grab the same task
        let op = Operation::<MockData>::dequeue(&mut txn, worker_b, now)
            .await
            .unwrap();
        assert!(op.is_some(), "Worker B should have claimed the task");

        // Worker B tries to grab the same task again
        let op = Operation::<MockData>::dequeue(&mut txn, worker_b, now)
            .await
            .unwrap();
        assert!(op.is_none(), "Worker B should not see the locked task");
    }

    #[sqlx::test]
    async fn test_reschedule_logic(pool: SqlitePool) {
        let pool = DbAccess::for_tests(pool);

        let mut connection = pool.write().await.unwrap();
        let mut txn = connection.begin().await.unwrap();
        let data = MockData {
            payload: "retry_test".to_string(),
        };
        let mut op = Operation::new(data);
        op.enqueue(&mut txn).await.unwrap();

        let retry_time = Utc::now() + chrono::Duration::minutes(5);
        op.reschedule(&mut txn, retry_time).await.unwrap();

        let op = Operation::<MockData>::dequeue(&mut txn, Uuid::new_v4(), retry_time)
            .await
            .unwrap()
            .unwrap();

        assert_eq!(op.retries, 1);
        assert_eq!(op.scheduled_at, retry_time);
    }

    #[sqlx::test]
    async fn test_upsert_behavior(pool: SqlitePool) {
        let pool = DbAccess::for_tests(pool);

        let mut connection = pool.write().await.unwrap();
        let mut txn = connection.begin().await.unwrap();
        let data = MockData {
            payload: "stable_id".to_string(),
        };
        let op1 = Operation::new(data.clone());
        let mut op2 = Operation::new(data);
        op2.retries = 5;

        // Inserting the same ID twice (due to "INSERT OR REPLACE")
        op1.enqueue(&mut txn).await.unwrap();
        op2.enqueue(&mut txn).await.unwrap();

        let op = Operation::<MockData>::dequeue(&mut txn, Uuid::new_v4(), Utc::now())
            .await
            .unwrap()
            .unwrap();

        assert_eq!(op.retries, 5);
    }

    #[sqlx::test]
    async fn test_dequeue_deletes_undeserializable_operation(pool: SqlitePool) {
        let pool = DbAccess::for_tests(pool);

        // Shares `OperationKind` with `MockData` but has an incompatible serialized shape, so
        // decoding an enqueued `MockData` as this type fails at the codec layer.
        #[derive(Serialize, Deserialize, Debug)]
        struct IncompatibleData {
            number: i64,
        }

        impl OperationData for IncompatibleData {
            fn kind() -> OperationKind {
                OperationKind::FetchUserProfile
            }

            fn generate_id(&self) -> OperationId {
                OperationId(self.number.to_le_bytes().to_vec())
            }
        }

        let mut connection = pool.write().await.unwrap();
        let mut txn = connection.begin().await.unwrap();
        let data = MockData {
            payload: "undeserializable".to_string(),
        };
        Operation::new(data).enqueue(&mut txn).await.unwrap();

        // Dequeueing with a type that can't deserialize the payload returns None and deletes the
        // offending row instead of surfacing an error.
        let op = Operation::<IncompatibleData>::dequeue(&mut txn, Uuid::new_v4(), Utc::now())
            .await
            .unwrap();
        assert!(op.is_none());

        // The row was deleted, so a subsequent dequeue with the correct type finds nothing.
        let op = Operation::<MockData>::dequeue(&mut txn, Uuid::new_v4(), Utc::now())
            .await
            .unwrap();
        assert!(op.is_none());
    }

    #[sqlx::test]
    async fn test_delete_persistence(pool: SqlitePool) {
        let pool = DbAccess::for_tests(pool);

        let mut connection = pool.write().await.unwrap();
        let mut txn = connection.begin().await.unwrap();
        let data = MockData {
            payload: "delete_me".to_string(),
        };
        let op_id = data.generate_id();
        let op = Operation::new(data);
        op.enqueue(&mut txn).await.unwrap();

        let now = Utc::now();
        let op = Operation::<MockData>::dequeue(&mut txn, Uuid::new_v4(), now)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(op.operation_id, op_id);

        // Delete and verify
        op.delete(&mut txn).await.unwrap();
        let op = Operation::<MockData>::dequeue(&mut txn, Uuid::new_v4(), now)
            .await
            .unwrap();
        assert!(op.is_none());
    }
}
