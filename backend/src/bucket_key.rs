// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Keys that let a counter recognize something without storing it.

use std::fmt;

use hmac::{Hmac, KeyInit, Mac};
use rand::RngExt;
use sha2::Sha256;

/// A key that stored bucket names are computed under.
#[derive(Clone)]
pub(crate) struct BucketKey([u8; 32]);

impl BucketKey {
    pub(crate) fn random() -> Self {
        Self(rand::rng().random())
    }

    pub(crate) fn from_stored(bytes: Vec<u8>) -> sqlx::Result<Self> {
        let bytes: [u8; 32] = bytes
            .try_into()
            .map_err(|_| sqlx::Error::Decode("bucket key is not 32 bytes".into()))?;
        Ok(Self(bytes))
    }

    pub(crate) fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    /// The stored name of `input`, under a label that separates subsystems.
    pub(crate) fn bucket(&self, label: &[u8], input: &[u8]) -> [u8; 32] {
        let mut mac =
            Hmac::<Sha256>::new_from_slice(&self.0).expect("HMAC accepts a key of any length");
        mac.update(label);
        mac.update(input);
        mac.finalize().into_bytes().into()
    }
}

impl fmt::Debug for BucketKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("BucketKey(redacted)")
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn labels_separate_names() {
        let key = BucketKey::random();

        assert_ne!(key.bucket(b"one", b"input"), key.bucket(b"two", b"input"));
    }

    #[test]
    fn the_same_input_gets_the_same_name() {
        let key = BucketKey::random();

        assert_eq!(
            key.bucket(b"label", b"input"),
            key.bucket(b"label", b"input")
        );
    }

    #[test]
    fn debug_hides_the_key() {
        assert_eq!(format!("{:?}", BucketKey::random()), "BucketKey(redacted)");
    }
}
