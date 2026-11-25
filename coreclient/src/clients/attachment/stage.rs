// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::path::{Path, PathBuf};

use aircommon::identifiers::RemoteAttachmentId;
use mimi_content::{Disposition, NestedPart};
use tokio::task::spawn_blocking;

use crate::{
    AttachmentId, AttachmentUrl, ProvisionAttachmentError,
    clients::{
        CoreUser,
        attachment::{
            aead::{AIR_ATTACHMENT_ENCRYPTION_ALG, AIR_ATTACHMENT_HASH_ALG},
            upload::ProbedAttachment,
        },
    },
};

impl CoreUser {
    /// Sniffs the header of the file at `path`.
    ///
    /// Refuses an image whose header does not decode or whose source is too large.
    pub async fn stage_attachment(
        &self,
        path: PathBuf,
    ) -> anyhow::Result<Result<StagedAttachment, ProvisionAttachmentError>> {
        let probe_path = path.clone();
        match spawn_blocking(move || ProbedAttachment::from_path(probe_path)).await?? {
            Ok(probed) => Ok(Ok(StagedAttachment {
                id: AttachmentId::random(),
                path,
                probed,
            })),
            Err(error) => Ok(Err(error)),
        }
    }
}

/// What the header sniff learned about a file to attach.
#[derive(Debug, Clone)]
pub struct StagedAttachment {
    id: AttachmentId,
    path: PathBuf,
    probed: ProbedAttachment,
}

impl StagedAttachment {
    /// Id of the attachment, kept when the message is sent.
    pub fn id(&self) -> AttachmentId {
        self.id
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn filename(&self) -> &str {
        &self.probed.filename
    }

    pub fn content_type(&self) -> &str {
        &self.probed.content_type
    }

    pub fn size(&self) -> u64 {
        self.probed.size
    }

    pub fn is_image(&self) -> bool {
        self.probed.image_dimensions.is_some()
    }

    pub fn image_dimensions(&self) -> Option<(u32, u32)> {
        self.probed.image_dimensions
    }

    /// External part of the attachment before it is processed.
    ///
    /// Size and content type are those of the source file. Key, nonce and hash are empty.
    pub(crate) fn provisional_part(&self) -> NestedPart {
        let url = AttachmentUrl::new(
            RemoteAttachmentId::new(self.id.uuid),
            self.image_dimensions(),
        );
        NestedPart::ExternalPart {
            disposition: Disposition::Attachment,
            language: String::new(),
            content_type: self.content_type().to_owned(),
            url: url.to_string(),
            expires: 0,
            size: self.size(),
            enc_alg: AIR_ATTACHMENT_ENCRYPTION_ALG,
            key: Vec::new(),
            nonce: Vec::new(),
            aad: Vec::new(),
            hash_alg: AIR_ATTACHMENT_HASH_ALG,
            content_hash: Vec::new(),
            description: Default::default(),
            filename: self.filename().to_owned(),
        }
    }
}
