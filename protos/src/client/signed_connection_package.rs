// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::io;

use aircommon::{
    LibraryError,
    credentials::keys::{
        UsernameKeyType, UsernameSignature, UsernameSigningKey, UsernameVerifyingKey,
    },
    crypto::{
        ConnectionDecryptionKey, ConnectionEncryptionKey, Labeled,
        errors::RandomnessError,
        hash::Hashable,
        signatures::{
            private_keys::SignatureVerificationError,
            signable::{Signable, SignedStruct, Verifiable, VerifiedStruct},
        },
    },
    identifiers::UsernameHash,
    messages::{
        connection_package::{
            ConnectionPackage, ConnectionPackageHash, ConnectionPackageMetadata,
            VersionedConnectionPackageIn,
        },
        connection_package_v1::CONNECTION_PACKAGE_EXPIRATION,
    },
    time::{ExpirationData, TimeStamp},
};
use prost::Message;
use thiserror::Error;
use tls_codec::{Serialize, Size};
use tonic::Status;
use tracing::error;

use crate::{
    auth_service::{
        convert::UsernameHashError,
        v1::{SignedConnectionPackage, SignedConnectionPackagePayload},
    },
    client::component::AirFeatures,
    common::convert::ExpirationDataError,
    validation::{MissingFieldError, MissingFieldExt},
};

impl SignedConnectionPackage {
    /// Generates a new signed connection package with a random encryption key.
    ///
    /// The connection package has a default lifetime as defined in
    /// [`CONNECTION_PACKAGE_EXPIRATION`].
    ///
    /// The connection package has attached Air features as defined in
    /// [`AirFeatures::default_leaf_or_key_package_features`].
    pub fn generate(
        user_handle_hash: UsernameHash,
        signing_key: &UsernameSigningKey,
        is_last_resort: bool,
    ) -> Result<
        (ConnectionDecryptionKey, Self, ConnectionPackageMetadata),
        SignedConnectionPackageError,
    > {
        let decryption_key = ConnectionDecryptionKey::generate()?;

        let lifetime = ExpirationData::new(CONNECTION_PACKAGE_EXPIRATION);

        let payload = SignedConnectionPackagePayload {
            encryption_key: Some(decryption_key.encryption_key().clone().into()),
            lifetime: Some(lifetime.clone().into()),
            verifying_key: Some(signing_key.verifying_key().clone().into()),
            username_hash: Some(user_handle_hash.into()),
            is_last_resort,
            air_features: Some(AirFeatures::default_leaf_or_key_package_features().into()),
        };

        let payload_bytes = payload.encode_to_vec();
        let payload_tbs = SignedConnectionPackageTbs { payload_bytes };
        let hash = ConnectionPackageHash::from_bytes(payload_tbs.hash().into_bytes());
        let package = payload_tbs.sign(signing_key)?;

        let metadata = ConnectionPackageMetadata {
            hash,
            lifetime,
            is_last_resort,
        };

        Ok((decryption_key, package, metadata))
    }
}

#[derive(Debug, Error)]
pub enum SignedConnectionPackageError {
    #[error(transparent)]
    LibraryError(#[from] LibraryError),
    #[error("Error generating decryption key: {0}")]
    DecryptionKeyError(#[from] RandomnessError),
}

#[derive(Debug)]
struct SignedConnectionPackageTbs {
    payload_bytes: Vec<u8>,
}

// Needed for labeled hash
impl Size for &SignedConnectionPackageTbs {
    fn tls_serialized_len(&self) -> usize {
        self.payload_bytes.len()
    }
}

// Needed for labeled hash
impl Serialize for &SignedConnectionPackageTbs {
    fn tls_serialize<W: io::Write>(&self, writer: &mut W) -> Result<usize, tls_codec::Error> {
        writer.write_all(&self.payload_bytes)?;
        Ok(self.payload_bytes.len())
    }
}

const LABEL: &str = "SignedConnectionPackage";

impl Labeled for SignedConnectionPackageTbs {
    const LABEL: &'static str = LABEL;
}

impl Hashable for SignedConnectionPackageTbs {}

impl Signable for SignedConnectionPackageTbs {
    type SignedOutput = SignedConnectionPackage;

    fn unsigned_payload(&self) -> Result<Vec<u8>, tls_codec::Error> {
        Ok(self.payload_bytes.clone())
    }

    fn label(&self) -> &str {
        LABEL
    }
}

impl SignedStruct<SignedConnectionPackageTbs, UsernameKeyType> for SignedConnectionPackage {
    fn from_payload(tbs: SignedConnectionPackageTbs, signature: UsernameSignature) -> Self {
        Self {
            payload: tbs.payload_bytes,
            signature: Some(signature.into()),
        }
    }
}

/// Payload which was decoded from the protobuf message.
#[derive(Debug)]
struct DecodedPayload {
    encryption_key: ConnectionEncryptionKey,
    lifetime: ExpirationData,
    verifying_key: UsernameVerifyingKey,
    username_hash: UsernameHash,
    is_last_resort: bool,
    air_features: AirFeatures,
}

#[derive(Debug)]
pub struct SignedConnectionPackageIn {
    payload: DecodedPayload,
    tbs: SignedConnectionPackageTbs,
    signature: UsernameSignature,
}

impl SignedConnectionPackageIn {
    /// Addressable hash of the connection package.
    pub fn hash(&self) -> ConnectionPackageHash {
        ConnectionPackageHash::from_bytes(self.tbs.hash().into_bytes())
    }

    pub fn payload_bytes(&self) -> &[u8] {
        self.tbs.payload_bytes.as_slice()
    }
}

impl TryFrom<SignedConnectionPackage> for SignedConnectionPackageIn {
    type Error = SignedConnectionPackageInError;

    fn try_from(v1: SignedConnectionPackage) -> Result<Self, Self::Error> {
        let SignedConnectionPackagePayload {
            encryption_key,
            lifetime,
            verifying_key,
            username_hash,
            is_last_resort,
            air_features,
        } = SignedConnectionPackagePayload::decode(v1.payload.as_slice())?;
        let payload = DecodedPayload {
            encryption_key: encryption_key.ok_or_missing_field("encryption_key")?.into(),
            lifetime: lifetime.ok_or_missing_field("lifetime")?.try_into()?,
            verifying_key: verifying_key.ok_or_missing_field("verifying_key")?.into(),
            username_hash: username_hash
                .ok_or_missing_field("username_hash")?
                .try_into()?,
            is_last_resort,
            air_features: air_features.ok_or_missing_field("air_features")?.into(),
        };
        Ok(Self {
            payload,
            tbs: SignedConnectionPackageTbs {
                payload_bytes: v1.payload,
            },
            signature: v1.signature.ok_or_missing_field("signature")?.into(),
        })
    }
}

#[derive(Debug, Error)]
pub enum SignedConnectionPackageInError {
    #[error(transparent)]
    DecodeError(#[from] prost::DecodeError),
    #[error(transparent)]
    MissingField(#[from] MissingFieldError<&'static str>),
    #[error(transparent)]
    Expiration(#[from] ExpirationDataError),
    #[error(transparent)]
    Username(#[from] UsernameHashError),
}

impl From<SignedConnectionPackageInError> for Status {
    fn from(value: SignedConnectionPackageInError) -> Self {
        match value {
            error @ SignedConnectionPackageInError::MissingField(_) => {
                Status::invalid_argument(error.to_string())
            }
            SignedConnectionPackageInError::DecodeError(error) => {
                Status::invalid_argument(error.to_string())
            }
            SignedConnectionPackageInError::Expiration(error) => {
                error!(%error, "invalid expiration data");
                Status::invalid_argument("Invalid expiration data")
            }
            SignedConnectionPackageInError::Username(error) => {
                error!(%error, "invalid username hash");
                Status::invalid_argument("Invalid username hash")
            }
        }
    }
}

impl Verifiable for SignedConnectionPackageIn {
    fn unsigned_payload(&self) -> Result<Vec<u8>, tls_codec::Error> {
        Ok(self.tbs.payload_bytes.clone())
    }

    fn signature(&self) -> impl AsRef<[u8]> {
        self.signature.as_ref()
    }

    fn label(&self) -> &str {
        LABEL
    }
}

impl VerifiedStruct<SignedConnectionPackageIn> for VerifiedSignedConnectionPackage {
    type SealingType = private_mod::Seal;

    fn from_verifiable(verifiable: SignedConnectionPackageIn, _seal: Self::SealingType) -> Self {
        Self {
            payload: verifiable.payload,
            tbs: verifiable.tbs,
            signature: verifiable.signature,
        }
    }
}

mod private_mod {
    #[derive(Default)]
    pub struct Seal;
}

/// A signed connection package whose signature has been verified.
pub struct VerifiedSignedConnectionPackage {
    payload: DecodedPayload,
    tbs: SignedConnectionPackageTbs,
    signature: UsernameSignature,
}

impl VerifiedSignedConnectionPackage {
    pub fn payload_bytes(&self) -> &[u8] {
        self.tbs.payload_bytes.as_slice()
    }

    /// Splits the package into (payload_bytes, signature, is_last_resort).
    pub fn into_parts(self) -> (Vec<u8>, UsernameSignature, bool) {
        (
            self.tbs.payload_bytes,
            self.signature,
            self.payload.is_last_resort,
        )
    }

    pub fn signature(&self) -> &UsernameSignature {
        &self.signature
    }

    pub fn hash(&self) -> ConnectionPackageHash {
        ConnectionPackageHash::from_bytes(self.tbs.hash().into_bytes())
    }

    pub fn encryption_key(&self) -> &ConnectionEncryptionKey {
        &self.payload.encryption_key
    }

    pub fn expires_at(&self) -> TimeStamp {
        self.payload.lifetime.not_after()
    }

    pub fn is_last_resort(&self) -> bool {
        self.payload.is_last_resort
    }

    pub fn username_hash(&self) -> &UsernameHash {
        &self.payload.username_hash
    }

    pub fn verifying_key(&self) -> &UsernameVerifyingKey {
        &self.payload.verifying_key
    }

    pub fn air_features(&self) -> &AirFeatures {
        &self.payload.air_features
    }
}

pub enum AnyConnectionPackageIn {
    Legacy(VersionedConnectionPackageIn),
    Signed(SignedConnectionPackageIn),
}

pub enum AnyConnectionPackage {
    Legacy(ConnectionPackage),
    Signed(VerifiedSignedConnectionPackage),
}

impl AnyConnectionPackageIn {
    pub fn verify(
        self,
        expected_hash: &UsernameHash,
    ) -> Result<AnyConnectionPackage, ConnectionPackageVerificationError> {
        match self {
            AnyConnectionPackageIn::Legacy(package) => {
                let verified = package.verify()?;
                if verified.username_hash() != expected_hash {
                    return Err(ConnectionPackageVerificationError::UsernameHashMismatch);
                }
                Ok(AnyConnectionPackage::Legacy(verified.into_current()))
            }
            AnyConnectionPackageIn::Signed(package) => {
                if package.payload.username_hash != *expected_hash {
                    return Err(ConnectionPackageVerificationError::UsernameHashMismatch);
                }
                let verifying_key = package.payload.verifying_key.clone();
                let verified: VerifiedSignedConnectionPackage = package.verify(&verifying_key)?;
                Ok(AnyConnectionPackage::Signed(verified))
            }
        }
    }
}

#[derive(Debug, Error)]
pub enum ConnectionPackageVerificationError {
    #[error(transparent)]
    Signature(#[from] SignatureVerificationError),
    #[error("Username hash does not match the requested username")]
    UsernameHashMismatch,
}

impl AnyConnectionPackage {
    pub fn hash(&self) -> ConnectionPackageHash {
        match self {
            AnyConnectionPackage::Legacy(cp) => cp.hash(),
            AnyConnectionPackage::Signed(cp) => cp.hash(),
        }
    }

    pub fn encryption_key(&self) -> &ConnectionEncryptionKey {
        match self {
            AnyConnectionPackage::Legacy(cp) => cp.encryption_key(),
            AnyConnectionPackage::Signed(cp) => cp.encryption_key(),
        }
    }

    pub fn expires_at(&self) -> TimeStamp {
        match self {
            AnyConnectionPackage::Legacy(cp) => cp.expires_at(),
            AnyConnectionPackage::Signed(cp) => cp.expires_at(),
        }
    }

    pub fn is_last_resort(&self) -> bool {
        match self {
            AnyConnectionPackage::Legacy(cp) => cp.is_last_resort(),
            AnyConnectionPackage::Signed(cp) => cp.is_last_resort(),
        }
    }

    pub fn air_features(&self) -> Option<&AirFeatures> {
        match self {
            AnyConnectionPackage::Legacy(_) => None,
            AnyConnectionPackage::Signed(cp) => Some(cp.air_features()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aircommon::identifiers::Username;

    fn username_hash(name: &str) -> UsernameHash {
        Username::new(name.to_owned())
            .unwrap()
            .calculate_hash()
            .unwrap()
    }

    #[test]
    fn generate_roundtrip() {
        let signing_key = UsernameSigningKey::generate().unwrap();
        let hash = username_hash("alice");

        let (_decryption_key, package, metadata) =
            SignedConnectionPackage::generate(hash, &signing_key, true).unwrap();
        let payload_bytes = package.payload.clone();

        let package_in = SignedConnectionPackageIn::try_from(package).unwrap();
        assert_eq!(package_in.hash(), metadata.hash);

        let verified: VerifiedSignedConnectionPackage =
            package_in.verify(signing_key.verifying_key()).unwrap();
        assert_eq!(verified.hash(), metadata.hash);
        assert_eq!(verified.username_hash(), &hash);
        assert_eq!(
            verified.verifying_key().as_slice(),
            signing_key.verifying_key().as_slice()
        );
        assert!(verified.is_last_resort());
        assert_eq!(verified.expires_at(), metadata.lifetime.not_after());
        assert_eq!(verified.payload_bytes(), payload_bytes.as_slice());
    }

    #[test]
    fn verify_fails_with_other_key() {
        let signing_key = UsernameSigningKey::generate().unwrap();
        let other_key = UsernameSigningKey::generate().unwrap();
        let hash = username_hash("alice");

        let (_decryption_key, package, _metadata) =
            SignedConnectionPackage::generate(hash, &signing_key, false).unwrap();
        let package_in = SignedConnectionPackageIn::try_from(package).unwrap();

        let result: Result<VerifiedSignedConnectionPackage, _> =
            package_in.verify(other_key.verifying_key());
        assert!(matches!(
            result,
            Err(SignatureVerificationError::VerificationFailure)
        ));
    }

    #[test]
    fn verify_fails_on_tampered_payload() {
        let signing_key = UsernameSigningKey::generate().unwrap();
        let hash = username_hash("alice");

        let (_decryption_key, mut package, metadata) =
            SignedConnectionPackage::generate(hash, &signing_key, false).unwrap();
        // The last byte falls inside the varint-encoded `air_features` field
        // and flipping it breaks decoding. `encryption_key` is a nested
        // message wrapping a raw `bytes` field, so its first four bytes are
        // tag/length framing (both outer and inner); byte 4 onward is raw key
        // content, safe to flip without breaking protobuf framing.
        package.payload[4] ^= 0xff;

        let package_in = SignedConnectionPackageIn::try_from(package).unwrap();
        assert_ne!(package_in.hash(), metadata.hash);

        let result: Result<VerifiedSignedConnectionPackage, _> =
            package_in.verify(signing_key.verifying_key());
        assert!(result.is_err());
    }

    #[test]
    fn try_from_rejects_missing_signature() {
        let signing_key = UsernameSigningKey::generate().unwrap();
        let hash = username_hash("alice");

        let (_decryption_key, mut package, _metadata) =
            SignedConnectionPackage::generate(hash, &signing_key, false).unwrap();
        package.signature = None;

        let error = SignedConnectionPackageIn::try_from(package).unwrap_err();
        assert!(matches!(
            error,
            SignedConnectionPackageInError::MissingField(_)
        ));
    }

    #[test]
    fn any_verify_signed_binds_username_hash() {
        let signing_key = UsernameSigningKey::generate().unwrap();
        let hash_a = username_hash("alice");
        let hash_b = username_hash("bobby");

        let (_decryption_key, package, _metadata) =
            SignedConnectionPackage::generate(hash_a, &signing_key, false).unwrap();
        let package_in = SignedConnectionPackageIn::try_from(package).unwrap();
        let any_in = AnyConnectionPackageIn::Signed(package_in);

        let Err(error) = any_in.verify(&hash_b) else {
            panic!("verification should fail on username hash mismatch");
        };
        assert!(matches!(
            error,
            ConnectionPackageVerificationError::UsernameHashMismatch
        ));

        let (_decryption_key, package, metadata) =
            SignedConnectionPackage::generate(hash_a, &signing_key, false).unwrap();
        let package_in = SignedConnectionPackageIn::try_from(package).unwrap();
        let any_in = AnyConnectionPackageIn::Signed(package_in);

        let any = any_in.verify(&hash_a).unwrap();
        assert!(any.air_features().is_some());
        assert_eq!(any.hash(), metadata.hash);
        assert!(matches!(any, AnyConnectionPackage::Signed(_)));
    }

    #[test]
    fn any_verify_legacy_binds_username_hash() {
        let signing_key = UsernameSigningKey::generate().unwrap();
        let hash_a = username_hash("alice");
        let hash_b = username_hash("bobby");

        let (_decryption_key, cp, _metadata) =
            ConnectionPackage::generate(hash_a, &signing_key, false).unwrap();
        let proto = crate::auth_service::v1::ConnectionPackage::from(cp);
        let cp_in = VersionedConnectionPackageIn::try_from(proto).unwrap();
        let any_in = AnyConnectionPackageIn::Legacy(cp_in);

        let Err(error) = any_in.verify(&hash_b) else {
            panic!("verification should fail on username hash mismatch");
        };
        assert!(matches!(
            error,
            ConnectionPackageVerificationError::UsernameHashMismatch
        ));

        let (_decryption_key, cp, metadata) =
            ConnectionPackage::generate(hash_a, &signing_key, false).unwrap();
        let proto = crate::auth_service::v1::ConnectionPackage::from(cp);
        let cp_in = VersionedConnectionPackageIn::try_from(proto).unwrap();
        let any_in = AnyConnectionPackageIn::Legacy(cp_in);

        let any = any_in.verify(&hash_a).unwrap();
        assert!(any.air_features().is_none());
        assert_eq!(any.hash(), metadata.hash);
        assert!(matches!(any, AnyConnectionPackage::Legacy(_)));
    }
}
