// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

#![expect(clippy::large_enum_variant)]

use prost::bytes::Bytes;

tonic::include_proto!("relay_service.v1");

pub const METADATA_SESSION_ID: &str = "session-id";

impl LinkClientRequest {
    pub fn into_relay_frame(self) -> RelayFrame {
        prost::Message::encode_to_vec(&self).into()
    }
}

impl<B: Into<Bytes>> From<B> for RelayFrame {
    fn from(b: B) -> Self {
        Self { payload: b.into() }
    }
}

impl RelayFrame {
    pub fn as_slice(&self) -> &[u8] {
        self.payload.as_ref()
    }
}
