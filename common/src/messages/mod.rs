// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use mls_assist::{
    openmls::prelude::OpenMlsRand, openmls_rust_crypto::OpenMlsRustCrypto,
    openmls_traits::OpenMlsProvider,
};
use serde::{Deserialize, Serialize};
use tls_codec::{TlsDeserializeBytes, TlsSerialize, TlsSize};

use crate::crypto::{
    ear::{AeadCiphertext, Ciphertext},
    errors::RandomnessError,
};

pub mod client_as;
pub mod client_as_out;
pub mod client_ds;
pub mod client_ds_out;
pub mod client_qs;
pub mod connection_package;
pub mod connection_package_v1;
pub mod push_token;
pub mod welcome_attribution_info;

#[derive(
    Serialize,
    Deserialize,
    TlsSerialize,
    TlsDeserializeBytes,
    TlsSize,
    PartialEq,
    Eq,
    Clone,
    Debug,
    sqlx::Type,
)]
#[sqlx(transparent)]
pub struct FriendshipToken(#[serde(with = "serde_bytes")] Vec<u8>);

impl FriendshipToken {
    pub fn from_bytes(token: Vec<u8>) -> Self {
        Self(token)
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.0
    }

    pub fn random() -> Result<Self, RandomnessError> {
        let token = OpenMlsRustCrypto::default()
            .rand()
            .random_vec(32)
            .map_err(|_| RandomnessError::InsufficientRandomness)?;

        Ok(Self(token))
    }

    pub fn token(&self) -> &[u8] {
        self.0.as_ref()
    }
}

/// Enum encoding the version of the Air protocol that was used to create
/// the given message.
#[derive(
    Debug,
    TlsSerialize,
    TlsDeserializeBytes,
    TlsSize,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
)]
#[repr(u8)]
#[derive(Default)]
pub enum AirProtocolVersion {
    #[default]
    Alpha,
}

// === Queue ===

#[derive(
    Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TlsSerialize, TlsDeserializeBytes, TlsSize,
)]
pub struct QueueMessage {
    pub sequence_number: u64,
    pub ciphertext: AeadCiphertext,
}

#[derive(Debug, Clone)]
pub struct EncryptedQsQueueMessageCtype;

pub type EncryptedQsQueueMessage = Ciphertext<EncryptedQsQueueMessageCtype>;

#[derive(Debug, Clone)]
pub struct EncryptedAsQueueMessageCtype;

pub type EncryptedAsQueueMessage = Ciphertext<EncryptedAsQueueMessageCtype>;

#[cfg(test)]
mod test {
    use crate::{codec::PersistenceCodec, crypto::ear::AeadCiphertext};

    use super::*;

    #[test]
    fn test_queue_message_serde_codec() {
        let message = QueueMessage {
            sequence_number: 1,
            ciphertext: AeadCiphertext::dummy(),
        };
        insta::assert_binary_snapshot!(".cbor", PersistenceCodec::to_vec(&message).unwrap());
    }

    #[test]
    fn test_queue_message_serde_json() {
        let message = QueueMessage {
            sequence_number: 1,
            ciphertext: AeadCiphertext::dummy(),
        };
        insta::assert_json_snapshot!(message);
    }
}
