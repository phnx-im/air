// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use tls_codec::{TlsDeserializeBytes, TlsSerialize, TlsSize};

use crate::{
    credentials::{
        AsCredential, AsCredentialBody, ClientCredentialPayload,
        VerifiableAsIntermediateCredential, VerifiableClientCredential, keys::ClientSignature,
    },
    crypto::{
        hash::Hash,
        indexed_aead::{
            ciphertexts::IndexedCiphertext,
            keys::{UserProfileKeyIndex, UserProfileKeyType},
        },
    },
    identifiers::UserId,
    messages::connection_package_v1::ConnectionPackageV1In,
};

#[derive(Debug)]
pub struct UserConnectionPackagesResponse {
    pub connection_packages: Vec<ConnectionPackageV1In>,
}

#[derive(Debug)]
pub struct AsCredentialsResponseIn {
    // TODO: We might want a Verifiable... type variant here that ensures that
    // this is matched against the local trust store or something.
    pub as_credentials: Vec<AsCredential>,
    pub as_intermediate_credentials: Vec<VerifiableAsIntermediateCredential>,
    pub revoked_credentials: Vec<Hash<AsCredentialBody>>,
}

#[derive(Debug)]
pub struct RegisterUserResponseIn {
    pub client_credential: VerifiableClientCredential,
}

#[derive(Debug)]
pub struct RegisterUserParamsIn {
    pub client_payload: ClientCredentialPayload,
    pub encrypted_user_profile: EncryptedUserProfile,
}

pub struct GetUserProfileParams {
    pub user_id: UserId,
    pub key_index: UserProfileKeyIndex,
}

#[derive(Debug)]
pub struct EncryptedUserProfileCtype;
pub type EncryptedUserProfile = IndexedCiphertext<UserProfileKeyType, EncryptedUserProfileCtype>;

#[derive(Debug, TlsSerialize, TlsDeserializeBytes, TlsSize)]
pub struct GetUserProfileResponse {
    pub encrypted_user_profile: EncryptedUserProfile,
}

#[derive(Debug)]
pub struct UpdateUserProfileParamsTbs {
    pub user_id: UserId,
    pub user_profile: EncryptedUserProfile,
}

#[derive(Debug, TlsSerialize, TlsDeserializeBytes, TlsSize)]
pub struct StageUserProfileParamsTbs {
    pub user_id: UserId,
    pub user_profile: EncryptedUserProfile,
}

#[derive(Debug, TlsSerialize, TlsDeserializeBytes, TlsSize)]
pub struct StageUserProfileParams {
    payload: StageUserProfileParamsTbs,
    signature: ClientSignature,
}

#[derive(Debug, TlsSerialize, TlsDeserializeBytes, TlsSize)]
pub struct MergeUserProfileParamsTbs {
    pub user_id: UserId,
}

#[derive(Debug, TlsSerialize, TlsDeserializeBytes, TlsSize)]
pub struct MergeUserProfileParams {
    payload: MergeUserProfileParamsTbs,
    signature: ClientSignature,
}

#[derive(Debug)]
pub enum UserHandleDeleteResponse {
    Success,
    NotFound,
}
