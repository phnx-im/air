// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::{
    crypto::{
        self,
        aead::AEAD_KEY_SIZE,
        signatures::{self, private_keys::SIGNATURE_PUBLIC_KEY_SIZE},
    },
    identifiers,
    messages::{
        self, FRIENDSHIP_TOKEN_SIZE,
        push_token::{self, MAX_ENCRYPTED_PUSH_TOKEN_SIZE},
    },
};
use openmls::key_packages;
use tls_codec::{DeserializeBytes, Serialize};
use tonic::Status;
use uuid::Uuid;

use crate::{
    common::convert::InvalidNonceLen,
    queue_service::v1::ApqKeyPackage,
    validation::{InvalidFieldLength, MissingFieldError, MissingFieldExt},
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

impl TryFrom<QsUserVerifyingKey> for signatures::keys::QsUserVerifyingKey {
    type Error = InvalidFieldLength;

    fn try_from(proto: QsUserVerifyingKey) -> Result<Self, Self::Error> {
        InvalidFieldLength::check(
            "QS user verifying key",
            &proto.bytes,
            SIGNATURE_PUBLIC_KEY_SIZE,
        )?;
        Ok(Self::from_bytes(proto.bytes))
    }
}

impl From<signatures::keys::QsUserVerifyingKey> for QsUserVerifyingKey {
    fn from(value: signatures::keys::QsUserVerifyingKey) -> Self {
        Self {
            bytes: value.into_bytes(),
        }
    }
}

impl TryFrom<FriendshipToken> for messages::FriendshipToken {
    type Error = InvalidFieldLength;

    fn try_from(proto: FriendshipToken) -> Result<Self, Self::Error> {
        InvalidFieldLength::check("friendship token", &proto.bytes, FRIENDSHIP_TOKEN_SIZE)?;
        Ok(Self::from_bytes(proto.bytes))
    }
}

impl From<messages::FriendshipToken> for FriendshipToken {
    fn from(value: messages::FriendshipToken) -> Self {
        Self {
            bytes: value.into_bytes(),
        }
    }
}

impl TryFrom<QsClientVerifyingKey> for signatures::keys::QsClientVerifyingKey {
    type Error = InvalidFieldLength;

    fn try_from(proto: QsClientVerifyingKey) -> Result<Self, Self::Error> {
        InvalidFieldLength::check(
            "QS client verifying key",
            &proto.bytes,
            SIGNATURE_PUBLIC_KEY_SIZE,
        )?;
        Ok(Self::from_bytes(proto.bytes))
    }
}

impl From<signatures::keys::QsClientVerifyingKey> for QsClientVerifyingKey {
    fn from(value: signatures::keys::QsClientVerifyingKey) -> Self {
        Self {
            bytes: value.into_bytes(),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum InvalidEncryptedPushToken {
    #[error(transparent)]
    InvalidNonceLen(#[from] InvalidNonceLen),
    #[error(
        "encrypted push token too long: at most {MAX_ENCRYPTED_PUSH_TOKEN_SIZE} bytes, got {0}"
    )]
    TooLong(usize),
}

impl From<InvalidEncryptedPushToken> for Status {
    fn from(e: InvalidEncryptedPushToken) -> Self {
        Status::invalid_argument(e.to_string())
    }
}

impl TryFrom<EncryptedPushToken> for push_token::EncryptedPushToken {
    type Error = InvalidEncryptedPushToken;

    fn try_from(proto: EncryptedPushToken) -> Result<Self, Self::Error> {
        let ciphertext = proto.ciphertext.unwrap_or_default();
        if ciphertext.ciphertext.len() > MAX_ENCRYPTED_PUSH_TOKEN_SIZE {
            return Err(InvalidEncryptedPushToken::TooLong(
                ciphertext.ciphertext.len(),
            ));
        }
        Ok(ciphertext.try_into()?)
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

impl TryFrom<&KeyPackage> for key_packages::KeyPackageIn {
    type Error = tls_codec::Error;

    fn try_from(proto: &KeyPackage) -> Result<Self, Self::Error> {
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

impl TryFrom<&ApqKeyPackage> for apqmls::messages::ApqKeyPackageIn {
    type Error = tls_codec::Error;

    fn try_from(proto: &ApqKeyPackage) -> Result<Self, Self::Error> {
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

#[cfg(test)]
mod tests {
    use aircommon::crypto::{aead::AEAD_NONCE_SIZE, signatures::keys::QsUserSigningKey};

    use crate::common::v1::Ciphertext;

    use super::*;

    fn assert_exact_size<T, E>(size: usize, convert: impl Fn(Vec<u8>) -> Result<T, E>) {
        assert!(convert(vec![1; size]).is_ok());
        assert!(convert(vec![1; size - 1]).is_err());
        assert!(convert(vec![1; size + 1]).is_err());
        assert!(convert(Vec::new()).is_err());
    }

    #[test]
    fn qs_user_verifying_key_requires_exact_size() {
        assert_exact_size(SIGNATURE_PUBLIC_KEY_SIZE, |bytes| {
            signatures::keys::QsUserVerifyingKey::try_from(QsUserVerifyingKey { bytes })
        });
    }

    #[test]
    fn qs_client_verifying_key_requires_exact_size() {
        assert_exact_size(SIGNATURE_PUBLIC_KEY_SIZE, |bytes| {
            signatures::keys::QsClientVerifyingKey::try_from(QsClientVerifyingKey { bytes })
        });
    }

    #[test]
    fn friendship_token_requires_exact_size() {
        assert_exact_size(FRIENDSHIP_TOKEN_SIZE, |bytes| {
            messages::FriendshipToken::try_from(FriendshipToken { bytes })
        });
    }

    #[test]
    fn generated_values_pass_size_checks() {
        let signing_key = QsUserSigningKey::generate().unwrap();
        let bytes = signing_key.verifying_key().as_slice().to_vec();
        assert!(
            signatures::keys::QsUserVerifyingKey::try_from(QsUserVerifyingKey { bytes }).is_ok()
        );

        let token = messages::FriendshipToken::random().unwrap();
        assert!(messages::FriendshipToken::try_from(FriendshipToken::from(token)).is_ok());
    }

    fn encrypted_push_token(
        ciphertext_len: usize,
    ) -> Result<push_token::EncryptedPushToken, InvalidEncryptedPushToken> {
        EncryptedPushToken {
            ciphertext: Some(Ciphertext {
                ciphertext: vec![1; ciphertext_len],
                nonce: vec![2; AEAD_NONCE_SIZE],
            }),
        }
        .try_into()
    }

    #[test]
    fn encrypted_push_token_is_bounded() {
        assert!(encrypted_push_token(0).is_ok());
        assert!(encrypted_push_token(MAX_ENCRYPTED_PUSH_TOKEN_SIZE).is_ok());
        assert!(matches!(
            encrypted_push_token(MAX_ENCRYPTED_PUSH_TOKEN_SIZE + 1),
            Err(InvalidEncryptedPushToken::TooLong(len)) if len == MAX_ENCRYPTED_PUSH_TOKEN_SIZE + 1
        ));
    }

    #[test]
    fn encrypted_push_token_still_checks_nonce() {
        let result = push_token::EncryptedPushToken::try_from(EncryptedPushToken {
            ciphertext: Some(Ciphertext {
                ciphertext: vec![1; 16],
                nonce: vec![2; AEAD_NONCE_SIZE - 1],
            }),
        });
        assert!(matches!(
            result,
            Err(InvalidEncryptedPushToken::InvalidNonceLen(_))
        ));
    }

    #[test]
    fn invalid_sizes_map_to_invalid_argument() {
        let error = signatures::keys::QsUserVerifyingKey::try_from(QsUserVerifyingKey {
            bytes: Vec::new(),
        })
        .unwrap_err();
        assert_eq!(Status::from(error).code(), tonic::Code::InvalidArgument);

        let error = encrypted_push_token(MAX_ENCRYPTED_PUSH_TOKEN_SIZE + 1).unwrap_err();
        assert_eq!(Status::from(error).code(), tonic::Code::InvalidArgument);
    }
}
