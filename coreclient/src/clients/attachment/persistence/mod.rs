// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

mod attachment;
mod reference;
mod remote;
mod transfer;

use chrono::{DateTime, Utc};
use sqlx::{Database, Decode, Encode, Sqlite, Type, encode::IsNull, error::BoxDynError};
use uuid::Uuid;

use crate::{AttachmentId, MessageId, PartIndex, db::access::WriteConnection};

pub(crate) use attachment::{
    AttachmentRecord, ProcessedMetadata, move_attachment_content_to_side_table,
};
pub(crate) use reference::{AttachmentRole, MessageAttachmentRecord, UnqueuedAttachmentMessage};
pub(crate) use remote::RemoteAttachmentRecord;
pub(crate) use transfer::AttachmentTransferRecord;
use transfer::{TransferDirection, TransferFailure};

/// A new attachment referenced by one message part, with its initial status.
///
/// Stored as an attachment, its reference and its transfer.
#[derive(Debug)]
pub(crate) struct NewAttachment {
    pub(crate) attachment_id: AttachmentId,
    pub(crate) message_id: MessageId,
    pub(crate) part_index: PartIndex,
    pub(crate) role: AttachmentRole,
    pub(crate) filename: Option<String>,
    pub(crate) content_type: String,
    pub(crate) size: Option<u64>,
    pub(crate) content_hash: Option<Vec<u8>>,
    pub(crate) dimensions: Option<(u32, u32)>,
    pub(crate) blurhash: Option<String>,
    pub(crate) status: AttachmentStatus,
    pub(crate) is_animated: Option<bool>,
    pub(crate) created_at: DateTime<Utc>,
}

impl NewAttachment {
    pub(crate) fn attachment(&self) -> AttachmentRecord {
        AttachmentRecord {
            attachment_id: self.attachment_id,
            content_type: self.content_type.clone(),
            size: self.size,
            content_hash: self.content_hash.clone(),
            dimensions: self.dimensions,
            blurhash: self.blurhash.clone(),
            is_animated: self.is_animated,
            created_at: self.created_at,
        }
    }

    pub(crate) fn reference(&self) -> MessageAttachmentRecord {
        MessageAttachmentRecord {
            message_id: self.message_id,
            part_index: self.part_index,
            attachment_id: self.attachment_id,
            role: self.role,
            filename: self.filename.clone(),
        }
    }

    /// Transfer of the initial status, started at creation if running.
    pub(crate) fn transfer(&self) -> Option<AttachmentTransferRecord> {
        AttachmentTransferRecord::from_status(self.attachment_id, self.status, self.created_at)
    }

    /// Stores the attachment, its reference from the message part and its
    /// transfer.
    pub(crate) async fn store(
        &self,
        mut connection: impl WriteConnection,
        content: Option<&[u8]>,
    ) -> sqlx::Result<()> {
        self.attachment().store(&mut connection, content).await?;
        self.reference().store(&mut connection).await?;
        if let Some(transfer) = self.transfer() {
            transfer.store(&mut connection).await?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum AttachmentStatus {
    /// Unknown status
    Unknown = 0,
    /// The download has not started yet.
    Pending = 1,
    /// The download is in progress.
    Downloading = 2,
    /// The upload is in progress.
    Uploading = 5,
    /// The download has completed successfully.
    Ready = 3,
    /// The download has failed.
    DownloadFailed = 4,
    /// The upload has failed.
    UploadFailed = 6,
    /// The attachment doesn't exist on the server (expired?).
    NotFound = 7,
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
    /// Currently uploading
    Uploading(Vec<u8>),
    /// Failed to download
    DownloadFailed,
    /// Upload failed
    UploadFailed(Vec<u8>),
    /// Attachment expired
    NotFound,
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
            (Some(content), AttachmentStatus::Uploading) => AttachmentContent::Uploading(content),
            (None, AttachmentStatus::Uploading) => AttachmentContent::Unknown,
            (Some(content), AttachmentStatus::UploadFailed) => {
                AttachmentContent::UploadFailed(content)
            }
            (None, AttachmentStatus::UploadFailed) => AttachmentContent::Unknown,
            (_, AttachmentStatus::DownloadFailed) => AttachmentContent::DownloadFailed,
            (_, AttachmentStatus::NotFound) => AttachmentContent::NotFound,
            (_, AttachmentStatus::Unknown) => AttachmentContent::Unknown,
        }
    }
}

impl Type<Sqlite> for AttachmentId {
    fn type_info() -> <Sqlite as Database>::TypeInfo {
        <Uuid as Type<Sqlite>>::type_info()
    }
}

impl<'q> Encode<'q, Sqlite> for AttachmentId {
    fn encode_by_ref(
        &self,
        buf: &mut <Sqlite as Database>::ArgumentBuffer,
    ) -> Result<IsNull, BoxDynError> {
        Encode::<Sqlite>::encode(self.uuid, buf)
    }
}

impl<'r> Decode<'r, Sqlite> for AttachmentId {
    fn decode(value: <Sqlite as Database>::ValueRef<'r>) -> Result<Self, BoxDynError> {
        let uuid: Uuid = Decode::<Sqlite>::decode(value)?;
        Ok(Self { uuid })
    }
}

impl Type<Sqlite> for PartIndex {
    fn type_info() -> <Sqlite as Database>::TypeInfo {
        <u32 as Type<Sqlite>>::type_info()
    }
}

impl<'q> Encode<'q, Sqlite> for PartIndex {
    fn encode_by_ref(
        &self,
        buf: &mut <Sqlite as Database>::ArgumentBuffer,
    ) -> Result<IsNull, BoxDynError> {
        Encode::<Sqlite>::encode(self.0, buf)
    }
}

impl<'r> Decode<'r, Sqlite> for PartIndex {
    fn decode(value: <Sqlite as Database>::ValueRef<'r>) -> Result<Self, BoxDynError> {
        Ok(Self(Decode::<Sqlite>::decode(value)?))
    }
}

#[cfg(test)]
pub(crate) mod test {
    use aircommon::identifiers::RemoteAttachmentId;
    use chrono::SubsecRound;
    use mimi_content::content_container::{EncryptionAlgorithm, HashAlgorithm};
    use uuid::Uuid;

    use crate::{
        ChatMessage,
        chats::{messages::persistence::tests::test_chat_message, persistence::tests::test_chat},
        db::access::DbAccess,
    };

    use super::*;

    pub(crate) fn test_attachment_record(message_id: MessageId) -> NewAttachment {
        NewAttachment {
            attachment_id: AttachmentId::random(),
            message_id,
            // Random, so that several records of one message don't collide
            part_index: PartIndex(rand::random()),
            role: AttachmentRole::Attachment,
            filename: Some("image.png".to_owned()),
            content_type: "image/png".to_string(),
            size: Some(123),
            content_hash: Some(b"hash".to_vec()),
            dimensions: Some((4, 3)),
            blurhash: None,
            status: AttachmentStatus::Pending,
            is_animated: Some(false),
            created_at: Utc::now().round_subsecs(6),
        }
    }

    pub(crate) fn test_attachment_record_with(
        message_id: MessageId,
        status: AttachmentStatus,
        created_at: DateTime<Utc>,
    ) -> NewAttachment {
        NewAttachment {
            status,
            created_at,
            ..test_attachment_record(message_id)
        }
    }

    pub(crate) fn test_remote_attachment_record() -> RemoteAttachmentRecord {
        RemoteAttachmentRecord {
            remote_attachment_id: RemoteAttachmentId::new(Uuid::new_v4()),
            size: 123,
            enc_alg: EncryptionAlgorithm::Aes256Gcm,
            enc_key: b"key".to_vec(),
            nonce: b"nonce".to_vec(),
            aad: b"aad".to_vec(),
            hash_alg: HashAlgorithm::Sha3_512,
            hash: b"hash".to_vec(),
        }
    }

    pub(super) async fn stored_message(pool: &DbAccess) -> anyhow::Result<ChatMessage> {
        let chat = test_chat();
        chat.store(pool.write().await?).await?;
        let message = test_chat_message(chat.id());
        message.store(pool.write().await?).await?;
        Ok(message)
    }

    pub(super) async fn count(pool: &DbAccess, table: &str) -> anyhow::Result<i64> {
        Ok(
            sqlx::query_scalar(sqlx::AssertSqlSafe(format!("SELECT COUNT(*) FROM {table}")))
                .fetch_one(pool.read().await?.as_mut())
                .await?,
        )
    }
}
