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

const MIN_USER_HANDLE_LENGTH: usize = 5;
const MAX_USER_HANDLE_LENGTH: usize = 63;
// TODO: Temporarily allow both '-' and '_' so older clients remain compatible while new
// ones emit dashes. Remove '_' once the transition is complete.
const USER_HANDLE_CHARSET: &[u8] = b"-_0123456789abcdefghijklmnopqrstuvwxyz";

pub const USER_HANDLE_VALIDITY_PERIOD: Duration = Duration::days(30);

/// Validated plaintext user handle
#[derive(
    Clone, PartialEq, Eq, Hash, TlsSize, TlsSerialize, TlsDeserializeBytes, Serialize, Deserialize,
)]
#[serde(transparent)]
pub struct UserHandle {
    plaintext: TlsString,
}

impl fmt::Debug for UserHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UserHandle")
            .field("plaintext", &"<redacted>")
            .finish()
    }
}

impl UserHandle {
    pub fn new(plaintext: String) -> Result<Self, UserHandleValidationError> {
        Self::validate(&plaintext)?;
        Ok(Self {
            plaintext: TlsString(plaintext),
        })
    }

    fn validate(plaintext: &str) -> Result<(), UserHandleValidationError> {
        if plaintext.len() < MIN_USER_HANDLE_LENGTH {
            return Err(UserHandleValidationError::TooShort);
        }
        if plaintext.len() > MAX_USER_HANDLE_LENGTH {
            return Err(UserHandleValidationError::TooLong);
        }
        for c in plaintext.bytes() {
            if !USER_HANDLE_CHARSET.contains(&c) {
                return Err(UserHandleValidationError::InvalidCharacter);
            }
        }
        for pair in plaintext.as_bytes().windows(2) {
            if pair[0] == b'-' && pair[1] == b'-' {
                return Err(UserHandleValidationError::ConsecutiveDashes);
            }
        }
        if let Some(first_char) = plaintext.chars().next()
            && first_char.is_ascii_digit()
        {
            return Err(UserHandleValidationError::LeadingDigit);
        }
        Ok(())
    }

    pub fn calculate_hash(&self) -> Result<UserHandleHash, UserHandleHashError> {
        let argon2 = Argon2::default();
        let const_salt = b"user handle salt"; // TODO(security): this is not what we want
        let mut hash = [0u8; 32];
        argon2.hash_password_into(self.plaintext.0.as_bytes(), const_salt, &mut hash)?;
        Ok(UserHandleHash { hash })
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
pub struct UserHandleHash {
    #[serde(with = "serde_bytes")]
    hash: [u8; 32],
}

impl UserHandleHash {
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
pub enum UserHandleValidationError {
    /// User handle is too short
    TooShort,
    /// User handle is too long
    TooLong,
    /// User handle contains invalid characters
    InvalidCharacter,
    /// User handle contains consecutive dashes
    ConsecutiveDashes,
    /// Leading characters are not allowed to be digits
    LeadingDigit,
}

#[derive(Debug, thiserror::Error)]
pub enum UserHandleHashError {
    #[error(transparent)]
    Argon2(#[from] argon2::Error),
}

mod sqlx_impls {
    use sqlx::{Database, Decode, Encode, Sqlite, Type, encode::IsNull, error::BoxDynError};

    use super::*;

    // `UserHandle` is only persisted in the client database, so we only implement the sqlx traits
    // for Sqlite.

    impl Type<Sqlite> for UserHandle {
        fn type_info() -> <Sqlite as Database>::TypeInfo {
            <String as Type<Sqlite>>::type_info()
        }
    }

    impl Encode<'_, Sqlite> for UserHandle {
        fn encode_by_ref(
            &self,
            buf: &mut <Sqlite as Database>::ArgumentBuffer<'_>,
        ) -> Result<IsNull, BoxDynError> {
            Encode::<Sqlite>::encode(self.plaintext().to_owned(), buf)
        }
    }

    impl Decode<'_, Sqlite> for UserHandle {
        fn decode(value: <Sqlite as Database>::ValueRef<'_>) -> Result<Self, BoxDynError> {
            let plaintext: String = Decode::<Sqlite>::decode(value)?;
            let value = UserHandle::new(plaintext)?;
            Ok(value)
        }
    }

    impl<DB> Type<DB> for UserHandleHash
    where
        DB: Database,
        Vec<u8>: Type<DB>,
    {
        fn type_info() -> <DB as Database>::TypeInfo {
            <Vec<u8> as Type<DB>>::type_info()
        }
    }

    impl<'q, DB> Encode<'q, DB> for UserHandleHash
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

    impl<'r, DB> Decode<'r, DB> for UserHandleHash
    where
        DB: Database,
        for<'a> &'a [u8]: Decode<'a, DB>,
    {
        fn decode(value: <DB as Database>::ValueRef<'r>) -> Result<Self, BoxDynError> {
            let bytes: &[u8] = Decode::<DB>::decode(value)?;
            let value = UserHandleHash::new(bytes.try_into()?);
            Ok(value)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_user_handle_string() -> String {
        "test-user-123".to_string()
    }

    #[test]
    fn test_user_handle_new_valid() {
        let handle_str = valid_user_handle_string();
        let handle = UserHandle::new(handle_str.clone());
        assert_eq!(handle.unwrap().plaintext(), handle_str);
    }

    #[test]
    fn test_user_handle_new_too_short() {
        let handle_str = "abcd".to_string(); // Length 4, MIN_USER_HANDLE_LENGTH is 5
        let handle = UserHandle::new(handle_str);
        assert!(matches!(
            handle.unwrap_err(),
            UserHandleValidationError::TooShort
        ));
    }

    #[test]
    fn test_user_handle_new_too_long() {
        let handle_str = "a".repeat(MAX_USER_HANDLE_LENGTH + 1);
        let handle = UserHandle::new(handle_str);
        assert!(matches!(
            handle.unwrap_err(),
            UserHandleValidationError::TooLong
        ));
    }

    #[test]
    fn test_user_handle_new_invalid_character() {
        let handle_str = "user-handle!".to_string(); // '!' is not in USER_HANDLE_CHARSET
        let handle = UserHandle::new(handle_str);
        assert!(matches!(
            handle.unwrap_err(),
            UserHandleValidationError::InvalidCharacter
        ));

        let handle_str_uppercase = "UserHandle".to_string(); // 'U' is not in USER_HANDLE_CHARSET
        let handle_uppercase = UserHandle::new(handle_str_uppercase);
        assert!(matches!(
            handle_uppercase.unwrap_err(),
            UserHandleValidationError::InvalidCharacter
        ));
    }

    // TODO: Remove this test once underscores are no longer allowed
    #[test]
    fn test_user_handle_temporarily_allows_underscore() {
        let handle = UserHandle::new("legacy_name".to_string());
        assert!(handle.is_ok());
    }

    #[test]
    fn test_user_handle_new_unicode_character() {
        let handle_str = "user-hændle".to_string(); // 'æ' is a Unicode character, not in USER_HANDLE_CHARSET
        let handle = UserHandle::new(handle_str);
        assert!(matches!(
            handle.unwrap_err(),
            UserHandleValidationError::InvalidCharacter
        ));

        let handle_str_emoji = "user😊handle".to_string(); // Emoji is a Unicode character
        let handle_emoji = UserHandle::new(handle_str_emoji);
        assert!(matches!(
            handle_emoji.unwrap_err(),
            UserHandleValidationError::InvalidCharacter
        ));
    }

    #[test]
    fn test_user_handle_new_consecutive_dashes() {
        let handle_str = "aaa--bbbb".to_string(); // Consecutive dashes
        let handle = UserHandle::new(handle_str);
        assert!(matches!(
            handle.unwrap_err(),
            UserHandleValidationError::ConsecutiveDashes
        ));
    }

    #[test]
    fn test_user_handle_debug_redacted() {
        let handle = UserHandle::new(valid_user_handle_string()).unwrap();
        let debug_output = format!("{handle:?}");
        assert!(debug_output.contains("<redacted>"));
        assert!(!debug_output.contains("test-user-123")); // Ensure original plaintext is not visible
    }

    #[test]
    fn test_user_handle_hash_produces_hash() {
        let handle = UserHandle::new(valid_user_handle_string()).unwrap();
        let handle_hash = handle.calculate_hash().unwrap();
        assert_eq!(
            hex::encode(handle_hash.hash),
            "c637090294208e446deb561ee0020e9e9f75f269da55cada75da5e2b973cd90e"
        );
    }

    #[test]
    fn test_user_handle_hash_consistency() {
        // Hashing the same input with an empty salt should produce the same hash
        let handle_str = valid_user_handle_string();
        let handle1 = UserHandle::new(handle_str.clone()).unwrap();
        let handle2 = UserHandle::new(handle_str).unwrap();

        let hash1 = handle1.calculate_hash().unwrap();
        let hash2 = handle2.calculate_hash().unwrap();

        assert_eq!(hash1.hash, hash2.hash);
    }
}
