// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{
    fs,
    path::{Path, PathBuf},
    sync::Arc,
};

use aircommon::{DEFAULT_MAX_ATTACHMENT_SIZE, identifiers::RemoteAttachmentId};
use airprotos::common::v1::AttachmentTooLargeDetail;
use mimi_content::{Disposition, NestedPart};
use tokio::task::spawn_blocking;
use tracing::warn;

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

/// Largest file that still fits the attachment limit when it is encrypted.
///
/// Encryption adds a length prefix of up to 4 bytes and a tag of 16 bytes.
const MAX_FILE_SIZE: u64 = DEFAULT_MAX_ATTACHMENT_SIZE - 4 - 16;

impl CoreUser {
    /// Sniffs the header of the file.
    ///
    /// Refuses a file over the attachment limit, and an image whose header
    /// does not decode or whose source is too large. An image is measured
    /// against the limit only after it is processed.
    pub async fn stage_attachment(
        &self,
        file: AttachmentFile,
    ) -> anyhow::Result<Result<StagedAttachment, ProvisionAttachmentError>> {
        spawn_blocking(move || StagedAttachment::from_file(file)).await?
    }
}

/// File of an attachment to send.
///
/// Clones share the file. An owned file is deleted when the last clone is
/// dropped, any other file stays with the caller.
#[derive(Debug, Clone)]
pub struct AttachmentFile(Arc<AttachmentFileInner>);

#[derive(Debug)]
struct AttachmentFileInner {
    path: PathBuf,
    delete_on_drop: bool,
}

impl Drop for AttachmentFileInner {
    fn drop(&mut self) {
        let Self {
            path,
            delete_on_drop,
        } = self;
        if *delete_on_drop && let Err(error) = fs::remove_file(path) {
            warn!(%error, "Failed to delete attachment file");
        }
    }
}

impl AttachmentFile {
    /// Keeps the file of the caller.
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self::with_ownership(path.into(), false)
    }

    /// Takes over the file and deletes it with the last clone.
    pub fn owned(path: impl Into<PathBuf>) -> Self {
        Self::with_ownership(path.into(), true)
    }

    fn with_ownership(path: PathBuf, delete_on_drop: bool) -> Self {
        Self(Arc::new(AttachmentFileInner {
            path,
            delete_on_drop,
        }))
    }

    pub fn path(&self) -> &Path {
        let Self(inner) = self;
        &inner.path
    }
}

/// What the header sniff learned about a file to attach.
#[derive(Debug, Clone)]
pub struct StagedAttachment {
    id: AttachmentId,
    file: AttachmentFile,
    probed: ProbedAttachment,
}

impl StagedAttachment {
    pub(crate) fn from_file(
        file: AttachmentFile,
    ) -> anyhow::Result<Result<Self, ProvisionAttachmentError>> {
        let probed = match ProbedAttachment::from_path(file.path().to_owned())? {
            Ok(probed) => probed,
            Err(error) => return Ok(Err(error)),
        };
        if !probed.is_image() && MAX_FILE_SIZE < probed.size {
            return Ok(Err(ProvisionAttachmentError::TooLarge(
                AttachmentTooLargeDetail {
                    max_size_bytes: MAX_FILE_SIZE,
                    actual_size_bytes: probed.size,
                },
            )));
        }
        Ok(Ok(Self {
            id: AttachmentId::random(),
            file,
            probed,
        }))
    }

    /// Id of the attachment, kept when the message is sent.
    pub fn id(&self) -> AttachmentId {
        self.id
    }

    pub fn path(&self) -> &Path {
        self.file.path()
    }

    pub(crate) fn into_file(self) -> AttachmentFile {
        self.file
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

#[cfg(test)]
mod tests {
    use std::fs::File;

    use tempfile::tempdir;

    use super::*;

    #[test]
    fn file_over_the_limit_is_refused() -> anyhow::Result<()> {
        let directory = tempdir()?;
        let path = directory.path().join("large.bin");
        let file = File::create(&path)?;

        file.set_len(MAX_FILE_SIZE)?;
        assert!(StagedAttachment::from_file(AttachmentFile::new(&path))?.is_ok());

        file.set_len(MAX_FILE_SIZE + 1)?;
        let Err(ProvisionAttachmentError::TooLarge(detail)) =
            StagedAttachment::from_file(AttachmentFile::new(path))?
        else {
            panic!("a file over the limit must be refused");
        };
        assert_eq!(detail.max_size_bytes, MAX_FILE_SIZE);
        assert_eq!(detail.actual_size_bytes, MAX_FILE_SIZE + 1);
        Ok(())
    }

    #[test]
    fn file_is_kept() -> anyhow::Result<()> {
        let directory = tempdir()?;
        let path = directory.path().join("kept.bin");
        File::create(&path)?;

        drop(StagedAttachment::from_file(AttachmentFile::new(&path))?);
        assert!(path.exists());
        Ok(())
    }

    #[test]
    fn owned_file_is_deleted_with_last_clone() -> anyhow::Result<()> {
        let directory = tempdir()?;
        let path = directory.path().join("owned.bin");
        File::create(&path)?;

        let file = AttachmentFile::owned(&path);
        let clone = file.clone();
        drop(file);
        assert!(path.exists());
        drop(clone);
        assert!(!path.exists());
        Ok(())
    }
}
