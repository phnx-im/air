// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::{
    credentials::{self, keys},
    crypto::{self, Labeled, hash},
    identifiers,
    messages::{
        self,
        client_as::{self},
    },
};
use displaydoc::Display;
use openmls::prelude::HpkeCiphertext;
use thiserror::Error;
use tonic::Status;

use crate::{
    common::{
        convert::{ExpirationDataError, InvalidIndexedCiphertext},
        v1::Signature,
    },
    convert::TryRefInto,
    validation::{MissingFieldError, MissingFieldExt},
};

use super::v1::{
    AirProtocolVersion, AsCredential, AsCredentialBody, AsIntermediateCredential,
    AsIntermediateCredentialBody, AsIntermediateCredentialCsr, AsIntermediateCredentialPayload,
    AsIntermediateVerifyingKey, AsVerifyingKey, ClientCredential, ClientCredentialCsr,
    ClientCredentialPayload, ClientVerifyingKey, ConnectionEncryptionKey, ConnectionOfferMessage,
    ConnectionPackage, ConnectionPackagePayload, EncryptedUserProfile, HandleSignature,
    HandleVerifyingKey, Hash, SignatureScheme, UserHandleHash, UserId,
};

impl From<identifiers::UserId> for UserId {
    fn from(value: identifiers::UserId) -> Self {
        let (uuid, domain) = value.into_parts();
        Self {
            uuid: Some(uuid.into()),
            domain: Some(domain.into()),
        }
    }
}

impl TryFrom<UserId> for identifiers::UserId {
    type Error = UserIdError;

    fn try_from(proto: UserId) -> Result<Self, Self::Error> {
        Ok(Self::new(
            proto.uuid.ok_or_missing_field("user_id")?.into(),
            proto.domain.ok_or_missing_field("domain")?.try_ref_into()?,
        ))
    }
}

#[derive(Debug, thiserror::Error)]
pub enum UserIdError {
    #[error(transparent)]
    MissingField(#[from] MissingFieldError<&'static str>),
    #[error(transparent)]
    Fqdn(#[from] identifiers::FqdnError),
}

impl From<UserIdError> for Status {
    fn from(e: UserIdError) -> Self {
        Status::invalid_argument(format!("invalid user id: {e}"))
    }
}

impl From<credentials::ClientCredentialCsr> for ClientCredentialCsr {
    fn from(value: credentials::ClientCredentialCsr) -> Self {
        Self {
            msl_version: value.version as u32,
            user_id: Some(value.user_id.into()),
            signature_scheme: value.signature_scheme as i32,
            verifying_key: Some(value.verifying_key.into()),
        }
    }
}

impl TryFrom<ClientCredentialCsr> for credentials::ClientCredentialCsr {
    type Error = ClientCredentialCsrError;

    fn try_from(proto: ClientCredentialCsr) -> Result<Self, Self::Error> {
        let version = match proto.msl_version {
            0 => messages::AirProtocolVersion::Alpha,
            version => return Err(ClientCredentialCsrError::UnexpectedMlsVersion(version)),
        };
        let signature_scheme = SignatureScheme::try_from(proto.signature_scheme)
            .map_err(|_| UnsupportedSignatureScheme)?
            .try_into()?;

        Ok(Self {
            version,
            user_id: proto.user_id.ok_or_missing_field("user_id")?.try_into()?,
            signature_scheme,
            verifying_key: proto
                .verifying_key
                .ok_or_missing_field("verifying_key")?
                .into(),
        })
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ClientCredentialCsrError {
    #[error("unexpected MLS version: {0}")]
    UnexpectedMlsVersion(u32),
    #[error(transparent)]
    Field(#[from] MissingFieldError<&'static str>),
    #[error(transparent)]
    Signature(#[from] UnsupportedSignatureScheme),
    #[error(transparent)]
    ClientId(#[from] UserIdError),
}

#[derive(Debug, thiserror::Error)]
#[error("unsupported MLS version: {0}")]
pub struct UnsupportedMlsVersion(u32);

impl TryFrom<SignatureScheme> for openmls::prelude::SignatureScheme {
    type Error = UnsupportedSignatureScheme;

    fn try_from(value: SignatureScheme) -> Result<Self, Self::Error> {
        use openmls::prelude::SignatureScheme::*;
        match value {
            SignatureScheme::Unspecified => Err(UnsupportedSignatureScheme),
            SignatureScheme::EcdsaSecp256r1Sha256 => Ok(ECDSA_SECP256R1_SHA256),
            SignatureScheme::EcdsaSecp384r1Sha384 => Ok(ECDSA_SECP384R1_SHA384),
            SignatureScheme::EcdsaSecp521r1Sha512 => Ok(ECDSA_SECP521R1_SHA512),
            SignatureScheme::Ed25519 => Ok(ED25519),
            SignatureScheme::Ed448 => Ok(ED448),
        }
    }
}

impl From<openmls::prelude::SignatureScheme> for SignatureScheme {
    fn from(value: openmls::prelude::SignatureScheme) -> Self {
        use openmls::prelude::SignatureScheme::*;
        match value {
            ECDSA_SECP256R1_SHA256 => SignatureScheme::EcdsaSecp256r1Sha256,
            ECDSA_SECP384R1_SHA384 => SignatureScheme::EcdsaSecp384r1Sha384,
            ECDSA_SECP521R1_SHA512 => SignatureScheme::EcdsaSecp521r1Sha512,
            ED25519 => SignatureScheme::Ed25519,
            ED448 => SignatureScheme::Ed448,
        }
    }
}

#[derive(Debug, thiserror::Error)]
#[error("unsupported signature scheme")]
pub struct UnsupportedSignatureScheme;

impl From<credentials::keys::ClientVerifyingKey> for ClientVerifyingKey {
    fn from(value: credentials::keys::ClientVerifyingKey) -> Self {
        Self {
            bytes: value.into_bytes(),
        }
    }
}

impl From<ClientVerifyingKey> for credentials::keys::ClientVerifyingKey {
    fn from(proto: ClientVerifyingKey) -> Self {
        Self::from_bytes(proto.bytes)
    }
}

impl From<messages::AirProtocolVersion> for AirProtocolVersion {
    fn from(value: messages::AirProtocolVersion) -> Self {
        Self {
            version: value as u32,
        }
    }
}

impl TryFrom<AirProtocolVersion> for messages::AirProtocolVersion {
    type Error = UnsupportedMlsVersion;

    fn try_from(value: AirProtocolVersion) -> Result<Self, Self::Error> {
        match value.version {
            0 => Ok(messages::AirProtocolVersion::Alpha),
            _ => Err(UnsupportedMlsVersion(value.version)),
        }
    }
}

impl<T: Labeled> From<hash::Hash<T>> for Hash {
    fn from(value: hash::Hash<T>) -> Self {
        Self {
            bytes: value.into_bytes().to_vec(),
        }
    }
}

const HASH_SIZE: usize = hash::HASH_SIZE;

#[derive(Debug, thiserror::Error)]
pub enum HashError {
    #[error("Invalid hash length: expected {HASH_SIZE}, got {got}")]
    InvalidHashLength { got: usize },
}

impl<T: Labeled> TryFrom<Hash> for hash::Hash<T> {
    type Error = HashError;

    fn try_from(proto: Hash) -> Result<Self, Self::Error> {
        let hash_bytes = <[u8; HASH_SIZE]>::try_from(proto.bytes.as_slice()).map_err(|_| {
            HashError::InvalidHashLength {
                got: proto.bytes.len(),
            }
        })?;
        Ok(hash::Hash::from_bytes(hash_bytes))
    }
}

impl TryFrom<ClientCredential> for credentials::VerifiableClientCredential {
    type Error = ClientCredentialError;

    fn try_from(proto: ClientCredential) -> Result<Self, Self::Error> {
        let payload = proto.payload.ok_or_missing_field("payload")?.try_into()?;
        let signature = proto.signature.ok_or_missing_field("signature")?.into();
        Ok(Self::new(payload, signature))
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ClientCredentialError {
    #[error(transparent)]
    MissingField(#[from] MissingFieldError<&'static str>),
    #[error(transparent)]
    Payload(#[from] ClientCredentialPayloadError),
}

impl From<credentials::ClientCredential> for ClientCredential {
    fn from(value: credentials::ClientCredential) -> Self {
        let (payload, signature) = value.into_parts();
        Self {
            payload: Some(payload.into()),
            signature: Some(signature.into()),
        }
    }
}

impl From<credentials::ClientCredentialPayload> for ClientCredentialPayload {
    fn from(value: credentials::ClientCredentialPayload) -> Self {
        Self {
            csr: Some(value.csr.into()),
            expiration_data: Some(value.expiration_data.into()),
            credential_fingerprint: Some(value.signer_fingerprint.into()),
        }
    }
}

impl TryFrom<ClientCredentialPayload> for credentials::ClientCredentialPayload {
    type Error = ClientCredentialPayloadError;

    fn try_from(proto: ClientCredentialPayload) -> Result<Self, Self::Error> {
        let csr = proto.csr.ok_or_missing_field("csr")?.try_into()?;
        let expiration_data = proto.expiration_data.map(TryFrom::try_from).transpose()?;
        let signer_fingerprint = proto
            .credential_fingerprint
            .ok_or_missing_field("credential_fingerprint")?
            .try_into()?;
        Ok(credentials::ClientCredentialPayload::new(
            csr,
            expiration_data,
            signer_fingerprint,
        ))
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ClientCredentialPayloadError {
    #[error(transparent)]
    MissingField(#[from] MissingFieldError<&'static str>),
    #[error(transparent)]
    Csr(#[from] ClientCredentialCsrError),
    #[error(transparent)]
    ExpirationData(#[from] ExpirationDataError),
    #[error("Invalid credential fingerprint: {0}")]
    CredentialFingerprint(#[from] HashError),
}

impl From<ClientCredentialPayloadError> for Status {
    fn from(e: ClientCredentialPayloadError) -> Self {
        Status::invalid_argument(format!("invalid client payload: {e}"))
    }
}

impl From<messages::connection_package_v1::ConnectionPackageV1> for ConnectionPackage {
    fn from(value: messages::connection_package_v1::ConnectionPackageV1) -> Self {
        let (payload, signature) = value.into_parts();
        Self {
            payload: Some(payload.into()),
            signature: Some(signature.into()),
        }
    }
}

impl From<messages::connection_package::ConnectionPackage> for ConnectionPackage {
    fn from(value: messages::connection_package::ConnectionPackage) -> Self {
        let (payload, signature) = value.into_parts();
        Self {
            payload: Some(payload.into()),
            signature: Some(signature.into()),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ConnectionPackageError {
    #[error(transparent)]
    MissingField(#[from] MissingFieldError<&'static str>),
    #[error(transparent)]
    Csr(#[from] ClientCredentialCsrError),
    #[error(transparent)]
    ExpirationData(#[from] ExpirationDataError),
    #[error("Invalid credential fingerprint: {0}")]
    CredentialFingerprint(#[from] HashError),
    #[error(transparent)]
    UserHandleHash(#[from] UserHandleHashError),
    #[error(transparent)]
    Version(#[from] UnsupportedMlsVersion),
}

impl From<ConnectionPackageError> for Status {
    fn from(error: ConnectionPackageError) -> Self {
        Status::invalid_argument(format!("invalid connection package: {error}"))
    }
}

impl From<messages::connection_package::ConnectionPackagePayload> for ConnectionPackagePayload {
    fn from(value: messages::connection_package::ConnectionPackagePayload) -> Self {
        Self {
            protocol_version: Some(value.protocol_version.into()),
            encryption_key: Some(value.encryption_key.into()),
            lifetime: Some(value.lifetime.into()),
            verifying_key: Some(value.verifying_key.into()),
            user_handle_hash: Some(value.user_handle_hash.into()),
            is_last_resort: Some(value.is_last_resort.0),
        }
    }
}

impl From<messages::connection_package_v1::ConnectionPackageV1Payload>
    for ConnectionPackagePayload
{
    fn from(value: messages::connection_package_v1::ConnectionPackageV1Payload) -> Self {
        Self {
            protocol_version: Some(value.protocol_version.into()),
            encryption_key: Some(value.encryption_key.into()),
            lifetime: Some(value.lifetime.into()),
            verifying_key: Some(value.verifying_key.into()),
            user_handle_hash: Some(value.user_handle_hash.into()),
            is_last_resort: None,
        }
    }
}

impl From<messages::connection_package::VersionedConnectionPackage> for ConnectionPackage {
    fn from(value: messages::connection_package::VersionedConnectionPackage) -> Self {
        match value {
            messages::connection_package::VersionedConnectionPackage::V1(cp_v1) => {
                ConnectionPackage::from(cp_v1)
            }
            messages::connection_package::VersionedConnectionPackage::V2(cp_v2) => {
                ConnectionPackage::from(cp_v2)
            }
        }
    }
}

impl TryFrom<ConnectionPackage> for messages::connection_package::VersionedConnectionPackageIn {
    type Error = ConnectionPackageError;

    fn try_from(proto: ConnectionPackage) -> Result<Self, Self::Error> {
        let payload: ConnectionPackagePayload = proto.payload.ok_or_missing_field("payload")?;
        let protocol_version = payload
            .protocol_version
            .ok_or_missing_field("protocol_version")?
            .try_into()?;
        let encryption_key = payload
            .encryption_key
            .ok_or_missing_field("encryption_key")?
            .into();
        let lifetime = payload
            .lifetime
            .ok_or_missing_field("lifetime")?
            .try_into()?;
        let verifying_key = payload
            .verifying_key
            .ok_or_missing_field("verifying_key")?
            .into();
        let user_handle_hash = payload
            .user_handle_hash
            .ok_or_missing_field("user_handle_hash")?
            .try_into()?;
        let is_last_resort = payload.is_last_resort.map(|b| b.into());
        let signature = proto.signature.ok_or_missing_field("signature")?.into();
        let result = if let Some(is_last_resort) = is_last_resort {
            let payload = messages::connection_package::ConnectionPackagePayload {
                protocol_version,
                encryption_key,
                lifetime,
                verifying_key,
                user_handle_hash,
                is_last_resort,
            };
            Self::V2(messages::connection_package::ConnectionPackageIn::new(
                payload, signature,
            ))
        } else {
            let payload = messages::connection_package_v1::ConnectionPackageV1Payload {
                protocol_version,
                encryption_key,
                lifetime,
                verifying_key,
                user_handle_hash,
            };
            Self::V1(messages::connection_package_v1::ConnectionPackageV1In::new(
                payload, signature,
            ))
        };
        Ok(result)
    }
}

impl From<crypto::ConnectionEncryptionKey> for ConnectionEncryptionKey {
    fn from(value: crypto::ConnectionEncryptionKey) -> Self {
        Self {
            bytes: value.into_bytes(),
        }
    }
}

impl From<ConnectionEncryptionKey> for crypto::ConnectionEncryptionKey {
    fn from(proto: ConnectionEncryptionKey) -> Self {
        Self::from_bytes(proto.bytes)
    }
}

impl From<messages::client_as_out::EncryptedUserProfile> for EncryptedUserProfile {
    fn from(value: messages::client_as_out::EncryptedUserProfile) -> Self {
        Self {
            ciphertext: Some(value.into()),
        }
    }
}

impl TryFrom<EncryptedUserProfile> for messages::client_as_out::EncryptedUserProfile {
    type Error = InvalidIndexedCiphertext;

    fn try_from(proto: EncryptedUserProfile) -> Result<Self, Self::Error> {
        proto.ciphertext.unwrap_or_default().try_into()
    }
}

impl From<credentials::AsCredential> for AsCredential {
    fn from(value: credentials::AsCredential) -> Self {
        let (body, fingerprint) = value.into_parts();
        Self {
            body: Some(body.into()),
            fingerprint: Some(fingerprint.into()),
        }
    }
}

impl TryFrom<AsCredential> for credentials::AsCredential {
    type Error = AsCredentialError;

    fn try_from(proto: AsCredential) -> Result<Self, Self::Error> {
        Ok(Self::from_parts(
            proto.body.ok_or_missing_field("body")?.try_into()?,
            proto
                .fingerprint
                .ok_or_missing_field("fingerprint")?
                .try_into()?,
        ))
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AsCredentialError {
    #[error(transparent)]
    MissingField(#[from] MissingFieldError<&'static str>),
    #[error(transparent)]
    Body(#[from] AsCredentialBodyError),
    #[error("Invalid fingerprint: {0}")]
    Fingerprint(#[from] HashError),
}

impl From<credentials::AsCredentialBody> for AsCredentialBody {
    fn from(value: credentials::AsCredentialBody) -> Self {
        let signature_scheme: SignatureScheme = value.signature_scheme.into();
        Self {
            version: Some(value.version.into()),
            user_domain: Some(value.user_domain.into()),
            expiration_data: Some(value.expiration_data.into()),
            signature_scheme: signature_scheme.into(),
            verifying_key: Some(value.verifying_key.into()),
        }
    }
}

impl TryFrom<AsCredentialBody> for credentials::AsCredentialBody {
    type Error = AsCredentialBodyError;

    fn try_from(proto: AsCredentialBody) -> Result<Self, Self::Error> {
        let signature_scheme = SignatureScheme::try_from(proto.signature_scheme)
            .map_err(|_| UnsupportedSignatureScheme)?
            .try_into()?;
        Ok(Self {
            version: proto.version.ok_or_missing_field("version")?.try_into()?,
            user_domain: proto
                .user_domain
                .ok_or_missing_field("user_domain")?
                .try_ref_into()?,
            expiration_data: proto
                .expiration_data
                .ok_or_missing_field("expiration_data")?
                .try_into()?,
            signature_scheme,
            verifying_key: proto
                .verifying_key
                .ok_or_missing_field("verifying_key")?
                .into(),
        })
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AsCredentialBodyError {
    #[error(transparent)]
    MissingField(#[from] MissingFieldError<&'static str>),
    #[error(transparent)]
    Version(#[from] UnsupportedMlsVersion),
    #[error(transparent)]
    ExpirationData(#[from] ExpirationDataError),
    #[error(transparent)]
    SignatureScheme(#[from] UnsupportedSignatureScheme),
    #[error(transparent)]
    Fqdn(#[from] identifiers::FqdnError),
}

impl From<credentials::keys::AsVerifyingKey> for AsVerifyingKey {
    fn from(value: credentials::keys::AsVerifyingKey) -> Self {
        Self {
            bytes: value.into_bytes(),
        }
    }
}

impl From<AsVerifyingKey> for credentials::keys::AsVerifyingKey {
    fn from(proto: AsVerifyingKey) -> Self {
        Self::from_bytes(proto.bytes)
    }
}

impl From<credentials::AsIntermediateCredential> for AsIntermediateCredential {
    fn from(value: credentials::AsIntermediateCredential) -> Self {
        let (body, fingerpint) = value.into_parts();
        Self {
            body: Some(body.into()),
            fingerprint: Some(fingerpint.into()),
        }
    }
}

impl TryFrom<AsIntermediateCredential> for credentials::VerifiableAsIntermediateCredential {
    type Error = AsIntermediateCredentialError;

    fn try_from(proto: AsIntermediateCredential) -> Result<Self, Self::Error> {
        let body = proto.body.ok_or_missing_field("body")?;
        Ok(Self::from_parts(
            body.credential
                .ok_or_missing_field("credential")?
                .try_into()?,
            body.signature.ok_or_missing_field("signature")?.into(),
        ))
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AsIntermediateCredentialError {
    #[error(transparent)]
    MissingField(#[from] MissingFieldError<&'static str>),
    #[error(transparent)]
    Payload(#[from] AsIntermediateCredentialPayloadError),
}

impl From<credentials::AsIntermediateCredentialBody> for AsIntermediateCredentialBody {
    fn from(value: credentials::AsIntermediateCredentialBody) -> Self {
        let (credential, signature) = value.into_parts();
        Self {
            credential: Some(credential.into()),
            signature: Some(signature.into()),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AsIntermediateCredentialBodyError {
    #[error(transparent)]
    MissingField(#[from] MissingFieldError<&'static str>),
    #[error(transparent)]
    Credential(#[from] AsIntermediateCredentialPayloadError),
}

impl From<credentials::AsIntermediateCredentialPayload> for AsIntermediateCredentialPayload {
    fn from(value: credentials::AsIntermediateCredentialPayload) -> Self {
        Self {
            csr: Some(value.csr.into()),
            expiration_data: Some(value.expiration_data.into()),
            signer_fingerprint: Some(value.signer_fingerprint.into()),
        }
    }
}

impl TryFrom<AsIntermediateCredentialPayload> for credentials::AsIntermediateCredentialPayload {
    type Error = AsIntermediateCredentialPayloadError;

    fn try_from(proto: AsIntermediateCredentialPayload) -> Result<Self, Self::Error> {
        let csr = proto.csr.ok_or_missing_field("csr")?.try_into()?;
        let expiration_data = proto
            .expiration_data
            .ok_or_missing_field("expiration_data")?
            .try_into()?;
        let signer_fingerprint = proto
            .signer_fingerprint
            .ok_or_missing_field("signer_fingerprint")?
            .try_into()?;
        Ok(Self {
            csr,
            expiration_data,
            signer_fingerprint,
        })
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AsIntermediateCredentialPayloadError {
    #[error(transparent)]
    MissingField(#[from] MissingFieldError<&'static str>),
    #[error(transparent)]
    Csr(#[from] AsIntermediateCredentialCsrError),
    #[error(transparent)]
    ExpirationData(#[from] ExpirationDataError),
    #[error("Invalid signer fingerprint: {0}")]
    SignerFingerprint(#[from] HashError),
}

impl From<credentials::AsIntermediateCredentialCsr> for AsIntermediateCredentialCsr {
    fn from(value: credentials::AsIntermediateCredentialCsr) -> Self {
        Self {
            version: Some(value.version.into()),
            user_domain: Some(value.user_domain.into()),
            signature_scheme: SignatureScheme::from(value.signature_scheme).into(),
            verifying_key: Some(value.verifying_key.into()),
        }
    }
}

impl TryFrom<AsIntermediateCredentialCsr> for credentials::AsIntermediateCredentialCsr {
    type Error = AsIntermediateCredentialCsrError;

    fn try_from(proto: AsIntermediateCredentialCsr) -> Result<Self, Self::Error> {
        let version = proto.version.ok_or_missing_field("version")?.try_into()?;
        let signature_scheme = SignatureScheme::try_from(proto.signature_scheme)
            .map_err(|_| UnsupportedSignatureScheme)?
            .try_into()?;
        Ok(Self {
            version,
            user_domain: proto
                .user_domain
                .ok_or_missing_field("user_domain")?
                .try_ref_into()?,
            signature_scheme,
            verifying_key: proto
                .verifying_key
                .ok_or_missing_field("verifying_key")?
                .into(),
        })
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AsIntermediateCredentialCsrError {
    #[error(transparent)]
    MissingField(#[from] MissingFieldError<&'static str>),
    #[error(transparent)]
    Version(#[from] UnsupportedMlsVersion),
    #[error(transparent)]
    Fqdn(#[from] identifiers::FqdnError),
    #[error(transparent)]
    SignatureScheme(#[from] UnsupportedSignatureScheme),
}

impl From<credentials::keys::AsIntermediateVerifyingKey> for AsIntermediateVerifyingKey {
    fn from(value: credentials::keys::AsIntermediateVerifyingKey) -> Self {
        Self {
            bytes: value.into_bytes(),
        }
    }
}

impl From<AsIntermediateVerifyingKey> for credentials::keys::AsIntermediateVerifyingKey {
    fn from(proto: AsIntermediateVerifyingKey) -> Self {
        Self::from_bytes(proto.bytes)
    }
}

impl From<client_as::ConnectionOfferMessage> for ConnectionOfferMessage {
    fn from(value: client_as::ConnectionOfferMessage) -> Self {
        let (ciphertext, connection_package_hash) = value.into_parts();
        let ciphertext: HpkeCiphertext = ciphertext.as_ref().clone();
        Self {
            ciphertext: Some(ciphertext.into()),
            connection_package_hash: Some(connection_package_hash.into()),
        }
    }
}

#[derive(Debug, thiserror::Error, Display)]
pub enum ConnectionOfferMessageError {
    /// Missing field
    MissingField(#[from] MissingFieldError<&'static str>),
    /// Invalid connection package hash
    InvalidConnectionPackageHash(#[from] HashError),
}

impl TryFrom<ConnectionOfferMessage> for client_as::ConnectionOfferMessage {
    type Error = ConnectionOfferMessageError;

    fn try_from(proto: ConnectionOfferMessage) -> Result<Self, Self::Error> {
        let ciphertext: HpkeCiphertext = proto.ciphertext.ok_or_missing_field("ciphertext")?.into();
        let connection_package_hash = proto
            .connection_package_hash
            .ok_or_missing_field("hash")?
            .try_into()?;
        Ok(Self::new(connection_package_hash, ciphertext.into()))
    }
}

impl From<HandleVerifyingKey> for keys::HandleVerifyingKey {
    fn from(proto: HandleVerifyingKey) -> Self {
        Self::from_bytes(proto.bytes)
    }
}

impl From<keys::HandleVerifyingKey> for HandleVerifyingKey {
    fn from(value: keys::HandleVerifyingKey) -> Self {
        Self {
            bytes: value.into_bytes(),
        }
    }
}

impl TryFrom<UserHandleHash> for identifiers::UserHandleHash {
    type Error = UserHandleHashError;

    fn try_from(proto: UserHandleHash) -> Result<Self, Self::Error> {
        Ok(Self::new(
            proto
                .bytes
                .try_into()
                .map_err(|_| UserHandleHashError::InvalidLength)?,
        ))
    }
}

impl From<identifiers::UserHandleHash> for UserHandleHash {
    fn from(value: identifiers::UserHandleHash) -> Self {
        Self {
            bytes: value.into_bytes().to_vec(),
        }
    }
}

#[derive(Debug, Error, Display)]
pub enum UserHandleHashError {
    /// Invalid hash length
    InvalidLength,
}

impl From<UserHandleHashError> for Status {
    fn from(error: UserHandleHashError) -> Self {
        let msg = error.to_string();
        match error {
            UserHandleHashError::InvalidLength => Status::invalid_argument(msg),
        }
    }
}

impl From<keys::HandleSignature> for HandleSignature {
    fn from(value: keys::HandleSignature) -> Self {
        Self {
            signature: Some(Signature {
                value: value.into_bytes(),
            }),
        }
    }
}

impl From<HandleSignature> for keys::HandleSignature {
    fn from(proto: HandleSignature) -> Self {
        keys::HandleSignature::from_bytes(proto.signature.unwrap_or_default().value)
    }
}
