// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use flutter_rust_bridge::frb;
use rand::Rng;

const MAX_USERNAME_LENGTH: usize = 63;
const SUFFIX_LENGTH: usize = 5;
const HYPHEN_LENGTH: usize = 1;
const MAX_BASE_LENGTH: usize = 58;

#[frb(sync)]
pub fn username_from_display(display: String) -> String {
    // Only keep the first three whitespace-delimited tokens so extremely long
    // display names do not dominate the generated handle.
    let truncated_input = first_tokens(&display, 3).join(" ");
    // Convert to latin characters and lowercase the handle base.
    let ascii = deunicode::deunicode(&truncated_input).to_lowercase();

    let mut out = String::with_capacity(ascii.len() + 6);
    let mut last_was_dash = false;
    let mut started = false; // first char can't be a digit

    for ch in ascii.chars() {
        if ch.is_ascii_alphanumeric() {
            if !started {
                if ch.is_ascii_digit() {
                    continue; // skip leading digits
                }
                started = true;
            }
            out.push(ch);
            last_was_dash = false;
        } else if started && !last_was_dash {
            out.push('-');
            last_was_dash = true;
        }
    }

    if last_was_dash {
        out.pop(); // no trailing dash
    }

    if out.is_empty() {
        out.push_str("user");
    }

    // Ensure the base part leaves enough room for the hyphen + numeric suffix.
    truncate_clean(&mut out, MAX_BASE_LENGTH);
    let max_allowed_before_suffix = MAX_USERNAME_LENGTH - SUFFIX_LENGTH - HYPHEN_LENGTH;
    truncate_clean(&mut out, max_allowed_before_suffix);

    // append random 5-digit suffix
    let suffix = rand::thread_rng().gen_range(10000..100000);
    out.push('-');
    out.push_str(&suffix.to_string());

    out
}

/// Truncate `out` to `max_len`, removing trailing dashes and ensuring the
/// fallback `user` value if the string becomes empty.
fn truncate_clean(out: &mut String, max_len: usize) {
    if out.len() > max_len {
        out.truncate(max_len);
        while out.ends_with('-') {
            out.pop();
        }
        if out.is_empty() {
            out.push_str("user");
        }
    }
}

/// Split `display` into at most `limit` meaningful tokens following the
/// username rules:
///
/// * alphanumeric sequences form words that may span non-ASCII characters.
/// * ASCII punctuation (apostrophes, hyphens, etc.) behaves like whitespace and
///   merely separates tokens.
/// * Any other non-alphanumeric symbol (emoji, etc.) forms its own token.
fn first_tokens(display: &str, limit: usize) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();

    for ch in display.chars() {
        if ch.is_whitespace() {
            if !current.is_empty() {
                tokens.push(std::mem::take(&mut current));
                if tokens.len() == limit {
                    return tokens;
                }
            }
        } else {
            if ch.is_alphanumeric() {
                current.push(ch);
                continue;
            }
            // ASCII punctuation (like apostrophes) should act purely as
            // separators. Emoji and other symbols count as their own token.
            if ch.is_ascii_punctuation() {
                if !current.is_empty() {
                    tokens.push(std::mem::take(&mut current));
                    if tokens.len() == limit {
                        return tokens;
                    }
                }
                continue;
            }
            if !current.is_empty() {
                tokens.push(std::mem::take(&mut current));
                if tokens.len() == limit {
                    return tokens;
                }
            }
            tokens.push(ch.to_string());
            if tokens.len() == limit {
                return tokens;
            }
        }
    }

    if !current.is_empty() && tokens.len() < limit {
        tokens.push(current);
    }

    tokens
}

#[cfg(test)]
mod tests {
    use aircommon::identifiers::UserHandle;

    use super::*;

    fn assert_valid(username: &str) {
        UserHandle::new(username.to_string()).unwrap();
        let suffix = username.rsplit('-').next().unwrap();
        assert_eq!(suffix.len(), SUFFIX_LENGTH);
        assert!(suffix.chars().all(|c| c.is_ascii_digit()));
    }

    #[test]
    fn limits_to_three_tokens() {
        let out = username_from_display("Very Long Display Name With Many Extra Words".to_string());
        assert!(out.starts_with("very-long-display"));
        assert_valid(&out);
    }

    #[test]
    fn trims_excessive_length() {
        let long = "a".repeat(200);
        let out = username_from_display(long);
        assert_valid(&out);
    }

    #[test]
    fn fallback_when_no_letters() {
        let out = username_from_display("12345".to_string());
        assert!(out.starts_with("user-"));
        assert_valid(&out);
    }

    #[test]
    fn removes_trailing_dashes_after_truncate() {
        let out = username_from_display("ééé---".to_string());
        assert_valid(&out);
        assert!(!out.contains("--"));
    }

    #[test]
    fn handles_ukrainian() {
        let out = username_from_display("Марія Іваненко".to_string());
        assert!(out.starts_with("mariia-ivanenko-"));
        assert_valid(&out);
    }

    #[test]
    fn handles_arabic() {
        let out = username_from_display("علي سالم".to_string());
        assert!(out.starts_with("ly-slm-"));
        assert_valid(&out);
    }

    #[test]
    fn handles_chinese() {
        let out = username_from_display("王小明".to_string());
        assert!(out.starts_with("wang-xiao-ming-"));
        assert_valid(&out);
    }

    #[test]
    fn handles_accents_latin() {
        let out = username_from_display("Hélène D'Açaïre".to_string());
        assert!(out.starts_with("helene-d-acaire-"));
        assert_valid(&out);
    }

    #[test]
    fn emojis_only_produces_nonempty_base() {
        let out = username_from_display("😀🎉🔥".to_string());
        assert!(out.starts_with("grinning-tada-fire-"));
    }

    #[test]
    fn emoji_as_separator_between_words() {
        let out = username_from_display("Hello 👋 world".to_string());
        assert!(out.starts_with("hello-wave-world-"));
    }

    #[test]
    fn emoji_tokenization_limit() {
        let tokens = first_tokens("😀😃😄😁😆", 3);
        assert_eq!(tokens.len(), 3);
        assert_eq!(tokens, vec!["😀", "😃", "😄"]);
        let out = username_from_display("😀😃😄😁😆".to_string());
        assert_valid(&out);
    }
}
