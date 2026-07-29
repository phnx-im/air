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

/// Exporter secret for a self-group, scoped to a single group, epoch and
/// component.
///
/// It holds the 32-byte output of the MLS safe exporter
/// (`safe_export_secret(AIR_COMPONENT_ID)`) on the self-group's T group. The
/// exporter secret itself is never used as an encryption key; it is only a KDF
/// key from which the per-epoch [`crate::crypto::aead::keys::SelfGroupMessageKey`]
/// is derived.
#[derive(Debug)]
pub struct SelfGroupExporterSecretType;
pub type SelfGroupExporterSecret = Key<SelfGroupExporterSecretType>;

impl RawKey for SelfGroupExporterSecretType {}

impl KdfKey for SelfGroupExporterSecret {
    const ADDITIONAL_LABEL: &'static str = "SelfGroupExporterSecret";
}
