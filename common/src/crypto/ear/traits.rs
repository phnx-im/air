// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! This module contains traits to facilitate EAR of other structs on the
//! backend. Any struct that needs to be encrypted at rest needs to implement
//! the [`EarEncryptable`] trait.

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
pub trait EarKey: AsRef<Secret<AEAD_KEY_SIZE>> {
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
/// an EAR key.
pub trait EarEncryptable<EarKeyType: EarKey, CT>: tls_codec::Serialize {
    /// Encrypt the value under the given [`EarKey`]. Returns an
    /// [`EncryptionError`] or the ciphertext.
    fn encrypt(&self, ear_key: &EarKeyType) -> Result<Ciphertext<CT>, EncryptionError> {
        let plaintext = self.tls_serialize_detached().map_err(|e| {
            tracing::error!("Could not serialize plaintext: {:?}", e);
            EncryptionError::SerializationError
        })?;
        let ciphertext = ear_key.encrypt(plaintext.as_slice())?;
        Ok(ciphertext.into())
    }

    fn encrypt_with_aad<Aad: tls_codec::Serialize>(
        &self,
        ear_key: &EarKeyType,
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
        let ciphertext = ear_key.encrypt(payload)?;
        Ok(ciphertext.into())
    }
}

/// A trait that can be derived for structs that are encryptable/decryptable by
/// an EAR key.
pub trait EarDecryptable<EarKeyType: EarKey, CT>: tls_codec::DeserializeBytes + Sized {
    /// Decrypt the given ciphertext using the given [`EarKey`]. Returns a
    /// [`DecryptionError`] or the resulting plaintext.
    fn decrypt(ear_key: &EarKeyType, ciphertext: &Ciphertext<CT>) -> Result<Self, DecryptionError> {
        let plaintext = ear_key.decrypt(&ciphertext.ct)?;
        Self::tls_deserialize_exact_bytes(&plaintext)
            .map_err(|_| DecryptionError::DeserializationError)
    }

    fn decrypt_with_aad<Aad: tls_codec::Serialize>(
        ear_key: &EarKeyType,
        ciphertext: &Ciphertext<CT>,
        aad: &Aad,
    ) -> Result<Self, DecryptionError> {
        let aad = aad.tls_serialize_detached().map_err(|e| {
            tracing::error!(error = %e, "Could not serialize aad");
            DecryptionError::SerializationError
        })?;
        let plaintext = ear_key.decrypt_with_aad(&ciphertext.ct, aad.as_slice())?;
        Self::tls_deserialize_exact_bytes(&plaintext)
            .map_err(|_| DecryptionError::DeserializationError)
    }
}
