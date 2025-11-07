// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::{
    credentials::keys::{self, ClientKeyType, ClientSignature, HandleKeyType},
    crypto::signatures::signable::{Signable, SignedStruct, Verifiable, VerifiedStruct},
};
use prost::Message;

use crate::auth_service::v1::{ReportSpamPayload, ReportSpamRequest};

use super::v1::{
    CreateHandlePayload, CreateHandleRequest, DeleteHandlePayload, DeleteHandleRequest,
    DeleteUserPayload, DeleteUserRequest, HandleSignature, InitListenHandlePayload,
    InitListenHandleRequest, IssueTokensPayload, IssueTokensRequest, MergeUserProfilePayload,
    MergeUserProfileRequest, PublishConnectionPackagesPayload, PublishConnectionPackagesRequest,
    RefreshHandlePayload, RefreshHandleRequest, StageUserProfilePayload, StageUserProfileRequest,
};

const DELETE_USER_PAYLOAD_LABEL: &str = "DeleteUserPayload";

impl SignedStruct<DeleteUserPayload, ClientKeyType> for DeleteUserRequest {
    fn from_payload(payload: DeleteUserPayload, signature: ClientSignature) -> Self {
        Self {
            payload: Some(payload),
            signature: Some(signature.into()),
        }
    }
}

impl Signable for DeleteUserPayload {
    type SignedOutput = DeleteUserRequest;

    fn unsigned_payload(&self) -> Result<Vec<u8>, tls_codec::Error> {
        Ok(self.encode_to_vec())
    }

    fn label(&self) -> &str {
        DELETE_USER_PAYLOAD_LABEL
    }
}

impl VerifiedStruct<DeleteUserRequest> for DeleteUserPayload {
    type SealingType = private_mod::Seal;

    fn from_verifiable(verifiable: DeleteUserRequest, _seal: Self::SealingType) -> Self {
        verifiable.payload.unwrap()
    }
}

impl Verifiable for DeleteUserRequest {
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
        DELETE_USER_PAYLOAD_LABEL
    }
}

const PUBLISH_CONNECTION_PACKAGES_PAYLOAD_LABEL: &str = "PublishConnectionPackagesPayload";

impl SignedStruct<PublishConnectionPackagesPayload, HandleKeyType>
    for PublishConnectionPackagesRequest
{
    fn from_payload(
        payload: PublishConnectionPackagesPayload,
        signature: keys::HandleSignature,
    ) -> Self {
        let signature_proto: HandleSignature = signature.into();
        Self {
            payload: Some(payload),
            signature: signature_proto.signature,
        }
    }
}

impl Signable for PublishConnectionPackagesPayload {
    type SignedOutput = PublishConnectionPackagesRequest;

    fn unsigned_payload(&self) -> Result<Vec<u8>, tls_codec::Error> {
        Ok(self.encode_to_vec())
    }

    fn label(&self) -> &str {
        PUBLISH_CONNECTION_PACKAGES_PAYLOAD_LABEL
    }
}

impl VerifiedStruct<PublishConnectionPackagesRequest> for PublishConnectionPackagesPayload {
    type SealingType = private_mod::Seal;

    fn from_verifiable(
        verifiable: PublishConnectionPackagesRequest,
        _seal: Self::SealingType,
    ) -> Self {
        verifiable.payload.unwrap()
    }
}

impl Verifiable for PublishConnectionPackagesRequest {
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
        PUBLISH_CONNECTION_PACKAGES_PAYLOAD_LABEL
    }
}

struct MissingPayloadError;

impl From<MissingPayloadError> for tls_codec::Error {
    fn from(_: MissingPayloadError) -> Self {
        tls_codec::Error::EncodingError("missing payload".to_owned())
    }
}

const STAGE_USER_PROFILE_PAYLOAD_LABEL: &str = "StageUserProfilePayload";

impl SignedStruct<StageUserProfilePayload, ClientKeyType> for StageUserProfileRequest {
    fn from_payload(payload: StageUserProfilePayload, signature: ClientSignature) -> Self {
        Self {
            payload: Some(payload),
            signature: Some(signature.into()),
        }
    }
}

impl Signable for StageUserProfilePayload {
    type SignedOutput = StageUserProfileRequest;

    fn unsigned_payload(&self) -> Result<Vec<u8>, tls_codec::Error> {
        Ok(self.encode_to_vec())
    }

    fn label(&self) -> &str {
        STAGE_USER_PROFILE_PAYLOAD_LABEL
    }
}

impl VerifiedStruct<StageUserProfileRequest> for StageUserProfilePayload {
    type SealingType = private_mod::Seal;

    fn from_verifiable(verifiable: StageUserProfileRequest, _seal: Self::SealingType) -> Self {
        verifiable.payload.unwrap()
    }
}

impl Verifiable for StageUserProfileRequest {
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
        STAGE_USER_PROFILE_PAYLOAD_LABEL
    }
}

const MERGE_USER_PROFILE_PAYLOAD_LABEL: &str = "MergeUserProfilePayload";

impl SignedStruct<MergeUserProfilePayload, ClientKeyType> for MergeUserProfileRequest {
    fn from_payload(payload: MergeUserProfilePayload, signature: ClientSignature) -> Self {
        Self {
            payload: Some(payload),
            signature: Some(signature.into()),
        }
    }
}

impl Signable for MergeUserProfilePayload {
    type SignedOutput = MergeUserProfileRequest;

    fn unsigned_payload(&self) -> Result<Vec<u8>, tls_codec::Error> {
        Ok(self.encode_to_vec())
    }

    fn label(&self) -> &str {
        MERGE_USER_PROFILE_PAYLOAD_LABEL
    }
}

impl VerifiedStruct<MergeUserProfileRequest> for MergeUserProfilePayload {
    type SealingType = private_mod::Seal;

    fn from_verifiable(verifiable: MergeUserProfileRequest, _seal: Self::SealingType) -> Self {
        verifiable.payload.unwrap()
    }
}

impl Verifiable for MergeUserProfileRequest {
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
        MERGE_USER_PROFILE_PAYLOAD_LABEL
    }
}

const ISSUE_TOKENS_PAYLOAD_LABEL: &str = "IssueTokensPayload";

impl SignedStruct<IssueTokensPayload, ClientKeyType> for IssueTokensRequest {
    fn from_payload(payload: IssueTokensPayload, signature: ClientSignature) -> Self {
        IssueTokensRequest {
            payload: Some(payload),
            signature: Some(signature.into()),
        }
    }
}

impl Signable for IssueTokensPayload {
    type SignedOutput = IssueTokensRequest;

    fn unsigned_payload(&self) -> Result<Vec<u8>, tls_codec::Error> {
        Ok(self.encode_to_vec())
    }

    fn label(&self) -> &str {
        ISSUE_TOKENS_PAYLOAD_LABEL
    }
}

impl VerifiedStruct<IssueTokensRequest> for IssueTokensPayload {
    type SealingType = private_mod::Seal;

    fn from_verifiable(verifiable: IssueTokensRequest, _seal: Self::SealingType) -> Self {
        verifiable.payload.unwrap()
    }
}

impl Verifiable for IssueTokensRequest {
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
        ISSUE_TOKENS_PAYLOAD_LABEL
    }
}

const REPORT_SPAM_PAYLOAD_LABEL: &str = "ReportSpamPayload";

impl SignedStruct<ReportSpamPayload, ClientKeyType> for ReportSpamRequest {
    fn from_payload(payload: ReportSpamPayload, signature: ClientSignature) -> Self {
        ReportSpamRequest {
            payload: Some(payload),
            signature: Some(signature.into()),
        }
    }
}

impl Signable for ReportSpamPayload {
    type SignedOutput = ReportSpamRequest;

    fn unsigned_payload(&self) -> Result<Vec<u8>, tls_codec::Error> {
        Ok(self.encode_to_vec())
    }

    fn label(&self) -> &str {
        REPORT_SPAM_PAYLOAD_LABEL
    }
}

impl VerifiedStruct<ReportSpamRequest> for ReportSpamPayload {
    type SealingType = private_mod::Seal;

    fn from_verifiable(verifiable: ReportSpamRequest, _seal: Self::SealingType) -> Self {
        verifiable.payload.unwrap()
    }
}

impl Verifiable for ReportSpamRequest {
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
        REPORT_SPAM_PAYLOAD_LABEL
    }
}

const CREATE_HANDLE_PAYLOAD_LABEL: &str = "CreateHandlePayload";

impl SignedStruct<CreateHandlePayload, keys::HandleKeyType> for CreateHandleRequest {
    fn from_payload(payload: CreateHandlePayload, signature: keys::HandleSignature) -> Self {
        CreateHandleRequest {
            payload: Some(payload),
            signature: Some(signature.into()),
        }
    }
}

impl Signable for CreateHandlePayload {
    type SignedOutput = CreateHandleRequest;

    fn unsigned_payload(&self) -> Result<Vec<u8>, tls_codec::Error> {
        Ok(self.encode_to_vec())
    }

    fn label(&self) -> &str {
        CREATE_HANDLE_PAYLOAD_LABEL
    }
}

impl VerifiedStruct<CreateHandleRequest> for CreateHandlePayload {
    type SealingType = private_mod::Seal;

    fn from_verifiable(verifiable: CreateHandleRequest, _seal: Self::SealingType) -> Self {
        verifiable.payload.unwrap()
    }
}

impl Verifiable for CreateHandleRequest {
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
            .and_then(|s| s.signature.as_ref())
            .map(|s| s.value.as_slice())
            .unwrap_or_default()
    }

    fn label(&self) -> &str {
        CREATE_HANDLE_PAYLOAD_LABEL
    }
}

const DELETE_HANDLE_PAYLOAD_LABEL: &str = "DeleteHandlePayload";

impl SignedStruct<DeleteHandlePayload, keys::HandleKeyType> for DeleteHandleRequest {
    fn from_payload(payload: DeleteHandlePayload, signature: keys::HandleSignature) -> Self {
        DeleteHandleRequest {
            payload: Some(payload),
            signature: Some(signature.into()),
        }
    }
}

impl Signable for DeleteHandlePayload {
    type SignedOutput = DeleteHandleRequest;

    fn unsigned_payload(&self) -> Result<Vec<u8>, tls_codec::Error> {
        Ok(self.encode_to_vec())
    }

    fn label(&self) -> &str {
        DELETE_HANDLE_PAYLOAD_LABEL
    }
}

impl VerifiedStruct<DeleteHandleRequest> for DeleteHandlePayload {
    type SealingType = private_mod::Seal;

    fn from_verifiable(verifiable: DeleteHandleRequest, _seal: Self::SealingType) -> Self {
        verifiable.payload.unwrap()
    }
}

impl Verifiable for DeleteHandleRequest {
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
            .and_then(|s| s.signature.as_ref())
            .map(|s| s.value.as_slice())
            .unwrap_or_default()
    }

    fn label(&self) -> &str {
        DELETE_HANDLE_PAYLOAD_LABEL
    }
}

const REFRESH_HANDLE_PAYLOAD_LABEL: &str = "RefreshHandlePayload";

impl SignedStruct<RefreshHandlePayload, keys::HandleKeyType> for RefreshHandleRequest {
    fn from_payload(payload: RefreshHandlePayload, signature: keys::HandleSignature) -> Self {
        RefreshHandleRequest {
            payload: Some(payload),
            signature: Some(signature.into()),
        }
    }
}

impl Signable for RefreshHandlePayload {
    type SignedOutput = RefreshHandleRequest;

    fn unsigned_payload(&self) -> Result<Vec<u8>, tls_codec::Error> {
        Ok(self.encode_to_vec())
    }

    fn label(&self) -> &str {
        REFRESH_HANDLE_PAYLOAD_LABEL
    }
}

impl VerifiedStruct<RefreshHandleRequest> for RefreshHandlePayload {
    type SealingType = private_mod::Seal;

    fn from_verifiable(verifiable: RefreshHandleRequest, _seal: Self::SealingType) -> Self {
        verifiable.payload.unwrap()
    }
}

impl Verifiable for RefreshHandleRequest {
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
            .and_then(|s| s.signature.as_ref())
            .map(|s| s.value.as_slice())
            .unwrap_or_default()
    }

    fn label(&self) -> &str {
        REFRESH_HANDLE_PAYLOAD_LABEL
    }
}

const INIT_LISTEN_HANDLE_REQUEST_LABEL: &str = "InitListenHandleRequest";

impl SignedStruct<InitListenHandlePayload, keys::HandleKeyType> for InitListenHandleRequest {
    fn from_payload(payload: InitListenHandlePayload, signature: keys::HandleSignature) -> Self {
        InitListenHandleRequest {
            payload: Some(payload),
            signature: Some(signature.into()),
        }
    }
}

impl Signable for InitListenHandlePayload {
    type SignedOutput = InitListenHandleRequest;

    fn unsigned_payload(&self) -> Result<Vec<u8>, tls_codec::Error> {
        Ok(self.encode_to_vec())
    }

    fn label(&self) -> &str {
        INIT_LISTEN_HANDLE_REQUEST_LABEL
    }
}

impl VerifiedStruct<InitListenHandleRequest> for InitListenHandlePayload {
    type SealingType = private_mod::Seal;

    fn from_verifiable(verifiable: InitListenHandleRequest, _seal: Self::SealingType) -> Self {
        verifiable.payload.unwrap()
    }
}

impl Verifiable for InitListenHandleRequest {
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
            .and_then(|s| s.signature.as_ref())
            .map(|s| s.value.as_slice())
            .unwrap_or_default()
    }

    fn label(&self) -> &str {
        INIT_LISTEN_HANDLE_REQUEST_LABEL
    }
}

mod private_mod {
    #[derive(Default)]
    pub struct Seal;
}
