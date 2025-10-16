// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! This module contains structs and enums that represent messages that are
//! passed between clients and the backend.
//! TODO: We should eventually factor this module out, together with the crypto
//! module, to allow re-use by the client implementation.

use mls_assist::{
    messages::{AssistedMessageIn, AssistedWelcome, SerializedMlsMessage},
    openmls::prelude::{GroupEpoch, GroupId, LeafNodeIndex, MlsMessageIn, RatchetTreeIn},
    openmls_traits::types::HpkeCiphertext,
};
use serde::{Deserialize, Serialize};
use tls_codec::{
    DeserializeBytes, Serialize as TlsSerializeTrait, TlsDeserializeBytes, TlsSerialize, TlsSize,
};

use crate::{
    credentials::keys::ClientVerifyingKey,
    crypto::{
        ear::{
            EarDecryptable, EarEncryptable,
            keys::{EncryptedUserProfileKey, GroupStateEarKey, RatchetKey},
        },
        hpke::{HpkeDecryptable, HpkeEncryptable, JoinerInfoKeyType},
        ratchet::QueueRatchet,
    },
    identifiers::QsReference,
    time::TimeStamp,
};

use super::{
    AirProtocolVersion, EncryptedQsQueueMessageCtype, client_as::EncryptedFriendshipPackage,
    welcome_attribution_info::EncryptedWelcomeAttributionInfo,
};

/// This is the pseudonymous client id used on the DS.
#[derive(TlsSerialize, TlsDeserializeBytes, TlsSize)]
pub(crate) struct DsClientId {
    id: Vec<u8>,
}

// === DS ===

pub type QsQueueRatchet = QueueRatchet<EncryptedQsQueueMessageCtype, QsQueueMessagePayload>;

#[derive(
    Debug, PartialEq, TlsSerialize, TlsDeserializeBytes, TlsSize, Clone, Serialize, Deserialize,
)]
#[repr(u8)]
pub enum QsQueueMessageType {
    WelcomeBundle,
    MlsMessage,
    UserProfileKeyUpdate,
}

#[derive(
    Debug, PartialEq, TlsSerialize, TlsDeserializeBytes, TlsSize, Clone, Serialize, Deserialize,
)]
pub struct QsQueueMessagePayload {
    pub timestamp: TimeStamp,
    pub message_type: QsQueueMessageType,
    #[serde(with = "serde_bytes")]
    pub payload: Vec<u8>,
}

impl QsQueueMessagePayload {
    pub fn extract(self) -> Result<ExtractedQsQueueMessage, tls_codec::Error> {
        let payload = match self.message_type {
            QsQueueMessageType::WelcomeBundle => {
                let wb = WelcomeBundle::tls_deserialize_exact_bytes(&self.payload)?;
                ExtractedQsQueueMessagePayload::WelcomeBundle(wb)
            }
            QsQueueMessageType::MlsMessage => {
                let message = MlsMessageIn::tls_deserialize_exact_bytes(self.payload.as_slice())?;
                ExtractedQsQueueMessagePayload::MlsMessage(Box::new(message))
            }
            QsQueueMessageType::UserProfileKeyUpdate => {
                let message = UserProfileKeyUpdateParams::tls_deserialize_exact_bytes(
                    self.payload.as_slice(),
                )?;
                ExtractedQsQueueMessagePayload::UserProfileKeyUpdate(message)
            }
        };
        Ok(ExtractedQsQueueMessage {
            timestamp: self.timestamp,
            payload,
        })
    }
}

#[derive(Debug)]
pub struct ExtractedQsQueueMessage {
    pub timestamp: TimeStamp,
    pub payload: ExtractedQsQueueMessagePayload,
}

#[derive(Debug)]
pub enum ExtractedQsQueueMessagePayload {
    WelcomeBundle(WelcomeBundle),
    MlsMessage(Box<MlsMessageIn>),
    UserProfileKeyUpdate(UserProfileKeyUpdateParams),
}

impl TryFrom<WelcomeBundle> for QsQueueMessagePayload {
    type Error = tls_codec::Error;

    fn try_from(welcome_bundle: WelcomeBundle) -> Result<Self, Self::Error> {
        let payload = welcome_bundle.tls_serialize_detached()?;
        Ok(Self {
            timestamp: TimeStamp::now(),
            message_type: QsQueueMessageType::WelcomeBundle,
            payload,
        })
    }
}

impl TryFrom<&UserProfileKeyUpdateParams> for QsQueueMessagePayload {
    type Error = tls_codec::Error;

    fn try_from(params: &UserProfileKeyUpdateParams) -> Result<Self, Self::Error> {
        let payload = params.tls_serialize_detached()?;
        Ok(Self {
            timestamp: TimeStamp::now(),
            message_type: QsQueueMessageType::UserProfileKeyUpdate,
            payload,
        })
    }
}

impl From<SerializedMlsMessage> for QsQueueMessagePayload {
    fn from(value: SerializedMlsMessage) -> Self {
        Self {
            timestamp: TimeStamp::now(),
            message_type: QsQueueMessageType::MlsMessage,
            payload: value.0,
        }
    }
}

impl EarEncryptable<RatchetKey, EncryptedQsQueueMessageCtype> for QsQueueMessagePayload {}
impl EarDecryptable<RatchetKey, EncryptedQsQueueMessageCtype> for QsQueueMessagePayload {}

#[derive(TlsSerialize, TlsDeserializeBytes, TlsSize)]
pub struct AadMessage {
    version: AirProtocolVersion,
    payload: AadPayload,
}

impl From<AadPayload> for AadMessage {
    fn from(payload: AadPayload) -> Self {
        Self {
            version: AirProtocolVersion::default(),
            payload,
        }
    }
}

impl AadMessage {
    pub fn version(&self) -> AirProtocolVersion {
        self.version
    }

    pub fn into_payload(self) -> AadPayload {
        self.payload
    }
}

#[derive(TlsSerialize, TlsDeserializeBytes, TlsSize)]
#[repr(u8)]
pub enum AadPayload {
    GroupOperation(GroupOperationParamsAad),
    JoinConnectionGroup(JoinConnectionGroupParamsAad),
    Resync,
    DeleteGroup,
    // There is no SelfRemoveClient entry, since that message consists of a
    // single proposal and since we don't otherwise support individual
    // proposals, there is not need to signal it explicitly.
}

#[derive(
    PartialEq, Eq, Debug, Clone, Serialize, Deserialize, TlsSerialize, TlsDeserializeBytes, TlsSize,
)]
pub struct DsEventMessage {
    pub group_id: GroupId,
    pub sender_index: LeafNodeIndex,
    pub epoch: GroupEpoch,
    // Timestamp set by the DS at the time of processing the message.
    pub timestamp: TimeStamp,
    #[serde(with = "serde_bytes")]
    pub payload: Vec<u8>,
}

impl DsEventMessage {
    pub fn group_id(&self) -> &GroupId {
        &self.group_id
    }

    pub fn sender_index(&self) -> LeafNodeIndex {
        self.sender_index
    }

    pub fn epoch(&self) -> GroupEpoch {
        self.epoch
    }

    pub fn payload(&self) -> &[u8] {
        &self.payload
    }
}

#[derive(Debug)]
pub struct CreateGroupParams {
    pub group_id: GroupId,
    pub leaf_node: RatchetTreeIn,
    pub encrypted_user_profile_key: EncryptedUserProfileKey,
    pub creator_qs_reference: QsReference,
    pub group_info: MlsMessageIn,
    pub room_state: Vec<u8>,
}

#[derive(Debug)]
pub struct WelcomeInfoParams {
    pub group_id: GroupId,
    pub sender: ClientVerifyingKey,
    pub epoch: GroupEpoch,
}

#[derive(Debug)]
pub struct ExternalCommitInfoParams {
    pub group_id: GroupId,
}

#[derive(Debug)]
pub struct ConnectionGroupInfoParams {
    pub group_id: GroupId,
}

#[derive(Debug)]
pub struct AddUsersInfo {
    pub welcome: AssistedWelcome,
    pub encrypted_welcome_attribution_infos: Vec<EncryptedWelcomeAttributionInfo>,
}

#[derive(Debug)]
pub struct GroupOperationParams {
    pub commit: AssistedMessageIn,
    pub add_users_info_option: Option<AddUsersInfo>,
}

#[derive(TlsSerialize, TlsDeserializeBytes, TlsSize)]
pub struct GroupOperationParamsAad {
    pub new_encrypted_user_profile_keys: Vec<EncryptedUserProfileKey>,
}

#[derive(Debug)]
pub struct UpdateParams {
    pub commit: AssistedMessageIn,
}

#[derive(Debug)]
pub struct JoinConnectionGroupParams {
    pub external_commit: AssistedMessageIn,
    pub qs_client_reference: QsReference,
}

#[derive(TlsSerialize, TlsDeserializeBytes, TlsSize)]
pub struct JoinConnectionGroupParamsAad {
    pub encrypted_friendship_package: EncryptedFriendshipPackage,
    pub encrypted_user_profile_key: EncryptedUserProfileKey,
}

#[derive(Debug)]
pub struct ResyncParams {
    pub external_commit: AssistedMessageIn,
    pub sender_index: LeafNodeIndex,
}

#[derive(Debug)]
pub struct SelfRemoveParams {
    pub remove_proposal: AssistedMessageIn,
}

#[derive(Debug)]
pub struct DeleteGroupParams {
    pub commit: AssistedMessageIn,
}

#[derive(Debug, Clone, TlsDeserializeBytes, TlsSize, TlsSerialize)]
pub struct UserProfileKeyUpdateParams {
    pub group_id: GroupId,
    pub sender_index: LeafNodeIndex,
    pub user_profile_key: EncryptedUserProfileKey,
}

#[derive(TlsSerialize, TlsSize, Clone, TlsDeserializeBytes)]
pub struct DsJoinerInformation {
    pub group_state_ear_key: GroupStateEarKey,
}

#[derive(Debug, TlsSerialize, TlsDeserializeBytes, TlsSize, Clone)]
pub struct EncryptedDsJoinerInformation {
    pub ciphertext: HpkeCiphertext,
}

impl AsRef<HpkeCiphertext> for EncryptedDsJoinerInformation {
    fn as_ref(&self) -> &HpkeCiphertext {
        &self.ciphertext
    }
}

impl From<HpkeCiphertext> for EncryptedDsJoinerInformation {
    fn from(ciphertext: HpkeCiphertext) -> Self {
        Self { ciphertext }
    }
}

impl HpkeEncryptable<JoinerInfoKeyType, EncryptedDsJoinerInformation> for DsJoinerInformation {}
impl HpkeDecryptable<JoinerInfoKeyType, EncryptedDsJoinerInformation> for DsJoinerInformation {}

#[derive(Debug, TlsSerialize, TlsDeserializeBytes, TlsSize, Clone)]
pub struct WelcomeBundle {
    pub welcome: AssistedWelcome,
    // This is the part the DS shouldn't see.
    pub encrypted_attribution_info: EncryptedWelcomeAttributionInfo,
    // This part is added by the DS later.
    pub encrypted_joiner_info: EncryptedDsJoinerInformation,
}
