// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! This module contains structs and enums that represent messages that are
//! passed between clients and the backend.
//! TODO: We should eventually factor this module out, together with the crypto
//! module, to allow re-use by the client implementation.

use mls_assist::openmls::prelude::{KeyPackage, KeyPackageIn};

use crate::{
    crypto::hpke::ClientIdEncryptionKey,
    identifiers::{QsClientId, QsUserId},
};

// === User ===

#[derive(Debug)]
#[cfg_attr(test, derive(Clone, PartialEq, Eq))]
pub struct CreateUserRecordResponse {
    pub user_id: QsUserId,
    pub qs_client_id: QsClientId,
}

// === Client ===

#[derive(Debug)]
pub struct CreateClientRecordResponse {
    pub qs_client_id: QsClientId,
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
