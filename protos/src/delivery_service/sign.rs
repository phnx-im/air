// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use prost::Message;

use crate::delivery_service::v1::{
    GetAttachmentPayload, GetAttachmentRequest, ProvisionAttachmentPayload,
    ProvisionAttachmentRequest,
};

use super::v1::{
    CreateGroupPayload, CreateGroupRequest, DeleteGroupPayload, DeleteGroupRequest,
    GroupOperationPayload, GroupOperationRequest, ResyncPayload, ResyncRequest, SelfRemovePayload,
    SelfRemoveRequest, SendMessagePayload, SendMessageRequest, UpdatePayload,
    UpdateProfileKeyPayload, UpdateProfileKeyRequest, UpdateRequest, WelcomeInfoPayload,
    WelcomeInfoRequest,
};

use phnxcommon::{
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

impl Verifiable for SendMessageRequest {
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

const UPDATE_PAYLOAD_LABEL: &str = "UpdatePayload";

impl SignedStruct<UpdatePayload, ClientKeyType> for UpdateRequest {
    fn from_payload(payload: UpdatePayload, signature: ClientSignature) -> Self {
        Self {
            payload: Some(payload),
            signature: Some(signature.into()),
        }
    }
}

impl Signable for UpdatePayload {
    type SignedOutput = UpdateRequest;

    fn unsigned_payload(&self) -> Result<Vec<u8>, tls_codec::Error> {
        Ok(self.encode_to_vec())
    }

    fn label(&self) -> &str {
        UPDATE_PAYLOAD_LABEL
    }
}

impl VerifiedStruct<UpdateRequest> for UpdatePayload {
    type SealingType = private_mod::Seal;

    fn from_verifiable(verifiable: UpdateRequest, _seal: Self::SealingType) -> Self {
        verifiable.payload.unwrap()
    }
}

impl Verifiable for UpdateRequest {
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
        UPDATE_PAYLOAD_LABEL
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

impl SignedStruct<GetAttachmentPayload, ClientKeyType> for GetAttachmentRequest {
    fn from_payload(payload: GetAttachmentPayload, signature: ClientSignature) -> Self {
        Self {
            payload: Some(payload),
            signature: Some(signature.into()),
        }
    }
}

impl Signable for GetAttachmentPayload {
    type SignedOutput = GetAttachmentRequest;

    fn unsigned_payload(&self) -> Result<Vec<u8>, tls_codec::Error> {
        Ok(self.encode_to_vec())
    }

    fn label(&self) -> &str {
        GET_ATTACHMENT_PAYLOAD_LABEL
    }
}

impl VerifiedStruct<GetAttachmentRequest> for GetAttachmentPayload {
    type SealingType = private_mod::Seal;

    fn from_verifiable(verifiable: GetAttachmentRequest, _seal: Self::SealingType) -> Self {
        verifiable.payload.unwrap()
    }
}

impl Verifiable for GetAttachmentRequest {
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
