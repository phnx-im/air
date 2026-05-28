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

pub struct SessionId {}

impl SessionId {
    pub fn generate(bytes: &[u8], mut has_collision: impl FnMut(&str) -> bool) -> Option<String> {
        const MIN_DIGITS: u32 = 8;

        let n = u64::from_be_bytes(bytes.get(..8)?.try_into().ok()?);
        let max_digits = u64::MAX.ilog10(); // 19
        for digits in MIN_DIGITS..=max_digits {
            let code = format!("{:0width$}", n % 10u64.pow(digits), width = digits as usize);
            if !has_collision(&code) {
                return Some(code);
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bytes_for(n: u64) -> [u8; 8] {
        n.to_be_bytes()
    }

    #[test]
    fn returns_8_digit_code_when_no_collision() {
        let code = SessionId::generate(&bytes_for(12345), |_| false).unwrap();
        assert_eq!(code.len(), 8);
    }

    #[test]
    fn code_is_zero_padded_to_8_digits() {
        // n=5 → 5 % 10^8 = 5 → formatted as "00000005"
        let code = SessionId::generate(&bytes_for(5), |_| false).unwrap();
        assert_eq!(code, "00000005");
    }

    #[test]
    fn escalates_digits_on_collision() {
        // collide on every 8-digit code; should escalate to 9 digits
        let code = SessionId::generate(&bytes_for(12345), |c| c.len() == 8).unwrap();
        assert_eq!(code.len(), 9);
    }

    #[test]
    fn escalates_multiple_times() {
        // collide on 8 and 9 digit codes; should escalate to 10 digits
        let code = SessionId::generate(&bytes_for(12345), |c| c.len() <= 9).unwrap();
        assert_eq!(code.len(), 10);
    }

    #[test]
    fn all_lengths_collide() {
        assert!(SessionId::generate(&bytes_for(12345), |_| true).is_none());
    }

    #[test]
    fn input_too_short() {
        assert!(SessionId::generate(&[0u8; 7], |_| false).is_none());
    }

    #[test]
    fn accepts_exactly_8_bytes() {
        assert!(SessionId::generate(&[0u8; 8], |_| false).is_some());
    }

    #[test]
    fn uses_only_first_8_bytes() {
        let mut a = [0u8; 16];
        a[..8].copy_from_slice(&bytes_for(42));
        let mut b = [0u8; 16];
        b[..8].copy_from_slice(&bytes_for(42));
        b[8..].fill(0xff); // tail bytes differ
        assert_eq!(
            SessionId::generate(&a, |_| false),
            SessionId::generate(&b, |_| false),
        );
    }

    #[test]
    fn code_contains_only_digits() {
        let code = SessionId::generate(&bytes_for(u64::MAX), |_| false).unwrap();
        assert!(code.chars().all(|c| c.is_ascii_digit()));
    }
}
