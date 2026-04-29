// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

// Note: This module should be part of the `airprotos` crate, but we cannot do that because it
// depends on `aircommon` which creates a circular dependency.

use airmacros::{DeserializeTaggedMap, SerializeTaggedMap};

use crate::codec::{self, PersistenceCodec};

/// Custom component storing client-specific features and data.
///
/// Stored in the app data extension of the group context, leaf node or key package.
#[derive(Debug, Default, SerializeTaggedMap, DeserializeTaggedMap)]
pub struct AirComponent {
    /// Features supported by the client in the corresponding context (group, leaf node or key
    /// package).
    #[tag(1)]
    pub features: AirFeatures,
}

/// List of features supported by the client.
#[derive(Debug, Default, PartialEq, Eq, SerializeTaggedMap, DeserializeTaggedMap)]
pub struct AirFeatures {
    /// Whether the client supports encrypted group profiles.
    #[tag(1)]
    pub encrypted_group_profiles: bool,
    /// Whether the client supports [APQMLS] (Amortized PQ MLS Combiner).
    ///
    /// [APQMLS]: https://datatracker.ietf.org/doc/html/draft-ietf-mls-combiner
    #[tag(2)]
    pub pq_groups: bool,
}

impl AirComponent {
    /// Creates a new air component with all supported features to be stored in a leaf node or a
    /// key package.
    ///
    /// Note: This is *not* the default implementation of `AirComponent::default`. It contains all
    /// supported features of the current version of the client.
    pub fn default_leaf_or_key_package_component() -> Self {
        Self {
            features: AirFeatures {
                encrypted_group_profiles: true,
                pq_groups: true,
            },
        }
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>, codec::Error> {
        PersistenceCodec::to_vec(self)
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, codec::Error> {
        PersistenceCodec::from_slice(bytes)
    }
}
