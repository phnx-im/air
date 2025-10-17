// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use super::*;

#[derive(Error, Debug, PartialEq, Eq, Clone)]
pub enum KeyGenerationError {
    /// Error generating signature keypair
    #[error("Error generating signature keypair")]
    KeypairGeneration,
}

#[derive(Debug, Error)]
pub enum RandomnessError {
    #[error("Insufficient randomness")]
    InsufficientRandomness,
}

#[derive(Error, Debug, Clone)]
pub enum DecryptionError {
    /// Error decrypting ciphertext.
    #[error("Error decrypting ciphertext.")]
    DecryptionError,
    /// Error deserializing payload.
    #[error("Error deserializing payload.")]
    DeserializationError,
    /// Error serializing AAD.
    #[error("Error serializing AAD.")]
    SerializationError,
    /// Error ratcheting forward
    #[error("Error ratcheting forward")]
    RatchetForwardError(#[from] LibraryError),
}

#[derive(Error, Debug, Clone)]
pub enum EncryptionError {
    /// Not enough randomness to generate Nonce
    #[error("Not enough randomness to generate Nonce")]
    RandomnessError,
    /// Encryption error
    #[error("Encryption error")]
    EncryptionError,
    /// Codec error
    #[error("Codec error")]
    SerializationError,
    /// Error ratcheting forward
    #[error("Error ratcheting forward")]
    RatchetForwardError(#[from] LibraryError),
}
