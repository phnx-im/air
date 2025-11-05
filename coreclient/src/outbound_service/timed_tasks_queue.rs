// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use chrono::{DateTime, Utc};
use sqlx::{
    Database, Decode, Encode, Sqlite, Type, encode::IsNull, error::BoxDynError,
    sqlite::SqliteTypeInfo,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TaskKind {
    KeyPackageUpload,
}

impl TaskKind {
    pub(super) fn default_interval(&self) -> chrono::Duration {
        match self {
            TaskKind::KeyPackageUpload => chrono::Duration::weeks(1),
        }
    }
}

pub(crate) struct TimedTaskQueue {
    due_at: DateTime<Utc>,
    kind: TaskKind,
}

impl TimedTaskQueue {
    pub(crate) fn new_key_package_upload_task(due_at: DateTime<Utc>) -> Self {
        Self {
            due_at,
            kind: TaskKind::KeyPackageUpload,
        }
    }
}

mod persistence {
    use sqlx::{SqliteExecutor, query, query_scalar};
    use tracing::debug;
    use uuid::Uuid;

    use super::*;

    impl TimedTaskQueue {
        pub(crate) async fn enqueue(&self, executor: impl SqliteExecutor<'_>) -> sqlx::Result<()> {
            debug!(
                ?self.due_at, ?self.kind, "Enqueueing timed task"
            );

            query!(
                "INSERT INTO timed_tasks_queue
                    (due_at, task_kind)
                VALUES (?1, ?2)",
                self.due_at,
                self.kind,
            )
            .execute(executor)
            .await?;
            Ok(())
        }

        pub(crate) async fn dequeue(
            executor: impl SqliteExecutor<'_>,
            task_id: Uuid,
            now: DateTime<Utc>,
        ) -> sqlx::Result<Option<TaskKind>> {
            query_scalar!(
                r#"
                UPDATE timed_tasks_queue
                SET locked_by = ?1
                WHERE task_kind = (
                    SELECT task_kind
                    FROM timed_tasks_queue
                    WHERE 
                        (locked_by IS NULL OR locked_by != ?1)
                        AND due_at <= ?2
                    ORDER BY due_at ASC
                    LIMIT 1
                )
                RETURNING task_kind AS "task_kind: TaskKind"
                "#,
                task_id,
                now
            )
            .fetch_optional(executor)
            .await
        }

        pub(crate) async fn set_due_date(
            executor: impl SqliteExecutor<'_>,
            task_kind: TaskKind,
            due_at: DateTime<Utc>,
        ) -> sqlx::Result<()> {
            query!(
                "UPDATE timed_tasks_queue
                SET due_at = ?
                WHERE task_kind = ?",
                due_at,
                task_kind
            )
            .execute(executor)
            .await?;
            Ok(())
        }
    }
}

impl Type<Sqlite> for TaskKind {
    fn type_info() -> SqliteTypeInfo {
        <String as Type<Sqlite>>::type_info()
    }
}

impl<'r> Decode<'r, Sqlite> for TaskKind {
    fn decode(value: <Sqlite as Database>::ValueRef<'r>) -> Result<Self, BoxDynError> {
        let s: &str = Decode::<Sqlite>::decode(value)?;
        match s {
            "KeyPackageUpload" => Ok(TaskKind::KeyPackageUpload),
            _ => Err(format!("Unknown TaskKind variant: {}", s).into()),
        }
    }
}

impl<'q> Encode<'q, Sqlite> for TaskKind {
    fn encode_by_ref(
        &self,
        buf: &mut <Sqlite as Database>::ArgumentBuffer<'q>,
    ) -> Result<IsNull, BoxDynError> {
        let s = match self {
            TaskKind::KeyPackageUpload => "KeyPackageUpload",
        };
        <&str as Encode<Sqlite>>::encode(s, buf)
    }
}
