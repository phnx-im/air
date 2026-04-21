// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! This module contains structs implementing the various AEAD keys. Keys can
//! either provide their own constructors or implement the [`KdfDerivable`]
//! trait to allow derivation from other key.

use crate::crypto::{
    RawKey,
    indexed_aead::keys::{Key, RandomlyGeneratable},
    kdf::{KdfDerivable, keys::RatchetSecret},
};

use super::{AEAD_KEY_SIZE, Ciphertext, traits::AeadKey};

// Group state EAR key

/// Key to encrypt/decrypt the roster of the DS group state. Roster keys can be
/// derived either from an initial client KDF key or from a derived roster KDF
/// key.
#[derive(Debug)]
pub struct GroupStateEarKeyType;

impl RandomlyGeneratable for GroupStateEarKeyType {}

impl AeadKey for GroupStateEarKey {}

pub type GroupStateEarKey = Key<GroupStateEarKeyType>;

// Push token ear key

/// EAR key for the [`crate::messages::push_token::PushToken`] structs.
#[derive(Debug)]
pub struct PushTokenEarKeyType;

pub type PushTokenEarKey = Key<PushTokenEarKeyType>;

impl RandomlyGeneratable for PushTokenEarKeyType {}

impl AeadKey for PushTokenEarKey {}

// Client credential EAR key

#[derive(Debug)]
pub struct ClientCredentialEarKeyType;

pub type ClientCredentialEarKey = Key<ClientCredentialEarKeyType>;

impl RandomlyGeneratable for ClientCredentialEarKeyType {}

impl AeadKey for ClientCredentialEarKey {}

// Ratchet key

#[derive(Debug)]
pub struct RatchetKeyType;

pub type RatchetKey = Key<RatchetKeyType>;

impl AeadKey for RatchetKey {}

impl KdfDerivable<RatchetSecret, Vec<u8>, AEAD_KEY_SIZE> for RatchetKey {
    const LABEL: &'static str = "RatchetKey";
}

// WelcomeAttributionInfo EAR key

#[derive(Debug)]
pub struct WelcomeAttributionInfoEarKeyType;

pub type WelcomeAttributionInfoEarKey = Key<WelcomeAttributionInfoEarKeyType>;

impl RandomlyGeneratable for WelcomeAttributionInfoEarKeyType {}

impl AeadKey for WelcomeAttributionInfoEarKey {}

// FriendshipPackage EAR key

#[derive(Debug)]
pub struct FriendshipPackageEarKeyType;

pub type FriendshipPackageEarKey = Key<FriendshipPackageEarKeyType>;

impl RandomlyGeneratable for FriendshipPackageEarKeyType {}

impl AeadKey for FriendshipPackageEarKey {}

// Identity link wrapper key

#[derive(Debug)]
pub struct IdentityLinkWrapperKeyType;

pub type IdentityLinkWrapperKey = Key<IdentityLinkWrapperKeyType>;

impl RandomlyGeneratable for IdentityLinkWrapperKeyType {}

impl AeadKey for IdentityLinkWrapperKey {}

#[derive(Debug)]
pub struct EncryptedUserProfileKeyCtype;
pub type EncryptedUserProfileKey = Ciphertext<EncryptedUserProfileKeyCtype>;

#[derive(Debug)]
pub struct AttachmentEarKeyType;

impl RawKey for AttachmentEarKeyType {}

pub type AttachmentEarKey = Key<AttachmentEarKeyType>;

impl RandomlyGeneratable for AttachmentEarKeyType {}

impl AeadKey for AttachmentEarKey {}
