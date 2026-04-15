// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later
use std::fmt;

use argon2::Argon2;
use chrono::Duration;
use displaydoc::Display;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tls_codec::{TlsDeserializeBytes, TlsSerialize, TlsSize};

use super::TlsString;

const MIN_USERNAME_LENGTH: usize = 5;
const MAX_USERNAME_LENGTH: usize = 63;
const USERNAME_CHARSET: &[u8] = b"-0123456789abcdefghijklmnopqrstuvwxyz";

pub const USERNAME_VALIDITY_PERIOD: Duration = Duration::days(180);
pub const USERNAME_REFRESH_THRESHOLD: Duration = Duration::days(90);

/// Validated plaintext username
#[derive(
    Clone, PartialEq, Eq, Hash, TlsSize, TlsSerialize, TlsDeserializeBytes, Serialize, Deserialize,
)]
#[serde(transparent)]
pub struct Username {
    plaintext: TlsString,
}

impl fmt::Debug for Username {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Username")
            .field("plaintext", &"<redacted>")
            .finish()
    }
}

impl Username {
    pub fn new(plaintext: String) -> Result<Self, UsernameValidationError> {
        Self::validate(&plaintext)?;
        Ok(Self::new_from_raw(plaintext))
    }

    fn new_from_raw(plaintext: String) -> Self {
        Self {
            plaintext: TlsString(plaintext),
        }
    }

    fn validate(plaintext: &str) -> Result<(), UsernameValidationError> {
        if plaintext.len() < MIN_USERNAME_LENGTH {
            return Err(UsernameValidationError::TooShort);
        }
        if plaintext.len() > MAX_USERNAME_LENGTH {
            return Err(UsernameValidationError::TooLong);
        }
        for c in plaintext.bytes() {
            if !USERNAME_CHARSET.contains(&c) {
                return Err(UsernameValidationError::InvalidCharacter);
            }
        }
        for pair in plaintext.as_bytes().windows(2) {
            if pair[0] == b'-' && pair[1] == b'-' {
                return Err(UsernameValidationError::ConsecutiveDashes);
            }
        }
        if let Some(first_char) = plaintext.chars().next()
            && first_char.is_ascii_digit()
        {
            return Err(UsernameValidationError::LeadingDigit);
        }
        Ok(())
    }

    pub fn calculate_hash(&self) -> Result<UsernameHash, UsernameHashError> {
        let argon2 = Argon2::default();
        let const_salt = b"user handle salt"; // TODO(security): this is not what we want
        let mut hash = [0u8; 32];
        argon2.hash_password_into(self.plaintext.0.as_bytes(), const_salt, &mut hash)?;
        Ok(UsernameHash { hash })
    }

    pub fn plaintext(&self) -> &str {
        &self.plaintext.0
    }

    pub fn into_plaintext(self) -> String {
        self.plaintext.0
    }
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, TlsSerialize, TlsSize, Serialize, Deserialize,
)]
pub struct UsernameHash {
    #[serde(with = "serde_bytes")]
    hash: [u8; 32],
}

impl UsernameHash {
    pub fn new(hash: [u8; 32]) -> Self {
        Self { hash }
    }

    pub fn into_bytes(self) -> [u8; 32] {
        self.hash
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.hash
    }
}

#[derive(Debug, Error, Display)]
pub enum UsernameValidationError {
    /// Username is too short
    TooShort,
    /// Username is too long
    TooLong,
    /// Username contains invalid characters
    InvalidCharacter,
    /// Username contains consecutive dashes
    ConsecutiveDashes,
    /// Leading characters are not allowed to be digits
    LeadingDigit,
}

#[derive(Debug, thiserror::Error)]
pub enum UsernameHashError {
    #[error(transparent)]
    Argon2(#[from] argon2::Error),
}

mod sqlx_impls {
    use sqlx::{Database, Decode, Encode, Sqlite, Type, encode::IsNull, error::BoxDynError};

    use super::*;

    // `Username` is only persisted in the client database, so we only implement the sqlx traits
    // for Sqlite.

    impl Type<Sqlite> for Username {
        fn type_info() -> <Sqlite as Database>::TypeInfo {
            <String as Type<Sqlite>>::type_info()
        }
    }

    impl Encode<'_, Sqlite> for Username {
        fn encode_by_ref(
            &self,
            buf: &mut <Sqlite as Database>::ArgumentBuffer<'_>,
        ) -> Result<IsNull, BoxDynError> {
            Encode::<Sqlite>::encode(self.plaintext().to_owned(), buf)
        }
    }

    impl Decode<'_, Sqlite> for Username {
        fn decode(value: <Sqlite as Database>::ValueRef<'_>) -> Result<Self, BoxDynError> {
            let plaintext: String = Decode::<Sqlite>::decode(value)?;
            let value = Username::new_from_raw(plaintext);
            Ok(value)
        }
    }

    impl<DB> Type<DB> for UsernameHash
    where
        DB: Database,
        Vec<u8>: Type<DB>,
    {
        fn type_info() -> <DB as Database>::TypeInfo {
            <Vec<u8> as Type<DB>>::type_info()
        }
    }

    impl<'q, DB> Encode<'q, DB> for UsernameHash
    where
        DB: Database,
        Vec<u8>: Encode<'q, DB>,
    {
        fn encode_by_ref(
            &self,
            buf: &mut <DB as Database>::ArgumentBuffer<'q>,
        ) -> Result<IsNull, BoxDynError> {
            let bytes = self.as_bytes().to_vec();
            Encode::<DB>::encode(bytes, buf)
        }
    }

    impl<'r, DB> Decode<'r, DB> for UsernameHash
    where
        DB: Database,
        for<'a> &'a [u8]: Decode<'a, DB>,
    {
        fn decode(value: <DB as Database>::ValueRef<'r>) -> Result<Self, BoxDynError> {
            let bytes: &[u8] = Decode::<DB>::decode(value)?;
            let value = UsernameHash::new(bytes.try_into()?);
            Ok(value)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_username_string() -> String {
        "test-user-123".to_string()
    }

    #[test]
    fn test_username_new_valid() {
        let username_str = valid_username_string();
        let username = Username::new(username_str.clone());
        assert_eq!(username.unwrap().plaintext(), username_str);
    }

    #[test]
    fn test_username_new_too_short() {
        let username_str = "abcd".to_string(); // Length 4, MIN_USERNAME_LENGTH is 5
        let username = Username::new(username_str);
        assert!(matches!(
            username.unwrap_err(),
            UsernameValidationError::TooShort
        ));
    }

    #[test]
    fn test_username_new_too_long() {
        let username_str = "a".repeat(MAX_USERNAME_LENGTH + 1);
        let username = Username::new(username_str);
        assert!(matches!(
            username.unwrap_err(),
            UsernameValidationError::TooLong
        ));
    }

    #[test]
    fn test_username_new_invalid_character() {
        let username_str = "user-name-1!".to_string(); // '!' is not in USERNAME_CHARSET
        let username = Username::new(username_str);
        assert!(matches!(
            username.unwrap_err(),
            UsernameValidationError::InvalidCharacter
        ));

        // Uppercase is not in charset
        let username_uppercase = Username::new("UserName1".to_string());
        assert!(matches!(
            username_uppercase.unwrap_err(),
            UsernameValidationError::InvalidCharacter
        ));
    }

    #[test]
    fn test_username_rejects_underscore() {
        let username = Username::new("legacy_name".to_string());
        assert!(matches!(
            username.unwrap_err(),
            UsernameValidationError::InvalidCharacter
        ));
    }

    #[test]
    fn test_username_new_unicode_character() {
        let username_str = "user-hændle".to_string(); // 'æ' is a Unicode character, not in USERNAME_CHARSET
        let username = Username::new(username_str);
        assert!(matches!(
            username.unwrap_err(),
            UsernameValidationError::InvalidCharacter
        ));

        let username_str_emoji = "user😊name".to_string(); // Emoji is a Unicode character
        let username_emoji = Username::new(username_str_emoji);
        assert!(matches!(
            username_emoji.unwrap_err(),
            UsernameValidationError::InvalidCharacter
        ));
    }

    #[test]
    fn test_username_new_consecutive_dashes() {
        let username_str = "aaa--bbbb".to_string(); // Consecutive dashes
        let username = Username::new(username_str);
        assert!(matches!(
            username.unwrap_err(),
            UsernameValidationError::ConsecutiveDashes
        ));
    }

    #[test]
    fn test_username_debug_redacted() {
        let username = Username::new(valid_username_string()).unwrap();
        let debug_output = format!("{username:?}");
        assert!(debug_output.contains("<redacted>"));
        assert!(!debug_output.contains("test-user-123")); // Ensure original plaintext is not visible
    }

    #[test]
    fn test_username_hash_produces_hash() {
        let username = Username::new(valid_username_string()).unwrap();
        let username_hash = username.calculate_hash().unwrap();
        assert_eq!(
            hex::encode(username_hash.hash),
            "c637090294208e446deb561ee0020e9e9f75f269da55cada75da5e2b973cd90e"
        );
    }

    #[test]
    fn test_username_hash_consistency() {
        // Hashing the same input with an empty salt should produce the same hash
        let username_str = valid_username_string();
        let username1 = Username::new(username_str.clone()).unwrap();
        let username2 = Username::new(username_str).unwrap();

        let hash1 = username1.calculate_hash().unwrap();
        let hash2 = username2.calculate_hash().unwrap();

        assert_eq!(hash1.hash, hash2.hash);
    }
}
