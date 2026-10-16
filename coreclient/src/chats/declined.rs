// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use chrono::{DateTime, Utc};

use crate::ChatId;

/// An incoming connection request the user declined. It is remembered, so a
/// late copy of the request is dropped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DeclinedConnectionRequest {
    pub(crate) request_id: ChatId,
    pub(crate) declined_at: DateTime<Utc>,
}

mod persistence {
    use sqlx::{query, query_as, query_scalar};

    use crate::db::access::{ReadConnection, WriteConnection};

    use super::*;

    impl DeclinedConnectionRequest {
        /// A later decline of the same request replaces the time. Only a
        /// sibling's entry can meet a record, and every device applies those
        /// in the same order, so all devices end up with the same time.
        pub(crate) async fn store(&self, mut connection: impl WriteConnection) -> sqlx::Result<()> {
            query!(
                "INSERT INTO declined_connection_request (request_id, declined_at)
                VALUES (?, ?)
                ON CONFLICT (request_id) DO UPDATE SET declined_at = excluded.declined_at",
                self.request_id,
                self.declined_at,
            )
            .execute(connection.as_mut())
            .await?;
            Ok(())
        }

        pub(crate) async fn exists(
            mut connection: impl ReadConnection,
            request_id: ChatId,
        ) -> sqlx::Result<bool> {
            query_scalar!(
                r#"SELECT EXISTS(
                    SELECT 1 FROM declined_connection_request WHERE request_id = ?
                ) AS "declined!: bool""#,
                request_id,
            )
            .fetch_one(connection.as_mut())
            .await
        }

        /// Every decline, sorted by request id.
        pub(crate) async fn load_all(
            mut connection: impl ReadConnection,
        ) -> sqlx::Result<Vec<Self>> {
            query_as!(
                DeclinedConnectionRequest,
                r#"SELECT
                    request_id AS "request_id: ChatId",
                    declined_at AS "declined_at: DateTime<Utc>"
                FROM declined_connection_request
                ORDER BY request_id"#,
            )
            .fetch_all(connection.as_mut())
            .await
        }

        pub(crate) async fn exists_declined_until(
            mut connection: impl ReadConnection,
            cutoff: DateTime<Utc>,
        ) -> sqlx::Result<bool> {
            query_scalar!(
                r#"SELECT EXISTS(
                    SELECT 1 FROM declined_connection_request WHERE declined_at <= ?
                ) AS "declined!: bool""#,
                cutoff,
            )
            .fetch_one(connection.as_mut())
            .await
        }

        /// Deletes the declines made at or before `cutoff`.
        pub(crate) async fn delete_declined_until(
            mut connection: impl WriteConnection,
            cutoff: DateTime<Utc>,
        ) -> sqlx::Result<()> {
            query!(
                "DELETE FROM declined_connection_request WHERE declined_at <= ?",
                cutoff
            )
            .execute(connection.as_mut())
            .await?;
            Ok(())
        }
    }
}
