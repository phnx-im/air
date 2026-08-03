// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use openmls::{
    components::vc_derivation_info::{KeyPackageUpload, VC_COMPONENT_ID},
    prelude::ProcessedMessage,
};
use tls_codec::{DeserializeBytes, TlsDeserializeBytes, TlsSerialize, TlsSize};

/// Emulator client action to communicate in the SafeAAD key package component of a commit in the
/// self-group.
///
/// TODO: Will be moved to the openmls crate.
///
/// ```tls
/// enum {
///   reserved(0),
///   key_package_upload(1),
///   (255)
/// } ActionType;
///
/// struct {
///   ActionType action_type;
///   select (VirtualClientAction.action_type) {
///     case key_package_upload:
///       KeyPackageUpload key_package_upload;
///   };
/// } VirtualClientAction;
/// ```
#[derive(Debug, TlsSerialize, TlsDeserializeBytes, TlsSize)]
#[repr(u8)]
pub enum VirtualClientAction {
    #[tls_codec(discriminant = 1)]
    KeyPackageUpload(KeyPackageUpload),
}

/// Extract the [`VirtualClientAction`] from the message's safe AAD component.
///
/// Returns `None` if the component is not present.
pub fn extract_virtual_client_action(
    message: &ProcessedMessage,
) -> Result<Option<VirtualClientAction>, tls_codec::Error> {
    message
        .safe_aad_item(VC_COMPONENT_ID)
        .map(VirtualClientAction::tls_deserialize_exact_bytes)
        .transpose()
}
