// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::sync::Arc;

use anyhow::Context;
use tracing::{debug, warn};

use crate::{
    AttachmentId,
    clients::CoreUser,
    image_is_animated,
    utils::image::{ThumbnailImage, encode_thumbnail},
};

use persistence::{load_thumbnail, load_thumbnail_metadata, store_thumbnail};

/// A locally generated thumbnail of an image attachment
pub enum AttachmentThumbnail {
    Ready {
        /// WebP encoded thumbnail
        bytes: Vec<u8>,
        /// Whether the original is animated
        is_animated: bool,
    },
    /// Original is already within bounds, serve it directly.
    ///
    /// The original is never animated in this case.
    OriginalFits,
    /// Generation failed, the caller fails back to the original.
    Failed {
        /// Whether the original is animated
        is_animated: bool,
    },
}

pub struct AttachmentThumbnailMetadata {
    /// Whether the original is animated
    pub is_animated: bool,
}

impl CoreUser {
    /// Returns the metadata of the thumbnail without loading the bytes.
    ///
    /// Read-only, no generation. Returns `None` if there is no thumbnail yet generated.
    pub async fn attachment_thumbnail_metadata(
        &self,
        attachment_id: AttachmentId,
    ) -> anyhow::Result<Option<AttachmentThumbnailMetadata>> {
        Ok(load_thumbnail_metadata(self.db().read().await?, attachment_id).await?)
    }

    /// Read-only, no generation.
    ///
    /// Return `None` if there is no thumbnail yet generated.
    pub async fn attachment_thumbnail(
        &self,
        attachment_id: AttachmentId,
    ) -> anyhow::Result<Option<AttachmentThumbnail>> {
        Ok(load_thumbnail(self.db().read().await?, attachment_id).await?)
    }

    /// Decode, downscale, encode, persist.
    ///
    /// Persists the failure marker and returns `Failed` rather than erroring on an undecodable
    /// original.
    pub async fn generate_attachment_thumbnail(
        &self,
        attachment_id: AttachmentId,
        original: Arc<Vec<u8>>,
    ) -> anyhow::Result<AttachmentThumbnail> {
        let outcome = tokio::task::spawn_blocking({
            let original = original.clone();
            move || encode_thumbnail(&original)
        })
        .await
        .context("thumbnail encode task failed")?;
        let thumbnail = match outcome {
            Ok(ThumbnailImage::Encoded { bytes, is_animated }) => {
                debug!(
                    original_size = original.len(),
                    thumbnail_size = bytes.len(),
                    is_animated,
                    "Encoded thumbnail"
                );
                AttachmentThumbnail::Ready { bytes, is_animated }
            }
            Ok(ThumbnailImage::OriginalFits) => {
                debug!(original_size = original.len(), "Original fits as thumbnail");
                AttachmentThumbnail::OriginalFits
            }
            // Recorded so another generate attempt does not re-attempt the decode every time.
            Err(error) => {
                warn!(%error, ?attachment_id, "Failed to encode attachment thumbnail");
                let is_animated = image_is_animated(original.as_slice());
                AttachmentThumbnail::Failed { is_animated }
            }
        };
        store_thumbnail(self.db().write().await?, attachment_id, &thumbnail).await?;
        Ok(thumbnail)
    }
}

mod persistence {
    use chrono::Utc;
    use sqlx::{query, query_as, query_scalar};

    use crate::db::access::{ReadConnection, WriteConnection};

    use super::*;

    #[derive(Debug, Clone, Copy, sqlx::Type)]
    #[repr(i32)]
    enum ThumbnailState {
        Ready = 1,
        OriginalFits = 2,
        Failed = 3,
    }

    pub(super) async fn store_thumbnail(
        mut connection: impl WriteConnection,
        attachment_id: AttachmentId,
        thumbnail: &AttachmentThumbnail,
    ) -> sqlx::Result<()> {
        let (state, content, is_animated) = match thumbnail {
            AttachmentThumbnail::Ready { bytes, is_animated } => {
                (ThumbnailState::Ready, Some(bytes.as_slice()), *is_animated)
            }
            AttachmentThumbnail::OriginalFits => (ThumbnailState::OriginalFits, None, false),
            AttachmentThumbnail::Failed { is_animated } => {
                (ThumbnailState::Failed, None, *is_animated)
            }
        };
        let created_at = Utc::now();

        query!(
            "INSERT INTO attachment_thumbnail (
                attachment_id,
                state,
                content,
                is_animated,
                created_at
            ) VALUES (?, ?, ?, ?, ?)
            ON CONFLICT (attachment_id) DO UPDATE SET
                state = excluded.state,
                content = excluded.content,
                is_animated = excluded.is_animated",
            attachment_id,
            state,
            content,
            is_animated,
            created_at,
        )
        .execute(connection.as_mut())
        .await?;

        Ok(())
    }

    pub(super) async fn load_thumbnail_metadata(
        mut connection: impl ReadConnection,
        attachment_id: AttachmentId,
    ) -> sqlx::Result<Option<AttachmentThumbnailMetadata>> {
        let Some(is_animated) = query_scalar!(
            r#"SELECT
                is_animated
            FROM attachment_thumbnail
            WHERE attachment_id = ?"#,
            attachment_id,
        )
        .fetch_optional(connection.as_mut())
        .await?
        else {
            return Ok(None);
        };

        Ok(Some(AttachmentThumbnailMetadata { is_animated }))
    }

    pub(super) async fn load_thumbnail(
        mut connection: impl ReadConnection,
        attachment_id: AttachmentId,
    ) -> sqlx::Result<Option<AttachmentThumbnail>> {
        struct SqlThumbnail {
            state: ThumbnailState,
            content: Option<Vec<u8>>,
            is_animated: bool,
        }
        let Some(record) = query_as!(
            SqlThumbnail,
            r#"SELECT
                state AS "state: _",
                content,
                is_animated
            FROM attachment_thumbnail
            WHERE attachment_id = ?"#,
            attachment_id,
        )
        .fetch_optional(connection.as_mut())
        .await?
        else {
            return Ok(None);
        };

        Ok(match (record.state, record.content) {
            (ThumbnailState::Ready, Some(bytes)) => Some(AttachmentThumbnail::Ready {
                bytes,
                is_animated: record.is_animated,
            }),
            (ThumbnailState::OriginalFits, _) => Some(AttachmentThumbnail::OriginalFits),
            (ThumbnailState::Failed, _) => Some(AttachmentThumbnail::Failed {
                is_animated: record.is_animated,
            }),
            // Corrupt row => report as absent so the next request regenerates it
            (ThumbnailState::Ready, None) => None,
        })
    }
}
