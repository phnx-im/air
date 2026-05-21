// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::{
    crypto::{self, aead::AEAD_KEY_SIZE, signatures},
    identifiers,
    messages::{self, push_token},
};
use openmls::key_packages;
use tls_codec::{DeserializeBytes, Serialize};
use tonic::Status;
use uuid::Uuid;

use crate::{
    common::convert::InvalidNonceLen,
    queue_service::v1::ApqKeyPackage,
    validation::{MissingFieldError, MissingFieldExt},
};

use super::v1::{
    ClientIdEncryptionKey, EncryptedPushToken, FriendshipToken, KeyPackage, QsClientId,
    QsClientVerifyingKey, QsUserId, QsUserVerifyingKey, QueueMessage,
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

impl TryFrom<apqmls::messages::ApqKeyPackage> for ApqKeyPackage {
    type Error = tls_codec::Error;

    fn try_from(value: apqmls::messages::ApqKeyPackage) -> Result<Self, Self::Error> {
        Ok(ApqKeyPackage {
            t_key_package_tls: value.t_key_package().tls_serialize_detached()?,
            pq_key_package_tls: value.pq_key_package().tls_serialize_detached()?,
        })
    }
}

impl TryFrom<ApqKeyPackage> for apqmls::messages::ApqKeyPackageIn {
    type Error = tls_codec::Error;

    fn try_from(proto: ApqKeyPackage) -> Result<Self, Self::Error> {
        Ok(apqmls::messages::ApqKeyPackageIn::new(
            DeserializeBytes::tls_deserialize_exact_bytes(&proto.t_key_package_tls)?,
            DeserializeBytes::tls_deserialize_exact_bytes(&proto.pq_key_package_tls)?,
        ))
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
