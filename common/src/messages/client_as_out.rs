// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use tls_codec::{TlsDeserializeBytes, TlsSerialize, TlsSize};

use crate::{
    credentials::{
        AsCredential, AsCredentialBody, VerifiableAsIntermediateCredential,
        VerifiableUserCredential,
    },
    crypto::{
        hash::Hash,
        indexed_aead::{ciphertexts::IndexedCiphertext, keys::UserProfileKeyType},
    },
    messages::connection_package_v1::ConnectionPackageV1In,
};

#[derive(Debug)]
pub struct UserConnectionPackagesResponse {
    pub connection_packages: Vec<ConnectionPackageV1In>,
}

use super::client_as::BatchedTokenKeyResponse;

#[derive(Debug)]
pub struct AsCredentialsResponseIn {
    // TODO: We might want a Verifiable... type variant here that ensures that
    // this is matched against the local trust store or something.
    pub as_credentials: Vec<AsCredential>,
    pub as_intermediate_credentials: Vec<VerifiableAsIntermediateCredential>,
    pub revoked_credentials: Vec<Hash<AsCredentialBody>>,
    pub batched_token_keys: Vec<BatchedTokenKeyResponse>,
}

#[derive(Debug)]
pub struct RegisterUserResponseIn {
    pub user_credential: VerifiableUserCredential,
}

#[derive(Debug)]
pub struct EncryptedUserProfileCtype;
pub type EncryptedUserProfile = IndexedCiphertext<UserProfileKeyType, EncryptedUserProfileCtype>;

#[derive(Debug, TlsSerialize, TlsDeserializeBytes, TlsSize)]
pub struct GetUserProfileResponse {
    pub encrypted_user_profile: EncryptedUserProfile,
}

#[derive(Debug)]
pub enum UsernameDeleteResponse {
    Success,
    NotFound,
}
