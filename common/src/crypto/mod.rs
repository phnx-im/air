// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! A module that provides various traits and structs that allow the use of
//! cryptographic primitives such as AEAD, MACs and KDFs.
//!
//! TODO: Once const-generics allows the use of enums, we could get rid of a
//! number of structs and boilerplate code.
//! TODO: A proper RNG provider for use with all crypto functions that require
//! randomness, i.e. mainly secret and nonce sampling.
#![allow(unused_variables)]
use std::marker::PhantomData;

use hpke::{DecryptionKey, EncryptionKey};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tls_codec::{TlsDeserializeBytes, TlsSerialize, TlsSize};

use crate::{
    LibraryError,
    crypto::{ear::EarEncryptable, kdf::keys::ConnectionKeyType},
    messages::QueueMessage,
};

use self::{
    ear::{EarDecryptable, keys::RatchetKey},
    errors::RandomnessError,
    kdf::{KdfDerivable, keys::RatchetSecret},
};

pub mod ear;
pub mod errors;
pub mod hash;
pub mod hpke;
pub mod indexed_aead;
pub mod kdf;
pub mod ratchet;
pub mod secrets;
pub mod signatures;

/// Marker trait for keys that can be converted to and from raw bytes
pub trait RawKey {}

pub type RatchetKeyUpdate = Vec<u8>;

/// A trait for labeling structs
pub trait Labeled {
    const LABEL: &'static str;
}

impl<T: Labeled> Labeled for &T {
    const LABEL: &'static str = T::LABEL;
}

#[derive(Debug)]
pub struct RatchetKeyType;
pub type RatchetEncryptionKey = EncryptionKey<RatchetKeyType>;

impl RawKey for RatchetKeyType {}

pub type RatchetDecryptionKey = DecryptionKey<RatchetKeyType>;

pub type ConnectionEncryptionKey = EncryptionKey<ConnectionKeyType>;
pub type ConnectionDecryptionKey = DecryptionKey<ConnectionKeyType>;

#[cfg(test)]
mod test {
    use crate::codec::PersistenceCodec;

    use super::*;

    #[test]
    fn encryption_key_serde_codec() {
        let key = RatchetEncryptionKey::new_for_test(vec![1, 2, 3]);
        insta::assert_binary_snapshot!(".cbor", PersistenceCodec::to_vec(&key).unwrap());
    }

    #[test]
    fn encryption_key_serde_json() {
        let key = RatchetEncryptionKey::new_for_test(vec![1, 2, 3]);
        insta::assert_json_snapshot!(key);
    }
}
