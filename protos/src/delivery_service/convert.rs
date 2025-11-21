// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::{
    credentials::keys,
    crypto::{ear, secrets},
    identifiers,
    messages::{client_ds, client_ds_out::AddUsersInfoOut, welcome_attribution_info},
};
use mls_assist::messages::AssistedWelcome;
use openmls::prelude::{MlsMessageBodyIn, MlsMessageIn, group_info};
use tls_codec::{DeserializeBytes, Serialize};
use tonic::Status;

use crate::{
    common::convert::InvalidNonceLen,
    convert::{FromRef, TryFromRef, TryRefInto},
    delivery_service::v1::{
        TargetedApplicationMessage, targeted_message_payload::TargetedMessageType,
    },
    validation::{MissingFieldError, MissingFieldExt},
};

use super::v1::{
    AddUsersInfo, AssistedMessage, ClientVerifyingKey, EncryptedUserProfileKey,
    EncryptedWelcomeAttributionInfo, GroupEpoch, GroupInfo, GroupStateEarKey, LeafNodeIndex,
    MlsMessage, QsReference, RatchetTree, RoomState, SealedClientReference,
};

impl From<identifiers::SealedClientReference> for SealedClientReference {
    fn from(value: identifiers::SealedClientReference) -> Self {
        Self {
            ciphertext: Some(value.into_ciphertext().into()),
        }
    }
}

impl TryFrom<SealedClientReference> for identifiers::SealedClientReference {
    type Error = MissingFieldError<&'static str>;

    fn try_from(proto: SealedClientReference) -> Result<Self, Self::Error> {
        let ciphertext = proto.ciphertext.ok_or_missing_field("ciphertext")?;
        let ciphertext = openmls::prelude::HpkeCiphertext::from(ciphertext);
        Ok(ciphertext.into())
    }
}

impl From<identifiers::QsReference> for QsReference {
    fn from(value: identifiers::QsReference) -> Self {
        Self {
            client_homeserver_domain: Some(value.client_homeserver_domain.into()),
            sealed_reference: Some(value.sealed_reference.into()),
        }
    }
}

impl TryFrom<QsReference> for identifiers::QsReference {
    type Error = QsReferenceError;

    fn try_from(proto: QsReference) -> Result<Self, Self::Error> {
        Ok(Self {
            client_homeserver_domain: proto
                .client_homeserver_domain
                .as_ref()
                .ok_or_missing_field("client_homeserver_domain")?
                .try_ref_into()?,
            sealed_reference: proto
                .sealed_reference
                .ok_or_missing_field("sealed_reference")?
                .try_into()?,
        })
    }
}

#[derive(Debug, thiserror::Error)]
pub enum QsReferenceError {
    #[error(transparent)]
    Field(#[from] MissingFieldError<&'static str>),
    #[error(transparent)]
    Fqdn(#[from] identifiers::FqdnError),
}

impl From<QsReferenceError> for Status {
    fn from(e: QsReferenceError) -> Self {
        Status::invalid_argument(format!("invalid QS reference: {e}"))
    }
}

impl From<ear::keys::EncryptedUserProfileKey> for EncryptedUserProfileKey {
    fn from(value: ear::keys::EncryptedUserProfileKey) -> Self {
        Self {
            ciphertext: Some(value.into()),
        }
    }
}

impl TryFrom<EncryptedUserProfileKey> for ear::keys::EncryptedUserProfileKey {
    type Error = EncryptedUserProfileKeyError;

    fn try_from(proto: EncryptedUserProfileKey) -> Result<Self, Self::Error> {
        let ciphertext = proto
            .ciphertext
            .ok_or_missing_field(CiphertextField)?
            .try_into()?;
        Ok(ciphertext)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum EncryptedUserProfileKeyError {
    #[error(transparent)]
    Field(#[from] MissingFieldError<CiphertextField>),
    #[error(transparent)]
    Ciphertext(#[from] InvalidNonceLen),
}

impl From<EncryptedUserProfileKeyError> for Status {
    fn from(e: EncryptedUserProfileKeyError) -> Self {
        Status::invalid_argument(format!("invalid encrypted user profil key: {e}"))
    }
}

#[derive(Debug, derive_more::Display)]
#[display(fmt = "ciphertext")]
pub struct CiphertextField;

impl TryFromRef<'_, openmls::framing::MlsMessageOut> for MlsMessage {
    type Error = tls_codec::Error;

    fn try_from_ref(value: &openmls::framing::MlsMessageOut) -> Result<Self, Self::Error> {
        Ok(Self {
            tls: value.tls_serialize_detached()?,
        })
    }
}

impl From<Vec<u8>> for MlsMessage {
    fn from(value: Vec<u8>) -> Self {
        Self { tls: value }
    }
}

impl TryFromRef<'_, MlsMessage> for openmls::framing::MlsMessageIn {
    type Error = tls_codec::Error;

    fn try_from_ref(proto: &MlsMessage) -> Result<Self, Self::Error> {
        DeserializeBytes::tls_deserialize_exact_bytes(&proto.tls)
    }
}

impl TryFromRef<'_, mimi_room_policy::RoomState> for RoomState {
    type Error = tls_codec::Error;

    fn try_from_ref(value: &mimi_room_policy::RoomState) -> Result<Self, Self::Error> {
        Ok(Self {
            tls: value.tls_serialize_detached()?,
        })
    }
}

impl TryFromRef<'_, RoomState> for mimi_room_policy::RoomState {
    type Error = tls_codec::Error;

    fn try_from_ref(proto: &RoomState) -> Result<Self, Self::Error> {
        DeserializeBytes::tls_deserialize_exact_bytes(&proto.tls)
    }
}

impl TryFromRef<'_, openmls::treesync::RatchetTree> for RatchetTree {
    type Error = tls_codec::Error;

    fn try_from_ref(value: &openmls::treesync::RatchetTree) -> Result<Self, Self::Error> {
        Ok(Self {
            tls: value.tls_serialize_detached()?,
        })
    }
}

impl TryFromRef<'_, RatchetTree> for openmls::treesync::RatchetTreeIn {
    type Error = tls_codec::Error;

    fn try_from_ref(proto: &RatchetTree) -> Result<Self, Self::Error> {
        DeserializeBytes::tls_deserialize_exact_bytes(&proto.tls)
    }
}

impl FromRef<'_, ear::keys::GroupStateEarKey> for GroupStateEarKey {
    fn from_ref(value: &ear::keys::GroupStateEarKey) -> Self {
        Self {
            key: value.as_ref().secret().to_vec(),
        }
    }
}

impl TryFromRef<'_, GroupStateEarKey> for ear::keys::GroupStateEarKey {
    type Error = InvalidGroupStateEarKeyLen;

    fn try_from_ref(proto: &GroupStateEarKey) -> Result<Self, Self::Error> {
        let bytes: [u8; 32] = proto
            .key
            .as_slice()
            .try_into()
            .map_err(|_| InvalidGroupStateEarKeyLen(proto.key.len()))?;
        let secret = secrets::Secret::from(bytes);
        Ok(secret.into())
    }
}

#[derive(Debug, thiserror::Error)]
#[error("Invalid group state EAR key length: {0}")]
pub struct InvalidGroupStateEarKeyLen(usize);

impl From<InvalidGroupStateEarKeyLen> for Status {
    fn from(e: InvalidGroupStateEarKeyLen) -> Self {
        Status::invalid_argument(e.to_string())
    }
}

impl From<GroupEpoch> for openmls::group::GroupEpoch {
    fn from(epoch: GroupEpoch) -> Self {
        epoch.value.into()
    }
}

impl From<openmls::group::GroupEpoch> for GroupEpoch {
    fn from(epoch: openmls::group::GroupEpoch) -> Self {
        Self {
            value: epoch.as_u64(),
        }
    }
}

impl From<LeafNodeIndex> for openmls::prelude::LeafNodeIndex {
    fn from(leaf_node_index: LeafNodeIndex) -> Self {
        Self::new(leaf_node_index.index)
    }
}

impl From<openmls::prelude::LeafNodeIndex> for LeafNodeIndex {
    fn from(leaf_node_index: openmls::prelude::LeafNodeIndex) -> Self {
        Self {
            index: leaf_node_index.u32(),
        }
    }
}

impl TryFromRef<'_, mls_assist::messages::AssistedMessageOut> for AssistedMessage {
    type Error = tls_codec::Error;

    fn try_from_ref(value: &mls_assist::messages::AssistedMessageOut) -> Result<Self, Self::Error> {
        Ok(Self {
            tls: value.tls_serialize_detached()?,
        })
    }
}

impl TryFromRef<'_, AssistedMessage> for mls_assist::messages::AssistedMessageIn {
    type Error = tls_codec::Error;

    fn try_from_ref(proto: &AssistedMessage) -> Result<Self, Self::Error> {
        DeserializeBytes::tls_deserialize_exact_bytes(&proto.tls)
    }
}

impl From<keys::ClientVerifyingKey> for ClientVerifyingKey {
    fn from(value: keys::ClientVerifyingKey) -> Self {
        Self {
            bytes: value.into_bytes(),
        }
    }
}

impl From<ClientVerifyingKey> for keys::ClientVerifyingKey {
    fn from(proto: ClientVerifyingKey) -> Self {
        Self::from_bytes(proto.bytes)
    }
}

impl From<welcome_attribution_info::EncryptedWelcomeAttributionInfo>
    for EncryptedWelcomeAttributionInfo
{
    fn from(value: welcome_attribution_info::EncryptedWelcomeAttributionInfo) -> Self {
        Self {
            ciphertext: Some(value.into()),
        }
    }
}

impl TryFrom<EncryptedWelcomeAttributionInfo>
    for welcome_attribution_info::EncryptedWelcomeAttributionInfo
{
    type Error = EncryptedWelcomeAttributionInfoError;

    fn try_from(proto: EncryptedWelcomeAttributionInfo) -> Result<Self, Self::Error> {
        let ciphertext: Self = proto
            .ciphertext
            .ok_or_missing_field(CiphertextField)?
            .try_into()?;
        Ok(ciphertext)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum EncryptedWelcomeAttributionInfoError {
    #[error(transparent)]
    Field(#[from] MissingFieldError<CiphertextField>),
    #[error(transparent)]
    Ciphertext(#[from] InvalidNonceLen),
}

impl From<EncryptedWelcomeAttributionInfoError> for Status {
    fn from(e: EncryptedWelcomeAttributionInfoError) -> Self {
        Status::invalid_argument(format!("invalid encrypted welcome attribution info: {e}"))
    }
}

impl TryFrom<AddUsersInfoOut> for AddUsersInfo {
    type Error = tls_codec::Error;

    fn try_from(value: AddUsersInfoOut) -> Result<Self, Self::Error> {
        Ok(Self {
            welcome: Some(value.welcome.try_ref_into()?),
            encrypted_welcome_attribution_info: value
                .encrypted_welcome_attribution_infos
                .into_iter()
                .map(From::from)
                .collect(),
        })
    }
}

impl TryFrom<AddUsersInfo> for client_ds::AddUsersInfo {
    type Error = AddUsersInfoError;

    fn try_from(proto: AddUsersInfo) -> Result<Self, Self::Error> {
        let message: MlsMessageIn = proto
            .welcome
            .ok_or_missing_field(WelcomeField)?
            .try_ref_into()?;
        let MlsMessageBodyIn::Welcome(welcome) = message.extract() else {
            return Err(AddUsersInfoError::InvalidWelcome);
        };
        let welcome = AssistedWelcome { welcome };
        Ok(Self {
            welcome,
            encrypted_welcome_attribution_infos: proto
                .encrypted_welcome_attribution_info
                .into_iter()
                .map(TryFrom::try_from)
                .collect::<Result<Vec<_>, _>>()?,
        })
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AddUsersInfoError {
    #[error(transparent)]
    Tls(#[from] tls_codec::Error),
    #[error(transparent)]
    Field(#[from] MissingFieldError<WelcomeField>),
    #[error("invalid welcome message")]
    InvalidWelcome,
    #[error(transparent)]
    Info(#[from] EncryptedWelcomeAttributionInfoError),
}

impl From<AddUsersInfoError> for Status {
    fn from(e: AddUsersInfoError) -> Self {
        Status::invalid_argument(format!("invalid add users info: {e}"))
    }
}

#[derive(Debug, derive_more::Display)]
#[display(fmt = "welcome")]
pub struct WelcomeField;

impl TryFrom<group_info::GroupInfo> for GroupInfo {
    type Error = tls_codec::Error;

    fn try_from(value: group_info::GroupInfo) -> Result<Self, Self::Error> {
        Ok(Self {
            tls: value.tls_serialize_detached()?,
        })
    }
}

impl TryFromRef<'_, GroupInfo> for group_info::VerifiableGroupInfo {
    type Error = tls_codec::Error;

    fn try_from_ref(proto: &GroupInfo) -> Result<Self, Self::Error> {
        DeserializeBytes::tls_deserialize_exact_bytes(&proto.tls)
    }
}

impl TryFromRef<'_, aircommon::messages::client_ds_out::TargetedMessageType>
    for TargetedMessageType
{
    type Error = tls_codec::Error;

    fn try_from_ref(
        value: &aircommon::messages::client_ds_out::TargetedMessageType,
    ) -> Result<Self, Self::Error> {
        match value {
            aircommon::messages::client_ds_out::TargetedMessageType::ApplicationMessage {
                message,
                recipient,
            } => Ok(TargetedMessageType::ApplicationMessage(
                TargetedApplicationMessage {
                    message: Some(message.try_ref_into()?),
                    recipient: Some((*recipient).into()),
                },
            )),
        }
    }
}
