// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::sync::Arc;

use anyhow::Context;
use tracing::{debug, warn};

use crate::{
    AttachmentId,
    clients::CoreUser,
    utils::image::{ThumbnailImage, encode_thumbnail},
};

use persistence::load_thumbnail;
pub(super) use persistence::store_thumbnail;

/// A locally generated thumbnail of an image attachment
pub enum AttachmentThumbnail {
    Ready {
        /// WebP encoded thumbnail
        bytes: Vec<u8>,
    },
    /// Original is already within bounds, serve it directly.
    ///
    /// The original is never animated in this case.
    OriginalFits,
    /// Generation failed, the caller falls back to the original.
    Failed,
}

impl CoreUser {
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
                AttachmentThumbnail::Ready { bytes }
            }
            Ok(ThumbnailImage::OriginalFits) => {
                debug!(original_size = original.len(), "Original fits as thumbnail");
                AttachmentThumbnail::OriginalFits
            }
            // Recorded so another generate attempt does not re-attempt the decode every time.
            Err(error) => {
                warn!(%error, ?attachment_id, "Failed to encode attachment thumbnail");
                AttachmentThumbnail::Failed
            }
        };
        store_thumbnail(self.db().write().await?, attachment_id, &thumbnail).await?;
        Ok(thumbnail)
    }
}

mod persistence {
    use chrono::Utc;
    use sqlx::{query, query_as};

    use crate::db::access::{ReadConnection, WriteConnection};

    use super::*;

    #[derive(Debug, Clone, Copy, sqlx::Type)]
    #[repr(i32)]
    enum ThumbnailState {
        Ready = 1,
        OriginalFits = 2,
        Failed = 3,
    }

    pub(crate) async fn store_thumbnail(
        mut connection: impl WriteConnection,
        attachment_id: AttachmentId,
        thumbnail: &AttachmentThumbnail,
    ) -> sqlx::Result<()> {
        let (state, content) = match thumbnail {
            AttachmentThumbnail::Ready { bytes } => (ThumbnailState::Ready, Some(bytes.as_slice())),
            AttachmentThumbnail::OriginalFits => (ThumbnailState::OriginalFits, None),
            AttachmentThumbnail::Failed => (ThumbnailState::Failed, None),
        };
        let created_at = Utc::now();

        query!(
            "INSERT INTO attachment_thumbnail (
                attachment_id,
                state,
                content,
                created_at
            ) VALUES (?, ?, ?, ?)
            ON CONFLICT (attachment_id) DO UPDATE SET
                state = excluded.state,
                content = excluded.content",
            attachment_id,
            state,
            content,
            created_at,
        )
        .execute(connection.as_mut())
        .await?;

        Ok(())
    }

    pub(super) async fn load_thumbnail(
        mut connection: impl ReadConnection,
        attachment_id: AttachmentId,
    ) -> sqlx::Result<Option<AttachmentThumbnail>> {
        struct SqlThumbnail {
            state: ThumbnailState,
            content: Option<Vec<u8>>,
        }
        let Some(record) = query_as!(
            SqlThumbnail,
            r#"SELECT
                state AS "state: _",
                content
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
            (ThumbnailState::Ready, Some(bytes)) => Some(AttachmentThumbnail::Ready { bytes }),
            (ThumbnailState::OriginalFits, _) => Some(AttachmentThumbnail::OriginalFits),
            (ThumbnailState::Failed, _) => Some(AttachmentThumbnail::Failed),
            // Corrupt row => report as absent so the next request regenerates it
            (ThumbnailState::Ready, None) => None,
        })
    }
}

#[cfg(test)]
mod test {
    use chrono::Utc;
    use sqlx::{Pool, Sqlite, query};

    use crate::{
        MessageId,
        chats::{messages::persistence::tests::test_chat_message, persistence::tests::test_chat},
        clients::attachment::{AttachmentRecord, persistence::test::test_attachment_record},
        db::access::DbAccess,
    };

    use super::{persistence::load_thumbnail, *};

    async fn store_test_attachment(pool: &DbAccess) -> anyhow::Result<(AttachmentId, MessageId)> {
        let chat = test_chat();
        chat.store(pool.write().await?).await?;
        let message = test_chat_message(chat.id());
        message.store(pool.write().await?).await?;
        let record = test_attachment_record(chat.id(), message.id());
        record.store(pool.write().await?, None).await?;
        Ok((record.attachment_id, message.id()))
    }

    #[sqlx::test]
    async fn thumbnail_store_and_load(pool: Pool<Sqlite>) -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(pool);
        let (attachment_id, _) = store_test_attachment(&pool).await?;

        // Not yet generated
        assert!(
            load_thumbnail(pool.read().await?, attachment_id)
                .await?
                .is_none()
        );

        store_thumbnail(
            pool.write().await?,
            attachment_id,
            &AttachmentThumbnail::Failed,
        )
        .await?;
        assert!(matches!(
            load_thumbnail(pool.read().await?, attachment_id).await?,
            Some(AttachmentThumbnail::Failed)
        ));

        // Overwrites the previous state
        let bytes = b"thumbnail".to_vec();
        store_thumbnail(
            pool.write().await?,
            attachment_id,
            &AttachmentThumbnail::Ready {
                bytes: bytes.clone(),
            },
        )
        .await?;
        match load_thumbnail(pool.read().await?, attachment_id).await? {
            Some(AttachmentThumbnail::Ready { bytes: loaded }) => assert_eq!(loaded, bytes),
            _ => panic!("expected a ready thumbnail"),
        }

        // Overwriting with a stateless outcome clears the content
        store_thumbnail(
            pool.write().await?,
            attachment_id,
            &AttachmentThumbnail::OriginalFits,
        )
        .await?;
        assert!(matches!(
            load_thumbnail(pool.read().await?, attachment_id).await?,
            Some(AttachmentThumbnail::OriginalFits)
        ));

        Ok(())
    }

    #[sqlx::test]
    async fn thumbnail_ready_without_content_reads_as_absent(
        pool: Pool<Sqlite>,
    ) -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(pool);
        let (attachment_id, _) = store_test_attachment(&pool).await?;

        let created_at = Utc::now();
        query(
            "INSERT INTO attachment_thumbnail (attachment_id, state, created_at) VALUES (?, 1, ?)",
        )
        .bind(attachment_id)
        .bind(created_at)
        .execute(pool.write().await?.as_mut())
        .await?;

        assert!(
            load_thumbnail(pool.read().await?, attachment_id)
                .await?
                .is_none()
        );

        Ok(())
    }

    #[sqlx::test]
    async fn thumbnail_deleted_with_attachment(pool: Pool<Sqlite>) -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(pool);
        let (attachment_id, message_id) = store_test_attachment(&pool).await?;

        store_thumbnail(
            pool.write().await?,
            attachment_id,
            &AttachmentThumbnail::Ready {
                bytes: b"thumbnail".to_vec(),
            },
        )
        .await?;

        AttachmentRecord::delete_by_message_id(pool.write().await?, message_id).await?;

        assert!(
            load_thumbnail(pool.read().await?, attachment_id)
                .await?
                .is_none()
        );

        Ok(())
    }
}
