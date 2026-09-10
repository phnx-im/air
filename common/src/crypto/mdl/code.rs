// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! The user-visible code of the multi-device linking protocol.
//!
//! A code is the decimal string `rendezvous_id || password || check_digit`.
//! The rendezvous ID is public and assigned by the relay. The 16-digit
//! password carries the whole security of the exchange. The Damm check
//! digit catches the typos that would otherwise burn a session, which is
//! why the existing device verifies it before it contacts the relay.

use std::fmt;

use rand::RngExt;
use secrecy::zeroize::Zeroize;

/// Digits of the secret half of a linking code.
pub const PASSWORD_DIGITS: usize = 16;

/// Digits of the shortest rendezvous ID the relay assigns.
pub const MIN_RENDEZVOUS_DIGITS: usize = 3;

/// Digits a linking code has at the very least.
pub const MIN_CODE_DIGITS: usize = MIN_RENDEZVOUS_DIGITS + PASSWORD_DIGITS + 1;

/// The order-10 quasigroup of the Damm algorithm, which is totally
/// antisymmetric and weakly totally antisymmetric. That is what makes the
/// check digit catch every single-digit error and every transposition of
/// two adjacent digits.
const DAMM_TABLE: [[u8; 10]; 10] = [
    [0, 3, 1, 7, 5, 9, 8, 6, 4, 2],
    [7, 0, 9, 2, 1, 5, 4, 8, 6, 3],
    [4, 2, 0, 6, 8, 7, 1, 3, 5, 9],
    [1, 7, 5, 0, 9, 8, 3, 4, 2, 6],
    [6, 1, 2, 3, 0, 4, 5, 9, 7, 8],
    [3, 6, 7, 4, 2, 0, 9, 5, 8, 1],
    [5, 8, 6, 9, 7, 2, 0, 1, 3, 4],
    [8, 9, 4, 5, 3, 6, 2, 0, 1, 7],
    [9, 4, 3, 8, 6, 1, 7, 2, 0, 5],
    [2, 5, 8, 1, 4, 3, 6, 7, 9, 0],
];

/// The Damm interim digit of a run of ASCII decimal digits.
///
/// Over a bare payload this is its check digit. Over a payload with its
/// check digit appended it is zero exactly when the code is intact.
///
/// Returns `None` if `digits` holds anything but ASCII decimal digits.
fn damm(digits: &str) -> Option<u8> {
    let mut interim = 0usize;
    for byte in digits.bytes() {
        let digit = usize::from(byte.checked_sub(b'0').filter(|d| *d < 10)?);
        interim = usize::from(DAMM_TABLE[interim][digit]);
    }
    u8::try_from(interim).ok()
}

/// Why a typed linking code could not be used.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum LinkingCodeError {
    /// The rendezvous ID is not a run of decimal digits.
    #[error("rendezvous id is not decimal")]
    InvalidRendezvousId,
    /// Fewer than [`MIN_CODE_DIGITS`] digits were entered.
    #[error("linking code is too short")]
    TooShort,
    /// The Damm check digit does not match, so the code was mistyped.
    #[error("linking code check digit does not match")]
    CheckDigitMismatch,
}

/// The secret half of a linking code.
///
/// Sixteen decimal digits, each drawn uniformly at random, which is about
/// 53.1 bits. It is the CPace password-related string, used once and never
/// stored.
#[derive(Clone, PartialEq, Eq)]
pub struct LinkingPassword(String);

impl Drop for LinkingPassword {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

impl LinkingPassword {
    /// Draws a fresh password from the thread CSPRNG.
    pub fn generate() -> Self {
        let mut rng = rand::rng();
        let digits = (0..PASSWORD_DIGITS)
            .map(|_| char::from(b'0' + rng.random_range(0..10u8)))
            .collect();
        Self(digits)
    }

    /// The digits, which are what CPace takes as its `PRS`.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for LinkingPassword {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("LinkingPassword(<redacted>)")
    }
}

/// A linking code, either freshly generated or parsed from user input.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct LinkingCode {
    rendezvous_id: String,
    password: LinkingPassword,
}

impl LinkingCode {
    /// A code for `rendezvous_id` with a fresh password.
    pub fn generate(rendezvous_id: &str) -> Result<Self, LinkingCodeError> {
        Self::new(rendezvous_id, LinkingPassword::generate())
    }

    /// A code pairing `rendezvous_id` with an existing password.
    pub fn new(rendezvous_id: &str, password: LinkingPassword) -> Result<Self, LinkingCodeError> {
        if damm(rendezvous_id).is_none() {
            return Err(LinkingCodeError::InvalidRendezvousId);
        }
        Ok(Self {
            rendezvous_id: rendezvous_id.to_owned(),
            password,
        })
    }

    pub fn rendezvous_id(&self) -> &str {
        &self.rendezvous_id
    }

    pub fn password(&self) -> &LinkingPassword {
        &self.password
    }

    /// The canonical digit string the user carries to the other device.
    pub fn to_digits(&self) -> String {
        let mut digits = self.rendezvous_id.clone();
        digits.push_str(self.password.as_str());
        let check = damm(&digits).unwrap_or_default();
        digits.push(char::from(b'0' + check));
        digits
    }

    /// Parses user input into a code.
    ///
    /// Non-digit characters are ignored, so grouping into blocks and any
    /// separators the user copied along do not matter. The check digit is
    /// verified here, before the caller contacts the relay, because a
    /// mistyped password would otherwise burn the session.
    pub fn parse(input: &str) -> Result<Self, LinkingCodeError> {
        let digits: String = input.chars().filter(char::is_ascii_digit).collect();
        if digits.len() < MIN_CODE_DIGITS {
            return Err(LinkingCodeError::TooShort);
        }
        if damm(&digits) != Some(0) {
            return Err(LinkingCodeError::CheckDigitMismatch);
        }

        // The password has a fixed length, so the rendezvous ID can grow
        // without a delimiter as long as the split happens from the end.
        let password_start = digits.len() - PASSWORD_DIGITS - 1;
        let check_start = digits.len() - 1;
        Ok(Self {
            rendezvous_id: digits[..password_start].to_owned(),
            password: LinkingPassword(digits[password_start..check_start].to_owned()),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn code(rendezvous_id: &str, password: &str) -> String {
        LinkingCode::new(rendezvous_id, LinkingPassword(password.to_owned()))
            .unwrap()
            .to_digits()
    }

    #[test]
    fn generated_passwords_are_sixteen_digits() {
        let password = LinkingPassword::generate();
        assert_eq!(password.as_str().len(), PASSWORD_DIGITS);
        assert!(password.as_str().bytes().all(|b| b.is_ascii_digit()));
    }

    #[test]
    fn a_generated_code_round_trips() {
        let generated = LinkingCode::generate("417").unwrap();
        let parsed = LinkingCode::parse(&generated.to_digits()).unwrap();
        assert_eq!(parsed, generated);
    }

    #[test]
    fn separators_and_grouping_are_ignored() {
        let generated = LinkingCode::generate("417").unwrap();
        let digits = generated.to_digits();
        let spaced = format!(
            "{} - {} {} {} {} - {}",
            &digits[..3],
            &digits[3..7],
            &digits[7..11],
            &digits[11..15],
            &digits[15..19],
            &digits[19..],
        );
        assert_eq!(LinkingCode::parse(&spaced).unwrap(), generated);
    }

    #[test]
    fn the_split_runs_from_the_end() {
        let generated = LinkingCode::generate("1234567").unwrap();
        let parsed = LinkingCode::parse(&generated.to_digits()).unwrap();
        assert_eq!(parsed.rendezvous_id(), "1234567");
        assert_eq!(parsed.password().as_str().len(), PASSWORD_DIGITS);
    }

    #[test]
    fn short_input_is_rejected_before_the_check_digit() {
        let short = "1".repeat(MIN_CODE_DIGITS - 1);
        assert_eq!(LinkingCode::parse(&short), Err(LinkingCodeError::TooShort));
    }

    #[test]
    fn a_non_decimal_rendezvous_id_is_rejected() {
        assert_eq!(
            LinkingCode::generate("41a"),
            Err(LinkingCodeError::InvalidRendezvousId)
        );
    }

    #[test]
    fn every_single_digit_error_is_caught() {
        let digits = code("417", "5093184726109958");
        for position in 0..digits.len() {
            for replacement in b'0'..=b'9' {
                let mut typo = digits.clone().into_bytes();
                if typo[position] == replacement {
                    continue;
                }
                typo[position] = replacement;
                let typo = String::from_utf8(typo).unwrap();
                assert_eq!(
                    LinkingCode::parse(&typo),
                    Err(LinkingCodeError::CheckDigitMismatch),
                    "single-digit error at {position} slipped through"
                );
            }
        }
    }

    #[test]
    fn every_adjacent_transposition_is_caught() {
        let digits = code("417", "5093184726109958");
        for position in 0..digits.len() - 1 {
            let mut swapped = digits.clone().into_bytes();
            if swapped[position] == swapped[position + 1] {
                continue;
            }
            swapped.swap(position, position + 1);
            let swapped = String::from_utf8(swapped).unwrap();
            assert_eq!(
                LinkingCode::parse(&swapped),
                Err(LinkingCodeError::CheckDigitMismatch),
                "transposition at {position} slipped through"
            );
        }
    }

    #[test]
    fn the_check_digit_follows_the_damm_definition() {
        // The Damm run over payload plus check digit lands on zero.
        let digits = code("417", "5093184726109958");
        assert_eq!(damm(&digits), Some(0));
        assert_eq!(
            damm(&digits[..digits.len() - 1]).map(|d| char::from(b'0' + d)),
            digits.chars().next_back()
        );
    }
}
