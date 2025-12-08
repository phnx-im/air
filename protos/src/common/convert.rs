// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::fmt;

use aircommon::{
    credentials::keys::{AsIntermediateSignature, AsSignature, ClientSignature, HandleSignature},
    crypto::{
        self,
        ear::{self, AeadCiphertext},
        indexed_aead,
        kdf::KDF_KEY_SIZE,
        secrets::Secret,
        signatures::signable,
    },
    identifiers, time,
};
use chrono::DateTime;
use itertools::Itertools;
use semver::BuildMetadata;
use tonic::Status;

use crate::{
    common::v1::{ExpirationData, Version},
    convert::{FromRef, TryFromRef, TryRefInto},
    validation::{MissingFieldError, MissingFieldExt},
};

use super::v1::{
    Ciphertext, Fqdn, GroupId, HpkeCiphertext, IndexedCiphertext, QualifiedGroupId,
    RatchetEncryptionKey, RatchetSecret, Signature, Timestamp, Uuid,
};

impl From<uuid::Uuid> for Uuid {
    fn from(value: uuid::Uuid) -> Self {
        let (hi, lo) = value.as_u64_pair();
        Self {
            hi: hi.to_le(),
            lo: lo.to_le(),
        }
    }
}

impl From<Uuid> for uuid::Uuid {
    fn from(value: Uuid) -> Self {
        let (hi, lo) = (u64::from_le(value.hi), u64::from_le(value.lo));
        uuid::Uuid::from_u64_pair(hi, lo)
    }
}

impl TryFromRef<'_, Fqdn> for identifiers::Fqdn {
    type Error = identifiers::FqdnError;

    fn try_from_ref(value: &Fqdn) -> Result<Self, Self::Error> {
        value.value.parse()
    }
}

impl From<identifiers::Fqdn> for Fqdn {
    fn from(value: identifiers::Fqdn) -> Self {
        Fqdn {
            value: value.into(),
        }
    }
}

#[derive(Debug, derive_more::Display)]
pub enum QualifiedGroupIdField {
    #[display(fmt = "group_uuid")]
    GroupUuid,
    #[display(fmt = "domain")]
    Domain,
}

impl TryFromRef<'_, QualifiedGroupId> for identifiers::QualifiedGroupId {
    type Error = QualifiedGroupIdError;

    fn try_from_ref(proto: &QualifiedGroupId) -> Result<Self, Self::Error> {
        use QualifiedGroupIdField::*;
        Ok(identifiers::QualifiedGroupId::new(
            proto.group_uuid.ok_or_missing_field(GroupUuid)?.into(),
            proto
                .domain
                .as_ref()
                .ok_or_missing_field(Domain)?
                .try_ref_into()?,
        ))
    }
}

#[derive(Debug, thiserror::Error)]
pub enum QualifiedGroupIdError {
    #[error(transparent)]
    Field(#[from] MissingFieldError<QualifiedGroupIdField>),
    #[error(transparent)]
    Fqdn(#[from] identifiers::FqdnError),
}

impl From<QualifiedGroupIdError> for Status {
    fn from(e: QualifiedGroupIdError) -> Self {
        Status::invalid_argument(format!("invalid qualified group id: {e}"))
    }
}

impl FromRef<'_, identifiers::QualifiedGroupId> for QualifiedGroupId {
    fn from_ref(value: &identifiers::QualifiedGroupId) -> QualifiedGroupId {
        QualifiedGroupId {
            group_uuid: Some(value.group_uuid().into()),
            domain: Some(value.owning_domain().clone().into()),
        }
    }
}

impl FromRef<'_, GroupId> for openmls::group::GroupId {
    fn from_ref(proto: &GroupId) -> Self {
        Self::from_slice(&proto.value)
    }
}

impl FromRef<'_, openmls::group::GroupId> for GroupId {
    fn from_ref(value: &openmls::group::GroupId) -> GroupId {
        GroupId {
            value: value.to_vec(),
        }
    }
}

impl<CT> From<ear::Ciphertext<CT>> for Ciphertext {
    fn from(value: ear::Ciphertext<CT>) -> Self {
        AeadCiphertext::from(value).into()
    }
}

impl<CT> TryFrom<Ciphertext> for ear::Ciphertext<CT> {
    type Error = InvalidNonceLen;

    fn try_from(proto: Ciphertext) -> Result<Self, Self::Error> {
        Ok(ear::AeadCiphertext::try_from(proto)?.into())
    }
}

impl TryFrom<Ciphertext> for ear::AeadCiphertext {
    type Error = InvalidNonceLen;

    fn try_from(proto: Ciphertext) -> Result<Self, Self::Error> {
        let nonce_len = proto.nonce.len();
        let nonce = proto
            .nonce
            .try_into()
            .map_err(|_| InvalidNonceLen(nonce_len))?;
        Ok(ear::AeadCiphertext::new(proto.ciphertext, nonce))
    }
}

impl From<ear::AeadCiphertext> for Ciphertext {
    fn from(value: ear::AeadCiphertext) -> Self {
        let (ciphertext, nonce) = value.into_parts();
        Self {
            ciphertext,
            nonce: nonce.to_vec(),
        }
    }
}

#[derive(Debug, thiserror::Error)]
#[error("invalid key index length {0}")]
pub struct InvalidKeyIndexLen(usize);

impl From<InvalidKeyIndexLen> for Status {
    fn from(e: InvalidKeyIndexLen) -> Self {
        Status::invalid_argument(format!("invalid key index length: {}", e.0))
    }
}

#[derive(Debug, thiserror::Error)]
pub enum InvalidIndexedCiphertext {
    #[error(transparent)]
    InvalidKeyIndexLen(#[from] InvalidKeyIndexLen),
    #[error(transparent)]
    InvalidNonceLen(#[from] InvalidNonceLen),
    #[error(transparent)]
    MissingField(#[from] MissingFieldError<&'static str>),
}

impl From<InvalidIndexedCiphertext> for Status {
    fn from(e: InvalidIndexedCiphertext) -> Self {
        Status::invalid_argument(format!("invalid indexed ciphertext: {e}"))
    }
}

impl<KT: indexed_aead::keys::RawIndex, CT> TryFrom<IndexedCiphertext>
    for indexed_aead::ciphertexts::IndexedCiphertext<KT, CT>
{
    type Error = InvalidIndexedCiphertext;

    fn try_from(proto: IndexedCiphertext) -> Result<Self, Self::Error> {
        let len = proto.key_index.len();
        let ciphertext = proto
            .ciphertext
            .ok_or_missing_field("ciphertext")?
            .try_into()?;
        let secret = proto
            .key_index
            .try_into()
            .map_err(|_| InvalidKeyIndexLen(len))?;
        let key_index = indexed_aead::keys::Index::<KT>::from_bytes(secret);
        Ok(Self::from_parts(key_index, ciphertext))
    }
}

impl<KT: indexed_aead::keys::RawIndex, CT>
    From<indexed_aead::ciphertexts::IndexedCiphertext<KT, CT>> for IndexedCiphertext
{
    fn from(value: indexed_aead::ciphertexts::IndexedCiphertext<KT, CT>) -> Self {
        let (key_index, ciphertext) = value.into_parts();
        Self {
            key_index: key_index.into_bytes().to_vec(),
            ciphertext: Some(ciphertext.into()),
        }
    }
}

#[derive(Debug, thiserror::Error)]
#[error("invalid ciphertext nonce length {0}")]
pub struct InvalidNonceLen(usize);

impl From<InvalidNonceLen> for Status {
    fn from(e: InvalidNonceLen) -> Self {
        Status::invalid_argument(format!("invalid ciphertext nonce length: {}", e.0))
    }
}

impl From<time::TimeStamp> for Timestamp {
    fn from(value: time::TimeStamp) -> Self {
        let seconds = value.as_ref().timestamp();
        let nanos = value
            .as_ref()
            .timestamp_subsec_nanos()
            .try_into()
            .unwrap_or_default();
        Self { seconds, nanos }
    }
}

impl From<Timestamp> for time::TimeStamp {
    fn from(value: Timestamp) -> Self {
        DateTime::from_timestamp(value.seconds, value.nanos.try_into().unwrap_or_default())
            .unwrap_or_default()
            .into()
    }
}

impl From<ClientSignature> for Signature {
    fn from(value: ClientSignature) -> Self {
        Self {
            value: value.into_bytes(),
        }
    }
}

impl From<Signature> for ClientSignature {
    fn from(value: Signature) -> Self {
        signable::Signature::from_bytes(value.value)
    }
}

impl From<HandleSignature> for Signature {
    fn from(value: HandleSignature) -> Self {
        Self {
            value: value.into_bytes(),
        }
    }
}

impl From<Signature> for HandleSignature {
    fn from(value: Signature) -> Self {
        signable::Signature::from_bytes(value.value)
    }
}

impl From<AsIntermediateSignature> for Signature {
    fn from(value: AsIntermediateSignature) -> Self {
        Self {
            value: value.into_bytes(),
        }
    }
}

impl From<Signature> for AsIntermediateSignature {
    fn from(value: Signature) -> Self {
        signable::Signature::from_bytes(value.value)
    }
}

impl From<AsSignature> for Signature {
    fn from(value: AsSignature) -> Self {
        Self {
            value: value.into_bytes(),
        }
    }
}

impl From<Signature> for AsSignature {
    fn from(value: Signature) -> Self {
        signable::Signature::from_bytes(value.value)
    }
}

impl From<RatchetEncryptionKey> for crypto::RatchetEncryptionKey {
    fn from(proto: RatchetEncryptionKey) -> Self {
        Self::from_bytes(proto.bytes)
    }
}

impl From<crypto::RatchetEncryptionKey> for RatchetEncryptionKey {
    fn from(value: crypto::RatchetEncryptionKey) -> Self {
        Self {
            bytes: value.into_bytes(),
        }
    }
}

impl TryFrom<RatchetSecret> for crypto::kdf::keys::RatchetSecret {
    type Error = RatchetSecretError;

    fn try_from(proto: RatchetSecret) -> Result<Self, Self::Error> {
        let len = proto.bytes.len();
        let secret: [u8; KDF_KEY_SIZE] = proto
            .bytes
            .try_into()
            .map_err(|_| RatchetSecretError::InvalidSecretLength(len))?;
        Ok(Self::from(Secret::from(secret)))
    }
}

#[derive(Debug, thiserror::Error)]
pub enum RatchetSecretError {
    #[error("invalid secret length: expected {KDF_KEY_SIZE}, got {0}")]
    InvalidSecretLength(usize),
}

impl From<RatchetSecretError> for Status {
    fn from(e: RatchetSecretError) -> Self {
        Status::invalid_argument(format!("invalid ratchet secret: {e}"))
    }
}

impl From<crypto::kdf::keys::RatchetSecret> for RatchetSecret {
    fn from(value: crypto::kdf::keys::RatchetSecret) -> Self {
        Self {
            bytes: value.as_ref().secret().to_vec(),
        }
    }
}

impl From<openmls::prelude::HpkeCiphertext> for HpkeCiphertext {
    fn from(value: openmls::prelude::HpkeCiphertext) -> Self {
        Self {
            kem_output: value.kem_output.into(),
            ciphertext: value.ciphertext.into(),
        }
    }
}

impl From<HpkeCiphertext> for openmls::prelude::HpkeCiphertext {
    fn from(proto: HpkeCiphertext) -> Self {
        Self {
            kem_output: proto.kem_output.into(),
            ciphertext: proto.ciphertext.into(),
        }
    }
}

impl TryFrom<ExpirationData> for time::ExpirationData {
    type Error = ExpirationDataError;

    fn try_from(value: ExpirationData) -> Result<Self, Self::Error> {
        Ok(Self::from_parts(
            value.not_before.ok_or_missing_field("not_before")?.into(),
            value.not_after.ok_or_missing_field("not_after")?.into(),
        ))
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ExpirationDataError {
    #[error(transparent)]
    MissingField(#[from] MissingFieldError<&'static str>),
}

impl From<time::ExpirationData> for ExpirationData {
    fn from(value: time::ExpirationData) -> Self {
        Self {
            not_before: Some(value.not_before().into()),
            not_after: Some(value.not_after().into()),
        }
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)?;
        if !self.pre.is_empty() {
            write!(f, "-{}", self.pre)?;
        }
        if self.build_number > 0 {
            write!(f, "+{}.", self.build_number)?;
        }
        for byte in self.commit_hash.iter() {
            write!(f, "{:02x}", byte)?;
        }
        Ok(())
    }
}

/// Convert a semver version to a proto version omitting the build metadata.
impl From<semver::Version> for Version {
    fn from(value: semver::Version) -> Self {
        Self {
            major: value.major,
            minor: value.minor,
            patch: value.patch,
            pre: value.pre.as_str().to_owned(),
            build_number: Default::default(),
            commit_hash: Default::default(),
        }
    }
}

impl TryFrom<Version> for semver::Version {
    type Error = semver::Error;

    fn try_from(value: Version) -> Result<Self, Self::Error> {
        let mut version = semver::Version::new(value.major, value.minor, value.patch);
        if !value.pre.is_empty() {
            version.pre = semver::Prerelease::new(&value.pre)?;
        }
        version.build = BuildMetadata::new(&format!(
            "{}.{}",
            value.build_number,
            value
                .commit_hash
                .iter()
                .format_with("", |byte, f| f(&format_args!("{:02x}", byte)))
        ))?;
        Ok(version)
    }
}

#[cfg(test)]
mod test {
    use uuid::uuid;

    use super::*;

    #[test]
    fn uuid_conversion() {
        let uuid = uuid!("a1a2a3a4-b1b2-c1c2-d1d2-d3d4d5d6d7d8");
        let proto_uuid = Uuid::from(uuid);
        assert_eq!(uuid, uuid::Uuid::from(proto_uuid));
        assert_eq!(proto_uuid, Uuid::from(uuid));
    }

    #[test]
    fn version_conversion() {
        let version: semver::Version = "1.2.3-alpha.1+12345678".parse().unwrap();
        let mut proto_version = Version::from(version.clone());
        assert_eq!(proto_version.major, version.major);
        assert_eq!(proto_version.minor, version.minor);
        assert_eq!(proto_version.patch, version.patch);
        assert_eq!(proto_version.pre, version.pre.as_str());
        assert_eq!(proto_version.build_number, 0);
        assert!(proto_version.commit_hash.is_empty());

        proto_version.build_number = 100;
        proto_version.commit_hash = vec![0xa1, 0xb1, 0xc1, 0xd1];
        let version = semver::Version::try_from(proto_version).unwrap();
        assert_eq!(version.major, version.major);
        assert_eq!(version.minor, version.minor);
        assert_eq!(version.patch, version.patch);
        assert_eq!(version.pre.as_str(), version.pre.as_str());
        assert_eq!(version.build.as_str(), "100.a1b1c1d1");
    }
}
