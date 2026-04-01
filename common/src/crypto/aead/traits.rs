// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! This module contains traits to facilitate AEAD symmetric encryption of other
//! structs. Any struct that needs to be encrypted needs to implement the
//! [`AeadEncryptable`] trait.

use aes_gcm::{
    KeyInit,
    aead::{Aead as AesGcmAead, Key, Nonce, Payload},
};
use tracing::{error, instrument};

use crate::crypto::{
    errors::{DecryptionError, EncryptionError, RandomnessError},
    secrets::Secret,
};

use super::{AEAD_KEY_SIZE, AEAD_NONCE_SIZE, Aead, AeadCiphertext, Ciphertext};

/// A trait meant for structs holding a symmetric key of size [`AEAD_KEY_SIZE`].
/// It enables use of these keys for encryption and decryption operations.
pub trait AeadKey: AsRef<Secret<AEAD_KEY_SIZE>> {
    // Encrypt the given plaintext under the given key. Generates a random nonce internally.
    #[instrument(level = "trace", skip_all, fields(key_type = std::any::type_name::<Self>()))]
    fn encrypt<'msg, 'aad>(
        &self,
        plaintext: impl Into<Payload<'msg, 'aad>>,
    ) -> Result<AeadCiphertext, EncryptionError> {
        // TODO: from_slice can potentially panic. However, we can rule this out
        // with a single test, since both the AEAD algorithm and the key size
        // are static.
        let key = Key::<Aead>::from_slice(self.as_ref().secret());
        let cipher: Aead = Aead::new(key);
        // TODO: Use a proper RNG provider instead.
        let nonce_raw = Secret::<AEAD_NONCE_SIZE>::random().map_err(|e| match e {
            RandomnessError::InsufficientRandomness => EncryptionError::RandomnessError,
        })?;
        let nonce = Nonce::<Aead>::from(nonce_raw.into_secret());
        // The Aead trait surfaces an error, but it's not clear under which
        // circumstances it would actually fail.
        let ciphertext = cipher
            .encrypt(&nonce, plaintext)
            .map_err(|_| EncryptionError::EncryptionError)?;
        Ok(AeadCiphertext {
            ciphertext,
            nonce: nonce.into(),
        })
    }

    // Decrypt the given ciphertext (including the nonce) using the given key.
    #[instrument(level = "trace", skip_all, fields(key_type = std::any::type_name::<Self>()))]
    fn decrypt(&self, ciphertext: &AeadCiphertext) -> Result<Vec<u8>, DecryptionError> {
        decrypt(
            self,
            &ciphertext.nonce,
            Payload {
                aad: &[],
                msg: ciphertext.ciphertext.as_slice(),
            },
        )
    }

    fn decrypt_with_aad(
        &self,
        ciphertext: &AeadCiphertext,
        aad: &[u8],
    ) -> Result<Vec<u8>, DecryptionError> {
        decrypt(
            self,
            &ciphertext.nonce,
            Payload {
                aad,
                msg: ciphertext.ciphertext.as_slice(),
            },
        )
    }
}

fn decrypt<'ctxt, 'aad>(
    key: impl AsRef<Secret<AEAD_KEY_SIZE>>,
    nonce: &[u8; AEAD_NONCE_SIZE],
    ciphertext: impl Into<Payload<'ctxt, 'aad>>,
) -> Result<Vec<u8>, DecryptionError> {
    // TODO: from_slice can potentially panic. However, we can rule this out
    // with a single test, since both the AEAD algorithm and the key size
    // are static.
    let key = Key::<Aead>::from_slice(key.as_ref().secret());
    let cipher: Aead = Aead::new(key);
    // TODO: Use a proper RNG provider instead.
    cipher.decrypt(nonce.into(), ciphertext).map_err(|e| {
        error!(%e,"Decryption error");
        DecryptionError::DecryptionError
    })
}

/// A trait that can be derived for structs that are encryptable/decryptable by
/// an AEAD key.
pub trait AeadEncryptable<KeyType: AeadKey, CT>: tls_codec::Serialize {
    /// Encrypt the value under the given [`AeadKey`]. Returns an
    /// [`EncryptionError`] or the ciphertext.
    fn encrypt(&self, key: &KeyType) -> Result<Ciphertext<CT>, EncryptionError> {
        let plaintext = self.tls_serialize_detached().map_err(|e| {
            tracing::error!("Could not serialize plaintext: {:?}", e);
            EncryptionError::SerializationError
        })?;
        let ciphertext = key.encrypt(plaintext.as_slice())?;
        Ok(ciphertext.into())
    }

    fn encrypt_with_aad<Aad: tls_codec::Serialize>(
        &self,
        key: &KeyType,
        aad: &Aad,
    ) -> Result<Ciphertext<CT>, EncryptionError> {
        let plaintext = self.tls_serialize_detached().map_err(|e| {
            tracing::error!("Could not serialize plaintext: {:?}", e);
            EncryptionError::SerializationError
        })?;
        let aad = aad.tls_serialize_detached().map_err(|e| {
            tracing::error!("Could not serialize plaintext: {:?}", e);
            EncryptionError::SerializationError
        })?;
        let payload = Payload {
            msg: plaintext.as_slice(),
            aad: aad.as_slice(),
        };
        let ciphertext = key.encrypt(payload)?;
        Ok(ciphertext.into())
    }
}

/// A trait that can be derived for structs that are encryptable/decryptable by
/// an AEAD key.
pub trait AeadDecryptable<KeyType: AeadKey, CT>: tls_codec::DeserializeBytes + Sized {
    /// Decrypt the given ciphertext using the given [`AeadKey`]. Returns a
    /// [`DecryptionError`] or the resulting plaintext.
    fn decrypt(key: &KeyType, ciphertext: &Ciphertext<CT>) -> Result<Self, DecryptionError> {
        let plaintext = key.decrypt(&ciphertext.ct)?;
        Self::tls_deserialize_exact_bytes(&plaintext)
            .map_err(|_| DecryptionError::DeserializationError)
    }

    fn decrypt_with_aad<Aad: tls_codec::Serialize>(
        key: &KeyType,
        ciphertext: &Ciphertext<CT>,
        aad: &Aad,
    ) -> Result<Self, DecryptionError> {
        let aad = aad.tls_serialize_detached().map_err(|e| {
            tracing::error!(error = %e, "Could not serialize aad");
            DecryptionError::SerializationError
        })?;
        let plaintext = key.decrypt_with_aad(&ciphertext.ct, aad.as_slice())?;
        Self::tls_deserialize_exact_bytes(&plaintext)
            .map_err(|_| DecryptionError::DeserializationError)
    }
}
