// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::identifiers::AttachmentId;
use chrono::{DateTime, Utc};
use mimi_content::content_container::{EncryptionAlgorithm, HashAlgorithm};
use sqlx::{
    Database, Decode, Encode, Sqlite, SqliteExecutor, Type, encode::IsNull, error::BoxDynError,
    query, query_as, query_scalar,
};

use crate::{ChatId, MessageId, store::StoreNotifier};

/// A record of an attachment.
///
/// Content is intentially not included in this struct.
#[derive(Debug)]
#[cfg_attr(test, derive(PartialEq, Eq))]
pub(crate) struct AttachmentRecord {
    pub(crate) attachment_id: AttachmentId,
    pub(super) chat_id: ChatId,
    pub(super) message_id: MessageId,
    pub(super) content_type: String,
    pub(super) status: AttachmentStatus,
    pub(super) created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy)]
#[cfg_attr(test, derive(PartialEq, Eq))]
#[repr(u8)]
pub enum AttachmentStatus {
    /// Unknown status
    Unknown = 0,
    /// The download has not started yet.
    Pending = 1,
    /// The download is in progress.
    Downloading = 2,
    /// The download has completed successfully.
    Ready = 3,
    /// The download has failed.
    Failed = 4,
}

impl AttachmentStatus {
    fn from_u32(value: u32) -> Self {
        match value {
            1 => Self::Pending,
            2 => Self::Downloading,
            3 => Self::Ready,
            4 => Self::Failed,
            _ => Self::Unknown,
        }
    }
}

#[derive(Debug)]
#[cfg_attr(test, derive(PartialEq, Eq))]
pub enum AttachmentContent {
    /// There no such attachment
    None,
    /// Fully downloaded
    Ready(Vec<u8>),
    /// Not yet started to download
    Pending,
    /// Currently downloading
    Downloading,
    /// Failed to download
    Failed,
    /// Unknown status
    Unknown,
}

impl AttachmentContent {
    pub fn into_bytes(self) -> Option<Vec<u8>> {
        match self {
            AttachmentContent::Ready(content) => Some(content),
            _ => None,
        }
    }

    fn from_parts(content: Option<Vec<u8>>, status: AttachmentStatus) -> Self {
        match (content, status) {
            (Some(content), AttachmentStatus::Ready) => AttachmentContent::Ready(content),
            (None, AttachmentStatus::Ready) => AttachmentContent::Unknown,
            (_, AttachmentStatus::Pending) => AttachmentContent::Pending,
            (_, AttachmentStatus::Downloading) => AttachmentContent::Downloading,
            (_, AttachmentStatus::Failed) => AttachmentContent::Failed,
            (_, AttachmentStatus::Unknown) => AttachmentContent::Unknown,
        }
    }
}

impl Type<Sqlite> for AttachmentStatus {
    fn type_info() -> <Sqlite as Database>::TypeInfo {
        // Note: don't use u8, sqlx gets confused with:
        // ```
        // mismatched types; Rust type (as SQL type `INTEGER`) is not compatible with SQL type `INTEGER`
        // ```
        <u32 as Type<Sqlite>>::type_info()
    }
}

impl<'q> Encode<'q, Sqlite> for AttachmentStatus {
    fn encode_by_ref(
        &self,
        buf: &mut <Sqlite as Database>::ArgumentBuffer<'q>,
    ) -> Result<IsNull, BoxDynError> {
        Encode::<Sqlite>::encode(*self as u32, buf)
    }
}

impl<'r> Decode<'r, Sqlite> for AttachmentStatus {
    fn decode(value: <Sqlite as Database>::ValueRef<'r>) -> Result<Self, BoxDynError> {
        let idx: u32 = Decode::<Sqlite>::decode(value)?;
        Ok(Self::from_u32(idx))
    }
}

impl AttachmentRecord {
    pub(crate) async fn store(
        &self,
        executor: impl SqliteExecutor<'_>,
        notifier: &mut StoreNotifier,
        content: Option<&[u8]>,
    ) -> sqlx::Result<()> {
        query!(
            "INSERT INTO attachment (
                attachment_id,
                chat_id,
                message_id,
                content_type,
                content,
                status,
                created_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?)",
            self.attachment_id,
            self.chat_id,
            self.message_id,
            self.content_type,
            content,
            self.status,
            self.created_at,
        )
        .execute(executor)
        .await?;
        notifier.add(self.attachment_id);
        Ok(())
    }

    pub(crate) async fn load_all_pending(
        executor: impl SqliteExecutor<'_>,
    ) -> sqlx::Result<Vec<AttachmentId>> {
        query_scalar!(
            r#"SELECT
                attachment_id AS "attachment_id: AttachmentId"
            FROM attachment
            WHERE status = ?
            ORDER BY created_at ASC"#,
            AttachmentStatus::Pending
        )
        .fetch_all(executor)
        .await
    }

    pub(crate) async fn load(
        executor: impl SqliteExecutor<'_>,
        attachment_id: AttachmentId,
    ) -> sqlx::Result<Option<Self>> {
        query_as!(
            AttachmentRecord,
            r#"
                SELECT
                    attachment_id AS "attachment_id: _",
                    chat_id AS "chat_id: _",
                    message_id AS "message_id: _",
                    content_type AS "content_type: _",
                    status AS "status: _",
                    created_at AS "created_at: _"
                FROM attachment
                WHERE attachment_id = ?"#,
            attachment_id
        )
        .fetch_optional(executor)
        .await
    }

    pub(crate) async fn update_status(
        executor: impl SqliteExecutor<'_>,
        attachment_id: AttachmentId,
        status: AttachmentStatus,
    ) -> sqlx::Result<()> {
        query!(
            "UPDATE attachment SET status = ? WHERE attachment_id = ?",
            status,
            attachment_id,
        )
        .execute(executor)
        .await?;
        Ok(())
    }

    pub(crate) async fn set_content(
        executor: impl SqliteExecutor<'_>,
        notifier: &mut StoreNotifier,
        attachment_id: AttachmentId,
        bytes: &[u8],
    ) -> sqlx::Result<()> {
        query!(
            "UPDATE attachment SET status = ?, content = ? WHERE attachment_id = ?",
            AttachmentStatus::Ready,
            bytes,
            attachment_id,
        )
        .execute(executor)
        .await?;
        notifier.update(attachment_id);
        Ok(())
    }

    pub(crate) async fn load_content(
        executor: impl SqliteExecutor<'_>,
        attachment_id: AttachmentId,
    ) -> sqlx::Result<AttachmentContent> {
        struct SqlParts {
            content: Option<Vec<u8>>,
            status: AttachmentStatus,
        }
        let record = query_as!(
            SqlParts,
            r#"SELECT
                content,
                status AS "status: _"
            FROM attachment WHERE attachment_id = ?"#,
            attachment_id
        )
        .fetch_optional(executor)
        .await?;
        match record {
            Some(record) => Ok(AttachmentContent::from_parts(record.content, record.status)),
            None => Ok(AttachmentContent::None),
        }
    }
}

#[derive(Debug)]
#[cfg_attr(test, derive(PartialEq, Eq))]
pub(crate) struct PendingAttachmentRecord {
    pub(super) attachment_id: AttachmentId,
    pub(super) size: u64,
    pub(super) enc_alg: EncryptionAlgorithm,
    pub(super) enc_key: Vec<u8>,
    pub(super) nonce: Vec<u8>,
    pub(super) aad: Vec<u8>,
    pub(super) hash_alg: HashAlgorithm,
    pub(super) hash: Vec<u8>,
}

impl PendingAttachmentRecord {
    pub(crate) async fn store(
        &self,
        executor: impl SqliteExecutor<'_>,
        notifier: &mut StoreNotifier,
    ) -> sqlx::Result<()> {
        let size = self.size as i64;
        let enc_alg: i64 = self.enc_alg.repr().into();
        let hash_alg: i64 = self.hash_alg.repr().into();
        query!(
            "INSERT INTO pending_attachment (
                attachment_id,
                size,
                enc_alg,
                enc_key,
                nonce,
                aad,
                hash_alg,
                hash
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            self.attachment_id,
            size,
            enc_alg,
            self.enc_key,
            self.nonce,
            self.aad,
            hash_alg,
            self.hash,
        )
        .execute(executor)
        .await?;
        notifier.add(self.attachment_id);
        Ok(())
    }

    pub(crate) async fn load_pending(
        executor: impl SqliteExecutor<'_>,
        attachment_id: AttachmentId,
    ) -> sqlx::Result<Option<Self>> {
        struct SqlPendingAttachmentRecord {
            size: u64,
            enc_alg: u16,
            enc_key: Vec<u8>,
            nonce: Vec<u8>,
            aad: Vec<u8>,
            hash_alg: u8,
            hash: Vec<u8>,
        }

        let record = query_as!(
            SqlPendingAttachmentRecord,
            r#"
                SELECT
                    pa.size AS "size: _",
                    pa.enc_alg AS "enc_alg: _",
                    pa.enc_key AS "enc_key: _",
                    pa.nonce AS "nonce: _",
                    pa.aad AS "aad: _",
                    pa.hash_alg AS "hash_alg: _",
                    pa.hash AS "hash: _"
                FROM pending_attachment pa
                INNER JOIN attachment a ON a.attachment_id = pa.attachment_id
                WHERE pa.attachment_id = ? AND a.status = 1
            "#,
            attachment_id
        )
        .fetch_optional(executor)
        .await?;
        Ok(record.map(
            |SqlPendingAttachmentRecord {
                 size,
                 enc_alg,
                 enc_key,
                 nonce,
                 aad,
                 hash_alg,
                 hash,
             }| {
                PendingAttachmentRecord {
                    attachment_id,
                    size,
                    enc_alg: EncryptionAlgorithm::from_repr(enc_alg),
                    enc_key,
                    nonce,
                    aad,
                    hash_alg: HashAlgorithm::from_repr(hash_alg),
                    hash,
                }
            },
        ))
    }

    pub(crate) async fn delete(
        executor: impl SqliteExecutor<'_>,
        attachment_id: AttachmentId,
    ) -> sqlx::Result<()> {
        query!(
            "DELETE FROM pending_attachment WHERE attachment_id = ?",
            attachment_id
        )
        .execute(executor)
        .await?;
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use chrono::SubsecRound;
    use sqlx::Pool;
    use uuid::Uuid;

    use crate::chats::{
        messages::persistence::tests::test_chat_message, persistence::tests::test_chat,
    };

    use super::*;

    fn test_attachment_record(chat_id: ChatId, message_id: MessageId) -> AttachmentRecord {
        AttachmentRecord {
            attachment_id: AttachmentId::new(Uuid::new_v4()),
            chat_id,
            message_id,
            content_type: "image/png".to_string(),
            status: AttachmentStatus::Pending,
            created_at: Utc::now().round_subsecs(6),
        }
    }

    #[sqlx::test]
    async fn attachment_record_store_and_load(pool: Pool<Sqlite>) -> anyhow::Result<()> {
        let mut notifier = StoreNotifier::noop();
        let chat = test_chat();
        chat.store(pool.acquire().await?.as_mut(), &mut notifier)
            .await?;
        let message = test_chat_message(chat.id());
        message.store(&pool, &mut notifier).await?;
        let record = test_attachment_record(chat.id(), message.id());

        // Store the record
        record.store(&pool, &mut notifier, None).await?;

        // Load the record
        let loaded_record = AttachmentRecord::load(&pool, record.attachment_id).await?;
        assert_eq!(loaded_record.as_ref(), Some(&record));

        Ok(())
    }

    #[sqlx::test]
    async fn attachment_content_lifecycle(pool: Pool<Sqlite>) -> anyhow::Result<()> {
        let mut notifier = StoreNotifier::noop();
        let chat = test_chat();
        chat.store(pool.acquire().await?.as_mut(), &mut notifier)
            .await?;
        let message = test_chat_message(chat.id());
        message.store(&pool, &mut notifier).await?;
        let record = test_attachment_record(chat.id(), message.id());

        // 1. Store the record with no content, status should be Pending.
        record.store(&pool, &mut notifier, None).await?;
        let loaded_content = AttachmentRecord::load_content(&pool, record.attachment_id).await?;
        assert_eq!(loaded_content, AttachmentContent::Pending);

        // 2. Update status to Downloading
        AttachmentRecord::update_status(&pool, record.attachment_id, AttachmentStatus::Downloading)
            .await?;
        let loaded_content = AttachmentRecord::load_content(&pool, record.attachment_id).await?;
        assert_eq!(loaded_content, AttachmentContent::Downloading);

        // 3. Set the content, which should move the status to Ready
        let content = b"some_image_content".to_vec();
        AttachmentRecord::set_content(&pool, &mut notifier, record.attachment_id, &content).await?;

        // Verify content and status
        let loaded_content = AttachmentRecord::load_content(&pool, record.attachment_id).await?;
        assert_eq!(loaded_content, AttachmentContent::Ready(content.clone()));

        let loaded_record = AttachmentRecord::load(&pool, record.attachment_id)
            .await?
            .unwrap();
        assert_eq!(loaded_record.status, AttachmentStatus::Ready);

        // 4. Update status to Failed
        AttachmentRecord::update_status(&pool, record.attachment_id, AttachmentStatus::Failed)
            .await?;
        let loaded_content = AttachmentRecord::load_content(&pool, record.attachment_id).await?;
        assert_eq!(loaded_content, AttachmentContent::Failed);

        // 5. Check loading content for a non-existent attachment
        let non_existent_id = AttachmentId::new(Uuid::new_v4());
        let loaded_content = AttachmentRecord::load_content(&pool, non_existent_id).await?;
        assert_eq!(loaded_content, AttachmentContent::None);

        Ok(())
    }

    #[sqlx::test]
    async fn load_all_pending_attachments(pool: Pool<Sqlite>) -> anyhow::Result<()> {
        let mut notifier = StoreNotifier::noop();
        let chat = test_chat();
        chat.store(pool.acquire().await?.as_mut(), &mut notifier)
            .await?;
        let message = test_chat_message(chat.id());
        message.store(&pool, &mut notifier).await?;

        // Create and store a few attachments with different statuses
        let created_at = Utc::now().round_subsecs(6);
        let mut pending_record_1 = test_attachment_record(chat.id(), message.id());
        pending_record_1.created_at = created_at;
        pending_record_1.store(&pool, &mut notifier, None).await?;

        let downloading_record = AttachmentRecord {
            status: AttachmentStatus::Downloading,
            ..test_attachment_record(chat.id(), message.id())
        };
        downloading_record.store(&pool, &mut notifier, None).await?;

        let mut pending_record_2 = test_attachment_record(chat.id(), message.id());
        pending_record_2.created_at = created_at
            .checked_add_signed(chrono::Duration::milliseconds(10))
            .unwrap();
        pending_record_2.store(&pool, &mut notifier, None).await?;

        // Load all pending attachments
        let pending_ids = AttachmentRecord::load_all_pending(&pool).await?;

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
    async fn pending_attachment_record_cycle(pool: Pool<Sqlite>) -> anyhow::Result<()> {
        tracing_subscriber::fmt::try_init().ok();

        let mut notifier = StoreNotifier::noop();
        let chat = test_chat();
        chat.store(pool.acquire().await?.as_mut(), &mut notifier)
            .await?;
        let message = test_chat_message(chat.id());
        message.store(&pool, &mut notifier).await?;

        // 1. Create the base AttachmentRecord in Pending state
        let attachment_record = test_attachment_record(chat.id(), message.id());
        attachment_record.store(&pool, &mut notifier, None).await?;

        // 2. Create and store the PendingAttachmentRecord
        let pending_record = PendingAttachmentRecord {
            attachment_id: attachment_record.attachment_id,
            size: 123,
            enc_alg: EncryptionAlgorithm::Aes256Gcm,
            enc_key: b"key".to_vec(),
            nonce: b"nonce".to_vec(),
            aad: b"aad".to_vec(),
            hash_alg: HashAlgorithm::Sha3_512,
            hash: b"hash".to_vec(),
        };
        pending_record.store(&pool, &mut notifier).await?;

        // 3. Load the pending record and verify it's correct
        let loaded_pending =
            PendingAttachmentRecord::load_pending(&pool, attachment_record.attachment_id).await?;
        assert_eq!(loaded_pending, Some(pending_record));

        // 4. Delete the pending record
        PendingAttachmentRecord::delete(&pool, attachment_record.attachment_id).await?;

        // 5. Try to load it again and assert it's gone
        let loaded_pending_after_delete =
            PendingAttachmentRecord::load_pending(&pool, attachment_record.attachment_id).await?;
        assert_eq!(loaded_pending_after_delete, None);

        Ok(())
    }

    #[sqlx::test]
    async fn load_pending_for_non_pending_attachment_fails(
        pool: Pool<Sqlite>,
    ) -> anyhow::Result<()> {
        let mut notifier = StoreNotifier::noop();
        let chat = test_chat();
        chat.store(pool.acquire().await?.as_mut(), &mut notifier)
            .await?;
        let message = test_chat_message(chat.id());
        message.store(&pool, &mut notifier).await?;

        // 1. Create the base AttachmentRecord and a PendingAttachmentRecord
        let attachment_record = test_attachment_record(chat.id(), message.id());
        attachment_record.store(&pool, &mut notifier, None).await?;

        let pending_record = PendingAttachmentRecord {
            attachment_id: attachment_record.attachment_id,
            size: 123,
            enc_alg: EncryptionAlgorithm::Aes256Gcm,
            enc_key: b"key".to_vec(),
            nonce: b"nonce".to_vec(),
            aad: b"aad".to_vec(),
            hash_alg: HashAlgorithm::Sha3_512,
            hash: b"hash".to_vec(),
        };
        pending_record.store(&pool, &mut notifier).await?;

        // 2. Update the status of the base attachment to something other than Pending
        AttachmentRecord::update_status(
            &pool,
            attachment_record.attachment_id,
            AttachmentStatus::Downloading,
        )
        .await?;

        // 3. Try to load the pending record. It should fail because the join on status=1 fails.
        let loaded_pending =
            PendingAttachmentRecord::load_pending(&pool, attachment_record.attachment_id).await?;
        assert!(loaded_pending.is_none());

        Ok(())
    }

    #[sqlx::test]
    async fn attachment_record_update_status(pool: Pool<Sqlite>) -> anyhow::Result<()> {
        let mut notifier = StoreNotifier::noop();

        let chat = test_chat();
        chat.store(pool.acquire().await?.as_mut(), &mut notifier)
            .await?;
        let message = test_chat_message(chat.id());
        message.store(&pool, &mut notifier).await?;

        let attachment_id = AttachmentId::new(Uuid::new_v4());
        let created_at = Utc::now().round_subsecs(6);

        let record = AttachmentRecord {
            attachment_id,
            chat_id: chat.id(),
            message_id: message.id(),
            content_type: "image/png".to_string(),
            status: AttachmentStatus::Pending,
            created_at,
        };

        let content = b"some_image_content".to_vec();

        record.store(&pool, &mut notifier, Some(&content)).await?;
        let loaded_record = AttachmentRecord::load(&pool, attachment_id).await?;
        assert_eq!(loaded_record.unwrap(), record);
        let loaded_content = AttachmentRecord::load_content(&pool, attachment_id).await?;
        assert_eq!(loaded_content, AttachmentContent::Pending);

        AttachmentRecord::update_status(&pool, attachment_id, AttachmentStatus::Ready).await?;
        let loaded_record = AttachmentRecord::load(&pool, attachment_id).await?;
        assert_eq!(
            loaded_record.unwrap(),
            AttachmentRecord {
                status: AttachmentStatus::Ready,
                ..record
            }
        );
        let loaded_content = AttachmentRecord::load_content(&pool, attachment_id).await?;
        assert_eq!(loaded_content, AttachmentContent::Ready(content));

        Ok(())
    }
}
