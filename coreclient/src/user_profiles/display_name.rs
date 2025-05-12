// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{fmt::Display, str::FromStr};

use phnxtypes::identifiers::{TlsStr, TlsString};
use thiserror::Error;

use super::*;

/// A display name is a human-readable name that can be used to identify a user.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct DisplayName {
    display_name: String,
}

// Note that this counts chars, not graphemes. While chars are at most 4 bytes,
// graphemes can be longer, so we need to adjust the logic if we ever want to
// count graphemes instead of chars.
const MAX_DISPLAY_NAME_CHARS: usize = 50;
const DISALLOWED_CHARACTERS: [char; 3] = ['\r', '\n', '\t'];

impl FromStr for DisplayName {
    type Err = DisplayNameError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let display_name: String = s
            .chars()
            .filter(|&c| !DISALLOWED_CHARACTERS.contains(&c))
            .collect::<String>() // intermediate cleaned string
            .trim() // trim whitespace
            .chars() // now safely truncate to 50 scalar values
            .take(MAX_DISPLAY_NAME_CHARS)
            .collect();

        if display_name.is_empty() {
            return Err(DisplayNameError::DisplayNameEmpty);
        }

        Ok(Self { display_name })
    }
}

#[derive(Debug, Error)]
pub enum DisplayNameError {
    #[error("Display name is empty")]
    DisplayNameEmpty,
}

impl sqlx::Type<Sqlite> for DisplayName {
    fn type_info() -> <Sqlite as Database>::TypeInfo {
        <String as sqlx::Type<Sqlite>>::type_info()
    }
}

impl<'q> Encode<'q, Sqlite> for DisplayName {
    fn encode_by_ref(
        &self,
        buf: &mut <Sqlite as Database>::ArgumentBuffer<'q>,
    ) -> Result<IsNull, BoxDynError> {
        Encode::<Sqlite>::encode_by_ref(&self.display_name, buf)
    }
}

impl<'r> Decode<'r, Sqlite> for DisplayName {
    fn decode(value: <Sqlite as Database>::ValueRef<'r>) -> Result<Self, BoxDynError> {
        let display_name: String = Decode::<Sqlite>::decode(value)?;
        Ok(Self { display_name })
    }
}

impl Display for DisplayName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.display_name)
    }
}

impl AsRef<str> for DisplayName {
    fn as_ref(&self) -> &str {
        // Use the trimmed version of the display name.
        &self.display_name
    }
}

impl tls_codec::Size for DisplayName {
    fn tls_serialized_len(&self) -> usize {
        TlsStr(&self.display_name).tls_serialized_len()
    }
}

impl tls_codec::Serialize for DisplayName {
    fn tls_serialize<W: std::io::Write>(&self, writer: &mut W) -> Result<usize, tls_codec::Error> {
        TlsStr(&self.display_name).tls_serialize(writer)
    }
}

impl tls_codec::DeserializeBytes for DisplayName {
    fn tls_deserialize_bytes(bytes: &[u8]) -> Result<(Self, &[u8]), tls_codec::Error> {
        let (TlsString(display_name), bytes) = TlsString::tls_deserialize_bytes(bytes)?;
        let display_name = DisplayName { display_name };
        Ok((display_name, bytes))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_ascii_under_limit() {
        let name = "Alice";
        let dn = DisplayName::from_str(name).unwrap();
        assert_eq!(dn.display_name, name);
    }

    #[test]
    fn rejects_empty_input() {
        let result = DisplayName::from_str("   ");
        assert!(matches!(result, Err(DisplayNameError::DisplayNameEmpty)));
        assert_eq!(result.unwrap_err().to_string(), "Display name is empty");
    }

    #[test]
    fn accepts_exactly_50_characters() {
        let name = "a".repeat(50);
        let dn = DisplayName::from_str(&name).unwrap();
        assert_eq!(dn.display_name, name);
    }

    #[test]
    fn caps_display_name_at_50_chars_after_filtering() {
        let input = "a\nb".repeat(30); // 90 chars, ~60 after filtering
        let dn = DisplayName::from_str(&input).unwrap();
        assert_eq!(dn.display_name.chars().count(), 50);
        assert!(!dn.display_name.contains('\n'));
    }

    #[test]
    fn trims_whitespace_correctly() {
        let name = "  hello\t  ";
        let dn = DisplayName::from_str(name).unwrap();
        assert_eq!(dn.display_name, "hello");
    }

    #[test]
    fn accepts_right_to_left_script() {
        let name = "سلام"; // Arabic
        let dn = DisplayName::from_str(name).unwrap();
        assert_eq!(dn.display_name, name);
    }

    #[test]
    fn trims_whitespace_in_rtl_string() {
        let input = "  سلام  ";
        let dn = DisplayName::from_str(input).unwrap();
        assert_eq!(dn.display_name, "سلام");
    }

    #[test]
    fn accepts_grapheme_clusters_over_4_bytes() {
        let flag = "🇺🇳"; // 2 scalar values, 8 bytes
        let repeated = flag.repeat(25); // 50 scalar values
        let dn = DisplayName::from_str(&repeated).unwrap();
        assert_eq!(dn.display_name, repeated);
    }

    #[test]
    fn filters_out_disallowed_characters_anywhere_in_input() {
        let input = "\nHello\r\t, \tWor\rld!\n";
        let expected = "Hello, World!"; // tabs are not disallowed in this case
        let dn = DisplayName::from_str(input).unwrap();

        for c in DISALLOWED_CHARACTERS {
            assert!(
                !dn.display_name.contains(c),
                "Found disallowed char: {:?}",
                c
            );
        }

        assert_eq!(dn.display_name, expected);
    }

    #[test]
    fn rejects_input_with_only_disallowed_and_whitespace() {
        let input = "\n\r  \t ";
        let result = DisplayName::from_str(input);
        assert!(matches!(result, Err(DisplayNameError::DisplayNameEmpty)));
    }

    #[test]
    fn handles_mixed_directionality() {
        let input = "שלום Alice مرحبا";
        let dn = DisplayName::from_str(input).unwrap();
        assert!(dn.display_name.contains("שלום"));
        assert!(dn.display_name.contains("Alice"));
        assert!(dn.display_name.contains("مرحبا"));
    }

    #[test]
    fn preserves_complex_emojis() {
        let input = "👨‍👩‍👧‍👦 Family";
        let dn = DisplayName::from_str(input).unwrap();
        assert!(dn.display_name.contains("👨‍👩‍👧‍👦"));
    }
}
