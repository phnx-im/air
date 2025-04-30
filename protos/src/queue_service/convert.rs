// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use openmls::key_packages;
use phnxtypes::{
    crypto::{
        self,
        ear::{self, AEAD_KEY_SIZE},
        secrets::Secret,
        signatures,
    },
    identifiers,
    messages::{self, push_token},
};
use tls_codec::{DeserializeBytes, Serialize};
use tonic::Status;
use uuid::Uuid;

use crate::{
    common::convert::InvalidNonceLen,
    validation::{MissingFieldError, MissingFieldExt},
};

use super::v1::{
    ClientIdEncryptionKey, EncryptedPushToken, FriendshipToken, KeyPackage, KeyPackageEarKey,
    QsClientId, QsClientVerifyingKey, QsEncryptedKeyPackage, QsUserId, QsUserVerifyingKey,
    QueueMessage,
};

impl From<identifiers::QsUserId> for QsUserId {
    fn from(value: identifiers::QsUserId) -> Self {
        let uuid = *value.as_uuid();
        Self {
            value: Some(uuid.into()),
        }
    }
}

impl TryFrom<QsUserId> for identifiers::QsUserId {
    type Error = MissingFieldError<&'static str>;

    fn try_from(proto: QsUserId) -> Result<Self, Self::Error> {
        Ok(identifiers::QsUserId::from(Uuid::from(
            proto.value.ok_or_missing_field("uuid")?,
        )))
    }
}

impl From<identifiers::QsClientId> for QsClientId {
    fn from(value: identifiers::QsClientId) -> Self {
        let uuid = *value.as_uuid();
        Self {
            value: Some(uuid.into()),
        }
    }
}

impl TryFrom<QsClientId> for identifiers::QsClientId {
    type Error = MissingFieldError<&'static str>;

    fn try_from(proto: QsClientId) -> Result<Self, Self::Error> {
        Ok(Self::from(Uuid::from(
            proto.value.ok_or_missing_field("uuid")?,
        )))
    }
}

impl From<QsUserVerifyingKey> for signatures::keys::QsUserVerifyingKey {
    fn from(proto: QsUserVerifyingKey) -> Self {
        Self::from_bytes(proto.bytes)
    }
}

impl From<signatures::keys::QsUserVerifyingKey> for QsUserVerifyingKey {
    fn from(value: signatures::keys::QsUserVerifyingKey) -> Self {
        Self {
            bytes: value.into_bytes(),
        }
    }
}

impl From<FriendshipToken> for messages::FriendshipToken {
    fn from(proto: FriendshipToken) -> Self {
        Self::from_bytes(proto.bytes)
    }
}

impl From<messages::FriendshipToken> for FriendshipToken {
    fn from(value: messages::FriendshipToken) -> Self {
        Self {
            bytes: value.into_bytes(),
        }
    }
}

impl From<QsClientVerifyingKey> for signatures::keys::QsClientVerifyingKey {
    fn from(proto: QsClientVerifyingKey) -> Self {
        Self::from_bytes(proto.bytes)
    }
}

impl From<signatures::keys::QsClientVerifyingKey> for QsClientVerifyingKey {
    fn from(value: signatures::keys::QsClientVerifyingKey) -> Self {
        Self {
            bytes: value.into_bytes(),
        }
    }
}

impl TryFrom<EncryptedPushToken> for push_token::EncryptedPushToken {
    type Error = InvalidNonceLen;

    fn try_from(proto: EncryptedPushToken) -> Result<Self, Self::Error> {
        proto.ciphertext.unwrap_or_default().try_into()
    }
}

impl From<push_token::EncryptedPushToken> for EncryptedPushToken {
    fn from(value: push_token::EncryptedPushToken) -> Self {
        Self {
            ciphertext: Some(value.into()),
        }
    }
}

impl TryFrom<key_packages::KeyPackage> for KeyPackage {
    type Error = tls_codec::Error;

    fn try_from(proto: key_packages::KeyPackage) -> Result<Self, Self::Error> {
        let tls = proto.tls_serialize_detached()?;
        Ok(KeyPackage { tls })
    }
}

impl TryFrom<KeyPackage> for key_packages::KeyPackageIn {
    type Error = tls_codec::Error;

    fn try_from(proto: KeyPackage) -> Result<Self, Self::Error> {
        DeserializeBytes::tls_deserialize_exact_bytes(&proto.tls)
    }
}

impl From<ear::keys::KeyPackageEarKey> for KeyPackageEarKey {
    fn from(value: ear::keys::KeyPackageEarKey) -> Self {
        Self {
            bytes: value.as_ref().secret().to_vec(),
        }
    }
}

impl TryFrom<KeyPackageEarKey> for ear::keys::KeyPackageEarKey {
    type Error = KeyPackageEarKeyError;

    fn try_from(value: KeyPackageEarKey) -> Result<Self, Self::Error> {
        let len = value.bytes.len();
        let secret: [u8; AEAD_KEY_SIZE] = value
            .bytes
            .try_into()
            .map_err(|_| KeyPackageEarKeyError::InvalidSecretLength(len))?;
        Ok(Self::from(Secret::from(secret)))
    }
}

#[derive(Debug, thiserror::Error)]
pub enum KeyPackageEarKeyError {
    #[error("invalid secret length: expected {AEAD_KEY_SIZE}, got {0}")]
    InvalidSecretLength(usize),
}

impl From<KeyPackageEarKeyError> for Status {
    fn from(e: KeyPackageEarKeyError) -> Self {
        Status::invalid_argument(format!("invalid key package ear key: {e}"))
    }
}

impl From<messages::QsEncryptedKeyPackage> for QsEncryptedKeyPackage {
    fn from(value: messages::QsEncryptedKeyPackage) -> Self {
        Self {
            ciphertext: Some(value.into()),
        }
    }
}

impl TryFrom<QsEncryptedKeyPackage> for messages::QsEncryptedKeyPackage {
    type Error = InvalidNonceLen;

    fn try_from(value: QsEncryptedKeyPackage) -> Result<Self, Self::Error> {
        value.ciphertext.unwrap_or_default().try_into()
    }
}

impl From<ClientIdEncryptionKey> for crypto::hpke::ClientIdEncryptionKey {
    fn from(proto: ClientIdEncryptionKey) -> Self {
        Self::from_bytes(proto.bytes)
    }
}

impl From<crypto::hpke::ClientIdEncryptionKey> for ClientIdEncryptionKey {
    fn from(value: crypto::hpke::ClientIdEncryptionKey) -> Self {
        Self {
            bytes: value.into_bytes(),
        }
    }
}

impl TryFrom<QueueMessage> for messages::QueueMessage {
    type Error = InvalidNonceLen;

    fn try_from(proto: QueueMessage) -> Result<Self, Self::Error> {
        Ok(Self {
            sequence_number: proto.sequence_number,
            ciphertext: proto.ciphertext.unwrap_or_default().try_into()?,
        })
    }
}

impl From<messages::QueueMessage> for QueueMessage {
    fn from(value: messages::QueueMessage) -> Self {
        Self {
            sequence_number: value.sequence_number,
            ciphertext: Some(value.ciphertext.into()),
        }
    }
}
