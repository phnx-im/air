// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::HashMap;

use aircommon::identifiers::RemoteAttachmentId;
use chrono::{DateTime, Utc};
use mimi_content::MessageStatus;
use sqlx::{
    Database, Decode, Encode, Sqlite, Type, encode::IsNull, error::BoxDynError, query, query_scalar,
};

use crate::{
    AttachmentId, ChatId, MessageId, PartIndex,
    clients::attachment::{AttachmentInfo, MessageAttachments},
    db::access::{ReadConnection, WriteConnection},
};

/// An own message with fully uploaded attachments that never reached the
/// outbound service.
pub(crate) struct UnqueuedAttachmentMessage {
    pub(crate) chat_id: ChatId,
    pub(crate) message_id: MessageId,
}

/// Row of `message_attachment`: a message part referencing an attachment.
#[derive(Debug, Clone)]
#[cfg_attr(test, derive(PartialEq, Eq))]
pub(crate) struct MessageAttachmentRecord {
    pub(crate) message_id: MessageId,
    pub(crate) part_index: PartIndex,
    pub(crate) attachment_id: AttachmentId,
    pub(crate) role: AttachmentRole,
    pub(crate) filename: Option<String>,
}

/// How a message part uses its attachment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum AttachmentRole {
    Attachment = 1,
    /// Disposition preview or inside a preview container
    Preview = 2,
}

impl AttachmentRole {
    pub(crate) fn new(preview: bool) -> Self {
        if preview {
            Self::Preview
        } else {
            Self::Attachment
        }
    }
}

impl Type<Sqlite> for AttachmentRole {
    fn type_info() -> <Sqlite as Database>::TypeInfo {
        // Note: don't use u8, sqlx gets confused with:
        // ```
        // mismatched types; Rust type (as SQL type `INTEGER`) is not compatible with SQL type `INTEGER`
        // ```
        <u32 as Type<Sqlite>>::type_info()
    }
}

impl<'q> Encode<'q, Sqlite> for AttachmentRole {
    fn encode_by_ref(
        &self,
        buf: &mut <Sqlite as Database>::ArgumentBuffer,
    ) -> Result<IsNull, BoxDynError> {
        Encode::<Sqlite>::encode(*self as u32, buf)
    }
}

impl<'r> Decode<'r, Sqlite> for AttachmentRole {
    fn decode(value: <Sqlite as Database>::ValueRef<'r>) -> Result<Self, BoxDynError> {
        let value: u32 = Decode::<Sqlite>::decode(value)?;
        Ok(Self::new(value == Self::Preview as u32))
    }
}

impl MessageAttachmentRecord {
    pub(crate) async fn store(&self, mut connection: impl WriteConnection) -> sqlx::Result<()> {
        query!(
            "INSERT INTO message_attachment (
                message_id,
                part_index,
                attachment_id,
                role,
                filename
            ) VALUES (?, ?, ?, ?, ?)",
            self.message_id,
            self.part_index,
            self.attachment_id,
            self.role,
            self.filename,
        )
        .execute(connection.as_mut())
        .await?;
        Ok(())
    }

    /// Sets the filename of every reference to the attachment.
    pub(crate) async fn update_filename(
        mut connection: impl WriteConnection,
        attachment_id: AttachmentId,
        filename: &str,
    ) -> sqlx::Result<()> {
        query!(
            "UPDATE message_attachment SET filename = ? WHERE attachment_id = ?",
            filename,
            attachment_id,
        )
        .execute(connection.as_mut())
        .await?;
        Ok(())
    }

    /// Loads the first reference to the attachment.
    pub(crate) async fn load_first(
        mut connection: impl ReadConnection,
        attachment_id: AttachmentId,
    ) -> sqlx::Result<Option<Self>> {
        let record = query!(
            r#"SELECT
                message_id AS "message_id: MessageId",
                part_index AS "part_index: PartIndex",
                attachment_id AS "attachment_id: AttachmentId",
                role AS "role: AttachmentRole",
                filename AS "filename?"
            FROM message_attachment
            WHERE attachment_id = ?
            ORDER BY rowid
            LIMIT 1"#,
            attachment_id
        )
        .fetch_optional(connection.as_mut())
        .await?;
        Ok(record.map(|record| Self {
            message_id: record.message_id,
            part_index: record.part_index,
            attachment_id: record.attachment_id,
            role: record.role,
            filename: record.filename,
        }))
    }

    /// Deletes the references of a message to its attachments and notifies
    /// about the changes.
    ///
    /// Attachments left without a reference are deleted by a trigger. This is
    /// used for network deletions where the message content becomes NullPart
    /// but the message row remains (so FK cascade doesn't apply).
    pub(crate) async fn delete_by_message_id(
        mut connection: impl WriteConnection,
        message_id: MessageId,
    ) -> sqlx::Result<()> {
        let attachment_ids: Vec<AttachmentId> = query_scalar!(
            r#"DELETE FROM message_attachment WHERE message_id = ?
            RETURNING attachment_id AS "attachment_id: _""#,
            message_id
        )
        .fetch_all(connection.as_mut())
        .await?;
        Self::notify_deleted(connection, attachment_ids).await
    }

    /// Deletes the reference of a message part.
    ///
    /// An attachment left without a reference is deleted by a trigger.
    pub(crate) async fn delete(
        mut connection: impl WriteConnection,
        message_id: MessageId,
        part_index: PartIndex,
    ) -> sqlx::Result<()> {
        let attachment_ids: Vec<AttachmentId> = query_scalar!(
            r#"DELETE FROM message_attachment WHERE message_id = ? AND part_index = ?
            RETURNING attachment_id AS "attachment_id: _""#,
            message_id,
            part_index,
        )
        .fetch_all(connection.as_mut())
        .await?;
        Self::notify_deleted(connection, attachment_ids).await
    }

    pub(crate) async fn delete_by_role(
        mut connection: impl WriteConnection,
        message_id: MessageId,
        role: AttachmentRole,
    ) -> sqlx::Result<()> {
        let attachment_ids: Vec<AttachmentId> = query_scalar!(
            r#"DELETE FROM message_attachment WHERE message_id = ? AND role = ?
            RETURNING attachment_id AS "attachment_id: _""#,
            message_id,
            role
        )
        .fetch_all(connection.as_mut())
        .await?;
        Self::notify_deleted(connection, attachment_ids).await
    }

    async fn notify_deleted(
        mut connection: impl WriteConnection,
        attachment_ids: Vec<AttachmentId>,
    ) -> sqlx::Result<()> {
        for attachment_id in attachment_ids {
            let exists = query_scalar!(
                r#"SELECT EXISTS(
                    SELECT 1 FROM attachment WHERE attachment_id = ?
                ) AS "exists: bool""#,
                attachment_id,
            )
            .fetch_one(connection.as_mut())
            .await?;
            if exists {
                connection.notifier().update(attachment_id);
            } else {
                connection.notifier().remove(attachment_id);
            }
        }
        Ok(())
    }

    /// Loads own messages whose attachments are all uploaded but which are
    /// neither sent nor queued for sending.
    ///
    /// Uploading an attachment and enqueueing its message are two steps, so a
    /// client killed in between leaves the content on the server and the message
    /// stuck. Received messages are stored as sent and never match.
    pub(crate) async fn load_unqueued_uploaded_messages(
        mut connection: impl ReadConnection,
    ) -> sqlx::Result<Vec<UnqueuedAttachmentMessage>> {
        let failed_status: u8 = MessageStatus::Error.into();
        let rows = query!(
            r#"SELECT DISTINCT
                m.chat_id AS "chat_id: ChatId",
                m.message_id AS "message_id: MessageId"
            FROM message m
            JOIN message_attachment ma ON ma.message_id = m.message_id
            LEFT JOIN chat_message_queue q ON q.message_id = m.message_id
            WHERE m.sent = 0
                AND m.status != ?1
                AND q.message_id IS NULL
                AND NOT EXISTS (
                    SELECT 1 FROM message_attachment other
                    JOIN attachment_transfer t ON t.attachment_id = other.attachment_id
                    WHERE other.message_id = m.message_id
                )"#,
            failed_status,
        )
        .fetch_all(connection.as_mut())
        .await?;
        Ok(rows
            .into_iter()
            .map(|row| UnqueuedAttachmentMessage {
                chat_id: row.chat_id,
                message_id: row.message_id,
            })
            .collect())
    }

    /// Load all attachment infos for a given message.
    ///
    /// Infos are ordered by the part index.
    pub(crate) async fn load_infos_by_message_id(
        mut connection: impl ReadConnection,
        message_id: MessageId,
    ) -> sqlx::Result<MessageAttachments> {
        let rows = query!(
            r#"SELECT
                ma.attachment_id AS "attachment_id: AttachmentId",
                ma.part_index AS "part_index: PartIndex",
                r.remote_attachment_id AS "remote_attachment_id?: RemoteAttachmentId",
                a.is_animated AS "is_animated?"
            FROM message_attachment ma
            JOIN attachment a ON a.attachment_id = ma.attachment_id
            LEFT JOIN attachment_remote r ON r.attachment_id = ma.attachment_id
            WHERE ma.message_id = ?
            ORDER BY ma.part_index"#,
            message_id
        )
        .fetch_all(connection.as_mut())
        .await?;
        let infos = rows
            .into_iter()
            .map(|row| AttachmentInfo {
                attachment_id: row.attachment_id,
                part_index: row.part_index,
                remote_attachment_id: row.remote_attachment_id,
                is_animated: row.is_animated,
            })
            .collect();
        Ok(MessageAttachments::new(message_id, infos))
    }

    pub(crate) async fn load_infos_in_range_inclusive(
        mut connection: impl ReadConnection,
        chat_id: ChatId,
        mut from: (DateTime<Utc>, MessageId),
        mut to: (DateTime<Utc>, MessageId),
    ) -> sqlx::Result<HashMap<MessageId, MessageAttachments>> {
        // Normalize the range
        if from > to {
            std::mem::swap(&mut from, &mut to);
        }
        let (from_timestamp, from_message_id) = from;
        let (to_timestamp, to_message_id) = to;

        let rows = query!(
            r#"
            SELECT
                ma.message_id AS "message_id: MessageId",
                ma.attachment_id AS "attachment_id: AttachmentId",
                ma.part_index AS "part_index: PartIndex",
                r.remote_attachment_id AS "remote_attachment_id?: RemoteAttachmentId",
                a.is_animated AS "is_animated?"
            FROM message_attachment ma
            JOIN message m ON m.message_id = ma.message_id
            JOIN attachment a ON a.attachment_id = ma.attachment_id
            LEFT JOIN attachment_remote r ON r.attachment_id = ma.attachment_id
            WHERE m.chat_id = ?
                AND (m.timestamp, m.message_id) >= (?, ?)
                AND (m.timestamp, m.message_id) <= (?, ?)
            ORDER BY ma.message_id, ma.part_index
            "#,
            chat_id,
            from_timestamp,
            from_message_id,
            to_timestamp,
            to_message_id,
        )
        .fetch_all(connection.as_mut())
        .await?;

        let mut attachment_infos: HashMap<MessageId, Vec<AttachmentInfo>> = HashMap::new();
        for row in rows {
            attachment_infos
                .entry(row.message_id)
                .or_default()
                .push(AttachmentInfo {
                    attachment_id: row.attachment_id,
                    part_index: row.part_index,
                    remote_attachment_id: row.remote_attachment_id,
                    is_animated: row.is_animated,
                });
        }

        Ok(attachment_infos
            .into_iter()
            .map(|(message_id, infos)| (message_id, MessageAttachments::new(message_id, infos)))
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use sqlx::{Pool, Sqlite};

    use crate::{
        ChatMessage,
        chats::messages::persistence::tests::test_chat_message_with_salt,
        clients::attachment::persistence::{
            AttachmentRecord,
            test::{count, stored_message, test_attachment_record, test_remote_attachment_record},
        },
        db::access::DbAccess,
    };

    use super::*;

    #[sqlx::test]
    async fn delete_message_cascade_attachment(pool: Pool<Sqlite>) -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(pool);
        let message = stored_message(&pool).await?;
        let record = test_attachment_record(message.id());

        record.store(pool.write().await?, Some(b"content")).await?;
        test_remote_attachment_record()
            .store(pool.write().await?, record.attachment_id)
            .await?;
        assert!(
            AttachmentRecord::load(pool.read().await?, record.attachment_id)
                .await?
                .is_some()
        );
        assert_eq!(count(&pool, "attachment_content").await?, 1);

        // The reference cascades with the message, and the trigger deletes the
        // attachment without references
        ChatMessage::delete(pool.write().await?, message.id()).await?;

        assert_eq!(count(&pool, "message_attachment").await?, 0);
        assert_eq!(count(&pool, "attachment").await?, 0);
        assert_eq!(count(&pool, "attachment_content").await?, 0);
        assert_eq!(count(&pool, "attachment_remote").await?, 0);
        assert_eq!(count(&pool, "attachment_transfer").await?, 0);

        Ok(())
    }

    #[sqlx::test]
    async fn delete_attachment_by_message_id(pool: Pool<Sqlite>) -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(pool);
        let message = stored_message(&pool).await?;

        let record1 = test_attachment_record(message.id());
        let record2 = test_attachment_record(message.id());
        record1.store(pool.write().await?, Some(b"one")).await?;
        record2.store(pool.write().await?, None).await?;

        let ids: Vec<_> =
            MessageAttachmentRecord::load_infos_by_message_id(pool.read().await?, message.id())
                .await?
                .iter()
                .map(|info| info.attachment_id)
                .collect();
        assert_eq!(ids.len(), 2);
        assert!(ids.contains(&record1.attachment_id));
        assert!(ids.contains(&record2.attachment_id));

        MessageAttachmentRecord::delete_by_message_id(pool.write().await?, message.id()).await?;

        for record in [&record1, &record2] {
            assert!(
                AttachmentRecord::load(pool.read().await?, record.attachment_id)
                    .await?
                    .is_none()
            );
        }
        assert_eq!(count(&pool, "attachment").await?, 0);
        assert_eq!(count(&pool, "attachment_content").await?, 0);
        let infos =
            MessageAttachmentRecord::load_infos_by_message_id(pool.read().await?, message.id())
                .await?;
        assert!(infos.is_empty());

        Ok(())
    }

    #[sqlx::test]
    async fn shared_attachment_lives_until_its_last_reference(
        pool: Pool<Sqlite>,
    ) -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(pool);
        let first = stored_message(&pool).await?;
        let second = test_chat_message_with_salt(first.chat_id(), [1; 16]);
        second.store(pool.write().await?).await?;

        let record = test_attachment_record(first.id());
        record.store(pool.write().await?, Some(b"content")).await?;
        MessageAttachmentRecord {
            message_id: second.id(),
            ..record.reference()
        }
        .store(pool.write().await?)
        .await?;

        // One of two references is gone
        MessageAttachmentRecord::delete_by_message_id(pool.write().await?, first.id()).await?;
        assert_eq!(count(&pool, "attachment").await?, 1);
        assert_eq!(count(&pool, "attachment_content").await?, 1);
        let loaded = MessageAttachmentRecord::load_first(pool.read().await?, record.attachment_id)
            .await?
            .unwrap();
        assert_eq!(loaded.message_id, second.id());

        // The last reference is gone through the message cascade
        ChatMessage::delete(pool.write().await?, second.id()).await?;
        assert_eq!(count(&pool, "attachment").await?, 0);
        assert_eq!(count(&pool, "attachment_content").await?, 0);

        Ok(())
    }
}
