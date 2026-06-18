// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use openmls::component::ComponentId;
use tls_codec::{TlsDeserializeBytes, TlsSerialize, TlsSize, VLBytes};

/// The component ID of the virtual client key package upload component
///
/// Private-use range
pub const VIRTUAL_CLIENT_KP_UPLOAD_COMPONENT_ID: ComponentId = 0x8002;

#[derive(Clone, TlsSerialize, TlsDeserializeBytes, TlsSize)]
pub struct VirtualClientKeyPackageUpload {
    pub epoch_id: VLBytes,
    pub random: VLBytes,
}
