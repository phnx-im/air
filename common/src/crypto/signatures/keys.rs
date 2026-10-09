// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use zeroize::Zeroize;

use crate::crypto::{
    RawKey,
    kdf::{Kdf, keys::SiblingSecret},
    signatures::signable::Signature,
};

use super::private_keys::{SigningKey, VerifyingKey, VerifyingKeyRef};

#[derive(Debug)]
pub struct LeafVerifyingKeyType;
pub type LeafVerifyingKeyRef<'a> = VerifyingKeyRef<'a, LeafVerifyingKeyType>;

#[derive(Debug)]
pub struct QsClientVerifyingKeyType;
pub type QsClientVerifyingKey = VerifyingKey<QsClientVerifyingKeyType>;

impl RawKey for QsClientVerifyingKeyType {}

pub type QsClientSigningKey = SigningKey<QsClientVerifyingKeyType>;

pub type QsClientSignature = Signature<QsClientVerifyingKeyType>;

#[derive(Debug)]
pub struct QsUserVerifyingKeyType;
pub type QsUserVerifyingKey = VerifyingKey<QsUserVerifyingKeyType>;

impl RawKey for QsUserVerifyingKeyType {}

pub type QsUserSigningKey = SigningKey<QsUserVerifyingKeyType>;

/// Salt for extracting the [`SiblingSecret`] from the QS user signing key
const SIBLING_SECRET_SALT: &[u8] = b"air sibling secret v1";

impl QsUserSigningKey {
    /// Derives the secret shared by all clients of the user.
    ///
    /// All clients of a user share the QS user signing key, while the server
    /// only knows its verifying key.
    pub fn derive_sibling_secret(&self) -> SiblingSecret {
        let (mut prk, _) = Kdf::extract(Some(SIBLING_SECRET_SALT), &self.signing_key);
        let sibling_secret = SiblingSecret::from_bytes(prk.into());
        prk.as_mut_slice().zeroize();
        sibling_secret
    }
}

pub type QsUserSignature = Signature<QsUserVerifyingKeyType>;

#[cfg(test)]
mod test {
    use crate::codec::PersistenceCodec;

    use super::*;

    #[test]
    fn qs_client_verifying_key_serde_codec() {
        let key = QsClientVerifyingKey::new_for_test(vec![1, 2, 3]);
        let bytes = PersistenceCodec::to_vec(&key).unwrap();
        let diag = cbor_diag::parse_bytes(&bytes[1..]).unwrap().to_hex();
        insta::assert_snapshot!(diag);
    }

    #[test]
    fn qs_client_verifying_key_serde_json() {
        let key = QsClientVerifyingKey::new_for_test(vec![1, 2, 3]);
        insta::assert_json_snapshot!(key);
    }
}
