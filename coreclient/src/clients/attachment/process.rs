// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Process incoming attachments.

use aircommon::identifiers::RemoteAttachmentId;
use mimi_content::content_container::NestedPart;
use tracing::error;

use super::persistence::PendingAttachmentRecord;

use crate::{
    AttachmentId, ChatMessage, MimiContentExt, VisitedPart,
    clients::{
        CoreUser,
        attachment::{AttachmentRecord, persistence::AttachmentStatus},
    },
};

pub(crate) enum ExtractedAttachment {
    External(AttachmentRecord, PendingAttachmentRecord),
    Inline(AttachmentRecord, Vec<u8>),
}

impl CoreUser {
    /// Extract attachments from message's mimi content and store them as pending.
    ///
    /// Note: This function cannot store the attachment records and pending attachment records
    /// directly, because first the message needs to be stored due to foreign key constraints.
    /// But this function also modifies the message's mimi content.
    pub(crate) fn extract_attachments(
        message: &mut ChatMessage,
    ) -> anyhow::Result<Vec<ExtractedAttachment>> {
        let chat_id = message.chat_id();
        let message_id = message.id();
        let created_at = message.timestamp();

        let Some(mimi_content) = message.message_mut().mimi_content_mut() else {
            return Ok(Vec::new());
        };

        let mut records: Vec<ExtractedAttachment> = Vec::new();
        mimi_content.visit_parts(|part| {
            let VisitedPart::Attachment { part, index, .. } = part else {
                return Ok(());
            };
            match part {
                NestedPart::SinglePart {
                    content_type,
                    content,
                    ..
                } => {
                    let record = AttachmentRecord {
                        attachment_id: AttachmentId::derive(message_id, index),
                        remote_attachment_id: None,
                        chat_id,
                        message_id,
                        content_type: content_type.clone(),
                        status: AttachmentStatus::Ready,
                        is_animated: None,
                        created_at,
                    };
                    records.push(ExtractedAttachment::Inline(record, content.clone()));
                }
                NestedPart::ExternalPart {
                    content_type,
                    url,
                    size,
                    enc_alg,
                    key,
                    nonce,
                    aad,
                    hash_alg,
                    content_hash,
                    ..
                } => {
                    let remote_attachment_id: RemoteAttachmentId = match url.parse() {
                        Ok(id) => id,
                        Err(error) => {
                            error!(%url, %error, "Invalid attachment url, dropping attachment");
                            return Ok(());
                        }
                    };
                    let record = AttachmentRecord {
                        attachment_id: AttachmentId::derive(message_id, index),
                        remote_attachment_id: Some(remote_attachment_id),
                        chat_id,
                        message_id,
                        content_type: content_type.clone(),
                        status: AttachmentStatus::Pending,
                        is_animated: None,
                        created_at,
                    };
                    let pending_record = PendingAttachmentRecord {
                        remote_attachment_id,
                        size: *size,
                        enc_alg: *enc_alg,
                        enc_key: key.clone(),
                        nonce: nonce.clone(),
                        aad: aad.clone(),
                        hash_alg: *hash_alg,
                        hash: content_hash.clone(),
                    };
                    records.push(ExtractedAttachment::External(record, pending_record));
                }
                _ => {
                    error!("Unexpected part visited as attachment, dropping attachment");
                }
            }
            Ok(())
        })?;

        // Strip key/nonce/hash/aad from the message
        mimi_content.visit_external_parts_mut(|part| {
            if let NestedPart::ExternalPart {
                key,
                nonce,
                content_hash,
                aad,
                ..
            } = part
            {
                key.clear();
                nonce.clear();
                content_hash.clear();
                aad.clear();
            }
            Ok(())
        })?;

        Ok(records)
    }
}

#[cfg(test)]
mod tests {
    use aircommon::{identifiers::UserId, time::TimeStamp};
    use mimi_content::{
        Disposition, MimiContent,
        content_container::{EncryptionAlgorithm, HashAlgorithm, PartSemantics},
    };
    use openmls::group::GroupId;
    use uuid::Uuid;

    use crate::{AttachmentUrl, ChatId, MessageId, PartIndex, chats::messages::ContentMessage};

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
        ChatMessage::new_for_test(
            ChatId::random(),
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
    fn extract_attachments_collects_and_strips_secrets() {
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
        let mut message = message_with(MimiContent {
            nested_part,
            ..Default::default()
        });
        let chat_id = message.chat_id();
        let message_id = message.id();

        let extracted = CoreUser::extract_attachments(&mut message).unwrap();

        let [
            ExtractedAttachment::External(png, png_pending),
            ExtractedAttachment::Inline(vcard, vcard_bytes),
            ExtractedAttachment::External(webp, webp_pending),
        ] = extracted.as_slice()
        else {
            panic!("unexpected extracted attachments: {}", extracted.len());
        };

        for (record, remote_id, content_type) in [
            (png, remote_png, "image/png"),
            (webp, remote_webp, "image/webp"),
        ] {
            assert_eq!(record.status, AttachmentStatus::Pending);
            assert_eq!(record.remote_attachment_id, Some(remote_id));
            assert_eq!(record.content_type, content_type);
            assert_eq!(record.chat_id, chat_id);
            assert_eq!(record.message_id, message_id);
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
        assert_eq!(
            png.attachment_id,
            AttachmentId::derive(message_id, PartIndex(2))
        );
        assert_eq!(
            vcard.attachment_id,
            AttachmentId::derive(message_id, PartIndex(3))
        );
        assert_eq!(
            webp.attachment_id,
            AttachmentId::derive(message_id, PartIndex(4))
        );
        assert_eq!(vcard.remote_attachment_id, None);
        assert_eq!(vcard.content_type, "text/vcard");
        assert_eq!(vcard.chat_id, chat_id);
        assert_eq!(vcard.message_id, message_id);
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
                    assert!(key.is_empty() && nonce.is_empty());
                    assert!(aad.is_empty() && content_hash.is_empty());
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
        let mut message = message_with(MimiContent::simple_markdown_message(
            "hello".to_owned(),
            [0; 16],
        ));
        assert!(
            CoreUser::extract_attachments(&mut message)
                .unwrap()
                .is_empty()
        );
    }
}
