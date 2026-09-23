// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! The outbox of self-group messages that still have to reach the user's other
//! devices.

use sqlx::query;

use crate::db::access::{ReadConnection, WriteConnection};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OutboxKind {
    Settings,
    BlockedContact,
}

impl OutboxKind {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            OutboxKind::Settings => "settings",
            OutboxKind::BlockedContact => "blocked_contact",
        }
    }
}

/// A staged change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OutboxEntry {
    pub(crate) key: Vec<u8>,
    pub(crate) payload: Vec<u8>,
    pub(crate) previous: Option<Vec<u8>>,
}

/// Stage a change, replacing the payload of one already staged under `key`.
///
/// `previous` is written only when the row is first inserted.
pub(crate) async fn stage(
    mut connection: impl WriteConnection,
    kind: OutboxKind,
    key: &[u8],
    payload: &[u8],
    previous: Option<&[u8]>,
) -> sqlx::Result<()> {
    let kind = kind.as_str();
    query!(
        "INSERT INTO self_group_outbox (kind, key, payload, previous)
        VALUES (?1, ?2, ?3, ?4)
        ON CONFLICT (kind, key) DO UPDATE SET payload = excluded.payload",
        kind,
        key,
        payload,
        previous,
    )
    .execute(connection.as_mut())
    .await?;
    Ok(())
}

/// The change staged under `key`, if any.
pub(crate) async fn load(
    mut connection: impl ReadConnection,
    kind: OutboxKind,
    key: &[u8],
) -> sqlx::Result<Option<OutboxEntry>> {
    let kind = kind.as_str();
    let row = query!(
        r#"SELECT
            key AS "key!: Vec<u8>",
            payload AS "payload!: Vec<u8>",
            previous AS "previous: Vec<u8>"
        FROM self_group_outbox
        WHERE kind = ?1 AND key = ?2"#,
        kind,
        key,
    )
    .fetch_optional(connection.as_mut())
    .await?;
    Ok(row.map(|row| OutboxEntry {
        key: row.key,
        payload: row.payload,
        previous: row.previous,
    }))
}

/// Every change staged under `kind`, sorted by key.
pub(crate) async fn load_kind(
    mut connection: impl ReadConnection,
    kind: OutboxKind,
) -> sqlx::Result<Vec<OutboxEntry>> {
    let kind = kind.as_str();
    let rows = query!(
        r#"SELECT
            key AS "key!: Vec<u8>",
            payload AS "payload!: Vec<u8>",
            previous AS "previous: Vec<u8>"
        FROM self_group_outbox
        WHERE kind = ?1
        ORDER BY key"#,
        kind,
    )
    .fetch_all(connection.as_mut())
    .await?;
    Ok(rows
        .into_iter()
        .map(|row| OutboxEntry {
            key: row.key,
            payload: row.payload,
            previous: row.previous,
        })
        .collect())
}

/// Drops the change staged under `key`.
pub(crate) async fn remove(
    mut connection: impl WriteConnection,
    kind: OutboxKind,
    key: &[u8],
) -> sqlx::Result<()> {
    let kind = kind.as_str();
    query!(
        "DELETE FROM self_group_outbox WHERE kind = ?1 AND key = ?2",
        kind,
        key,
    )
    .execute(connection.as_mut())
    .await?;
    Ok(())
}

/// Drops the change staged under `key` only if `payload` is still what it
/// intends.
///
/// Used when one of our own commits is accepted. A key re-staged with a
/// different payload while the commit was in flight stays, so the newer change
/// is re-issued rather than silently lost.
pub(crate) async fn complete_sent(
    mut connection: impl WriteConnection,
    kind: OutboxKind,
    key: &[u8],
    payload: &[u8],
) -> sqlx::Result<()> {
    let kind = kind.as_str();
    query!(
        "DELETE FROM self_group_outbox
        WHERE kind = ?1 AND key = ?2 AND payload = ?3",
        kind,
        key,
        payload,
    )
    .execute(connection.as_mut())
    .await?;
    Ok(())
}
