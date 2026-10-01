// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! State a client relays to its sibling clients through the QS.

use tls_codec::{DeserializeBytes, Serialize as _};
use uuid::Uuid;

use crate::{
    crypto::{
        aead::{AeadDecryptable, AeadEncryptable, keys::ClientStateKey},
        errors::{DecryptionError, EncryptionError},
    },
    identifiers::QsClientId,
};

use super::*;

/// Notifications a client suppresses because the user is looking at it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Suppression {
    None,
    Chat(Uuid),
    All,
}

const SUPPRESS_NONE: u8 = 0;
const SUPPRESS_CHAT: u8 = 1;
const SUPPRESS_ALL: u8 = 2;

/// Encoding of a [`Suppression`].
///
/// The chat id is the nil UUID unless a chat is suppressed, so that the length
/// of the ciphertext does not reveal the kind of suppression.
#[derive(Debug, Clone, Copy, PartialEq, Eq, TlsSize, TlsSerialize, TlsDeserializeBytes)]
pub struct ClientState {
    suppression: u8,
    chat_id: [u8; 16],
}

#[derive(Debug)]
pub struct EncryptedClientStateCtype;
pub type EncryptedClientState = Ciphertext<EncryptedClientStateCtype>;

impl AeadEncryptable<ClientStateKey, EncryptedClientStateCtype> for ClientState {}
impl AeadDecryptable<ClientStateKey, EncryptedClientStateCtype> for ClientState {}

impl ClientState {
    pub fn new(suppression: Suppression) -> Self {
        let (suppression, chat_id) = match suppression {
            Suppression::None => (SUPPRESS_NONE, Uuid::nil()),
            Suppression::Chat(chat_id) => (SUPPRESS_CHAT, chat_id),
            Suppression::All => (SUPPRESS_ALL, Uuid::nil()),
        };
        Self {
            suppression,
            chat_id: chat_id.into_bytes(),
        }
    }

    /// Unknown kinds decode as [`Suppression::None`].
    pub fn suppression(&self) -> Suppression {
        match self.suppression {
            SUPPRESS_CHAT => Suppression::Chat(Uuid::from_bytes(self.chat_id)),
            SUPPRESS_ALL => Suppression::All,
            _ => Suppression::None,
        }
    }

    /// Encrypts the state of the client `sender`.
    pub fn encrypt_to_bytes(
        &self,
        key: &ClientStateKey,
        sender: &QsClientId,
    ) -> Result<Vec<u8>, EncryptionError> {
        let ciphertext: AeadCiphertext = self.encrypt_with_aad(key, sender)?.into();
        ciphertext
            .tls_serialize_detached()
            .map_err(|_| EncryptionError::SerializationError)
    }

    /// Decrypts the state of the client `sender`.
    pub fn decrypt_from_bytes(
        key: &ClientStateKey,
        sender: &QsClientId,
        bytes: &[u8],
    ) -> Result<Self, DecryptionError> {
        let ciphertext = AeadCiphertext::tls_deserialize_exact_bytes(bytes)
            .map_err(|_| DecryptionError::DeserializationError)?;
        Self::decrypt_with_aad(key, &ciphertext.into(), sender)
    }
}

#[cfg(test)]
mod tests {
    use crate::crypto::{kdf::KdfDerivable, signatures::keys::QsUserSigningKey};

    use super::*;

    fn key(signing_key: &QsUserSigningKey) -> ClientStateKey {
        ClientStateKey::derive(&signing_key.derive_sibling_secret(), &Vec::new()).unwrap()
    }

    #[test]
    fn roundtrip() {
        let signing_key = QsUserSigningKey::generate().unwrap();
        let key = key(&signing_key);
        let sender = QsClientId::random(&mut rand::rng());

        for suppression in [
            Suppression::None,
            Suppression::Chat(Uuid::new_v4()),
            Suppression::All,
        ] {
            let bytes = ClientState::new(suppression)
                .encrypt_to_bytes(&key, &sender)
                .unwrap();
            let decrypted = ClientState::decrypt_from_bytes(&key, &sender, &bytes).unwrap();
            assert_eq!(decrypted.suppression(), suppression);
        }
    }

    #[test]
    fn ciphertext_length_does_not_reveal_suppression() {
        let signing_key = QsUserSigningKey::generate().unwrap();
        let key = key(&signing_key);
        let sender = QsClientId::random(&mut rand::rng());

        let lengths: Vec<_> = [
            Suppression::None,
            Suppression::Chat(Uuid::new_v4()),
            Suppression::All,
        ]
        .map(|suppression| {
            ClientState::new(suppression)
                .encrypt_to_bytes(&key, &sender)
                .unwrap()
                .len()
        })
        .into();
        assert!(lengths.iter().all(|len| *len == lengths[0]));
    }

    #[test]
    fn other_sender_or_user_is_rejected() {
        let signing_key = QsUserSigningKey::generate().unwrap();
        let key = key(&signing_key);
        let sender = QsClientId::random(&mut rand::rng());
        let bytes = ClientState::new(Suppression::All)
            .encrypt_to_bytes(&key, &sender)
            .unwrap();

        let other_sender = QsClientId::random(&mut rand::rng());
        assert!(ClientState::decrypt_from_bytes(&key, &other_sender, &bytes).is_err());

        let other_key = self::key(&QsUserSigningKey::generate().unwrap());
        assert!(ClientState::decrypt_from_bytes(&other_key, &sender, &bytes).is_err());
    }
}
