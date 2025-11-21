// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use tls_codec::{TlsDeserializeBytes, TlsSerialize, TlsSize};

use crate::clients::connection_offer::payload::ConnectionInfo;

#[derive(Debug, Clone, TlsSize, TlsSerialize, TlsDeserializeBytes)]
#[repr(u8)]
pub(crate) enum TargetedMessageContent {
    ConnectionRequest(ConnectionInfo),
}
