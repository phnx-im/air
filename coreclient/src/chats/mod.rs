// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::fmt::Display;

use aircommon::{
    codec::{self, PersistenceCodec},
    identifiers::{Fqdn, QualifiedGroupId, UserHandle, UserId},
    time::TimeStamp,
};
use chrono::{DateTime, Utc};
use openmls::group::GroupId;
use serde::{Deserialize, Serialize};
use serde_tuple::{Deserialize_tuple, Serialize_tuple};
use sqlx::{SqliteConnection, SqliteExecutor};
use uuid::Uuid;

use crate::{contacts::PartialContactType, groups::GroupDataBytes, store::StoreNotifier};

pub use draft::MessageDraft;
pub(crate) use {pending::PendingConnectionInfo, status::StatusRecord};

mod draft;
pub(crate) mod messages;
mod pending;
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
        handle: UserHandle,
    ) -> Self {
        let id = ChatId::try_from(&group_id).unwrap();
        Self {
            id,
            group_id,
            last_read: Utc::now(),
            last_message_at: None,
            status: ChatStatus::Active,
            chat_type: ChatType::HandleConnection(handle),
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

    pub fn status_mut(&mut self) -> &mut ChatStatus {
        &mut self.status
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
        executor: impl SqliteExecutor<'_>,
        notifier: &mut StoreNotifier,
        picture: Option<Vec<u8>>,
    ) -> sqlx::Result<()> {
        Self::update_picture(executor, notifier, self.id, picture.as_deref()).await?;
        self.attributes.set_picture(picture);
        Ok(())
    }

    pub(crate) async fn set_title(
        &mut self,
        executor: impl SqliteExecutor<'_>,
        notifier: &mut StoreNotifier,
        title: String,
    ) -> sqlx::Result<()> {
        Self::update_title(executor, notifier, self.id, &title).await?;
        self.attributes.set_title(title);
        Ok(())
    }

    pub(crate) async fn set_inactive(
        &mut self,
        executor: &mut SqliteConnection,
        notifier: &mut StoreNotifier,
        past_members: Vec<UserId>,
    ) -> sqlx::Result<()> {
        let new_status = ChatStatus::Inactive(InactiveChat { past_members });
        Self::update_status(executor, notifier, self.id, &new_status).await?;
        self.status = new_status;
        Ok(())
    }

    /// Confirm a connection chat by setting the chat type to `Connection`.
    pub(crate) async fn confirm(
        &mut self,
        executor: impl SqliteExecutor<'_>,
        notifier: &mut StoreNotifier,
        user_id: UserId,
    ) -> sqlx::Result<()> {
        if self.is_unconfirmed() {
            let chat_type = ChatType::Connection(user_id);
            self.set_chat_type(executor, notifier, &chat_type).await?;
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
    HandleConnection(UserHandle),
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
            ChatType::HandleConnection(handle) => Some(PartialContactType::Handle(handle.clone())),
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
/// local database. It is not used to be communicated with other clients. For that, see it
/// counterpart [`GroupData`].
#[derive(Debug, Clone, Hash, Eq, PartialEq)]
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

/// Data stored in the group data extension as blob.
///
/// Warning: This type is serialized and stored in the group context, so it must be stable and
/// backward compatible. Fields can be added (with `#[serde(default)]`) or reordered, but not
/// removed or renamed.
#[derive(Debug, Clone, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct GroupData {
    pub title: String,
    pub picture: Option<Vec<u8>>,
    /// A storage id for the encrypted chat attributes.
    ///
    /// Via this id, the chat attributes can be retrieved from the object storage.
    #[serde(default)]
    pub encrypted_group_profile: Option<ExternalGroupProfile>,
}

impl GroupData {
    /// Decodes the group data from the group data extension bytes.
    pub(crate) fn decode(group_data: &GroupDataBytes) -> Result<Self, codec::Error> {
        PersistenceCodec::from_slice(group_data.bytes())
    }

    /// Encodes the group data as bytes to be stored in the group data extension.
    pub(crate) fn encode(&self) -> Result<GroupDataBytes, codec::Error> {
        PersistenceCodec::to_vec(self).map(From::from)
    }

    /// Convert the data into the in-memory part and the external part.
    pub(crate) fn into_parts(self) -> (ChatAttributes, Option<ExternalGroupProfile>) {
        let Self {
            title,
            picture,
            encrypted_group_profile,
        } = self;
        (ChatAttributes { title, picture }, encrypted_group_profile)
    }
}

/// A pointer to an external encrypted group profile in the object storage.
///
/// Warning: This type is serialized and stored in the group context, so it must be stable and
/// backward compatible. Fields can be added at the end, renamed, but not removed or reordered.
#[derive(Debug, Clone, Eq, PartialEq, Hash, Serialize_tuple, Deserialize_tuple)]
pub struct ExternalGroupProfile {
    /// Object ID in the object storage.
    ///
    /// Via this ID, the chat attributes can be retrieved from the object storage.
    pub object_id: Uuid,
    /// The hash of the encrypted content stored in the object storage.
    #[serde(with = "serde_bytes")]
    pub encrypted_content_hash: Vec<u8>,
}

/// Group profile stored as encrypted blob in the object storage.
///
/// Warning: This type is stored in the remote object storage and therefore must be kept stable. It
/// is serialized/deserialized as tuple. Fields can be added at the end, renamed, but not removed or
/// reordered.
#[derive(Debug, Clone, Eq, PartialEq, Hash, Serialize_tuple, Deserialize_tuple)]
pub struct GroupProfile {
    pub title: String,
    pub description: Option<String>,
    #[serde(with = "serde_bytes")]
    pub picture: Option<Vec<u8>>,
}

#[cfg(test)]
mod test {
    use aircommon::codec::PersistenceCodec;
    use uuid::uuid;

    use super::*;

    #[test]
    fn group_data_stability() {
        let data = GroupData {
            title: "Group Title".to_string(),
            picture: Some(vec![1, 2, 3]),
            encrypted_group_profile: Some(ExternalGroupProfile {
                object_id: uuid!("89fea7df-3823-4688-8915-00ab38db1577"),
                encrypted_content_hash: vec![42, 43],
            }),
        };
        let bytes = PersistenceCodec::to_vec(&data).unwrap();
        insta::assert_binary_snapshot!(".cbor", bytes);
    }

    #[test]
    fn group_profile_stability() {
        let profile = GroupProfile {
            title: "Group Title".to_string(),
            description: Some("Group Description".to_string()),
            picture: Some(vec![1, 2, 3]),
        };
        let bytes = PersistenceCodec::to_vec(&profile).unwrap();
        insta::assert_binary_snapshot!(".cbor", bytes);
    }

    #[test]
    fn chat_attributes_backward_compatibility() {
        #[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
        struct OldGroupData {
            title: String,
            picture: Option<Vec<u8>>,
        }

        let chat_attributes_v1 = OldGroupData {
            title: "title".to_string(),
            picture: Some(vec![1, 2, 3]),
        };
        let chat_attributes = GroupData {
            title: chat_attributes_v1.title.clone(),
            picture: chat_attributes_v1.picture.clone(),
            encrypted_group_profile: Some(ExternalGroupProfile {
                object_id: uuid!("89fea7df-3823-4688-8915-00ab38db1577"),
                encrypted_content_hash: vec![42, 43],
            }),
        };

        let bytes = PersistenceCodec::to_vec(&chat_attributes).unwrap();
        let value: OldGroupData = PersistenceCodec::from_slice(&bytes).unwrap();
        assert_eq!(value, chat_attributes_v1);

        let bytes = PersistenceCodec::to_vec(&chat_attributes_v1).unwrap();
        let value: GroupData = PersistenceCodec::from_slice(&bytes).unwrap();
        assert_eq!(
            value,
            GroupData {
                encrypted_group_profile: None,
                ..chat_attributes
            }
        );
    }
}
