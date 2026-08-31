// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::{AttachmentId, clients::CoreUser};

/// A locally generated thumbnail of an image attachment
pub enum AttachmentThumbnail {
    Ready {
        bytes: Vec<u8>,
        is_animated: bool,
    },
    /// Original is already within bounds, server it directly.
    OriginalFits,
    /// Generation failed, the caller fails back to the original.
    Failed,
}

impl CoreUser {
    /// Read-only, no generation.
    ///
    /// Return `None` if there is no thumbnail yet generated.
    pub async fn attachment_thumbnail(
        &self,
        _id: AttachmentId,
    ) -> anyhow::Result<Option<AttachmentThumbnail>> {
        todo!()
    }

    /// Decode, downscale, encode, persist.
    ///
    /// Persists the failure marker and returns `Failed` rather than erroring on an undecodable
    /// original.
    pub async fn generate_attachment_thumbnail(
        &self,
        _id: AttachmentId,
        _original: &[u8],
    ) -> anyhow::Result<AttachmentThumbnail> {
        todo!()
    }
}
