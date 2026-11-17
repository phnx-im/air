// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::identifiers::RemoteAttachmentId;
use mimi_content::content_container::{EncryptionAlgorithm, HashAlgorithm};
use sqlx::query;

use crate::{
    AttachmentId, ChatId,
    db::access::{ReadConnection, WriteConnection},
};

use super::*;

/// Where and how the content of an attachment is stored on the server.
#[derive(Debug, Clone)]
#[cfg_attr(test, derive(PartialEq, Eq))]
pub(crate) struct RemoteAttachmentRecord {
    pub(crate) remote_attachment_id: RemoteAttachmentId,
    pub(crate) size: u64,
    pub(crate) enc_alg: EncryptionAlgorithm,
    pub(crate) enc_key: Vec<u8>,
    pub(crate) nonce: Vec<u8>,
    pub(crate) aad: Vec<u8>,
    pub(crate) hash_alg: HashAlgorithm,
    pub(crate) hash: Vec<u8>,
}

impl RemoteAttachmentRecord {
    /// Whether both decrypt to the same content.
    pub(crate) fn has_same_secrets(&self, other: &Self) -> bool {
        self.enc_key == other.enc_key && self.hash == other.hash
    }

    pub(crate) async fn store(
        &self,
        mut connection: impl WriteConnection,
        attachment_id: AttachmentId,
    ) -> sqlx::Result<()> {
        let size = self.size as i64;
        let enc_alg: u16 = self.enc_alg.into();
        let hash_alg: u8 = self.hash_alg.into();
        query!(
            "INSERT INTO attachment_remote (
                attachment_id,
                remote_attachment_id,
                size,
                enc_alg,
                enc_key,
                nonce,
                aad,
                hash_alg,
                hash
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
            attachment_id,
            self.remote_attachment_id,
            size,
            enc_alg,
            self.enc_key,
            self.nonce,
            self.aad,
            hash_alg,
            self.hash,
        )
        .execute(connection.as_mut())
        .await?;
        Ok(())
    }

    /// Loads the remote record of an attachment that is still to be
    /// downloaded, together with the chat of its first reference.
    ///
    /// Returns `None` unless the attachment is pending, downloading or failed
    /// for a reason other than not found.
    pub(crate) async fn load_for_download(
        mut connection: impl ReadConnection,
        attachment_id: AttachmentId,
    ) -> sqlx::Result<Option<(Self, ChatId)>> {
        let record = query!(
            r#"SELECT
                r.remote_attachment_id AS "remote_attachment_id: RemoteAttachmentId",
                r.size,
                r.enc_alg AS "enc_alg: u16",
                r.enc_key,
                r.nonce,
                r.aad,
                r.hash_alg AS "hash_alg: u8",
                r.hash,
                m.chat_id AS "chat_id: ChatId"
            FROM attachment_remote r
            JOIN attachment_transfer t ON t.attachment_id = r.attachment_id
            JOIN message_attachment ma ON ma.attachment_id = r.attachment_id
            JOIN message m ON m.message_id = ma.message_id
            WHERE r.attachment_id = ?1
                AND t.direction = ?2
                AND (t.failure IS NULL OR t.failure != ?3)
            ORDER BY ma.rowid
            LIMIT 1"#,
            attachment_id,
            TransferDirection::Download,
            TransferFailure::NotFound,
        )
        .fetch_optional(connection.as_mut())
        .await?;
        Ok(record.map(|record| {
            let remote = Self {
                remote_attachment_id: record.remote_attachment_id,
                size: u64::try_from(record.size).unwrap_or_default(),
                enc_alg: EncryptionAlgorithm::from(record.enc_alg),
                enc_key: record.enc_key,
                nonce: record.nonce,
                aad: record.aad,
                hash_alg: HashAlgorithm::from(record.hash_alg),
                hash: record.hash,
            };
            (remote, record.chat_id)
        }))
    }

    /// Loads the remote record with the given remote id and the id of the
    /// attachment it belongs to.
    pub(crate) async fn load_by_remote_id(
        mut connection: impl ReadConnection,
        remote_attachment_id: RemoteAttachmentId,
    ) -> sqlx::Result<Option<(AttachmentId, Self)>> {
        let record = query!(
            r#"SELECT
                attachment_id AS "attachment_id: AttachmentId",
                size,
                enc_alg AS "enc_alg: u16",
                enc_key,
                nonce,
                aad,
                hash_alg AS "hash_alg: u8",
                hash
            FROM attachment_remote
            WHERE remote_attachment_id = ?"#,
            remote_attachment_id,
        )
        .fetch_optional(connection.as_mut())
        .await?;
        Ok(record.map(|record| {
            let remote = Self {
                remote_attachment_id,
                size: u64::try_from(record.size).unwrap_or_default(),
                enc_alg: EncryptionAlgorithm::from(record.enc_alg),
                enc_key: record.enc_key,
                nonce: record.nonce,
                aad: record.aad,
                hash_alg: HashAlgorithm::from(record.hash_alg),
                hash: record.hash,
            };
            (record.attachment_id, remote)
        }))
    }
}

#[cfg(test)]
mod tests {
    use sqlx::{Pool, Sqlite};

    use crate::{
        clients::attachment::persistence::{
            AttachmentRecord, AttachmentStatus,
            test::{stored_message, test_attachment_record, test_remote_attachment_record},
        },
        db::access::DbAccess,
    };

    use super::*;

    #[sqlx::test]
    async fn remote_attachment_record_cycle(pool: Pool<Sqlite>) -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(pool);
        tracing_subscriber::fmt::try_init().ok();
        let message = stored_message(&pool).await?;

        let attachment_record = test_attachment_record(message.id());
        attachment_record.store(pool.write().await?, None).await?;
        let remote_record = test_remote_attachment_record();
        remote_record
            .store(pool.write().await?, attachment_record.attachment_id)
            .await?;

        let loaded = RemoteAttachmentRecord::load_for_download(
            pool.read().await?,
            attachment_record.attachment_id,
        )
        .await?;
        assert_eq!(loaded, Some((remote_record.clone(), message.chat_id())));

        let loaded = RemoteAttachmentRecord::load_by_remote_id(
            pool.read().await?,
            remote_record.remote_attachment_id,
        )
        .await?;
        assert_eq!(
            loaded,
            Some((attachment_record.attachment_id, remote_record.clone()))
        );

        // Kept after the download
        AttachmentRecord::set_content(
            pool.write().await?,
            attachment_record.attachment_id,
            b"content",
            false,
        )
        .await?;
        let loaded = RemoteAttachmentRecord::load_for_download(
            pool.read().await?,
            attachment_record.attachment_id,
        )
        .await?;
        assert_eq!(loaded, None);
        let infos =
            MessageAttachmentRecord::load_infos_by_message_id(pool.read().await?, message.id())
                .await?;
        let [info] = infos.iter().collect::<Vec<_>>()[..] else {
            panic!("expected one attachment info");
        };
        assert_eq!(
            info.remote_attachment_id,
            Some(remote_record.remote_attachment_id)
        );

        Ok(())
    }

    #[sqlx::test]
    async fn load_for_download_of_not_found_attachment_is_none(
        pool: Pool<Sqlite>,
    ) -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(pool);
        let message = stored_message(&pool).await?;

        let attachment_record = test_attachment_record(message.id());
        attachment_record.store(pool.write().await?, None).await?;
        test_remote_attachment_record()
            .store(pool.write().await?, attachment_record.attachment_id)
            .await?;

        AttachmentRecord::update_status(
            pool.write().await?,
            attachment_record.attachment_id,
            AttachmentStatus::NotFound,
        )
        .await?;

        let loaded = RemoteAttachmentRecord::load_for_download(
            pool.read().await?,
            attachment_record.attachment_id,
        )
        .await?;
        assert!(loaded.is_none());

        Ok(())
    }
}
