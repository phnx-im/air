// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Process incoming attachments.

use aircommon::identifiers::RemoteAttachmentId;
use anyhow::bail;
use mimi_content::content_container::NestedPart;

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
            let VisitedPart::Attachment { part, .. } = part else {
                return Ok(());
            };
            match part {
                NestedPart::SinglePart {
                    content_type,
                    content,
                    ..
                } => {
                    let record = AttachmentRecord {
                        attachment_id: AttachmentId::random(),
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
                            bail!("invalid attachment url {url}: {error}");
                        }
                    };
                    let record = AttachmentRecord {
                        attachment_id: AttachmentId::random(),
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
                    bail!("unexpected part visited as attachment");
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
