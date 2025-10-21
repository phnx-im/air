// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Common data model used in the server and client.

use std::fmt::Display;

pub use mls_assist::openmls_rust_crypto::RustCrypto;
pub use mls_assist::openmls_traits::random::OpenMlsRand;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tls_codec::{
    DeserializeBytes as TlsDeserializeBytesTrait, Serialize as TlsSerializeTrait, Size,
    TlsDeserializeBytes, TlsSerialize, TlsSize,
};

pub mod assert_matches;
pub mod codec;
pub mod credentials;
pub mod crypto;
pub mod endpoint_paths;
pub mod identifiers;
pub mod messages;
pub mod mls_group_config;
pub mod pow;
pub mod time;

pub const DEFAULT_PORT_HTTP: u16 = 9420;
pub const DEFAULT_PORT_HTTPS: u16 = 443;
pub const DEFAULT_PORT_GRPC: u16 = 50051;

pub const ACCEPTED_API_VERSIONS_HEADER: &str = "x-accepted-api-versions";

/// Unrecoverable error in this implementation.
#[derive(Debug, Clone, Error, Serialize, Deserialize)]
pub struct LibraryError;

impl LibraryError {
    pub fn missing_bound_check(_error: tls_codec::Error) -> Self {
        LibraryError {}
    }

    pub fn unexpected_crypto_error(_error: &str) -> Self {
        LibraryError {}
    }
}

impl Display for LibraryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
