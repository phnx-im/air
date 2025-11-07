// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::crypto::RawKey;

use super::private_keys::{SigningKey, VerifyingKey, VerifyingKeyRef};

#[derive(Debug)]
pub struct LeafVerifyingKeyType;
pub type LeafVerifyingKeyRef<'a> = VerifyingKeyRef<'a, LeafVerifyingKeyType>;

#[derive(Debug)]
pub struct QsClientVerifyingKeyType;
pub type QsClientVerifyingKey = VerifyingKey<QsClientVerifyingKeyType>;

impl RawKey for QsClientVerifyingKeyType {}

pub type QsClientSigningKey = SigningKey<QsClientVerifyingKeyType>;

#[derive(Debug)]
pub struct QsUserVerifyingKeyType;
pub type QsUserVerifyingKey = VerifyingKey<QsUserVerifyingKeyType>;

impl RawKey for QsUserVerifyingKeyType {}

pub type QsUserSigningKey = SigningKey<QsUserVerifyingKeyType>;

#[cfg(test)]
mod test {
    use crate::codec::PersistenceCodec;

    use super::*;

    #[test]
    fn qs_client_verifying_key_serde_codec() {
        let key = QsClientVerifyingKey::new_for_test(vec![1, 2, 3]);
        insta::assert_binary_snapshot!(".cbor", PersistenceCodec::to_vec(&key).unwrap());
    }

    #[test]
    fn qs_client_verifying_key_serde_json() {
        let key = QsClientVerifyingKey::new_for_test(vec![1, 2, 3]);
        insta::assert_json_snapshot!(key);
    }
}
