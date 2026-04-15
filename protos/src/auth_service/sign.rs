// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::{
    credentials::keys::{self, ClientKeyType, ClientSignature, UsernameKeyType},
    crypto::signatures::signable::{Signable, SignedStruct, Verifiable, VerifiedStruct},
};
use prost::Message;

use crate::auth_service::v1::{ReportSpamPayload, ReportSpamRequest};

use super::v1::{
    CreateUsernamePayload, CreateUsernameRequest, DeleteUserPayload, DeleteUserRequest,
    DeleteUsernamePayload, DeleteUsernameRequest, InitListenUsernamePayload,
    InitListenUsernameRequest, IssueTokensPayload, IssueTokensRequest, MergeUserProfilePayload,
    MergeUserProfileRequest, PublishConnectionPackagesPayload, PublishConnectionPackagesRequest,
    RefreshUsernamePayload, RefreshUsernameRequest, StageUserProfilePayload,
    StageUserProfileRequest, UsernameSignature,
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

impl SignedStruct<PublishConnectionPackagesPayload, UsernameKeyType>
    for PublishConnectionPackagesRequest
{
    fn from_payload(
        payload: PublishConnectionPackagesPayload,
        signature: keys::UsernameSignature,
    ) -> Self {
        let signature_proto: UsernameSignature = signature.into();
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

impl SignedStruct<CreateUsernamePayload, keys::UsernameKeyType> for CreateUsernameRequest {
    fn from_payload(payload: CreateUsernamePayload, signature: keys::UsernameSignature) -> Self {
        CreateUsernameRequest {
            payload: Some(payload),
            signature: Some(signature.into()),
        }
    }
}

impl Signable for CreateUsernamePayload {
    type SignedOutput = CreateUsernameRequest;

    fn unsigned_payload(&self) -> Result<Vec<u8>, tls_codec::Error> {
        Ok(self.encode_to_vec())
    }

    fn label(&self) -> &str {
        CREATE_HANDLE_PAYLOAD_LABEL
    }
}

impl VerifiedStruct<CreateUsernameRequest> for CreateUsernamePayload {
    type SealingType = private_mod::Seal;

    fn from_verifiable(verifiable: CreateUsernameRequest, _seal: Self::SealingType) -> Self {
        verifiable.payload.unwrap()
    }
}

impl Verifiable for CreateUsernameRequest {
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

impl SignedStruct<DeleteUsernamePayload, keys::UsernameKeyType> for DeleteUsernameRequest {
    fn from_payload(payload: DeleteUsernamePayload, signature: keys::UsernameSignature) -> Self {
        DeleteUsernameRequest {
            payload: Some(payload),
            signature: Some(signature.into()),
        }
    }
}

impl Signable for DeleteUsernamePayload {
    type SignedOutput = DeleteUsernameRequest;

    fn unsigned_payload(&self) -> Result<Vec<u8>, tls_codec::Error> {
        Ok(self.encode_to_vec())
    }

    fn label(&self) -> &str {
        DELETE_HANDLE_PAYLOAD_LABEL
    }
}

impl VerifiedStruct<DeleteUsernameRequest> for DeleteUsernamePayload {
    type SealingType = private_mod::Seal;

    fn from_verifiable(verifiable: DeleteUsernameRequest, _seal: Self::SealingType) -> Self {
        verifiable.payload.unwrap()
    }
}

impl Verifiable for DeleteUsernameRequest {
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

impl SignedStruct<RefreshUsernamePayload, keys::UsernameKeyType> for RefreshUsernameRequest {
    fn from_payload(payload: RefreshUsernamePayload, signature: keys::UsernameSignature) -> Self {
        RefreshUsernameRequest {
            payload: Some(payload),
            signature: Some(signature.into()),
        }
    }
}

impl Signable for RefreshUsernamePayload {
    type SignedOutput = RefreshUsernameRequest;

    fn unsigned_payload(&self) -> Result<Vec<u8>, tls_codec::Error> {
        Ok(self.encode_to_vec())
    }

    fn label(&self) -> &str {
        REFRESH_HANDLE_PAYLOAD_LABEL
    }
}

impl VerifiedStruct<RefreshUsernameRequest> for RefreshUsernamePayload {
    type SealingType = private_mod::Seal;

    fn from_verifiable(verifiable: RefreshUsernameRequest, _seal: Self::SealingType) -> Self {
        verifiable.payload.unwrap()
    }
}

impl Verifiable for RefreshUsernameRequest {
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

impl SignedStruct<InitListenUsernamePayload, keys::UsernameKeyType> for InitListenUsernameRequest {
    fn from_payload(
        payload: InitListenUsernamePayload,
        signature: keys::UsernameSignature,
    ) -> Self {
        InitListenUsernameRequest {
            payload: Some(payload),
            signature: Some(signature.into()),
        }
    }
}

impl Signable for InitListenUsernamePayload {
    type SignedOutput = InitListenUsernameRequest;

    fn unsigned_payload(&self) -> Result<Vec<u8>, tls_codec::Error> {
        Ok(self.encode_to_vec())
    }

    fn label(&self) -> &str {
        INIT_LISTEN_HANDLE_REQUEST_LABEL
    }
}

impl VerifiedStruct<InitListenUsernameRequest> for InitListenUsernamePayload {
    type SealingType = private_mod::Seal;

    fn from_verifiable(verifiable: InitListenUsernameRequest, _seal: Self::SealingType) -> Self {
        verifiable.payload.unwrap()
    }
}

impl Verifiable for InitListenUsernameRequest {
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
