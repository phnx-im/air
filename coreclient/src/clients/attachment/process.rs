// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Process incoming attachments.

use chrono::{DateTime, Utc};
use mimi_content::content_container::NestedPart;
use tracing::{error, warn};

use super::persistence::{AttachmentRole, MessageAttachmentRecord, RemoteAttachmentRecord};

use crate::{
    AttachmentId, AttachmentUrl, ChatMessage, MessageId, MimiContentExt, PartIndex, VisitedPart,
    clients::{
        CoreUser,
        attachment::{NewAttachment, persistence::AttachmentStatus},
    },
    db::access::WriteConnection,
};

pub(crate) enum ExtractedAttachment {
    External(NewAttachment, RemoteAttachmentRecord),
    Inline(NewAttachment, Vec<u8>),
}

/// An attachment part of a message body.
pub(crate) struct AttachmentPart<'a> {
    pub(crate) part: &'a NestedPart,
    pub(crate) index: PartIndex,
    pub(crate) role: AttachmentRole,
    pub(crate) blurhash: Option<&'a str>,
}

impl<'a> AttachmentPart<'a> {
    /// Returns the attachment part of a visited part, if it is one.
    pub(crate) fn from_visited(part: VisitedPart<'a>) -> Option<Self> {
        let VisitedPart::Attachment {
            part,
            index,
            preview,
            blurhash,
        } = part
        else {
            return None;
        };
        Some(Self {
            part,
            index,
            role: AttachmentRole::new(preview),
            blurhash,
        })
    }

    /// Collects the attachment parts of a message body.
    pub(crate) fn collect(content: &'a impl MimiContentExt) -> anyhow::Result<Vec<Self>> {
        let mut parts = Vec::new();
        content.visit_parts(|part| {
            parts.extend(Self::from_visited(part));
            Ok(())
        })?;
        Ok(parts)
    }

    /// Parsed url of an external part
    pub(crate) fn url(&self) -> Option<AttachmentUrl> {
        match self.part {
            NestedPart::ExternalPart { url, .. } => url.parse().ok(),
            _ => None,
        }
    }

    /// Remote record of an external part with a parsable url.
    ///
    /// The key is empty for a provisional part of an upload.
    pub(crate) fn remote(&self) -> Option<RemoteAttachmentRecord> {
        let NestedPart::ExternalPart {
            size,
            enc_alg,
            key,
            nonce,
            aad,
            hash_alg,
            content_hash,
            ..
        } = self.part
        else {
            return None;
        };
        Some(RemoteAttachmentRecord {
            remote_attachment_id: self.url()?.remote_attachment_id(),
            size: *size,
            enc_alg: *enc_alg,
            enc_key: key.clone(),
            nonce: nonce.clone(),
            aad: aad.clone(),
            hash_alg: *hash_alg,
            hash: content_hash.clone(),
        })
    }

    /// New attachment for this part.
    ///
    /// Returns `None` for a part that is neither a single nor an external part.
    pub(crate) fn record(
        &self,
        attachment_id: AttachmentId,
        message_id: MessageId,
        status: AttachmentStatus,
        created_at: DateTime<Utc>,
    ) -> Option<NewAttachment> {
        let (content_type, filename, size, content_hash) = match self.part {
            NestedPart::SinglePart {
                content_type,
                content,
                ..
            } => (content_type, None, u64::try_from(content.len()).ok(), None),
            NestedPart::ExternalPart {
                content_type,
                filename,
                size,
                content_hash,
                ..
            } => (
                content_type,
                Some(filename).filter(|filename| !filename.is_empty()),
                Some(*size),
                Some(content_hash).filter(|hash| !hash.is_empty()),
            ),
            NestedPart::NullPart { .. } | NestedPart::MultiPart { .. } => return None,
        };
        Some(NewAttachment {
            attachment_id,
            message_id,
            part_index: self.index,
            role: self.role,
            filename: filename.cloned(),
            content_type: content_type.clone(),
            size,
            content_hash: content_hash.cloned(),
            dimensions: self.url().and_then(|url| url.dimensions()),
            blurhash: self.blurhash.map(str::to_owned),
            status,
            is_animated: None,
            created_at,
        })
    }
}

impl CoreUser {
    /// Extracts the attachment records of the message's mimi content.
    ///
    /// The records are not stored here, because the message has to be stored
    /// first due to foreign key constraints. The content is left as is.
    pub(crate) fn extract_attachments(
        message: &ChatMessage,
    ) -> anyhow::Result<Vec<ExtractedAttachment>> {
        let message_id = message.id();
        let created_at = message.timestamp();

        let Some(mimi_content) = message.message().mimi_content() else {
            return Ok(Vec::new());
        };

        let mut records: Vec<ExtractedAttachment> = Vec::new();
        for part in AttachmentPart::collect(mimi_content)? {
            let attachment_id = AttachmentId::random();
            match part.part {
                NestedPart::SinglePart { content, .. } => {
                    let Some(record) = part.record(
                        attachment_id,
                        message_id,
                        AttachmentStatus::Ready,
                        created_at,
                    ) else {
                        continue;
                    };
                    records.push(ExtractedAttachment::Inline(record, content.clone()));
                }
                NestedPart::ExternalPart { url, .. } => {
                    let Some(remote) = part.remote() else {
                        error!(%url, "Invalid attachment url, dropping attachment");
                        continue;
                    };
                    let Some(record) = part.record(
                        attachment_id,
                        message_id,
                        AttachmentStatus::Pending,
                        created_at,
                    ) else {
                        continue;
                    };
                    records.push(ExtractedAttachment::External(record, remote));
                }
                NestedPart::NullPart { .. } | NestedPart::MultiPart { .. } => {
                    error!("Unexpected part visited as attachment, dropping attachment");
                }
            }
        }

        Ok(records)
    }

    /// Stores the attachments of a stored message.
    ///
    /// A part whose remote attachment is already stored with the same key and
    /// hash references the stored attachment. A part with the remote id of a
    /// stored attachment but another key or hash is skipped.
    pub(crate) async fn store_attachments(
        mut connection: impl WriteConnection,
        message: &ChatMessage,
    ) -> anyhow::Result<()> {
        for attachment in Self::extract_attachments(message)? {
            match attachment {
                ExtractedAttachment::External(record, remote_record) => {
                    if let Some((attachment_id, stored)) =
                        RemoteAttachmentRecord::load_by_remote_id(
                            &mut connection,
                            remote_record.remote_attachment_id,
                        )
                        .await?
                    {
                        if stored.has_same_secrets(&remote_record) {
                            MessageAttachmentRecord {
                                attachment_id,
                                ..record.reference()
                            }
                            .store(&mut connection)
                            .await?;
                        } else {
                            warn!(
                                message_id = ?message.id(),
                                "Skipping attachment with already known remote id"
                            );
                        }
                        continue;
                    }
                    record.store(&mut connection, None).await?;
                    remote_record
                        .store(&mut connection, record.attachment_id)
                        .await?;
                }
                ExtractedAttachment::Inline(record, content) => {
                    record.store(&mut connection, Some(&content)).await?;
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use aircommon::{
        identifiers::{RemoteAttachmentId, UserId},
        time::TimeStamp,
    };
    use mimi_content::{
        Disposition, MimiContent,
        content_container::{EncryptionAlgorithm, HashAlgorithm, PartSemantics},
    };
    use openmls::group::GroupId;
    use sqlx::SqlitePool;
    use uuid::Uuid;

    use crate::{
        ChatId,
        chats::{messages::ContentMessage, persistence::tests::test_chat},
        db::access::DbAccess,
    };

    use super::*;

    fn external(
        disposition: Disposition,
        content_type: &str,
        remote_id: RemoteAttachmentId,
        secret: u8,
    ) -> NestedPart {
        NestedPart::ExternalPart {
            disposition,
            language: String::new(),
            content_type: content_type.to_owned(),
            url: AttachmentUrl::new(remote_id, None).to_string(),
            expires: 0,
            size: 42,
            enc_alg: EncryptionAlgorithm::None,
            key: vec![secret; 32],
            nonce: vec![secret + 1; 12],
            aad: vec![secret + 2; 8],
            hash_alg: HashAlgorithm::Unspecified,
            content_hash: vec![secret + 3; 32],
            description: String::new(),
            filename: String::new(),
        }
    }

    fn single(content_type: &str, content: &[u8]) -> NestedPart {
        NestedPart::SinglePart {
            disposition: Disposition::Render,
            language: String::new(),
            content_type: content_type.to_owned(),
            content: content.to_vec(),
        }
    }

    fn message_with(content: MimiContent) -> ChatMessage {
        message_in(ChatId::random(), content)
    }

    fn message_in(chat_id: ChatId, content: MimiContent) -> ChatMessage {
        ChatMessage::new_for_test(
            chat_id,
            MessageId::random(),
            TimeStamp::now(),
            ContentMessage::new(
                UserId::random("localhost".parse().unwrap()),
                false,
                content,
                &GroupId::from_slice(&[0]),
            ),
        )
    }

    #[test]
    fn extract_attachments_collects_records_and_keeps_content() {
        let remote_png = RemoteAttachmentId::new(Uuid::new_v4());
        let remote_webp = RemoteAttachmentId::new(Uuid::new_v4());
        let nested_part = NestedPart::MultiPart {
            disposition: Disposition::Render,
            language: String::new(),
            part_semantics: PartSemantics::ProcessAll,
            parts: vec![
                single("text/markdown", b"hi"),
                external(Disposition::Attachment, "image/png", remote_png, 1),
                single("text/vcard", b"BEGIN:VCARD"),
                external(Disposition::Preview, "image/webp", remote_webp, 10),
            ],
        };
        let message = message_with(MimiContent {
            nested_part,
            ..Default::default()
        });
        let message_id = message.id();

        let extracted = CoreUser::extract_attachments(&message).unwrap();

        let [
            ExtractedAttachment::External(png, png_pending),
            ExtractedAttachment::Inline(vcard, vcard_bytes),
            ExtractedAttachment::External(webp, webp_pending),
        ] = extracted.as_slice()
        else {
            panic!("unexpected extracted attachments: {}", extracted.len());
        };

        for (record, content_type, index, role) in [
            (png, "image/png", PartIndex(2), AttachmentRole::Attachment),
            (webp, "image/webp", PartIndex(4), AttachmentRole::Preview),
        ] {
            assert_eq!(record.status, AttachmentStatus::Pending);
            assert_eq!(record.content_type, content_type);
            assert_eq!(record.message_id, message_id);
            assert_eq!(record.part_index, index);
            assert_eq!(record.role, role);
            assert_eq!(record.size, Some(42));
        }
        for (pending, remote_id, secret) in [
            (png_pending, remote_png, 1u8),
            (webp_pending, remote_webp, 10u8),
        ] {
            assert_eq!(pending.remote_attachment_id, remote_id);
            assert_eq!(pending.enc_key, vec![secret; 32]);
            assert_eq!(pending.nonce, vec![secret + 1; 12]);
            assert_eq!(pending.aad, vec![secret + 2; 8]);
            assert_eq!(pending.hash, vec![secret + 3; 32]);
        }

        assert_eq!(vcard.status, AttachmentStatus::Ready);
        assert_eq!(vcard.part_index, PartIndex(3));
        assert_eq!(vcard.role, AttachmentRole::Attachment);
        assert_eq!(vcard.content_type, "text/vcard");
        assert_eq!(vcard.message_id, message_id);
        assert_ne!(png.attachment_id, webp.attachment_id);
        assert_eq!(vcard_bytes, b"BEGIN:VCARD");

        let NestedPart::MultiPart { parts, .. } =
            &message.message().mimi_content().unwrap().nested_part
        else {
            panic!("expected multipart");
        };
        let mut externals = 0;
        for part in parts {
            match part {
                NestedPart::ExternalPart {
                    key,
                    nonce,
                    aad,
                    content_hash,
                    ..
                } => {
                    externals += 1;
                    assert!(!key.is_empty() && !nonce.is_empty());
                    assert!(!aad.is_empty() && !content_hash.is_empty());
                }
                NestedPart::SinglePart {
                    content_type,
                    content,
                    ..
                } if content_type == "text/vcard" => {
                    assert_eq!(content, b"BEGIN:VCARD");
                }
                _ => {}
            }
        }
        assert_eq!(externals, 2);
    }

    #[test]
    fn extract_attachments_without_attachments_is_empty() {
        let message = message_with(MimiContent::simple_markdown_message(
            "hello".to_owned(),
            [0; 16],
        ));
        assert!(CoreUser::extract_attachments(&message).unwrap().is_empty());
    }

    #[sqlx::test]
    async fn known_remote_attachment_is_shared_only_with_same_secrets(
        pool: SqlitePool,
    ) -> anyhow::Result<()> {
        let db = DbAccess::for_tests(pool);
        let chat = test_chat();
        chat.store(db.write().await?).await?;
        let remote_id = RemoteAttachmentId::new(Uuid::new_v4());
        let attachment_message = |secret| {
            message_in(
                chat.id(),
                MimiContent {
                    salt: Uuid::new_v4().as_bytes().to_vec(),
                    nested_part: NestedPart::MultiPart {
                        disposition: Disposition::Attachment,
                        language: String::new(),
                        part_semantics: PartSemantics::ProcessAll,
                        parts: vec![external(
                            Disposition::Attachment,
                            "image/png",
                            remote_id,
                            secret,
                        )],
                    },
                    ..Default::default()
                },
            )
        };
        let mut stored_infos = Vec::new();
        for secret in [1, 1, 5] {
            let message = attachment_message(secret);
            message.store(db.write().await?).await?;
            CoreUser::store_attachments(db.write().await?, &message).await?;
            let infos =
                MessageAttachmentRecord::load_infos_by_message_id(db.read().await?, message.id())
                    .await?;
            stored_infos.push(
                infos
                    .iter()
                    .map(|info| info.attachment_id)
                    .collect::<Vec<_>>(),
            );
        }

        let [first, same_secrets, other_secrets] = &stored_infos[..] else {
            panic!("expected three messages");
        };
        assert_eq!(first.len(), 1);
        assert_eq!(same_secrets, first);
        assert!(other_secrets.is_empty());
        let attachments: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM attachment")
            .fetch_one(db.read().await?.as_mut())
            .await?;
        assert_eq!(attachments, 1);

        Ok(())
    }
}
