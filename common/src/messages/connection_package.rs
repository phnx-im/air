// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Facts about connection packages that the client keeps and the server
//! stores. The package itself is the proto `SignedConnectionPackage` in
//! `airprotos`.

use chrono::Duration;
use serde::{Deserialize, Serialize};

use crate::{
    crypto::{Labeled, hash::Hash},
    time::ExpirationData,
};

pub const CONNECTION_PACKAGE_EXPIRATION: Duration = Duration::days(30);

/// Label of [`ConnectionPackageHash`]. The hash is computed over the signed
/// payload of the package in `airprotos`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConnectionPackageLabel;

impl Labeled for ConnectionPackageLabel {
    const LABEL: &'static str = "ConnectionPackage";
}

pub type ConnectionPackageHash = Hash<ConnectionPackageLabel>;

/// The facts about a connection package that its owner needs to keep outside the (possibly sealed)
/// package representation.
#[derive(Debug)]
pub struct ConnectionPackageMetadata {
    pub hash: ConnectionPackageHash,
    pub lifetime: ExpirationData,
    pub is_last_resort: bool,
}
