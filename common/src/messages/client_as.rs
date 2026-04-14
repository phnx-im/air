// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use mls_assist::openmls_traits::types::HpkeCiphertext;

use tls_codec::{TlsDeserializeBytes, TlsSerialize, TlsSize};

use crate::{
    credentials::{
        AsCredential, AsCredentialBody, AsIntermediateCredential, ClientCredential,
        ClientCredentialPayload,
    },
    crypto::{
        Labeled, RatchetEncryptionKey,
        aead::Ciphertext,
        hash::{Hash, Hashable},
        kdf::keys::RatchetSecret,
    },
    messages::connection_package::ConnectionPackageHash,
};

use super::client_as_out::EncryptedUserProfile;

// === User ===

#[derive(Debug)]
pub struct RegisterUserParams {
    pub client_payload: ClientCredentialPayload,
    pub queue_encryption_key: RatchetEncryptionKey,
    pub initial_ratchet_secret: RatchetSecret,
    pub encrypted_user_profile: EncryptedUserProfile,
}

#[derive(Debug)]
pub struct RegisterUserResponse {
    pub client_credential: ClientCredential,
}

// === Client ===

#[derive(Debug)]
pub struct EncryptedFriendshipPackageCtype;
pub type EncryptedFriendshipPackage = Ciphertext<EncryptedFriendshipPackageCtype>;

impl Labeled for EncryptedConnectionOffer {
    const LABEL: &'static str = "EncryptedConnectionOffer";
}

impl Hashable for EncryptedConnectionOffer {}

pub type ConnectionOfferHash = Hash<EncryptedConnectionOffer>;

#[derive(Debug, TlsDeserializeBytes, TlsSerialize, TlsSize)]
pub struct EncryptedConnectionOffer {
    ciphertext: HpkeCiphertext,
}

#[derive(Debug, TlsDeserializeBytes, TlsSerialize, TlsSize)]
pub struct ConnectionOfferMessage {
    connection_package_hash: ConnectionPackageHash,
    ciphertext: EncryptedConnectionOffer,
}

impl ConnectionOfferMessage {
    pub fn new(
        connection_package_hash: ConnectionPackageHash,
        ciphertext: EncryptedConnectionOffer,
    ) -> Self {
        Self {
            connection_package_hash,
            ciphertext,
        }
    }

    pub fn connection_offer_hash(&self) -> ConnectionOfferHash {
        self.ciphertext.hash()
    }

    pub fn into_parts(self) -> (EncryptedConnectionOffer, ConnectionPackageHash) {
        (self.ciphertext, self.connection_package_hash)
    }
}

impl From<HpkeCiphertext> for EncryptedConnectionOffer {
    fn from(ciphertext: HpkeCiphertext) -> Self {
        Self { ciphertext }
    }
}

impl AsRef<HpkeCiphertext> for EncryptedConnectionOffer {
    fn as_ref(&self) -> &HpkeCiphertext {
        &self.ciphertext
    }
}

// === Privacy Pass ===

/// TLS-serialized `AmortizedToken` for token redemption.
#[derive(Debug, Clone)]
pub struct SerializedToken(Vec<u8>);

impl SerializedToken {
    pub fn new(bytes: Vec<u8>) -> Self {
        Self(bytes)
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.0
    }
}

/// TLS-serialized `AmortizedBatchTokenRequest` for token issuance.
#[derive(Debug, Clone)]
pub struct SerializedTokenRequest(Vec<u8>);

impl SerializedTokenRequest {
    pub fn new(bytes: Vec<u8>) -> Self {
        Self(bytes)
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.0
    }
}

/// TLS-serialized `AmortizedBatchTokenResponse` from the server.
#[derive(Debug, Clone)]
pub struct SerializedTokenResponse(Vec<u8>);

impl SerializedTokenResponse {
    pub fn new(bytes: Vec<u8>) -> Self {
        Self(bytes)
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.0
    }
}

// === Anonymous requests ===

#[derive(Debug)]
pub struct AsCredentialsParams {}

/// A VOPRF public key for Privacy Pass token issuance.
#[derive(Debug)]
pub struct BatchedTokenKeyResponse {
    pub token_key_id: u8,
    pub public_key: Vec<u8>,
}

#[derive(Debug)]
pub struct AsCredentialsResponse {
    pub as_credentials: Vec<AsCredential>,
    pub as_intermediate_credentials: Vec<AsIntermediateCredential>,
    pub revoked_credentials: Vec<Hash<AsCredentialBody>>,
    pub batched_token_keys: Vec<BatchedTokenKeyResponse>,
}
