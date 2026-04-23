// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::fmt::Display;

use aircommon::{
    codec::{self, PersistenceCodec},
    crypto::aead::keys::IdentityLinkWrapperKey,
    identifiers::{Fqdn, QualifiedGroupId, UserId, Username},
};
use airprotos::client::group::{ExternalGroupProfile, GroupData};
use chrono::{DateTime, Utc};
use openmls::group::GroupId;
use serde::{Deserialize, Serialize};
use tracing::error;
use uuid::Uuid;

use crate::{contacts::PartialContactType, db_access::WriteConnection, groups::GroupDataBytes};

pub use draft::MessageDraft;
pub(crate) use {pending::PendingConnectionInfo, status::StatusRecord};

mod draft;
pub(crate) mod messages;
pub(crate) mod pending;
pub(crate) mod persistence;
mod sqlx_support;
pub(crate) mod status;

/// Id of a chat
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ChatId {
    pub uuid: Uuid,
}

impl Display for ChatId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.uuid)
    }
}

impl ChatId {
    pub fn random() -> Self {
        Self {
            uuid: Uuid::new_v4(),
        }
    }

    pub fn new(uuid: Uuid) -> Self {
        Self { uuid }
    }

    pub fn uuid(&self) -> Uuid {
        self.uuid
    }
}

impl From<Uuid> for ChatId {
    fn from(uuid: Uuid) -> Self {
        Self { uuid }
    }
}

impl TryFrom<&GroupId> for ChatId {
    type Error = tls_codec::Error;

    fn try_from(value: &GroupId) -> Result<Self, Self::Error> {
        let qgid = QualifiedGroupId::try_from(value.clone())?;
        let chat_id = Self {
            uuid: qgid.group_uuid(),
        };
        Ok(chat_id)
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct Chat {
    pub id: ChatId,
    // Id of the (active) MLS group representing this chat.
    pub group_id: GroupId,
    // The timestamp of the last message that was (marked as) read by the user.
    pub last_read: DateTime<Utc>,
    // The timestamp of the last message (content or system)
    //
    // `None` if the chat does not have any messages.
    pub last_message_at: Option<DateTime<Utc>>,
    pub status: ChatStatus,
    pub chat_type: ChatType,
    pub attributes: ChatAttributes,
}

impl Chat {
    pub(crate) fn new_handle_chat(
        group_id: GroupId,
        attributes: ChatAttributes,
        username: Username,
    ) -> Self {
        let id = ChatId::try_from(&group_id).unwrap();
        Self {
            id,
            group_id,
            last_read: Utc::now(),
            last_message_at: None,
            status: ChatStatus::Active,
            chat_type: ChatType::HandleConnection(username),
            attributes,
        }
    }

    pub(crate) fn new_targeted_message_chat(
        group_id: GroupId,
        attributes: ChatAttributes,
        user_id: UserId,
    ) -> Self {
        let id = ChatId::try_from(&group_id).unwrap();
        Self {
            id,
            group_id,
            last_read: Utc::now(),
            last_message_at: None,
            status: ChatStatus::Active,
            chat_type: ChatType::TargetedMessageConnection(user_id),
            attributes,
        }
    }

    pub(crate) fn new_group_chat(group_id: GroupId, attributes: ChatAttributes) -> Self {
        let id = ChatId::try_from(&group_id).unwrap();
        Self {
            id,
            group_id,
            last_read: Utc::now(),
            last_message_at: None,
            status: ChatStatus::Active,
            chat_type: ChatType::Group,
            attributes,
        }
    }

    pub(crate) fn new_pending_connection_chat(
        group_id: GroupId,
        user_id: UserId,
        attributes: ChatAttributes,
    ) -> Self {
        Self {
            id: ChatId::try_from(&group_id).unwrap(),
            group_id,
            last_read: Utc::now(),
            last_message_at: None,
            status: ChatStatus::Active,
            chat_type: ChatType::PendingConnection(user_id),
            attributes,
        }
    }

    pub fn id(&self) -> ChatId {
        self.id
    }

    pub fn group_id(&self) -> &GroupId {
        &self.group_id
    }

    pub fn chat_type(&self) -> &ChatType {
        &self.chat_type
    }

    pub fn is_unconfirmed(&self) -> bool {
        matches!(
            self.chat_type,
            ChatType::HandleConnection(_) | ChatType::TargetedMessageConnection(_)
        )
    }

    pub fn status(&self) -> &ChatStatus {
        &self.status
    }

    pub fn attributes(&self) -> &ChatAttributes {
        &self.attributes
    }

    pub fn last_read(&self) -> DateTime<Utc> {
        self.last_read
    }

    pub fn last_message_at(&self) -> Option<DateTime<Utc>> {
        self.last_message_at
    }

    pub(crate) fn owner_domain(&self) -> Fqdn {
        let qgid = QualifiedGroupId::try_from(self.group_id.clone()).unwrap();
        qgid.owning_domain().clone()
    }

    pub(crate) async fn set_picture(
        &mut self,
        connection: impl WriteConnection,
        picture: Option<Vec<u8>>,
    ) -> sqlx::Result<()> {
        Self::update_picture(connection, self.id, picture.as_deref()).await?;
        self.attributes.set_picture(picture);
        Ok(())
    }

    pub(crate) async fn set_title(
        &mut self,
        connection: impl WriteConnection,
        title: String,
    ) -> sqlx::Result<()> {
        Self::update_title(connection, self.id, &title).await?;
        self.attributes.set_title(title);
        Ok(())
    }

    pub(crate) async fn set_inactive(
        &mut self,
        connection: impl WriteConnection,
        past_members: Vec<UserId>,
    ) -> sqlx::Result<()> {
        let new_status = ChatStatus::Inactive(InactiveChat { past_members });
        Self::update_status(connection, self.id, &new_status).await?;
        self.status = new_status;
        Ok(())
    }

    /// Confirm a connection chat by setting the chat type to `Connection`.
    pub(crate) async fn confirm(
        &mut self,
        connection: impl WriteConnection,
        user_id: UserId,
    ) -> sqlx::Result<()> {
        if self.is_unconfirmed() {
            let chat_type = ChatType::Connection(user_id);
            self.set_chat_type(connection, &chat_type).await?;
            self.chat_type = chat_type;
        }
        Ok(())
    }
}

#[derive(Eq, PartialEq, Debug, Clone, Hash, Serialize, Deserialize)]
pub enum ChatStatus {
    Inactive(InactiveChat),
    Active,
    Blocked,
}

#[derive(Eq, PartialEq, Debug, Clone, Hash, Serialize, Deserialize)]
pub struct InactiveChat {
    pub past_members: Vec<UserId>,
}

impl InactiveChat {
    pub fn new(past_members: Vec<UserId>) -> Self {
        Self { past_members }
    }

    pub fn past_members(&self) -> &[UserId] {
        &self.past_members
    }

    pub fn past_members_mut(&mut self) -> &mut Vec<UserId> {
        &mut self.past_members
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub enum ChatType {
    /// A connection chat which was established via a handle and is not yet confirmed by the other
    /// party. (outgoing)
    HandleConnection(Username),
    /// A connection chat that is confirmed by the other party and for which we have received the
    /// necessary secrets.
    Connection(UserId),
    Group,
    /// A connection chat which was established via a targeted message and is not yet confirmed by the other
    /// party. (outgoing)
    TargetedMessageConnection(UserId),
    /// An incoming pending connection chat from a handle or a targeted message which is not yet
    /// confirmed by the user. (incoming)
    PendingConnection(UserId),
}

impl ChatType {
    pub fn unconfirmed_contact(&self) -> Option<PartialContactType> {
        match self {
            ChatType::HandleConnection(username) => {
                Some(PartialContactType::Handle(username.clone()))
            }
            ChatType::TargetedMessageConnection(user_id) => {
                Some(PartialContactType::TargetedMessage(user_id.clone()))
            }
            _ => None,
        }
    }
}

/// Attributes of a chat.
///
/// This type is an in-memory representation of the chat attributes and is only persisted in the
/// local database. It is not used to be communicated with other clients. For that, see its
/// counterpart [`GroupData`].
#[derive(Debug, Clone, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct ChatAttributes {
    pub title: String,
    pub picture: Option<Vec<u8>>,
}

impl ChatAttributes {
    pub fn new(title: String, picture: Option<Vec<u8>>) -> Self {
        Self { title, picture }
    }

    pub fn title(&self) -> &str {
        self.title.as_ref()
    }

    pub fn set_title(&mut self, title: String) {
        self.title = title;
    }

    pub fn picture(&self) -> Option<&[u8]> {
        self.picture.as_deref()
    }

    pub fn set_picture(&mut self, picture: Option<Vec<u8>>) {
        self.picture = picture;
    }
}

/// Extension trait for bridging [`GroupData`] and types in this coreclient.
pub(crate) trait GroupDataExt {
    /// Decodes the group data from the group data extension bytes.
    fn decode(bytes: &GroupDataBytes) -> Result<Self, codec::Error>
    where
        Self: Sized;

    /// Encodes the group data as bytes to be stored in the group data extension.
    fn encode(&self) -> Result<GroupDataBytes, codec::Error>;

    fn into_parts(
        self,
        identity_link_wrapper_key: &IdentityLinkWrapperKey,
    ) -> (ChatAttributes, Option<ExternalGroupProfile>);
}

impl GroupDataExt for GroupData {
    fn decode(bytes: &GroupDataBytes) -> Result<Self, codec::Error> {
        PersistenceCodec::from_slice(bytes.bytes())
    }

    fn encode(&self) -> Result<GroupDataBytes, codec::Error> {
        PersistenceCodec::to_vec(self).map(From::from)
    }

    fn into_parts(
        self,
        identity_link_wrapper_key: &IdentityLinkWrapperKey,
    ) -> (ChatAttributes, Option<ExternalGroupProfile>) {
        let Self {
            title,
            picture,
            encrypted_title,
            external_group_profile,
        } = self;

        // Always prefer the encrypted title over the plaintext title
        let title = if let Some(encrypted_title) = encrypted_title
            && let Ok(decrypted_title) = encrypted_title
                .decrypt(identity_link_wrapper_key)
                .inspect_err(|error| {
                    error!(%error, "Failed to decrypt group title; fallback to plaintext");
                }) {
            decrypted_title
        } else {
            title
        };
        (ChatAttributes { title, picture }, external_group_profile)
    }
}
