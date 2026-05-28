// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

#![expect(clippy::large_enum_variant)]

use std::fmt;

use prost::bytes::Bytes;
use sha2::{Digest, Sha256};

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

    pub fn as_u32(&self) -> Option<u32> {
        self.payload
            .as_ref()
            .try_into()
            .ok()
            .map(u32::from_be_bytes)
    }
}

impl LinkingSessionId {
    pub fn from_digest(sha256: &[u8; 32], digits: u32) -> Option<Self> {
        sha256[..8]
            .try_into()
            .ok()
            .map(u64::from_be_bytes)
            .map(|n| Self {
                value: format!("{:0width$}", n % 10u64.pow(digits), width = digits as usize),
            })
    }

    pub fn generate(bytes: &[u8], mut has_collision: impl FnMut(&Self) -> bool) -> Option<Self> {
        let digest: [u8; 32] = Sha256::digest(bytes).into();
        for digits in 8..=16 {
            let code = Self::from_digest(&digest, digits)?;
            if !has_collision(&code) {
                return Some(code);
            }
        }
        None
    }

    pub fn validate(&self, bytes: &[u8]) -> bool {
        let digest: [u8; 32] = Sha256::digest(bytes).into();
        let digits = self.len();
        Self::from_digest(&digest, digits).is_some_and(|other| other == *self)
    }

    pub fn len(&self) -> u32 {
        self.value.len() as u32
    }
}

impl AsRef<[u8]> for LinkingSessionId {
    fn as_ref(&self) -> &[u8] {
        self.value.as_bytes()
    }
}

impl fmt::Display for LinkingSessionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.value.fmt(f)
    }
}

impl<S: Into<String>> From<S> for LinkingSessionId {
    fn from(s: S) -> Self {
        Self { value: s.into() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn returns_8_digit_code_when_no_collision() {
        let code = LinkingSessionId::generate("hello-world".as_bytes(), |_| false).unwrap();
        assert_eq!(code.len(), 8);
    }

    #[test]
    fn escalates_digits_on_collision() {
        // collide on every 8-digit code; should escalate to 9 digits
        let code = LinkingSessionId::generate("hello-world".as_bytes(), |c| c.len() == 8).unwrap();
        assert_eq!(code.len(), 9);
    }

    #[test]
    fn escalates_multiple_times() {
        // collide on 8 and 9 digit codes; should escalate to 10 digits
        let code = LinkingSessionId::generate("hello-world".as_bytes(), |c| c.len() <= 9).unwrap();
        assert_eq!(code.len(), 10);
    }

    #[test]
    fn all_lengths_collide() {
        assert!(LinkingSessionId::generate("hello-world".as_bytes(), |_| true).is_none());
    }

    #[test]
    fn code_contains_only_digits() {
        let code = LinkingSessionId::generate("hello-world".as_bytes(), |_| false).unwrap();
        assert!(code.value.chars().all(|c| c.is_ascii_digit()));
    }
}
