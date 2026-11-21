// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{collections::HashMap, fmt, str::FromStr};

use aircommon::identifiers::{RemoteAttachmentId, RemoteAttachmentIdParseError};
use chrono::{DateTime, Utc};
pub use persistence::{AttachmentContent, AttachmentStatus};
pub(crate) use persistence::{
    AttachmentRecord, AttachmentTransferRecord, MessageAttachmentRecord, NewAttachment,
};
use thiserror::Error;
pub use thumbnail::AttachmentThumbnail;
use tls_codec::{TlsDeserializeBytes, TlsSerialize, TlsSize, VLBytes};
use tracing::info;
pub use upload::{ProvisionAttachmentError, UploadTaskError};
use url::Url;
use uuid::Uuid;

use crate::{ChatId, MessageId, PartIndex, clients::CoreUser, image_is_animated};

mod aead;
mod download;
pub(crate) mod migration;
pub(crate) mod persistence;
pub(crate) mod process;
pub(crate) mod progress;
mod thumbnail;
pub(crate) mod upload;

impl CoreUser {
    pub async fn pending_attachments(&self) -> anyhow::Result<Vec<AttachmentId>> {
        Ok(AttachmentTransferRecord::load_queued_downloads(self.db().read().await?).await?)
    }

    /// Moves downloads that a killed process left in `Downloading` back to
    /// `Pending`.
    pub async fn reset_interrupted_attachment_downloads(&self) -> anyhow::Result<()> {
        let attachment_ids =
            AttachmentTransferRecord::reset_downloading(self.db().write().await?).await?;
        for attachment_id in attachment_ids {
            info!(
                ?attachment_id,
                "Resetting an interrupted attachment download"
            );
        }
        Ok(())
    }

    #[cfg(feature = "test_utils")]
    pub async fn set_attachment_status(
        &self,
        attachment_id: AttachmentId,
        status: AttachmentStatus,
    ) -> anyhow::Result<()> {
        AttachmentRecord::update_status(self.db().write().await?, attachment_id, status).await?;
        Ok(())
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
                AttachmentContent::None
                | AttachmentContent::Pending
                | AttachmentContent::Downloading
                | AttachmentContent::DownloadFailed
                | AttachmentContent::NotFound
                | AttachmentContent::Unknown => return Ok(None),
            };
        AttachmentRecord::set_is_animated(self.db().write().await?, attachment_id, is_animated)
            .await?;

        Ok(Some(is_animated))
    }

    /// Returns the attachment infos for the given message ID.
    ///
    /// Infos are ordered by the position in the mimi content.
    pub async fn attachment_infos_for_message(&self, message_id: MessageId) -> MessageAttachments {
        let empty = || MessageAttachments::new(message_id, Vec::new());
        let Ok(read) = self.db().read().await else {
            return empty();
        };
        MessageAttachmentRecord::load_infos_by_message_id(read, message_id)
            .await
            .unwrap_or_else(|_| empty())
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
    ) -> HashMap<MessageId, MessageAttachments> {
        let Ok(read) = self.db().read().await else {
            return Default::default();
        };
        MessageAttachmentRecord::load_infos_in_range_inclusive(read, chat_id, from, to)
            .await
            .unwrap_or_default()
    }
}

/// Local attachment ID together with locally derived facts about the
/// attachment.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AttachmentInfo {
    pub attachment_id: AttachmentId,
    /// Index of the message part referencing the attachment
    pub part_index: PartIndex,
    pub remote_attachment_id: Option<RemoteAttachmentId>,
    /// Whether the attachment is an animated image, if already classified
    pub is_animated: Option<bool>,
}

/// An attachment ID
///
/// Uniquely identifies an attachment on this local client. Must *not* be shared outside of this
/// client.
#[derive(Debug, derive_more::Display, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[display("{uuid}")]
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

/// Attachment records stored for one message.
#[derive(Debug, Clone, PartialEq)]
pub struct MessageAttachments {
    message_id: MessageId,
    infos: Vec<AttachmentInfo>,
}

impl MessageAttachments {
    /// Bundles the attachment infos with the message they belong to.
    pub fn new(message_id: MessageId, infos: Vec<AttachmentInfo>) -> Self {
        Self { message_id, infos }
    }

    pub fn message_id(&self) -> MessageId {
        self.message_id
    }

    /// Finds the info of the attachment stored for the given part.
    ///
    /// Matches the part index first, then falls back to the remote attachment
    /// id of the part, if it has one. A record of another remote attachment is
    /// never matched.
    pub fn find(
        &self,
        index: PartIndex,
        remote_attachment_id: Option<RemoteAttachmentId>,
    ) -> Option<&AttachmentInfo> {
        if let Some(info) = self.infos.iter().find(|info| info.part_index == index) {
            // An edit can put another attachment at the index of the record
            match (info.remote_attachment_id, remote_attachment_id) {
                (Some(stored), Some(part)) if stored != part => {}
                _ => return Some(info),
            }
        }
        let remote_attachment_id = remote_attachment_id?;
        self.infos
            .iter()
            .find(|info| info.remote_attachment_id == Some(remote_attachment_id))
    }

    pub fn iter(&self) -> impl Iterator<Item = &AttachmentInfo> {
        self.infos.iter()
    }

    pub fn len(&self) -> usize {
        self.infos.len()
    }

    pub fn is_empty(&self) -> bool {
        self.infos.is_empty()
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
    fn find_skips_record_of_another_remote_attachment() {
        let message_id = MessageId::new(uuid!("b6a42a7a-62fa-4c10-acfb-6124d80aae09"));
        let stored = RemoteAttachmentId::new(uuid!("b6a42a7a-62fa-4c10-acfb-6124d80aae0a"));
        let other = RemoteAttachmentId::new(uuid!("b6a42a7a-62fa-4c10-acfb-6124d80aae0b"));
        let attachment_id = AttachmentId::random();
        let info = |remote_attachment_id| AttachmentInfo {
            attachment_id,
            part_index: PartIndex(3),
            remote_attachment_id,
            is_animated: None,
        };

        let attachments = MessageAttachments::new(message_id, vec![info(Some(stored))]);
        assert!(attachments.find(PartIndex(3), Some(stored)).is_some());
        assert!(attachments.find(PartIndex(3), None).is_some());
        assert!(attachments.find(PartIndex(3), Some(other)).is_none());

        // Not uploaded yet
        let attachments = MessageAttachments::new(message_id, vec![info(None)]);
        assert!(attachments.find(PartIndex(3), Some(other)).is_some());
    }

    #[test]
    fn find_falls_back_to_the_remote_id() {
        let message_id = MessageId::new(uuid!("b6a42a7a-62fa-4c10-acfb-6124d80aae09"));
        let remote = RemoteAttachmentId::new(uuid!("b6a42a7a-62fa-4c10-acfb-6124d80aae0a"));
        let info = AttachmentInfo {
            attachment_id: AttachmentId::random(),
            part_index: PartIndex(1),
            remote_attachment_id: Some(remote),
            is_animated: None,
        };
        let attachments = MessageAttachments::new(message_id, vec![info]);

        // An edit moved the attachment to another index
        assert_eq!(attachments.find(PartIndex(2), Some(remote)), Some(&info));
        assert!(attachments.find(PartIndex(2), None).is_none());
    }

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
    fn attachment_url_is_canonical() {
        let id = RemoteAttachmentId::new(uuid!("b6a42a7a-62fa-4c10-acfb-6124d80aae09"));
        for dimensions in [None, Some((1920, 1080))] {
            let url = AttachmentUrl::new(id, dimensions).to_string();
            assert!(RemoteAttachmentId::is_canonical(&url), "{url}");
        }
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
