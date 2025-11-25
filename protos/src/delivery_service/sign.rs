// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use prost::Message;

use crate::delivery_service::v1::{
    AssistedMessage, GetAttachmentUrlPayload, GetAttachmentUrlRequest, GroupStateEarKey,
    LeafNodeIndex, ProvisionAttachmentPayload, ProvisionAttachmentRequest, TargetedMessagePayload,
    TargetedMessageRequest,
};

use super::v1::{
    CreateGroupPayload, CreateGroupRequest, DeleteGroupPayload, DeleteGroupRequest,
    GroupOperationPayload, GroupOperationRequest, ResyncPayload, ResyncRequest, SelfRemovePayload,
    SelfRemoveRequest, SendMessagePayload, SendMessageRequest, UpdateProfileKeyPayload,
    UpdateProfileKeyRequest, WelcomeInfoPayload, WelcomeInfoRequest,
};

use aircommon::{
    credentials::keys::{ClientKeyType, ClientSignature},
    crypto::signatures::signable::{Signable, SignedStruct, Verifiable, VerifiedStruct},
};

const SEND_MESSAGE_PAYLOAD_LABEL: &str = "SendMessagePayload";

impl SignedStruct<SendMessagePayload, ClientKeyType> for SendMessageRequest {
    fn from_payload(payload: SendMessagePayload, signature: ClientSignature) -> Self {
        Self {
            payload: Some(payload),
            signature: Some(signature.into()),
        }
    }
}

impl Signable for SendMessagePayload {
    type SignedOutput = SendMessageRequest;

    fn unsigned_payload(&self) -> Result<Vec<u8>, tls_codec::Error> {
        Ok(self.encode_to_vec())
    }

    fn label(&self) -> &str {
        SEND_MESSAGE_PAYLOAD_LABEL
    }
}

impl VerifiedStruct<SendMessageRequest> for SendMessagePayload {
    type SealingType = private_mod::Seal;

    fn from_verifiable(verifiable: SendMessageRequest, _seal: Self::SealingType) -> Self {
        verifiable.payload.unwrap()
    }
}

/// For backwards compatibility, we need to be able to verify signatures over the
/// old payload format that did not include the `suppress_notifications` field.
#[derive(Clone, PartialEq, Eq, Hash, prost::Message)]
pub struct SendMessagePayloadV1 {
    #[prost(message, optional, tag = "1")]
    pub group_state_ear_key: Option<GroupStateEarKey>,
    #[prost(message, optional, tag = "2")]
    pub message: Option<AssistedMessage>,
    #[prost(message, optional, tag = "3")]
    pub sender: Option<LeafNodeIndex>,
}

impl Verifiable for SendMessageRequest {
    fn unsigned_payload(&self) -> Result<Vec<u8>, tls_codec::Error> {
        let payload = self.payload.as_ref().ok_or(MissingPayloadError)?;
        let bytes = if payload.suppress_notifications.is_some() {
            payload.encode_to_vec()
        } else {
            // Convert to old payload without optional field for backwards
            // compatible signature verification.
            SendMessagePayloadV1 {
                group_state_ear_key: payload.group_state_ear_key.clone(),
                message: payload.message.clone(),
                sender: payload.sender,
            }
            .encode_to_vec()
        };
        Ok(bytes)
    }

    fn signature(&self) -> impl AsRef<[u8]> {
        self.signature
            .as_ref()
            .map(|s| s.value.as_slice())
            .unwrap_or_default()
    }

    fn label(&self) -> &str {
        SEND_MESSAGE_PAYLOAD_LABEL
    }
}

const WELCOME_INFO_PAYLOAD_LABEL: &str = "WelcomeInfoPayload";

impl SignedStruct<WelcomeInfoPayload, ClientKeyType> for WelcomeInfoRequest {
    fn from_payload(payload: WelcomeInfoPayload, signature: ClientSignature) -> Self {
        Self {
            payload: Some(payload),
            signature: Some(signature.into()),
        }
    }
}

impl Signable for WelcomeInfoPayload {
    type SignedOutput = WelcomeInfoRequest;

    fn unsigned_payload(&self) -> Result<Vec<u8>, tls_codec::Error> {
        Ok(self.encode_to_vec())
    }

    fn label(&self) -> &str {
        WELCOME_INFO_PAYLOAD_LABEL
    }
}

impl Verifiable for WelcomeInfoRequest {
    fn unsigned_payload(&self) -> Result<Vec<u8>, tls_codec::Error> {
        Ok(self
            .payload
            .as_ref()
            .ok_or(MissingPayloadError)?
            .encode_to_vec())
    }

    fn signature(&self) -> impl AsRef<[u8]> {
        self.signature
            .as_ref()
            .map(|s| s.value.as_slice())
            .unwrap_or_default()
    }

    fn label(&self) -> &str {
        WELCOME_INFO_PAYLOAD_LABEL
    }
}

const CREATE_GROUP_PAYLOAD_LABEL: &str = "CreateGroupPayload";

impl VerifiedStruct<WelcomeInfoRequest> for WelcomeInfoPayload {
    type SealingType = private_mod::Seal;

    fn from_verifiable(verifiable: WelcomeInfoRequest, _seal: Self::SealingType) -> Self {
        verifiable.payload.unwrap()
    }
}

impl SignedStruct<CreateGroupPayload, ClientKeyType> for CreateGroupRequest {
    fn from_payload(payload: CreateGroupPayload, signature: ClientSignature) -> Self {
        Self {
            payload: Some(payload),
            signature: Some(signature.into()),
        }
    }
}

impl Signable for CreateGroupPayload {
    type SignedOutput = CreateGroupRequest;

    fn unsigned_payload(&self) -> Result<Vec<u8>, tls_codec::Error> {
        Ok(self.encode_to_vec())
    }

    fn label(&self) -> &str {
        CREATE_GROUP_PAYLOAD_LABEL
    }
}

impl VerifiedStruct<CreateGroupRequest> for CreateGroupPayload {
    type SealingType = private_mod::Seal;

    fn from_verifiable(verifiable: CreateGroupRequest, _seal: Self::SealingType) -> Self {
        verifiable.payload.unwrap()
    }
}

impl Verifiable for CreateGroupRequest {
    fn unsigned_payload(&self) -> Result<Vec<u8>, tls_codec::Error> {
        Ok(self
            .payload
            .as_ref()
            .ok_or(MissingPayloadError)?
            .encode_to_vec())
    }

    fn signature(&self) -> impl AsRef<[u8]> {
        self.signature
            .as_ref()
            .map(|s| s.value.as_slice())
            .unwrap_or_default()
    }

    fn label(&self) -> &str {
        CREATE_GROUP_PAYLOAD_LABEL
    }
}

const DELETE_GROUP_PAYLOAD_LABEL: &str = "DeleteGroupPayload";

impl SignedStruct<DeleteGroupPayload, ClientKeyType> for DeleteGroupRequest {
    fn from_payload(payload: DeleteGroupPayload, signature: ClientSignature) -> Self {
        Self {
            payload: Some(payload),
            signature: Some(signature.into()),
        }
    }
}

impl Signable for DeleteGroupPayload {
    type SignedOutput = DeleteGroupRequest;

    fn unsigned_payload(&self) -> Result<Vec<u8>, tls_codec::Error> {
        Ok(self.encode_to_vec())
    }

    fn label(&self) -> &str {
        DELETE_GROUP_PAYLOAD_LABEL
    }
}

impl VerifiedStruct<DeleteGroupRequest> for DeleteGroupPayload {
    type SealingType = private_mod::Seal;

    fn from_verifiable(verifiable: DeleteGroupRequest, _seal: Self::SealingType) -> Self {
        verifiable.payload.unwrap()
    }
}

impl Verifiable for DeleteGroupRequest {
    fn unsigned_payload(&self) -> Result<Vec<u8>, tls_codec::Error> {
        Ok(self
            .payload
            .as_ref()
            .ok_or(MissingPayloadError)?
            .encode_to_vec())
    }

    fn signature(&self) -> impl AsRef<[u8]> {
        self.signature
            .as_ref()
            .map(|s| s.value.as_slice())
            .unwrap_or_default()
    }

    fn label(&self) -> &str {
        DELETE_GROUP_PAYLOAD_LABEL
    }
}

const GROUP_OPERATION_PAYLOAD_LABEL: &str = "GroupOperationPayload";

impl SignedStruct<GroupOperationPayload, ClientKeyType> for GroupOperationRequest {
    fn from_payload(payload: GroupOperationPayload, signature: ClientSignature) -> Self {
        Self {
            payload: Some(payload),
            signature: Some(signature.into()),
        }
    }
}

impl Signable for GroupOperationPayload {
    type SignedOutput = GroupOperationRequest;

    fn unsigned_payload(&self) -> Result<Vec<u8>, tls_codec::Error> {
        Ok(self.encode_to_vec())
    }

    fn label(&self) -> &str {
        GROUP_OPERATION_PAYLOAD_LABEL
    }
}

impl VerifiedStruct<GroupOperationRequest> for GroupOperationPayload {
    type SealingType = private_mod::Seal;

    fn from_verifiable(verifiable: GroupOperationRequest, _seal: Self::SealingType) -> Self {
        verifiable.payload.unwrap()
    }
}

impl Verifiable for GroupOperationRequest {
    fn unsigned_payload(&self) -> Result<Vec<u8>, tls_codec::Error> {
        Ok(self
            .payload
            .as_ref()
            .ok_or(MissingPayloadError)?
            .encode_to_vec())
    }

    fn signature(&self) -> impl AsRef<[u8]> {
        self.signature
            .as_ref()
            .map(|s| s.value.as_slice())
            .unwrap_or_default()
    }

    fn label(&self) -> &str {
        GROUP_OPERATION_PAYLOAD_LABEL
    }
}

const TARGETED_MESSAGE_PAYLOAD_LABEL: &str = "TargetedMessagePayload";

impl SignedStruct<TargetedMessagePayload, ClientKeyType> for TargetedMessageRequest {
    fn from_payload(payload: TargetedMessagePayload, signature: ClientSignature) -> Self {
        Self {
            payload: Some(payload),
            signature: Some(signature.into()),
        }
    }
}

impl Signable for TargetedMessagePayload {
    type SignedOutput = TargetedMessageRequest;

    fn unsigned_payload(&self) -> Result<Vec<u8>, tls_codec::Error> {
        Ok(self.encode_to_vec())
    }

    fn label(&self) -> &str {
        TARGETED_MESSAGE_PAYLOAD_LABEL
    }
}

impl VerifiedStruct<TargetedMessageRequest> for TargetedMessagePayload {
    type SealingType = private_mod::Seal;

    fn from_verifiable(verifiable: TargetedMessageRequest, _seal: Self::SealingType) -> Self {
        verifiable.payload.unwrap()
    }
}

impl Verifiable for TargetedMessageRequest {
    fn unsigned_payload(&self) -> Result<Vec<u8>, tls_codec::Error> {
        Ok(self
            .payload
            .as_ref()
            .ok_or(MissingPayloadError)?
            .encode_to_vec())
    }

    fn signature(&self) -> impl AsRef<[u8]> {
        self.signature
            .as_ref()
            .map(|s| s.value.as_slice())
            .unwrap_or_default()
    }

    fn label(&self) -> &str {
        TARGETED_MESSAGE_PAYLOAD_LABEL
    }
}

const SELF_REMOVE_PAYLOAD_LABEL: &str = "SelfRemovePayload";

impl SignedStruct<SelfRemovePayload, ClientKeyType> for SelfRemoveRequest {
    fn from_payload(payload: SelfRemovePayload, signature: ClientSignature) -> Self {
        Self {
            payload: Some(payload),
            signature: Some(signature.into()),
        }
    }
}

impl Signable for SelfRemovePayload {
    type SignedOutput = SelfRemoveRequest;

    fn unsigned_payload(&self) -> Result<Vec<u8>, tls_codec::Error> {
        Ok(self.encode_to_vec())
    }

    fn label(&self) -> &str {
        SELF_REMOVE_PAYLOAD_LABEL
    }
}

impl VerifiedStruct<SelfRemoveRequest> for SelfRemovePayload {
    type SealingType = private_mod::Seal;

    fn from_verifiable(verifiable: SelfRemoveRequest, _seal: Self::SealingType) -> Self {
        verifiable.payload.unwrap()
    }
}

impl Verifiable for SelfRemoveRequest {
    fn unsigned_payload(&self) -> Result<Vec<u8>, tls_codec::Error> {
        Ok(self
            .payload
            .as_ref()
            .ok_or(MissingPayloadError)?
            .encode_to_vec())
    }

    fn signature(&self) -> impl AsRef<[u8]> {
        self.signature
            .as_ref()
            .map(|s| s.value.as_slice())
            .unwrap_or_default()
    }

    fn label(&self) -> &str {
        SELF_REMOVE_PAYLOAD_LABEL
    }
}

const RESYNC_PAYLOAD_LABEL: &str = "ResyncPayload";

impl SignedStruct<ResyncPayload, ClientKeyType> for ResyncRequest {
    fn from_payload(payload: ResyncPayload, signature: ClientSignature) -> Self {
        Self {
            sender_index: payload.sender,
            payload: Some(payload),
            signature: Some(signature.into()),
        }
    }
}

impl Signable for ResyncPayload {
    type SignedOutput = ResyncRequest;

    fn unsigned_payload(&self) -> Result<Vec<u8>, tls_codec::Error> {
        Ok(self.encode_to_vec())
    }

    fn label(&self) -> &str {
        RESYNC_PAYLOAD_LABEL
    }
}

impl VerifiedStruct<ResyncRequest> for ResyncPayload {
    type SealingType = private_mod::Seal;

    fn from_verifiable(verifiable: ResyncRequest, _seal: Self::SealingType) -> Self {
        verifiable.payload.unwrap()
    }
}

impl Verifiable for ResyncRequest {
    fn unsigned_payload(&self) -> Result<Vec<u8>, tls_codec::Error> {
        Ok(self
            .payload
            .as_ref()
            .ok_or(MissingPayloadError)?
            .encode_to_vec())
    }

    fn signature(&self) -> impl AsRef<[u8]> {
        self.signature
            .as_ref()
            .map(|s| s.value.as_slice())
            .unwrap_or_default()
    }

    fn label(&self) -> &str {
        RESYNC_PAYLOAD_LABEL
    }
}

const UPDATE_PROFILE_KEY_PAYLOAD_LABEL: &str = "UpdateProfileKeyPayload";

impl SignedStruct<UpdateProfileKeyPayload, ClientKeyType> for UpdateProfileKeyRequest {
    fn from_payload(payload: UpdateProfileKeyPayload, signature: ClientSignature) -> Self {
        Self {
            payload: Some(payload),
            signature: Some(signature.into()),
        }
    }
}

impl Signable for UpdateProfileKeyPayload {
    type SignedOutput = UpdateProfileKeyRequest;

    fn unsigned_payload(&self) -> Result<Vec<u8>, tls_codec::Error> {
        Ok(self.encode_to_vec())
    }

    fn label(&self) -> &str {
        UPDATE_PROFILE_KEY_PAYLOAD_LABEL
    }
}

impl VerifiedStruct<UpdateProfileKeyRequest> for UpdateProfileKeyPayload {
    type SealingType = private_mod::Seal;

    fn from_verifiable(verifiable: UpdateProfileKeyRequest, _seal: Self::SealingType) -> Self {
        verifiable.payload.unwrap()
    }
}

impl Verifiable for UpdateProfileKeyRequest {
    fn unsigned_payload(&self) -> Result<Vec<u8>, tls_codec::Error> {
        Ok(self
            .payload
            .as_ref()
            .ok_or(MissingPayloadError)?
            .encode_to_vec())
    }

    fn signature(&self) -> impl AsRef<[u8]> {
        self.signature
            .as_ref()
            .map(|s| s.value.as_slice())
            .unwrap_or_default()
    }

    fn label(&self) -> &str {
        UPDATE_PROFILE_KEY_PAYLOAD_LABEL
    }
}

const PROVISION_ATTACHMENT_PAYLOAD_LABEL: &str = "ProvisionAttachmentPayload";

impl SignedStruct<ProvisionAttachmentPayload, ClientKeyType> for ProvisionAttachmentRequest {
    fn from_payload(payload: ProvisionAttachmentPayload, signature: ClientSignature) -> Self {
        Self {
            payload: Some(payload),
            signature: Some(signature.into()),
        }
    }
}

impl Signable for ProvisionAttachmentPayload {
    type SignedOutput = ProvisionAttachmentRequest;

    fn unsigned_payload(&self) -> Result<Vec<u8>, tls_codec::Error> {
        Ok(self.encode_to_vec())
    }

    fn label(&self) -> &str {
        PROVISION_ATTACHMENT_PAYLOAD_LABEL
    }
}

impl VerifiedStruct<ProvisionAttachmentRequest> for ProvisionAttachmentPayload {
    type SealingType = private_mod::Seal;

    fn from_verifiable(verifiable: ProvisionAttachmentRequest, _seal: Self::SealingType) -> Self {
        verifiable.payload.unwrap()
    }
}

impl Verifiable for ProvisionAttachmentRequest {
    fn unsigned_payload(&self) -> Result<Vec<u8>, tls_codec::Error> {
        Ok(self
            .payload
            .as_ref()
            .ok_or(MissingPayloadError)?
            .encode_to_vec())
    }

    fn signature(&self) -> impl AsRef<[u8]> {
        self.signature
            .as_ref()
            .map(|s| s.value.as_slice())
            .unwrap_or_default()
    }

    fn label(&self) -> &str {
        PROVISION_ATTACHMENT_PAYLOAD_LABEL
    }
}

const GET_ATTACHMENT_PAYLOAD_LABEL: &str = "GetAttachmentPayload";

impl SignedStruct<GetAttachmentUrlPayload, ClientKeyType> for GetAttachmentUrlRequest {
    fn from_payload(payload: GetAttachmentUrlPayload, signature: ClientSignature) -> Self {
        Self {
            payload: Some(payload),
            signature: Some(signature.into()),
        }
    }
}

impl Signable for GetAttachmentUrlPayload {
    type SignedOutput = GetAttachmentUrlRequest;

    fn unsigned_payload(&self) -> Result<Vec<u8>, tls_codec::Error> {
        Ok(self.encode_to_vec())
    }

    fn label(&self) -> &str {
        GET_ATTACHMENT_PAYLOAD_LABEL
    }
}

impl VerifiedStruct<GetAttachmentUrlRequest> for GetAttachmentUrlPayload {
    type SealingType = private_mod::Seal;

    fn from_verifiable(verifiable: GetAttachmentUrlRequest, _seal: Self::SealingType) -> Self {
        verifiable.payload.unwrap()
    }
}

impl Verifiable for GetAttachmentUrlRequest {
    fn unsigned_payload(&self) -> Result<Vec<u8>, tls_codec::Error> {
        Ok(self
            .payload
            .as_ref()
            .ok_or(MissingPayloadError)?
            .encode_to_vec())
    }

    fn signature(&self) -> impl AsRef<[u8]> {
        self.signature
            .as_ref()
            .map(|s| s.value.as_slice())
            .unwrap_or_default()
    }

    fn label(&self) -> &str {
        GET_ATTACHMENT_PAYLOAD_LABEL
    }
}

struct MissingPayloadError;

impl From<MissingPayloadError> for tls_codec::Error {
    fn from(_: MissingPayloadError) -> Self {
        tls_codec::Error::EncodingError("missing payload".to_owned())
    }
}

mod private_mod {
    #[derive(Default)]
    pub struct Seal;
}
