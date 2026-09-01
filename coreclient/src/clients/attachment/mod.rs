// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{collections::HashMap, fmt, str::FromStr};

use aircommon::identifiers::{RemoteAttachmentId, RemoteAttachmentIdParseError};
use chrono::{DateTime, Utc};
pub use content::MimiContentExt;
pub(crate) use persistence::AttachmentRecord;
pub use persistence::{AttachmentContent, AttachmentStatus};
use thiserror::Error;
pub use thumbnail::AttachmentThumbnail;
use tls_codec::{TlsDeserializeBytes, TlsSerialize, TlsSize, VLBytes};
pub use upload::{ProvisionAttachmentError, UploadTaskError};
use url::Url;
use uuid::Uuid;

use crate::{ChatId, MessageId, clients::CoreUser, image_is_animated};

mod aead;
mod content;
mod download;
pub(crate) mod persistence;
mod process;
pub(crate) mod progress;
mod thumbnail;
pub(crate) mod upload;

impl CoreUser {
    pub async fn pending_attachments(&self) -> anyhow::Result<Vec<AttachmentId>> {
        Ok(AttachmentRecord::load_all_pending(self.db().read().await?).await?)
    }

    pub async fn load_attachment(
        &self,
        attachment_id: AttachmentId,
    ) -> anyhow::Result<AttachmentContent> {
        Ok(AttachmentRecord::load_content(self.db().read().await?, attachment_id).await?)
    }

    pub async fn attachment_status(
        &self,
        attachment_id: AttachmentId,
    ) -> anyhow::Result<Option<AttachmentStatus>> {
        Ok(AttachmentRecord::status(self.db().read().await?, attachment_id).await?)
    }

    /// Returns whether the attachment is an animated image.
    ///
    /// In case the value is unknown, it is classified and backfilled. This path is only needed for
    /// legacy attachments. Attachments are classified on upload and download.
    pub async fn attachment_is_animated(
        &self,
        attachment_id: AttachmentId,
    ) -> anyhow::Result<Option<bool>> {
        if let Some(value) =
            AttachmentRecord::is_animated(self.db().read().await?, attachment_id).await?
        {
            return Ok(Some(value));
        }

        // Unknown value => classify and backfill
        let is_animated =
            match AttachmentRecord::load_content(self.db().read().await?, attachment_id).await? {
                AttachmentContent::Ready(bytes)
                | AttachmentContent::Uploading(bytes)
                | AttachmentContent::UploadFailed(bytes) => image_is_animated(bytes.as_slice()),
                _ => return Ok(None),
            };
        AttachmentRecord::set_is_animated(self.db().write().await?, attachment_id, is_animated)
            .await?;

        Ok(Some(is_animated))
    }

    /// Returns the attachment infos for the given message ID.
    ///
    /// Infos are ordered by the position in the mimi content.
    pub async fn attachment_infos_for_message(&self, message_id: MessageId) -> Vec<AttachmentInfo> {
        let Ok(read) = self.db().read().await else {
            return Default::default();
        };
        AttachmentRecord::load_infos_by_message_id(read, message_id)
            .await
            .unwrap_or_default()
    }

    /// Returns the attachment infos for the given contiguous range of messages.
    ///
    /// The upper bound is inclusive.
    ///
    /// Infos are ordered by the position in the mimi content for each message.
    pub async fn attachment_infos_in_range_inclusive(
        &self,
        chat_id: ChatId,
        from: (DateTime<Utc>, MessageId),
        to: (DateTime<Utc>, MessageId),
    ) -> HashMap<MessageId, Vec<AttachmentInfo>> {
        let Ok(read) = self.db().read().await else {
            return Default::default();
        };
        AttachmentRecord::load_infos_in_range_inclusive(read, chat_id, from, to)
            .await
            .unwrap_or_default()
    }
}

/// Local attachment ID together with locally derived facts about the
/// attachment.
#[derive(Debug, Clone, Copy)]
pub struct AttachmentInfo {
    pub attachment_id: AttachmentId,
    /// Whether the attachment is an animated image, if already classified
    pub is_animated: Option<bool>,
}

impl From<AttachmentId> for AttachmentInfo {
    fn from(attachment_id: AttachmentId) -> Self {
        Self {
            attachment_id,
            is_animated: None,
        }
    }
}

/// An attachment ID
///
/// Uniquely identifies an attachment on this local client. Must *not* be shared outside of this
/// client.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AttachmentId {
    // Public for the FRB mirror
    pub uuid: Uuid,
}

impl AttachmentId {
    pub(crate) fn from_raw(uuid: Uuid) -> Self {
        Self { uuid }
    }

    pub(crate) fn random() -> Self {
        Self {
            uuid: Uuid::new_v4(),
        }
    }
}

#[derive(TlsSize, TlsSerialize, TlsDeserializeBytes)]
pub(crate) struct AttachmentBytes {
    bytes: VLBytes,
}

impl From<Vec<u8>> for AttachmentBytes {
    fn from(bytes: Vec<u8>) -> Self {
        Self {
            bytes: VLBytes::from(bytes),
        }
    }
}

impl AsRef<[u8]> for AttachmentBytes {
    fn as_ref(&self) -> &[u8] {
        self.bytes.as_slice()
    }
}

#[derive(Debug)]
pub struct AttachmentUrl {
    remote_attachment_id: RemoteAttachmentId,
    dimensions: Option<(u32, u32)>,
}

impl AttachmentUrl {
    pub fn new(remote_attachment_id: RemoteAttachmentId, dimensions: Option<(u32, u32)>) -> Self {
        Self {
            remote_attachment_id,
            dimensions,
        }
    }

    pub fn from_url(url: &Url) -> Result<Self, AttachmentUrlParseError> {
        let remote_attachment_id = RemoteAttachmentId::from_url(url)?;

        let width = url
            .query_pairs()
            .find_map(|(key, value)| (key == "width").then(|| value.parse::<u32>().ok())?);
        let dimensions = width.and_then(|width| {
            let height = url
                .query_pairs()
                .find_map(|(key, value)| (key == "height").then(|| value.parse::<u32>().ok())?)?;
            Some((width, height))
        });

        Ok(Self {
            remote_attachment_id,
            dimensions,
        })
    }

    pub fn remote_attachment_id(&self) -> RemoteAttachmentId {
        self.remote_attachment_id
    }

    pub fn dimensions(&self) -> Option<(u32, u32)> {
        self.dimensions
    }
}

impl FromStr for AttachmentUrl {
    type Err = AttachmentUrlParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let url = Url::parse(s)?;
        Self::from_url(&url)
    }
}

#[derive(Debug, Error)]
pub enum AttachmentUrlParseError {
    #[error(transparent)]
    Url(#[from] url::ParseError),
    #[error(transparent)]
    RemoteAttachmentId(#[from] RemoteAttachmentIdParseError),
}

impl fmt::Display for AttachmentUrl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "air:///attachment/{}", self.remote_attachment_id.uuid())?;
        if let Some((width, height)) = self.dimensions {
            write!(f, "?width={width}&height={height}")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;

    use uuid::uuid;

    #[test]
    fn attachment_url() {
        let id = uuid!("b6a42a7a-62fa-4c10-acfb-6124d80aae09");
        let url = "air:///attachment/b6a42a7a-62fa-4c10-acfb-6124d80aae09"
            .parse()
            .unwrap();
        let remote_attachment_id = RemoteAttachmentId::from_url(&url).unwrap();
        assert_eq!(remote_attachment_id.uuid(), id);

        let attachment_url = AttachmentUrl::new(remote_attachment_id, None);
        assert_eq!(attachment_url.to_string(), url.to_string());
    }

    #[test]
    fn attachment_url_with_dimensions() {
        let id = uuid!("b6a42a7a-62fa-4c10-acfb-6124d80aae09");
        let url = "air:///attachment/b6a42a7a-62fa-4c10-acfb-6124d80aae09?width=1920&height=1080"
            .parse()
            .unwrap();
        let remote_attachment_id = RemoteAttachmentId::from_url(&url).unwrap();
        assert_eq!(remote_attachment_id.uuid(), id);

        let attachment_url = AttachmentUrl::new(remote_attachment_id, Some((1920, 1080)));
        assert_eq!(attachment_url.to_string(), url.to_string());
    }
}
