// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! This module contains structs and enums that represent messages that are
//! passed between clients and the backend.
//! TODO: We should eventually factor this module out, together with the crypto
//! module, to allow re-use by the client implementation.

use mls_assist::openmls::prelude::{KeyPackage, KeyPackageIn};

use crate::{
    crypto::{
        RatchetEncryptionKey,
        hpke::ClientIdEncryptionKey,
        kdf::keys::RatchetSecret,
        signatures::keys::{QsClientVerifyingKey, QsUserVerifyingKey},
    },
    identifiers::{QsClientId, QsUserId},
};

use super::{FriendshipToken, push_token::EncryptedPushToken};

#[derive(serde::Serialize, serde::Deserialize)]
pub struct QsOpenWsParams {
    pub queue_id: QsClientId,
}

// === User ===

#[derive(Debug)]
pub struct CreateUserRecordParams {
    pub user_record_auth_key: QsUserVerifyingKey,
    pub friendship_token: FriendshipToken,
    pub client_record_auth_key: QsClientVerifyingKey,
    pub queue_encryption_key: RatchetEncryptionKey,
    pub encrypted_push_token: Option<EncryptedPushToken>,
    pub initial_ratchet_secret: RatchetSecret,
}

#[derive(Debug)]
#[cfg_attr(test, derive(Clone, PartialEq, Eq))]
pub struct CreateUserRecordResponse {
    pub user_id: QsUserId,
    pub qs_client_id: QsClientId,
}

#[derive(Debug)]
pub struct UpdateUserRecordParams {
    pub sender: QsUserId,
    pub user_record_auth_key: QsUserVerifyingKey,
    pub friendship_token: FriendshipToken,
}

#[derive(Debug)]
pub struct DeleteUserRecordParams {
    pub sender: QsUserId,
}

// === Client ===

#[derive(Debug)]
pub struct CreateClientRecordParams {
    pub sender: QsUserId,
    pub client_record_auth_key: QsClientVerifyingKey,
    pub queue_encryption_key: RatchetEncryptionKey,
    pub encrypted_push_token: Option<EncryptedPushToken>,
    pub initial_ratchet_secret: RatchetSecret, // TODO: This can be dropped once we support PCS
}

#[derive(Debug)]
pub struct CreateClientRecordResponse {
    pub qs_client_id: QsClientId,
}

#[derive(Debug)]
pub struct UpdateClientRecordParams {
    pub sender: QsClientId,
    pub client_record_auth_key: QsClientVerifyingKey,
    pub queue_encryption_key: RatchetEncryptionKey,
    pub encrypted_push_token: Option<EncryptedPushToken>,
}

#[derive(Debug)]
pub struct DeleteClientRecordParams {
    pub sender: QsClientId,
}

#[derive(Debug)]
pub struct PublishKeyPackagesParams {
    pub sender: QsClientId,
    pub key_packages: Vec<KeyPackageIn>,
}

#[derive(Debug)]
pub struct KeyPackageParams {
    pub sender: FriendshipToken,
}

#[derive(Debug)]
pub struct KeyPackageResponse {
    pub key_package: KeyPackage,
}

#[derive(Debug)]
pub struct KeyPackageResponseIn {
    pub key_package: KeyPackageIn,
}

#[derive(Debug)]
pub struct EncryptionKeyResponse {
    pub encryption_key: ClientIdEncryptionKey,
}
