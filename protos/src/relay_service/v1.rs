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

/// The string is not a rendezvous ID.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("malformed rendezvous id")]
pub struct InvalidRendezVousId;

impl RendezVousId {
    /// A rendezvous ID from its digits.
    ///
    /// Only a non-empty run of ASCII digits is accepted.
    pub fn new(value: String) -> Result<Self, InvalidRendezVousId> {
        if value.is_empty() || !value.bytes().all(|b| b.is_ascii_digit()) {
            return Err(InvalidRendezVousId);
        }
        Ok(Self { value })
    }
}

impl fmt::Display for RendezVousId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.value.fmt(f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_digit_runs_are_ids() {
        assert!(RendezVousId::new("000".to_owned()).is_ok());
        assert!(RendezVousId::new("12345".to_owned()).is_ok());
        assert_eq!(RendezVousId::new(String::new()), Err(InvalidRendezVousId));
        assert_eq!(
            RendezVousId::new("12a".to_owned()),
            Err(InvalidRendezVousId)
        );
        assert_eq!(
            RendezVousId::new(" 12".to_owned()),
            Err(InvalidRendezVousId)
        );
    }
}
