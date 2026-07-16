// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use mls_assist::openmls::{
    components::vc_derivation_info::{EpochId, KeyPackageUpload},
    prelude::LeafNodeIndex,
};
use serde::{Deserialize, Serialize};
use tls_codec::{TlsDeserializeBytes, TlsSerialize, TlsSize};

/// Identifier of a key package batch
///
/// Identifies a batch of key packages that can be used by any emulation client that belongs to a
/// virtual client.
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TlsSerialize, TlsDeserializeBytes, TlsSize,
)]
pub struct KeyPackageBatchId {
    /// Epoch ID in the virtual client self-group
    pub epoch_id: EpochId,
    /// The index of the leaf in the virtual client self-group of the client who created the batch
    pub leaf_index: LeafNodeIndex,
    /// Generation in the key package ratchet of the client who created the batch
    pub generation: u32,
}

impl KeyPackageBatchId {
    /// Returns `true` if the batch ID matches the given upload, otherwise `false`.
    pub fn matches_upload(&self, upload: &KeyPackageUpload) -> bool {
        self.epoch_id == upload.epoch_id
            && self.leaf_index == upload.leaf_index
            && self.generation == upload.generation
    }
}
