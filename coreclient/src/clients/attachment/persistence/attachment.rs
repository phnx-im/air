// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use chrono::{DateTime, Utc};
use sqlx::{query, query_scalar};

use crate::{
    AttachmentId,
    db::access::{ReadConnection, WriteConnection},
};

use super::*;

/// Row of `attachment`.
///
/// Content is intentionally not included in this struct.
#[derive(Debug)]
#[cfg_attr(test, derive(PartialEq, Eq))]
pub(crate) struct AttachmentRecord {
    /// Uniquely identifies the attachment on this local client.
    pub(crate) attachment_id: AttachmentId,
    pub(crate) content_type: String,
    pub(crate) size: Option<u64>,
    pub(crate) content_hash: Option<Vec<u8>>,
    pub(crate) dimensions: Option<(u32, u32)>,
    pub(crate) blurhash: Option<String>,
    pub(crate) is_animated: Option<bool>,
    pub(crate) created_at: DateTime<Utc>,
}

/// Moves attachment content blobs into the `attachment_content` table.
///
/// Code migration paired with the migration creating the table. Runs in small
/// batches so that pages freed by clearing a blob are reused by the next
/// batch's inserts and the database file does not grow. Uses unchecked
/// queries: the source column does not exist in the final schema.
pub(crate) async fn move_attachment_content_to_side_table(
    mut connection: impl WriteConnection,
) -> sqlx::Result<()> {
    loop {
        let mut txn = connection.begin().await?;
        sqlx::query(
            "INSERT INTO attachment_content (attachment_id, content)
            SELECT attachment_id, content FROM attachment
            WHERE content IS NOT NULL
            LIMIT 16",
        )
        .execute(txn.as_mut())
        .await?;
        let cleared = sqlx::query(
            "UPDATE attachment SET content = NULL
            WHERE content IS NOT NULL
            AND attachment_id IN (SELECT attachment_id FROM attachment_content)",
        )
        .execute(txn.as_mut())
        .await?
        .rows_affected();
        txn.commit().await?;
        if cleared == 0 {
            return Ok(());
        }
    }
}

impl AttachmentRecord {
    /// Stores the attachment and its content, if any.
    pub(crate) async fn store(
        &self,
        mut connection: impl WriteConnection,
        content: Option<&[u8]>,
    ) -> sqlx::Result<()> {
        let size = self.size.and_then(|size| i64::try_from(size).ok());
        let (width, height) = self.dimensions.unzip();
        query!(
            "INSERT INTO attachment (
                attachment_id,
                content_type,
                size,
                content_hash,
                width,
                height,
                blurhash,
                is_animated,
                created_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
            self.attachment_id,
            self.content_type,
            size,
            self.content_hash,
            width,
            height,
            self.blurhash,
            self.is_animated,
            self.created_at,
        )
        .execute(connection.as_mut())
        .await?;
        if let Some(content) = content {
            query!(
                "INSERT INTO attachment_content (attachment_id, content) VALUES (?, ?)",
                self.attachment_id,
                content,
            )
            .execute(connection.as_mut())
            .await?;
        }
        connection.notifier().add(self.attachment_id);
        Ok(())
    }

    #[cfg(test)]
    pub(crate) async fn load(
        mut connection: impl ReadConnection,
        attachment_id: AttachmentId,
    ) -> sqlx::Result<Option<Self>> {
        let record = query!(
            r#"SELECT
                attachment_id AS "attachment_id: AttachmentId",
                content_type,
                size AS "size?: i64",
                content_hash AS "content_hash?",
                width AS "width?: u32",
                height AS "height?: u32",
                blurhash AS "blurhash?",
                is_animated AS "is_animated?",
                created_at AS "created_at: DateTime<Utc>"
            FROM attachment
            WHERE attachment_id = ?"#,
            attachment_id
        )
        .fetch_optional(connection.as_mut())
        .await?;
        Ok(record.map(|record| Self {
            attachment_id: record.attachment_id,
            content_type: record.content_type,
            size: record.size.and_then(|size| u64::try_from(size).ok()),
            content_hash: record.content_hash,
            dimensions: record.width.zip(record.height),
            blurhash: record.blurhash,
            is_animated: record.is_animated,
            created_at: record.created_at,
        }))
    }

    pub(crate) async fn status(
        mut connection: impl ReadConnection,
        attachment_id: AttachmentId,
    ) -> sqlx::Result<Option<AttachmentStatus>> {
        let record = query!(
            r#"SELECT
                t.direction AS "direction?: u32",
                t.state AS "state?: u32",
                t.failure AS "failure?: u32"
            FROM attachment a
            LEFT JOIN attachment_transfer t ON t.attachment_id = a.attachment_id
            WHERE a.attachment_id = ?"#,
            attachment_id,
        )
        .fetch_optional(connection.as_mut())
        .await?;
        Ok(record.map(|record| {
            AttachmentStatus::from_transfer(record.direction, record.state, record.failure)
        }))
    }

    pub(crate) async fn is_animated(
        mut connection: impl ReadConnection,
        attachment_id: AttachmentId,
    ) -> sqlx::Result<Option<bool>> {
        query_scalar!(
            "SELECT is_animated
            FROM attachment
            WHERE attachment_id = ?",
            attachment_id,
        )
        .fetch_optional(connection.as_mut())
        .await
        .map(|value| value.flatten())
    }

    pub(crate) async fn set_is_animated(
        mut connection: impl WriteConnection,
        attachment_id: AttachmentId,
        is_animated: bool,
    ) -> sqlx::Result<()> {
        // Only updated if not yet classified => make it idempotent
        query!(
            "UPDATE attachment SET is_animated = ?
            WHERE attachment_id = ? AND is_animated IS NULL",
            is_animated,
            attachment_id,
        )
        .execute(connection.as_mut())
        .await?;
        Ok(())
    }

    pub(crate) async fn update_status(
        mut connection: impl WriteConnection,
        attachment_id: AttachmentId,
        status: AttachmentStatus,
    ) -> sqlx::Result<()> {
        let exists = query_scalar!(
            r#"SELECT EXISTS(
                SELECT 1 FROM attachment WHERE attachment_id = ?
            ) AS "exists: bool""#,
            attachment_id,
        )
        .fetch_one(connection.as_mut())
        .await?;
        if !exists {
            return Ok(());
        }
        match AttachmentTransferRecord::from_status(attachment_id, status, Utc::now()) {
            Some(transfer) => transfer.store(&mut connection).await?,
            None => AttachmentTransferRecord::delete(&mut connection, attachment_id).await?,
        }
        connection.notifier().update(attachment_id);
        Ok(())
    }

    /// Moves the attachment to a status, but only while it is still in the
    /// expected one. Returns whether the row changed.
    pub(crate) async fn update_status_from(
        mut connection: impl WriteConnection,
        attachment_id: AttachmentId,
        from: AttachmentStatus,
        to: AttachmentStatus,
    ) -> sqlx::Result<bool> {
        let current = Self::status(&mut connection, attachment_id).await?;
        let Some(current) = current else {
            return Ok(false);
        };
        if current != from {
            return Ok(false);
        }
        Self::update_status(connection, attachment_id, to).await?;
        Ok(true)
    }

    pub(crate) async fn set_content(
        mut connection: impl WriteConnection,
        attachment_id: AttachmentId,
        bytes: &[u8],
        is_animated: bool,
    ) -> sqlx::Result<()> {
        let updated = query!(
            "UPDATE attachment SET is_animated = ? WHERE attachment_id = ?",
            is_animated,
            attachment_id,
        )
        .execute(connection.as_mut())
        .await?
        .rows_affected();
        // Skip the content if the attachment is gone (deleted mid-download)
        if updated == 0 {
            return Ok(());
        }
        AttachmentTransferRecord::delete(&mut connection, attachment_id).await?;
        query!(
            "INSERT INTO attachment_content (
                attachment_id,
                content
            ) VALUES (?, ?)
            ON CONFLICT (attachment_id) DO UPDATE
            SET content = excluded.content",
            attachment_id,
            bytes,
        )
        .execute(connection.as_mut())
        .await?;
        connection.notifier().update(attachment_id);
        Ok(())
    }

    /// Stores the processed content of an upload, leaving the status alone.
    ///
    /// Written once re-encoding is done, before the upload starts, so a retry
    /// resumes from it without the original bytes. Returns `false` if the
    /// attachment is gone (deleted mid-upload), nothing is written then.
    pub(crate) async fn store_processed_content(
        mut connection: impl WriteConnection,
        attachment_id: AttachmentId,
        bytes: &[u8],
        processed: ProcessedMetadata<'_>,
    ) -> sqlx::Result<bool> {
        let ProcessedMetadata {
            content_type,
            filename,
            content_hash,
            dimensions,
            blurhash,
            is_animated,
        } = processed;
        let size = i64::try_from(bytes.len()).ok();
        let (width, height) = dimensions.unzip();
        let updated = query!(
            "UPDATE attachment SET
                content_type = ?,
                size = ?,
                content_hash = ?,
                width = ?,
                height = ?,
                blurhash = ?,
                is_animated = ?
            WHERE attachment_id = ?",
            content_type,
            size,
            content_hash,
            width,
            height,
            blurhash,
            is_animated,
            attachment_id,
        )
        .execute(connection.as_mut())
        .await?
        .rows_affected();
        if updated == 0 {
            return Ok(false);
        }
        MessageAttachmentRecord::update_filename(&mut connection, attachment_id, filename).await?;
        query!(
            "INSERT INTO attachment_content (
                attachment_id,
                content
            ) VALUES (?, ?)
            ON CONFLICT (attachment_id) DO UPDATE
            SET content = excluded.content",
            attachment_id,
            bytes,
        )
        .execute(connection.as_mut())
        .await?;
        connection.notifier().update(attachment_id);
        Ok(true)
    }

    pub(crate) async fn load_content(
        mut connection: impl ReadConnection,
        attachment_id: AttachmentId,
    ) -> sqlx::Result<AttachmentContent> {
        let record = query!(
            r#"SELECT
                ac.content AS "content?",
                t.direction AS "direction?: u32",
                t.state AS "state?: u32",
                t.failure AS "failure?: u32"
            FROM attachment a
            LEFT JOIN attachment_content ac ON ac.attachment_id = a.attachment_id
            LEFT JOIN attachment_transfer t ON t.attachment_id = a.attachment_id
            WHERE a.attachment_id = ?"#,
            attachment_id
        )
        .fetch_optional(connection.as_mut())
        .await?;
        match record {
            Some(record) => {
                let status =
                    AttachmentStatus::from_transfer(record.direction, record.state, record.failure);
                Ok(AttachmentContent::from_parts(record.content, status))
            }
            None => Ok(AttachmentContent::None),
        }
    }

    pub(crate) async fn has_content(
        mut connection: impl ReadConnection,
        attachment_id: AttachmentId,
    ) -> sqlx::Result<bool> {
        query_scalar!(
            r#"SELECT EXISTS(
                SELECT 1 FROM attachment_content WHERE attachment_id = ?
            ) AS "exists: _""#,
            attachment_id,
        )
        .fetch_one(connection.as_mut())
        .await
    }
}

/// What processing an upload learned about its content.
pub(crate) struct ProcessedMetadata<'a> {
    pub(crate) content_type: &'a str,
    pub(crate) filename: &'a str,
    pub(crate) content_hash: &'a [u8],
    pub(crate) dimensions: Option<(u32, u32)>,
    pub(crate) blurhash: Option<&'a str>,
    pub(crate) is_animated: Option<bool>,
}

#[cfg(test)]
mod tests {
    use chrono::SubsecRound;
    use sqlx::{Pool, Sqlite};

    use crate::{
        clients::attachment::persistence::{
            AttachmentContent, AttachmentStatus,
            test::{count, stored_message, test_attachment_record, test_attachment_record_with},
        },
        db::access::DbAccess,
    };

    use super::*;

    #[sqlx::test]
    async fn attachment_record_store_and_load(pool: Pool<Sqlite>) -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(pool);
        let message = stored_message(&pool).await?;
        let record = test_attachment_record(message.id());

        record.store(pool.write().await?, None).await?;

        let loaded_record =
            AttachmentRecord::load(pool.read().await?, record.attachment_id).await?;
        assert_eq!(loaded_record, Some(record.attachment()));
        let loaded_reference =
            MessageAttachmentRecord::load_first(pool.read().await?, record.attachment_id).await?;
        assert_eq!(loaded_reference, Some(record.reference()));
        let loaded_transfer =
            AttachmentTransferRecord::load(pool.read().await?, record.attachment_id).await?;
        assert_eq!(loaded_transfer, record.transfer());

        Ok(())
    }

    #[sqlx::test]
    async fn attachment_content_lifecycle(pool: Pool<Sqlite>) -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(pool);
        let message = stored_message(&pool).await?;
        let record = test_attachment_record(message.id());

        // 1. Store the record with no content, status should be Pending.
        record.store(pool.write().await?, None).await?;
        let loaded_content =
            AttachmentRecord::load_content(pool.read().await?, record.attachment_id).await?;
        assert_eq!(loaded_content, AttachmentContent::Pending);

        // 2. Update status to Downloading
        AttachmentRecord::update_status(
            pool.write().await?,
            record.attachment_id,
            AttachmentStatus::Downloading,
        )
        .await?;
        let loaded_content =
            AttachmentRecord::load_content(pool.read().await?, record.attachment_id).await?;
        assert_eq!(loaded_content, AttachmentContent::Downloading);

        // 3. Set the content, which should move the status to Ready
        let content = b"some_image_content".to_vec();
        AttachmentRecord::set_content(pool.write().await?, record.attachment_id, &content, true)
            .await?;

        let loaded_content =
            AttachmentRecord::load_content(pool.read().await?, record.attachment_id).await?;
        assert_eq!(loaded_content, AttachmentContent::Ready(content.clone()));

        let status = AttachmentRecord::status(pool.read().await?, record.attachment_id).await?;
        assert_eq!(status, Some(AttachmentStatus::Ready));
        assert_eq!(count(&pool, "attachment_transfer").await?, 0);

        // 4. Update status to Failed
        AttachmentRecord::update_status(
            pool.write().await?,
            record.attachment_id,
            AttachmentStatus::DownloadFailed,
        )
        .await?;
        let loaded_content =
            AttachmentRecord::load_content(pool.read().await?, record.attachment_id).await?;
        assert_eq!(loaded_content, AttachmentContent::DownloadFailed);

        // 5. Check loading content for a non-existent attachment
        let non_existent_id = AttachmentId::random();
        let loaded_content =
            AttachmentRecord::load_content(pool.read().await?, non_existent_id).await?;
        assert_eq!(loaded_content, AttachmentContent::None);

        Ok(())
    }

    #[sqlx::test]
    async fn status_round_trips_through_the_transfer(pool: Pool<Sqlite>) -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(pool);
        let message = stored_message(&pool).await?;
        let record = test_attachment_record(message.id());
        record.store(pool.write().await?, None).await?;

        for status in [
            AttachmentStatus::Pending,
            AttachmentStatus::Downloading,
            AttachmentStatus::DownloadFailed,
            AttachmentStatus::NotFound,
            AttachmentStatus::Uploading,
            AttachmentStatus::UploadFailed,
            AttachmentStatus::Ready,
        ] {
            AttachmentRecord::update_status(pool.write().await?, record.attachment_id, status)
                .await?;
            let loaded = AttachmentRecord::status(pool.read().await?, record.attachment_id).await?;
            assert_eq!(loaded, Some(status));
        }

        Ok(())
    }

    #[sqlx::test]
    async fn transition_status_only_from_the_expected_status(
        pool: Pool<Sqlite>,
    ) -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(pool);
        let message = stored_message(&pool).await?;
        let record = test_attachment_record_with(
            message.id(),
            AttachmentStatus::Ready,
            Utc::now().round_subsecs(6),
        );
        record.store(pool.write().await?, Some(b"content")).await?;

        let changed = AttachmentRecord::update_status_from(
            pool.write().await?,
            record.attachment_id,
            AttachmentStatus::Uploading,
            AttachmentStatus::UploadFailed,
        )
        .await?;
        assert!(!changed);
        let status = AttachmentRecord::status(pool.read().await?, record.attachment_id).await?;
        assert_eq!(status, Some(AttachmentStatus::Ready));

        let changed = AttachmentRecord::update_status_from(
            pool.write().await?,
            record.attachment_id,
            AttachmentStatus::Ready,
            AttachmentStatus::UploadFailed,
        )
        .await?;
        assert!(changed);
        let status = AttachmentRecord::status(pool.read().await?, record.attachment_id).await?;
        assert_eq!(status, Some(AttachmentStatus::UploadFailed));

        Ok(())
    }

    #[sqlx::test]
    async fn set_content_for_missing_attachment_is_noop(pool: Pool<Sqlite>) -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(pool);

        let attachment_id = AttachmentId::random();
        AttachmentRecord::set_content(pool.write().await?, attachment_id, b"content", false)
            .await?;

        let loaded_content =
            AttachmentRecord::load_content(pool.read().await?, attachment_id).await?;
        assert_eq!(loaded_content, AttachmentContent::None);

        // No orphaned content row is left behind
        assert_eq!(count(&pool, "attachment_content").await?, 0);

        Ok(())
    }

    #[sqlx::test]
    async fn set_is_animated_classifies_only_once(pool: Pool<Sqlite>) -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(pool);
        let message = stored_message(&pool).await?;
        let mut record = test_attachment_record(message.id());
        record.is_animated = None;
        record.store(pool.write().await?, None).await?;

        let is_animated =
            AttachmentRecord::is_animated(pool.read().await?, record.attachment_id).await?;
        assert_eq!(is_animated, None);

        AttachmentRecord::set_is_animated(pool.write().await?, record.attachment_id, true).await?;
        let is_animated =
            AttachmentRecord::is_animated(pool.read().await?, record.attachment_id).await?;
        assert_eq!(is_animated, Some(true));

        // A second classification does not overwrite the first
        AttachmentRecord::set_is_animated(pool.write().await?, record.attachment_id, false).await?;
        let is_animated =
            AttachmentRecord::is_animated(pool.read().await?, record.attachment_id).await?;
        assert_eq!(is_animated, Some(true));

        Ok(())
    }

    #[sqlx::test]
    async fn attachment_record_update_status(pool: Pool<Sqlite>) -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(pool);
        let message = stored_message(&pool).await?;
        let record = test_attachment_record(message.id());
        let attachment_id = record.attachment_id;

        let content = b"some_image_content".to_vec();

        record.store(pool.write().await?, Some(&content)).await?;
        let loaded_record = AttachmentRecord::load(pool.read().await?, attachment_id).await?;
        assert_eq!(loaded_record, Some(record.attachment()));
        let loaded_content =
            AttachmentRecord::load_content(pool.read().await?, attachment_id).await?;
        assert_eq!(loaded_content, AttachmentContent::Pending);

        AttachmentRecord::update_status(
            pool.write().await?,
            attachment_id,
            AttachmentStatus::Ready,
        )
        .await?;
        let loaded_record = AttachmentRecord::load(pool.read().await?, attachment_id).await?;
        assert_eq!(loaded_record, Some(record.attachment()));
        let status = AttachmentRecord::status(pool.read().await?, attachment_id).await?;
        assert_eq!(status, Some(AttachmentStatus::Ready));
        let loaded_content =
            AttachmentRecord::load_content(pool.read().await?, attachment_id).await?;
        assert_eq!(loaded_content, AttachmentContent::Ready(content));

        Ok(())
    }
}
