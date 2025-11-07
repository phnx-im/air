// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::crypto::ear::{Ciphertext, EarDecryptable, EarEncryptable, keys::AttachmentEarKey};
use mimi_content::content_container::{EncryptionAlgorithm, HashAlgorithm};

use super::AttachmentBytes;

pub(super) const AIR_ATTACHMENT_ENCRYPTION_ALG: EncryptionAlgorithm =
    EncryptionAlgorithm::Aes256Gcm12;

pub(super) const AIR_ATTACHMENT_HASH_ALG: HashAlgorithm = HashAlgorithm::Sha256;

#[derive(Debug, Clone)]
pub struct EncryptedAttachmentCtype;

pub type EncryptedAttachment = Ciphertext<EncryptedAttachmentCtype>;

impl EarEncryptable<AttachmentEarKey, EncryptedAttachmentCtype> for AttachmentBytes {}

impl EarDecryptable<AttachmentEarKey, EncryptedAttachmentCtype> for AttachmentBytes {}
