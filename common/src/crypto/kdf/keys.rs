// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! This module contains structs implementing keys that other keys can be
//! derived from. For keys (or other values) to be derived from one of these
//! keys, the target key (or value) needs to implement the [`KdfDerivable`]
//! trait.

use crate::crypto::{
    RawKey,
    indexed_aead::keys::{Key, RandomlyGeneratable},
};

use super::{KDF_KEY_SIZE, KdfDerivable, traits::KdfKey};

#[derive(Debug)]
pub struct RatchetSecretKeyType;
pub type RatchetSecret = Key<RatchetSecretKeyType>;

impl RandomlyGeneratable for RatchetSecretKeyType {}

impl KdfKey for RatchetSecret {
    const ADDITIONAL_LABEL: &'static str = "RatchetSecret";
}

impl KdfDerivable<RatchetSecret, Vec<u8>, KDF_KEY_SIZE> for RatchetSecret {
    const LABEL: &'static str = "RatchetSecret derive";
}

#[derive(Debug)]
pub struct ConnectionKeyType;

impl RawKey for ConnectionKeyType {}
