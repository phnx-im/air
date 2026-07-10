// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use mls_assist::openmls::{components::vc_derivation_info::EpochId, prelude::LeafNodeIndex};
use tls_codec::{DeserializeBytes, Serialize, VLByteSlice, VLBytes};

/// Identifier of a key package batch
///
/// Identifies a batch of key packages that can be used by any emulation client that belongs to a
/// virtual client.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyPackageBatchId {
    /// Epoch ID in the virtual client self-group
    pub epoch_id: EpochId,
    /// The index of the leaf in the virtual client self-group of the client who created the batch
    pub leaf_index: LeafNodeIndex,
    /// Generation in the key package ratchet of the client who created the batch
    pub generation: u32,
}

pub trait EpochIdExt {
    fn from_bytes(bytes: &[u8]) -> Self;
    fn to_bytes(&self) -> Vec<u8>;
}

impl EpochIdExt for EpochId {
    fn from_bytes(bytes: &[u8]) -> Self {
        // TODO: This is a temporary workaround for the lack of epoch ID bytes getter.
        let bytes = VLByteSlice(bytes).tls_serialize_detached().unwrap();
        EpochId::tls_deserialize_exact_bytes(&bytes).unwrap()
    }

    fn to_bytes(&self) -> Vec<u8> {
        // TODO: This is a temporary workaround for the lack of epoch ID bytes getter.
        let bytes = EpochId::tls_serialize_detached(self).unwrap();
        VLBytes::tls_deserialize_exact_bytes(&bytes).unwrap().into()
    }
}
