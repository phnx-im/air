// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

#![expect(clippy::large_enum_variant)]

use prost::bytes::Bytes;

tonic::include_proto!("relay_service.v1");

pub const METADATA_SESSION_ID: &str = "session-id";

impl LinkClientRequest {
    pub fn into_relay_frame(self) -> RelayFrame {
        RelayFrame::from_bytes(prost::Message::encode_to_vec(&self))
    }
}

impl RelayFrame {
    pub fn from_bytes(bytes: impl Into<Bytes>) -> Self {
        Self {
            payload: bytes.into(),
        }
    }

    pub fn as_slice(&self) -> &[u8] {
        self.payload.as_ref()
    }
}
