// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use chrono::{DateTime, Utc};
use sqlx::{
    Database, Encode, Sqlite, Type, encode::IsNull, error::BoxDynError, query, query_scalar,
};

use crate::{
    AttachmentId, ChatId,
    db::access::{ReadConnection, WriteConnection},
};

use super::*;

/// Row of `attachment_transfer`.
///
/// An attachment without a row has nothing left to transfer.
#[derive(Debug, Clone, Copy)]
#[cfg_attr(test, derive(PartialEq, Eq))]
pub(crate) struct AttachmentTransferRecord {
    pub(crate) attachment_id: AttachmentId,
    pub(crate) direction: TransferDirection,
    pub(crate) state: TransferState,
    pub(crate) failure: Option<TransferFailure>,
    pub(crate) started_at: Option<DateTime<Utc>>,
}

/// Value of `attachment_transfer.direction`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub(crate) enum TransferDirection {
    Download = 1,
    Upload = 2,
}

/// Value of `attachment_transfer.state`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub(crate) enum TransferState {
    Queued = 1,
    Running = 2,
    Failed = 3,
}

/// Value of `attachment_transfer.failure`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub(crate) enum TransferFailure {
    Unknown = 1,
    NotFound = 2,
    /// The server refused to upload
    Rejected = 3,
}

impl TransferDirection {
    fn from_raw(value: u32) -> Option<Self> {
        match value {
            1 => Some(Self::Download),
            2 => Some(Self::Upload),
            _ => None,
        }
    }
}

impl TransferState {
    fn from_raw(value: u32) -> Option<Self> {
        match value {
            1 => Some(Self::Queued),
            2 => Some(Self::Running),
            3 => Some(Self::Failed),
            _ => None,
        }
    }
}

impl TransferFailure {
    fn from_raw(value: u32) -> Option<Self> {
        match value {
            1 => Some(Self::Unknown),
            2 => Some(Self::NotFound),
            3 => Some(Self::Rejected),
            _ => None,
        }
    }
}

impl AttachmentStatus {
    /// Status of an attachment with the given transfer row, if any.
    pub(super) fn from_transfer(
        direction: Option<u32>,
        state: Option<u32>,
        failure: Option<u32>,
    ) -> Self {
        use {TransferDirection::*, TransferState::*};
        let Some(direction) = direction else {
            return Self::Ready;
        };
        let direction = TransferDirection::from_raw(direction);
        let state = state.and_then(TransferState::from_raw);
        let (Some(direction), Some(state)) = (direction, state) else {
            return Self::Unknown;
        };
        let failure = failure.and_then(TransferFailure::from_raw);
        match (direction, state, failure) {
            (Download, Queued, _) => Self::Pending,
            (Download, Running, _) => Self::Downloading,
            (Download, Failed, Some(TransferFailure::NotFound)) => Self::NotFound,
            (Download, Failed, _) => Self::DownloadFailed,
            (Upload, Queued | Running, _) => Self::Uploading,
            (Upload, Failed, _) => Self::UploadFailed,
        }
    }
}

impl Type<Sqlite> for TransferDirection {
    fn type_info() -> <Sqlite as Database>::TypeInfo {
        <u32 as Type<Sqlite>>::type_info()
    }
}

impl<'q> Encode<'q, Sqlite> for TransferDirection {
    fn encode_by_ref(
        &self,
        buf: &mut <Sqlite as Database>::ArgumentBuffer,
    ) -> Result<IsNull, BoxDynError> {
        Encode::<Sqlite>::encode(*self as u32, buf)
    }
}

impl Type<Sqlite> for TransferState {
    fn type_info() -> <Sqlite as Database>::TypeInfo {
        <u32 as Type<Sqlite>>::type_info()
    }
}

impl<'q> Encode<'q, Sqlite> for TransferState {
    fn encode_by_ref(
        &self,
        buf: &mut <Sqlite as Database>::ArgumentBuffer,
    ) -> Result<IsNull, BoxDynError> {
        Encode::<Sqlite>::encode(*self as u32, buf)
    }
}

impl Type<Sqlite> for TransferFailure {
    fn type_info() -> <Sqlite as Database>::TypeInfo {
        <u32 as Type<Sqlite>>::type_info()
    }
}

impl<'q> Encode<'q, Sqlite> for TransferFailure {
    fn encode_by_ref(
        &self,
        buf: &mut <Sqlite as Database>::ArgumentBuffer,
    ) -> Result<IsNull, BoxDynError> {
        Encode::<Sqlite>::encode(*self as u32, buf)
    }
}

impl AttachmentTransferRecord {
    /// Transfer of an attachment in `status`, `None` if there is no work left.
    ///
    /// A running transfer starts at `now`.
    pub(crate) fn from_status(
        attachment_id: AttachmentId,
        status: AttachmentStatus,
        now: DateTime<Utc>,
    ) -> Option<Self> {
        use {TransferDirection::*, TransferState::*};
        let (direction, state, failure) = match status {
            AttachmentStatus::Ready => return None,
            AttachmentStatus::Pending => (Download, Queued, None),
            AttachmentStatus::Downloading => (Download, Running, None),
            AttachmentStatus::DownloadFailed | AttachmentStatus::Unknown => {
                (Download, Failed, Some(TransferFailure::Unknown))
            }
            AttachmentStatus::NotFound => (Download, Failed, Some(TransferFailure::NotFound)),
            AttachmentStatus::Uploading => (Upload, Running, None),
            AttachmentStatus::UploadFailed => (Upload, Failed, Some(TransferFailure::Unknown)),
        };
        Some(Self {
            attachment_id,
            direction,
            state,
            failure,
            started_at: (state == Running).then_some(now),
        })
    }

    /// Inserts or replaces the transfer of the attachment.
    pub(crate) async fn store(&self, mut connection: impl WriteConnection) -> sqlx::Result<()> {
        query!(
            "INSERT INTO attachment_transfer (
                attachment_id,
                direction,
                state,
                failure,
                started_at
            ) VALUES (?, ?, ?, ?, ?)
            ON CONFLICT (attachment_id) DO UPDATE SET
                direction = excluded.direction,
                state = excluded.state,
                failure = excluded.failure,
                started_at = excluded.started_at",
            self.attachment_id,
            self.direction,
            self.state,
            self.failure,
            self.started_at,
        )
        .execute(connection.as_mut())
        .await?;
        Ok(())
    }

    pub(crate) async fn delete(
        mut connection: impl WriteConnection,
        attachment_id: AttachmentId,
    ) -> sqlx::Result<()> {
        query!(
            "DELETE FROM attachment_transfer WHERE attachment_id = ?",
            attachment_id,
        )
        .execute(connection.as_mut())
        .await?;
        Ok(())
    }

    /// Loads the transfer of the attachment.
    ///
    /// Fails to decode a row with an unknown value.
    #[cfg(test)]
    pub(crate) async fn load(
        mut connection: impl ReadConnection,
        attachment_id: AttachmentId,
    ) -> sqlx::Result<Option<Self>> {
        let record = query!(
            r#"SELECT
                direction AS "direction: u32",
                state AS "state: u32",
                failure AS "failure?: u32",
                started_at AS "started_at?: DateTime<Utc>"
            FROM attachment_transfer
            WHERE attachment_id = ?"#,
            attachment_id
        )
        .fetch_optional(connection.as_mut())
        .await?;
        let Some(record) = record else {
            return Ok(None);
        };
        let direction = TransferDirection::from_raw(record.direction);
        let state = TransferState::from_raw(record.state);
        let failure = record.failure.map(TransferFailure::from_raw);
        let (Some(direction), Some(state), None | Some(Some(_))) = (direction, state, failure)
        else {
            return Err(sqlx::Error::Decode(
                "unknown attachment transfer value".into(),
            ));
        };
        Ok(Some(Self {
            attachment_id,
            direction,
            state,
            failure: failure.flatten(),
            started_at: record.started_at,
        }))
    }

    /// Loads the attachments queued for download, oldest first.
    pub(crate) async fn load_queued_downloads(
        mut connection: impl ReadConnection,
    ) -> sqlx::Result<Vec<AttachmentId>> {
        query_scalar!(
            r#"SELECT
                t.attachment_id AS "attachment_id: AttachmentId"
            FROM attachment_transfer t
            JOIN attachment a ON a.attachment_id = t.attachment_id
            WHERE t.direction = ? AND t.state = ?
            ORDER BY a.created_at ASC"#,
            TransferDirection::Download,
            TransferState::Queued,
        )
        .fetch_all(connection.as_mut())
        .await
    }

    /// Moves every attachment in `Downloading` back to `Pending` and returns
    /// their ids.
    pub(crate) async fn reset_downloading(
        mut connection: impl WriteConnection,
    ) -> sqlx::Result<Vec<AttachmentId>> {
        let attachment_ids = query_scalar!(
            r#"UPDATE attachment_transfer
            SET state = ?, started_at = NULL
            WHERE direction = ? AND state = ?
            RETURNING attachment_id AS "attachment_id: AttachmentId""#,
            TransferState::Queued,
            TransferDirection::Download,
            TransferState::Running,
        )
        .fetch_all(connection.as_mut())
        .await?;
        for &attachment_id in &attachment_ids {
            connection.notifier().update(attachment_id);
        }
        Ok(attachment_ids)
    }

    /// Loads attachments of unsent messages that have been uploading since before `stale_before`.
    ///
    /// A running upload does not hold the global lock, so only uploads too old
    /// to still be in flight are reported.
    pub(crate) async fn load_stale_uploads(
        mut connection: impl ReadConnection,
        stale_before: DateTime<Utc>,
    ) -> sqlx::Result<Vec<AttachmentId>> {
        query_scalar!(
            r#"SELECT DISTINCT t.attachment_id AS "attachment_id: AttachmentId"
            FROM attachment_transfer t
            JOIN message_attachment ma ON ma.attachment_id = t.attachment_id
            JOIN message m ON m.message_id = ma.message_id
            WHERE t.direction = ?1
                AND t.state IN (?2, ?3)
                AND t.started_at < ?4
                AND m.sent = 0"#,
            TransferDirection::Upload,
            TransferState::Queued,
            TransferState::Running,
            stale_before,
        )
        .fetch_all(connection.as_mut())
        .await
    }

    /// Puts a failed attachment back into uploading.
    ///
    /// The start time of the transfer moves to `started_at`, because it is what
    /// the recovery pass measures staleness against.
    pub(crate) async fn restart_upload(
        mut connection: impl WriteConnection,
        attachment_id: AttachmentId,
        started_at: DateTime<Utc>,
    ) -> sqlx::Result<()> {
        Self {
            attachment_id,
            direction: TransferDirection::Upload,
            state: TransferState::Running,
            failure: None,
            started_at: Some(started_at),
        }
        .store(&mut connection)
        .await?;
        connection.notifier().update(attachment_id);
        Ok(())
    }

    /// Marks the upload of the attachment as failed.
    ///
    /// Does nothing if the attachment has no upload.
    pub(crate) async fn fail_upload(
        mut connection: impl WriteConnection,
        attachment_id: AttachmentId,
        failure: TransferFailure,
    ) -> sqlx::Result<()> {
        let updated = query!(
            "UPDATE attachment_transfer
            SET state = ?1, failure = ?2, started_at = NULL
            WHERE attachment_id = ?3 AND direction = ?4",
            TransferState::Failed,
            failure,
            attachment_id,
            TransferDirection::Upload,
        )
        .execute(connection.as_mut())
        .await?
        .rows_affected();
        if updated > 0 {
            connection.notifier().update(attachment_id);
        }
        Ok(())
    }

    /// Whether an upload of an attachment of the message is queued or running.
    pub(crate) async fn has_active_upload(
        mut connection: impl ReadConnection,
        message_id: MessageId,
    ) -> sqlx::Result<bool> {
        query_scalar!(
            r#"SELECT EXISTS(
                SELECT 1
                FROM message_attachment ma
                JOIN attachment_transfer t ON t.attachment_id = ma.attachment_id
                WHERE ma.message_id = ?1
                    AND t.direction = ?2
                    AND t.state IN (?3, ?4)
            ) AS "exists: bool""#,
            message_id,
            TransferDirection::Upload,
            TransferState::Queued,
            TransferState::Running,
        )
        .fetch_one(connection.as_mut())
        .await
    }

    /// Loads the queued or running uploads of queued messages, with the chat
    /// of their message.
    pub(crate) async fn load_active_uploads(
        mut connection: impl ReadConnection,
    ) -> sqlx::Result<Vec<(ChatId, AttachmentId)>> {
        let rows = query!(
            r#"SELECT
                q.chat_id AS "chat_id: ChatId",
                t.attachment_id AS "attachment_id: AttachmentId"
            FROM attachment_transfer t
            JOIN message_attachment ma ON ma.attachment_id = t.attachment_id
            JOIN chat_message_queue q ON q.message_id = ma.message_id
            WHERE t.direction = ?1
                AND t.state IN (?2, ?3)
            ORDER BY q.created_at, ma.part_index"#,
            TransferDirection::Upload,
            TransferState::Queued,
            TransferState::Running,
        )
        .fetch_all(connection.as_mut())
        .await?;
        Ok(rows
            .into_iter()
            .map(|row| (row.chat_id, row.attachment_id))
            .collect())
    }

    /// Loads the attachments of the message whose upload failed.
    pub(crate) async fn load_failed_uploads(
        mut connection: impl ReadConnection,
        message_id: MessageId,
    ) -> sqlx::Result<Vec<AttachmentId>> {
        query_scalar!(
            r#"SELECT t.attachment_id AS "attachment_id: AttachmentId"
            FROM message_attachment ma
            JOIN attachment_transfer t ON t.attachment_id = ma.attachment_id
            WHERE ma.message_id = ?1
                AND t.direction = ?2
                AND t.state = ?3
            ORDER BY ma.part_index"#,
            message_id,
            TransferDirection::Upload,
            TransferState::Failed,
        )
        .fetch_all(connection.as_mut())
        .await
    }

    /// Whether an upload of an attachment of the message failed.
    pub(crate) async fn has_failed_upload(
        mut connection: impl ReadConnection,
        message_id: MessageId,
    ) -> sqlx::Result<bool> {
        query_scalar!(
            r#"SELECT EXISTS(
                SELECT 1
                FROM message_attachment ma
                JOIN attachment_transfer t ON t.attachment_id = ma.attachment_id
                WHERE ma.message_id = ?1
                    AND t.direction = ?2
                    AND t.state = ?3
            ) AS "exists: bool""#,
            message_id,
            TransferDirection::Upload,
            TransferState::Failed,
        )
        .fetch_one(connection.as_mut())
        .await
    }
}

#[cfg(test)]
mod tests {
    use chrono::SubsecRound;
    use sqlx::{Pool, Sqlite};

    use crate::{
        clients::attachment::persistence::{
            AttachmentRecord, AttachmentStatus, NewAttachment,
            test::{stored_message, test_attachment_record, test_attachment_record_with},
        },
        db::access::DbAccess,
    };

    use super::*;

    #[sqlx::test]
    async fn restart_upload_moves_started_at(pool: Pool<Sqlite>) -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(pool);
        let message = stored_message(&pool).await?;
        let created_at = Utc::now().round_subsecs(6) - chrono::Duration::hours(2);
        let record =
            test_attachment_record_with(message.id(), AttachmentStatus::UploadFailed, created_at);
        record.store(pool.write().await?, Some(b"content")).await?;

        let started_at = Utc::now().round_subsecs(6);
        AttachmentTransferRecord::restart_upload(
            pool.write().await?,
            record.attachment_id,
            started_at,
        )
        .await?;

        let status = AttachmentRecord::status(pool.read().await?, record.attachment_id).await?;
        assert_eq!(status, Some(AttachmentStatus::Uploading));
        let loaded = AttachmentRecord::load(pool.read().await?, record.attachment_id)
            .await?
            .unwrap();
        assert_eq!(loaded.created_at, created_at);
        let transfer = AttachmentTransferRecord::load(pool.read().await?, record.attachment_id)
            .await?
            .unwrap();
        assert_eq!(transfer.started_at, Some(started_at));

        Ok(())
    }

    #[sqlx::test]
    async fn load_all_pending_attachments(pool: Pool<Sqlite>) -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(pool);
        let message = stored_message(&pool).await?;

        // Create and store a few attachments with different statuses
        let created_at = Utc::now().round_subsecs(6);
        let mut pending_record_1 = test_attachment_record(message.id());
        pending_record_1.created_at = created_at;
        pending_record_1.store(pool.write().await?, None).await?;

        let downloading_record = NewAttachment {
            status: AttachmentStatus::Downloading,
            ..test_attachment_record(message.id())
        };
        downloading_record.store(pool.write().await?, None).await?;

        let mut pending_record_2 = test_attachment_record(message.id());
        pending_record_2.created_at = created_at
            .checked_add_signed(chrono::Duration::milliseconds(10))
            .unwrap();
        pending_record_2.store(pool.write().await?, None).await?;

        let pending_ids =
            AttachmentTransferRecord::load_queued_downloads(pool.read().await?).await?;

        // Check that only the pending ones are returned, in ascending order of creation
        assert_eq!(
            pending_ids,
            vec![
                pending_record_1.attachment_id,
                pending_record_2.attachment_id
            ]
        );

        Ok(())
    }

    #[sqlx::test]
    async fn reset_downloading_attachments(pool: Pool<Sqlite>) -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(pool);
        let message = stored_message(&pool).await?;

        let statuses = [
            AttachmentStatus::Pending,
            AttachmentStatus::Downloading,
            AttachmentStatus::Downloading,
            AttachmentStatus::DownloadFailed,
            AttachmentStatus::Uploading,
            AttachmentStatus::Ready,
        ];
        let mut records = Vec::new();
        for status in statuses {
            let record = test_attachment_record_with(message.id(), status, Utc::now());
            record.store(pool.write().await?, None).await?;
            records.push(record);
        }

        let mut reset_ids =
            AttachmentTransferRecord::reset_downloading(pool.write().await?).await?;
        reset_ids.sort();
        let mut expected_ids = vec![records[1].attachment_id, records[2].attachment_id];
        expected_ids.sort();
        assert_eq!(reset_ids, expected_ids);

        let expected_statuses = [
            AttachmentStatus::Pending,
            AttachmentStatus::Pending,
            AttachmentStatus::Pending,
            AttachmentStatus::DownloadFailed,
            AttachmentStatus::Uploading,
            AttachmentStatus::Ready,
        ];
        for (record, expected) in records.iter().zip(expected_statuses) {
            let status = AttachmentRecord::status(pool.read().await?, record.attachment_id).await?;
            assert_eq!(status, Some(expected));
        }

        // Nothing is left to reset
        let reset_ids = AttachmentTransferRecord::reset_downloading(pool.write().await?).await?;
        assert!(reset_ids.is_empty());

        Ok(())
    }
}
