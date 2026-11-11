// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::{identifiers::UserId, messages::client_as::ConnectionOfferHash, time::TimeStamp};

use crate::{ChatId, clients::connection_offer::payload::ConnectionInfo};

/// An incoming username request whose sender is already a contact. The outbound
/// service closes its connection group, which tells the sender that the request
/// is redundant. The user never sees it.
pub(crate) struct ClosingConnectionRequest {
    pub(crate) request_id: ChatId,
    pub(crate) sender: UserId,
    pub(crate) connection_info: ConnectionInfo,
    /// The connection-offer PSK the closing join references.
    pub(crate) connection_offer_hash: ConnectionOfferHash,
    pub(crate) created_at: TimeStamp,
}

/// A request to close, with the server errors its close ran into.
pub(crate) struct DueClose {
    pub(crate) request: ClosingConnectionRequest,
    pub(crate) attempts: u32,
}

mod persistence {
    use aircommon::identifiers::Fqdn;
    use sqlx::{query, query_as};
    use uuid::Uuid;

    use crate::db::access::{ReadConnection, WriteConnection};

    use super::*;

    struct SqlDueClose {
        request_id: ChatId,
        sender_user_uuid: Uuid,
        sender_user_domain: Fqdn,
        connection_info: ConnectionInfo,
        connection_offer_hash: ConnectionOfferHash,
        created_at: TimeStamp,
        attempts: u32,
    }

    impl From<SqlDueClose> for DueClose {
        fn from(row: SqlDueClose) -> Self {
            let SqlDueClose {
                request_id,
                sender_user_uuid,
                sender_user_domain,
                connection_info,
                connection_offer_hash,
                created_at,
                attempts,
            } = row;
            Self {
                request: ClosingConnectionRequest {
                    request_id,
                    sender: UserId::new(sender_user_uuid, sender_user_domain),
                    connection_info,
                    connection_offer_hash,
                    created_at,
                },
                attempts,
            }
        }
    }

    impl ClosingConnectionRequest {
        /// Stores the request, unless it is stored already.
        pub(crate) async fn store(&self, mut connection: impl WriteConnection) -> sqlx::Result<()> {
            let sender_user_uuid = self.sender.uuid();
            let sender_user_domain = self.sender.domain();
            query!(
                "INSERT OR IGNORE INTO closing_connection_request (
                    request_id,
                    sender_user_uuid,
                    sender_user_domain,
                    connection_info,
                    connection_offer_hash,
                    created_at
                )
                VALUES (?, ?, ?, ?, ?, ?)",
                self.request_id,
                sender_user_uuid,
                sender_user_domain,
                self.connection_info,
                self.connection_offer_hash,
                self.created_at,
            )
            .execute(connection.as_mut())
            .await?;
            Ok(())
        }

        /// The requests whose close is due at `due_at`, oldest first.
        pub(crate) async fn load_due(
            mut connection: impl ReadConnection,
            due_at: TimeStamp,
        ) -> sqlx::Result<Vec<DueClose>> {
            let rows = query_as!(
                SqlDueClose,
                r#"SELECT
                    request_id AS "request_id: ChatId",
                    sender_user_uuid AS "sender_user_uuid: Uuid",
                    sender_user_domain AS "sender_user_domain: Fqdn",
                    connection_info AS "connection_info: ConnectionInfo",
                    connection_offer_hash AS "connection_offer_hash: _",
                    created_at AS "created_at: TimeStamp",
                    attempts AS "attempts: _"
                FROM closing_connection_request
                WHERE retry_at IS NULL OR retry_at <= ?
                ORDER BY created_at, request_id"#,
                due_at,
            )
            .fetch_all(connection.as_mut())
            .await?;
            Ok(rows.into_iter().map(DueClose::from).collect())
        }

        /// Keeps the request until `retry_at`.
        pub(crate) async fn defer(
            mut connection: impl WriteConnection,
            request_id: ChatId,
            attempts: u32,
            retry_at: TimeStamp,
        ) -> sqlx::Result<()> {
            query!(
                "UPDATE closing_connection_request
                SET attempts = ?, retry_at = ?
                WHERE request_id = ?",
                attempts,
                retry_at,
                request_id,
            )
            .execute(connection.as_mut())
            .await?;
            Ok(())
        }

        pub(crate) async fn delete(
            mut connection: impl WriteConnection,
            request_id: ChatId,
        ) -> sqlx::Result<()> {
            query!(
                "DELETE FROM closing_connection_request WHERE request_id = ?",
                request_id,
            )
            .execute(connection.as_mut())
            .await?;
            Ok(())
        }
    }
}
