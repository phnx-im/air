// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use openmls::{
    component::ComponentId, components::vc_derivation_info::KeyPackageUpload,
    prelude::ProcessedMessage,
};
use tls_codec::DeserializeBytes;

/// The component ID of the virtual client key package upload component
///
/// Private-use range
pub const VIRTUAL_CLIENT_KP_UPLOAD_COMPONENT_ID: ComponentId = 0x8002;

/// Extract the [`KeyPackageUpload`] from the message's safe AAD component.
///
/// Returns `None` if the component is not present.
pub fn extract_key_package_upload(
    message: &ProcessedMessage,
) -> Result<Option<KeyPackageUpload>, tls_codec::Error> {
    message
        .safe_aad_item(VIRTUAL_CLIENT_KP_UPLOAD_COMPONENT_ID)
        .map(KeyPackageUpload::tls_deserialize_exact_bytes)
        .transpose()
}
