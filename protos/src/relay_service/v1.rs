// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::fmt;

use prost::bytes::Bytes;

tonic::include_proto!("relay_service.v1");

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

impl RendezvousId {
    pub fn new(value: String) -> Self {
        Self { value }
    }

    pub fn as_str(&self) -> &str {
        &self.value
    }

    /// Whether this is a well-formed rendezvous ID, that is a non-empty run
    /// of ASCII digits.
    pub fn is_well_formed(&self) -> bool {
        !self.value.is_empty() && self.value.bytes().all(|b| b.is_ascii_digit())
    }
}

impl fmt::Display for RendezvousId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.value.fmt(f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn well_formed_ids_are_digit_runs() {
        assert!(RendezvousId::new("000".to_owned()).is_well_formed());
        assert!(RendezvousId::new("12345".to_owned()).is_well_formed());
        assert!(!RendezvousId::new(String::new()).is_well_formed());
        assert!(!RendezvousId::new("12a".to_owned()).is_well_formed());
        assert!(!RendezvousId::new(" 12".to_owned()).is_well_formed());
    }
}
