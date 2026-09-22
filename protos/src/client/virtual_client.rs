// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Emulator client actions, communicated in the SafeAAD of a self-group commit.
//!
//! The wire format is defined by the mls-virtual-clients draft and implemented
//! in openmls, see [`VirtualClientCommitData`].

pub use openmls::components::vc_commit_data::{
    VcCommitDataError, VirtualClientAction, VirtualClientCommitData,
};
use openmls::{components::vc_derivation_info::VC_COMPONENT_ID, prelude::ProcessedMessage};

/// Extract the [`VirtualClientCommitData`] from the message's safe AAD component.
///
/// Returns `None` if the component is not present.
pub fn extract_virtual_client_commit_data(
    message: &ProcessedMessage,
) -> Result<Option<VirtualClientCommitData>, VcCommitDataError> {
    message
        .safe_aad_item(VC_COMPONENT_ID)
        .map(VirtualClientCommitData::from_safe_aad_item_data)
        .transpose()
}
