// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! The user-visible code of the multi-device linking protocol.
//!
//! A code is the decimal string `rendezvous_id || password`. The rendezvous
//! ID is public and assigned by the relay.

use std::fmt;

use rand::RngExt;
use secrecy::zeroize::Zeroize;

/// Digits of the secret half of a linking code.
pub const PASSWORD_DIGITS: usize = 5;

/// Digits of the shortest rendezvous ID the relay assigns.
pub const MIN_RENDEZVOUS_DIGITS: usize = 3;

/// Digits a linking code has at the very least.
pub const MIN_CODE_DIGITS: usize = MIN_RENDEZVOUS_DIGITS + PASSWORD_DIGITS;

/// Why a typed linking code could not be used.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum LinkingCodeError {
    /// The rendezvous ID is not a run of decimal digits.
    #[error("rendezvous id is not decimal")]
    InvalidRendezvousId,
    /// Fewer than [`MIN_CODE_DIGITS`] digits were entered.
    #[error("linking code is too short")]
    TooShort,
}

/// The secret half of a linking code.
///
/// Five decimal digits, each drawn uniformly at random, which is about 16.6
/// bits.
#[derive(Clone, PartialEq, Eq)]
pub struct LinkingPassword([u8; PASSWORD_DIGITS]);

impl Drop for LinkingPassword {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

impl LinkingPassword {
    /// Draws a fresh password from the thread CSPRNG.
    pub fn generate() -> Self {
        let mut rng = rand::rng();
        Self(std::array::from_fn(|_| b'0' + rng.random_range(0..10u8)))
    }

    /// The ASCII digits, which are what CPace takes as its `PRS`.
    pub fn as_bytes(&self) -> &[u8; PASSWORD_DIGITS] {
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
        if rendezvous_id.is_empty() || !rendezvous_id.bytes().all(|b| b.is_ascii_digit()) {
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
        digits.extend(self.password.0.iter().copied().map(char::from));
        digits
    }

    /// Parses user input into a code.
    ///
    /// Non-digit characters are ignored, so grouping into blocks and any
    /// separators the user copied along do not matter.
    pub fn parse(input: &str) -> Result<Self, LinkingCodeError> {
        let digits: String = input.chars().filter(char::is_ascii_digit).collect();
        if digits.len() < MIN_CODE_DIGITS {
            return Err(LinkingCodeError::TooShort);
        }

        // The password has a fixed length, so the rendezvous ID can grow
        // without a delimiter as long as the split happens from the end.
        let password_start = digits.len() - PASSWORD_DIGITS;
        let mut password = LinkingPassword([0; PASSWORD_DIGITS]);
        password
            .0
            .copy_from_slice(&digits.as_bytes()[password_start..]);
        Ok(Self {
            rendezvous_id: digits[..password_start].to_owned(),
            password,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_passwords_are_ascii_digits() {
        let password = LinkingPassword::generate();
        assert!(password.as_bytes().iter().all(u8::is_ascii_digit));
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
        let spaced = format!("{} - {} {}", &digits[..3], &digits[3..5], &digits[5..]);
        assert_eq!(LinkingCode::parse(&spaced).unwrap(), generated);
    }

    #[test]
    fn the_split_runs_from_the_end() {
        let generated = LinkingCode::generate("1234567").unwrap();
        let digits = generated.to_digits();
        let parsed = LinkingCode::parse(&digits).unwrap();
        assert_eq!(parsed.rendezvous_id(), "1234567");
        assert_eq!(parsed.password().as_bytes(), &digits.as_bytes()[7..]);
    }

    #[test]
    fn a_code_is_the_rendezvous_id_followed_by_the_password() {
        let code = LinkingCode::new("417", LinkingPassword(*b"50931")).unwrap();
        assert_eq!(code.to_digits(), "41750931");
    }

    #[test]
    fn short_input_is_rejected() {
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
    fn an_empty_rendezvous_id_is_rejected() {
        assert_eq!(
            LinkingCode::generate(""),
            Err(LinkingCodeError::InvalidRendezvousId)
        );
    }
}
