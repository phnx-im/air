// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! This module contains traits to facilitate length-padded AEAD symmetric
//! encryption of other structs. Padding follows the "Padmé" scheme (see
//! [`crate::padme`]) to reduce the amount of information the ciphertext length
//! leaks about the plaintext length.
//!
//! Any struct that needs to be encrypted with padding opts in by implementing
//! [`PaddedAeadEncryptable`] (and [`PaddedAeadDecryptable`] for decryption) with
//! an empty impl block, relying on the default method implementations.
//!
//! Unlike the TLS-bound [`super::AeadEncryptable`]/[`super::AeadDecryptable`]
//! traits, both the value and the additional authenticated data (AAD) are
//! serialized through the versioned CBOR [`PersistenceCodec`] rather than
//! `tls_codec`.
//!
//! # Padding scheme
//!
//! The padded plaintext places the padding *before* the encoded content:
//!
//! ```text
//! pad_len (u32 LE) ‖ pad_len zero bytes ‖ content
//! ```
//!
//! where `content = PersistenceCodec::to_vec(self)`. The total padded length is
//! exactly `max(PAD_FLOOR, padme_len(4 + content.len()))`, so every content
//! length within a padding bucket produces the same ciphertext length.
//!
//! The padding is raw framing rather than a field of a CBOR wrapper struct on
//! purpose: CBOR would encode the padding as a byte string whose header length
//! grows with the padding length, which makes some target lengths unreachable —
//! content lengths at those boundaries would stand out from their bucket. The
//! raw prefix keeps the padded length byte-exact.
//!
//! On decryption the leading `u32` is read to determine `pad_len`, the padding
//! is skipped, and the remaining bytes are decoded via
//! [`PersistenceCodec::from_slice`]. The padding bytes are not required to be
//! zero: the AEAD layer already authenticates them.

use serde::Serialize;
use tracing::error;

use crate::{
    codec::PersistenceCodec,
    crypto::errors::{DecryptionError, EncryptionError},
    padme::padme_len,
};

use super::{AeadKey, Ciphertext, Payload};

/// Length of the `u32` little-endian padding-length prefix, in bytes.
const PAD_PREFIX_LEN: usize = 4;

/// Prefix `content` with Padmé-derived zero padding.
///
/// Produces `pad_len (u32 LE) ‖ pad_len zero bytes ‖ content`, where the total
/// length is `max(floor, padme_len(PAD_PREFIX_LEN + content.len()))`.
fn pad(content: &[u8], floor: usize) -> Result<Vec<u8>, EncryptionError> {
    let total = floor.max(padme_len(PAD_PREFIX_LEN + content.len()));
    // `total >= padme_len(PAD_PREFIX_LEN + content.len()) >= PAD_PREFIX_LEN +
    // content.len()`, so this subtraction cannot underflow.
    let pad_len = total - PAD_PREFIX_LEN - content.len();
    // A content this large is unrealistic, but don't truncate silently.
    let pad_len_prefix = u32::try_from(pad_len).map_err(|e| {
        error!(error = %e, "Padding length does not fit into a u32");
        EncryptionError::SerializationError
    })?;
    let mut padded = Vec::with_capacity(total);
    padded.extend_from_slice(&pad_len_prefix.to_le_bytes());
    // Append `pad_len` zero bytes after the prefix.
    padded.resize(PAD_PREFIX_LEN + pad_len, 0);
    padded.extend_from_slice(content);
    Ok(padded)
}

/// Reverse of [`pad`]: return the content slice of a padded plaintext.
///
/// Reads the leading `u32` padding length and returns the trailing content,
/// skipping the padding. Malformed framing (plaintext shorter than the prefix,
/// or a padding length that overruns the plaintext) yields
/// [`DecryptionError::DeserializationError`].
fn unpad(plaintext: &[u8]) -> Result<&[u8], DecryptionError> {
    let pad_len_prefix: [u8; PAD_PREFIX_LEN] = plaintext
        .get(..PAD_PREFIX_LEN)
        .and_then(|prefix| prefix.try_into().ok())
        .ok_or_else(|| {
            error!("Padded plaintext is shorter than the padding-length prefix");
            DecryptionError::DeserializationError
        })?;
    let pad_len = u32::from_le_bytes(pad_len_prefix) as usize;
    let content_start = PAD_PREFIX_LEN.checked_add(pad_len);
    content_start
        .and_then(|start| plaintext.get(start..))
        .ok_or_else(|| {
            error!("Padding length overruns the padded plaintext");
            DecryptionError::DeserializationError
        })
}

/// Serialize `content` and prefix it with Padmé-derived zero padding. See the
/// [module docs](self) for the padding scheme.
fn pad_and_serialize<T: Serialize>(content: &T, floor: usize) -> Result<Vec<u8>, EncryptionError> {
    let content = PersistenceCodec::to_vec(content).map_err(|e| {
        error!(error = %e, "Could not serialize plaintext");
        EncryptionError::SerializationError
    })?;
    pad(&content, floor)
}

/// A trait that can be implemented for structs that are encryptable by an
/// [`AeadKey`] with length padding. See the [module docs](self) for the padding
/// scheme.
pub trait PaddedAeadEncryptable<KeyType: AeadKey, CT>: Serialize + Sized {
    /// Minimum total length of the padded plaintext, in bytes.
    const PAD_FLOOR: usize = 128;

    /// Encrypt the value under the given [`AeadKey`] with length padding.
    /// Returns an [`EncryptionError`] or the ciphertext.
    fn encrypt_padded(&self, key: &KeyType) -> Result<Ciphertext<CT>, EncryptionError> {
        let plaintext = pad_and_serialize(self, Self::PAD_FLOOR)?;
        let ciphertext = key.encrypt(plaintext.as_slice())?;
        Ok(ciphertext.into())
    }

    /// Encrypt the value under the given [`AeadKey`] with length padding,
    /// authenticating the given additional data. Returns an [`EncryptionError`]
    /// or the ciphertext.
    fn encrypt_padded_with_aad<Aad: Serialize>(
        &self,
        key: &KeyType,
        aad: &Aad,
    ) -> Result<Ciphertext<CT>, EncryptionError> {
        let plaintext = pad_and_serialize(self, Self::PAD_FLOOR)?;
        let aad = PersistenceCodec::to_vec(aad).map_err(|e| {
            error!(error = %e, "Could not serialize aad");
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

/// A trait that can be implemented for structs that are decryptable by an
/// [`AeadKey`] with length padding. See the [module docs](self) for the padding
/// scheme.
pub trait PaddedAeadDecryptable<KeyType: AeadKey, CT>: serde::de::DeserializeOwned + Sized {
    /// Decrypt the given ciphertext using the given [`AeadKey`] and strip the
    /// padding. Returns a [`DecryptionError`] or the resulting plaintext.
    fn decrypt_padded(key: &KeyType, ciphertext: &Ciphertext<CT>) -> Result<Self, DecryptionError> {
        let plaintext = key.decrypt(ciphertext.aead_ciphertext())?;
        let content = unpad(&plaintext)?;
        PersistenceCodec::from_slice(content).map_err(|e| {
            error!(error = %e, "Could not deserialize plaintext");
            DecryptionError::DeserializationError
        })
    }

    /// Decrypt the given ciphertext using the given [`AeadKey`] and the given
    /// additional data, then strip the padding. Returns a [`DecryptionError`]
    /// or the resulting plaintext.
    fn decrypt_padded_with_aad<Aad: Serialize>(
        key: &KeyType,
        ciphertext: &Ciphertext<CT>,
        aad: &Aad,
    ) -> Result<Self, DecryptionError> {
        let aad = PersistenceCodec::to_vec(aad).map_err(|e| {
            error!(error = %e, "Could not serialize aad");
            DecryptionError::SerializationError
        })?;
        let plaintext = key.decrypt_with_aad(ciphertext.aead_ciphertext(), aad.as_slice())?;
        let content = unpad(&plaintext)?;
        PersistenceCodec::from_slice(content).map_err(|e| {
            error!(error = %e, "Could not deserialize plaintext");
            DecryptionError::DeserializationError
        })
    }
}

#[cfg(test)]
mod tests {
    use serde::Deserialize;

    use crate::crypto::secrets::Secret;

    use super::super::AEAD_KEY_SIZE;
    use super::*;

    /// AES-GCM authentication tag length, in bytes.
    const AES_GCM_TAG_LEN: usize = 16;

    struct TestKey(Secret<AEAD_KEY_SIZE>);

    impl TestKey {
        fn random() -> Self {
            Self(Secret::random().unwrap())
        }
    }

    impl AsRef<Secret<AEAD_KEY_SIZE>> for TestKey {
        fn as_ref(&self) -> &Secret<AEAD_KEY_SIZE> {
            &self.0
        }
    }

    impl AeadKey for TestKey {}

    /// Marker ciphertext type.
    struct TestCt;

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    struct TestPayload {
        text: String,
        #[serde(with = "serde_bytes")]
        blob: Vec<u8>,
    }

    impl TestPayload {
        fn new(text: &str, blob_len: usize) -> Self {
            Self {
                text: text.to_owned(),
                blob: vec![0xab; blob_len],
            }
        }
    }

    impl PaddedAeadEncryptable<TestKey, TestCt> for TestPayload {}
    impl PaddedAeadDecryptable<TestKey, TestCt> for TestPayload {}

    /// A second payload type with a custom `PAD_FLOOR`.
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    struct SmallFloorPayload {
        text: String,
    }

    impl PaddedAeadEncryptable<TestKey, TestCt> for SmallFloorPayload {
        const PAD_FLOOR: usize = 32;
    }
    impl PaddedAeadDecryptable<TestKey, TestCt> for SmallFloorPayload {}

    /// Observe the length of the padded plaintext behind a ciphertext (the AEAD
    /// ciphertext length minus the AES-GCM tag).
    fn padded_plaintext_len(ciphertext: &Ciphertext<TestCt>) -> usize {
        ciphertext.aead_ciphertext().ciphertext.len() - AES_GCM_TAG_LEN
    }

    #[test]
    fn roundtrip_without_aad() {
        let key = TestKey::random();
        let payload = TestPayload::new("hello", 3);
        let ciphertext = payload.encrypt_padded(&key).unwrap();
        let decrypted = TestPayload::decrypt_padded(&key, &ciphertext).unwrap();
        assert_eq!(payload, decrypted);
    }

    #[test]
    fn roundtrip_with_aad() {
        let key = TestKey::random();
        let payload = TestPayload::new("hello", 3);
        let aad = "some aad".to_owned();
        let ciphertext = payload.encrypt_padded_with_aad(&key, &aad).unwrap();
        let decrypted = TestPayload::decrypt_padded_with_aad(&key, &ciphertext, &aad).unwrap();
        assert_eq!(payload, decrypted);
    }

    #[test]
    fn wrong_aad_fails() {
        let key = TestKey::random();
        let payload = TestPayload::new("hello", 3);
        let aad = "some aad".to_owned();
        let ciphertext = payload.encrypt_padded_with_aad(&key, &aad).unwrap();
        let wrong_aad = "other aad".to_owned();
        let result = TestPayload::decrypt_padded_with_aad(&key, &ciphertext, &wrong_aad);
        assert!(matches!(result, Err(DecryptionError::DecryptionError)));
    }

    #[test]
    fn pad_produces_exact_length() {
        for content_len in [0usize, 5, 100, 124, 200, 1000] {
            let content = vec![0x11; content_len];
            let padded = pad(&content, 128).unwrap();
            let expected = 128.max(padme_len(PAD_PREFIX_LEN + content_len));
            assert_eq!(padded.len(), expected, "content_len = {content_len}");
        }
    }

    #[test]
    fn pad_unpad_roundtrip() {
        for content_len in [0usize, 1, 5, 124, 200] {
            let content = vec![0x11; content_len];
            let padded = pad(&content, 128).unwrap();
            assert_eq!(unpad(&padded).unwrap(), content.as_slice());
        }
    }

    #[test]
    fn ciphertext_length_matches_padme_exactly() {
        let key = TestKey::random();
        // Sweep content sizes covering: empty payload, payloads below the
        // 128-byte floor, and payloads large enough that Padmé governs.
        for blob_len in 0..=160 {
            let payload = TestPayload::new("", blob_len);
            let content = PersistenceCodec::to_vec(&payload).unwrap();
            let ciphertext = payload.encrypt_padded(&key).unwrap();
            let expected = 128.max(padme_len(PAD_PREFIX_LEN + content.len()));
            assert_eq!(
                padded_plaintext_len(&ciphertext),
                expected,
                "blob_len = {blob_len}"
            );
        }
    }

    #[test]
    fn floor_hides_small_size_differences() {
        let key = TestKey::random();
        // Both payloads are well below the 128-byte floor, so their padded
        // lengths - and thus ciphertext lengths - are exactly equal.
        let short = TestPayload::new("a", 1);
        let longer = TestPayload::new("a much longer string", 20);
        let short_ct = short.encrypt_padded(&key).unwrap();
        let longer_ct = longer.encrypt_padded(&key).unwrap();
        assert_eq!(
            padded_plaintext_len(&short_ct),
            padded_plaintext_len(&longer_ct)
        );
        assert_eq!(padded_plaintext_len(&short_ct), 128);
    }

    #[test]
    fn custom_pad_floor_is_honored() {
        let key = TestKey::random();
        let payload = SmallFloorPayload {
            text: "x".to_owned(),
        };
        let content = PersistenceCodec::to_vec(&payload).unwrap();
        // Content is small enough that the 32-byte floor applies rather than
        // the default 128.
        let expected = 32.max(padme_len(PAD_PREFIX_LEN + content.len()));
        assert_eq!(expected, 32);
        let ciphertext = payload.encrypt_padded(&key).unwrap();
        assert_eq!(padded_plaintext_len(&ciphertext), 32);
        let decrypted = SmallFloorPayload::decrypt_padded(&key, &ciphertext).unwrap();
        assert_eq!(payload, decrypted);
    }

    #[test]
    fn unpad_rejects_short_plaintext() {
        // Fewer than PAD_PREFIX_LEN bytes.
        let result = unpad(&[0u8; 3]);
        assert!(matches!(result, Err(DecryptionError::DeserializationError)));
    }

    #[test]
    fn unpad_rejects_overrunning_pad_len() {
        // pad_len = 100, but only 2 bytes follow the prefix.
        let mut plaintext = 100u32.to_le_bytes().to_vec();
        plaintext.extend_from_slice(&[0u8; 2]);
        let result = unpad(&plaintext);
        assert!(matches!(result, Err(DecryptionError::DeserializationError)));
    }

    #[test]
    fn unpad_returns_exact_content_slice() {
        // Padding bytes are not required to be zero on decode; unpad returns
        // exactly the bytes after the padding.
        let mut plaintext = 2u32.to_le_bytes().to_vec();
        plaintext.extend_from_slice(&[0xff, 0xff]); // padding
        plaintext.extend_from_slice(&[1, 2, 3, 4]); // content
        assert_eq!(unpad(&plaintext).unwrap(), &[1, 2, 3, 4]);
    }

    #[test]
    fn garbage_plaintext_fails_deserialization() {
        let key = TestKey::random();
        // A valid AEAD ciphertext whose plaintext is not a padded payload.
        let ciphertext: Ciphertext<TestCt> = key.encrypt([0xff; 7].as_slice()).unwrap().into();
        let result = TestPayload::decrypt_padded(&key, &ciphertext);
        assert!(matches!(result, Err(DecryptionError::DeserializationError)));
    }
}
