// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Process incoming attachments.

use std::mem;

use aircommon::identifiers::AttachmentId;
use mimi_content::content_container::NestedPartContent;
use tracing::error;

use super::{content::MimiContentExt, persistence::PendingAttachmentRecord};

use crate::{
    ChatMessage,
    clients::{
        CoreUser,
        attachment::{AttachmentRecord, persistence::AttachmentStatus},
    },
};

impl CoreUser {
    /// Extract attachments from message's mimi content and store them as pending.
    ///
    /// Note: This function cannot store the attachment records and pending attachment records
    /// directly, because first the message needs to be stored due to foreign key constraints.
    /// But this function also modifies the message's mimi content.
    pub(crate) fn extract_attachments(
        message: &mut ChatMessage,
    ) -> Vec<(AttachmentRecord, PendingAttachmentRecord)> {
        let mut records = Vec::new();

        let chat_id = message.chat_id();
        let message_id = message.id();
        let created_at = message.timestamp();

        let Some(mimi_content) = message.message_mut().mimi_content_mut() else {
            return Vec::new();
        };

        let visit_res = mimi_content.visit_attachments_mut(|part| {
            let NestedPartContent::ExternalPart {
                url,
                content_type,
                size,
                enc_alg,
                key,
                nonce,
                aad,
                hash_alg,
                content_hash,
                ..
            } = part
            else {
                error!("logic error: part is not an ExternalPart while visiting attachments");
                return Ok(());
            };

            let attachment_id: AttachmentId = match url.parse() {
                Ok(id) => id,
                Err(error) => {
                    error!(%url, %error, "invalid attachment url; dropping attachment");
                    let _ = mem::replace(part, NestedPartContent::NullPart);
                    return Ok(());
                }
            };

            // Note: the encryption data and the hash are moved from the mimi content into
            // pending attachment record.
            let record = AttachmentRecord {
                attachment_id,
                chat_id,
                message_id,
                content_type: content_type.clone(),
                status: AttachmentStatus::Pending,
                created_at,
            };
            let pending_record = PendingAttachmentRecord {
                attachment_id,
                size: *size,
                enc_alg: *enc_alg,
                enc_key: mem::take(key).into_vec(),
                nonce: mem::take(nonce).into_vec(),
                aad: mem::take(aad).into_vec(),
                hash_alg: *hash_alg,
                hash: mem::take(content_hash).into_vec(),
            };
            records.push((record, pending_record));

            Ok(())
        });
        if let Err(error) = visit_res {
            error!(%error, "Failed to visit attachment; continue");
        }
        records
    }
}
