// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Parser for the subset of ICU MessageFormat that Flutter's `gen-l10n`
//! accepts. We parse rather than pattern match because a regex cannot tell a
//! placeholder from a plural arm selector, which is exactly where translations
//! drift apart.

use std::collections::BTreeSet;

/// Plural and select selectors defined by CLDR. Anything else in an arm
/// position is either an explicit `=N` match or a typo.
const CLDR_CATEGORIES: &[&str] = &["zero", "one", "two", "few", "many", "other"];

/// What one message uses: the placeholders it interpolates and the plural or
/// select constructs it opens.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct ParsedMessage {
    pub(crate) placeholders: BTreeSet<String>,
    pub(crate) choices: Vec<Choice>,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Choice {
    pub(crate) name: String,
    pub(crate) kind: String,
    pub(crate) selectors: Vec<String>,
}

impl Choice {
    /// Selectors that are neither a CLDR category nor an explicit `=N` match.
    pub(crate) fn unknown_selectors(&self) -> Vec<&str> {
        let mut unknown = Vec::new();
        for selector in &self.selectors {
            let is_explicit = selector
                .strip_prefix('=')
                .is_some_and(|rest| !rest.is_empty() && rest.chars().all(|ch| ch.is_ascii_digit()));
            if !is_explicit && !CLDR_CATEGORIES.contains(&selector.as_str()) {
                unknown.push(selector.as_str());
            }
        }
        unknown
    }

    /// ICU requires an `other` arm as the catch-all. Select constructs need it
    /// too, since a value outside the listed arms would otherwise render empty.
    pub(crate) fn has_catch_all(&self) -> bool {
        self.selectors.iter().any(|selector| selector == "other")
    }
}

pub(crate) fn parse(text: &str) -> Result<ParsedMessage, String> {
    let mut parser = Parser {
        chars: text.chars().collect(),
        index: 0,
        out: ParsedMessage::default(),
    };
    parser.parse_body(0)?;
    Ok(parser.out)
}

struct Parser {
    chars: Vec<char>,
    index: usize,
    out: ParsedMessage,
}

impl Parser {
    /// Consume literal text and arguments until the input ends (`depth` 0) or
    /// an unmatched `}` closes the enclosing arm (`depth` above 0). The closing
    /// brace is left for the caller so it can tell a closed arm from a
    /// truncated one.
    fn parse_body(&mut self, depth: usize) -> Result<(), String> {
        while let Some(ch) = self.peek() {
            match ch {
                '{' => {
                    self.index += 1;
                    self.parse_argument()?;
                }
                '}' if depth > 0 => return Ok(()),
                '}' => return Err("unmatched '}'".to_owned()),
                _ => self.index += 1,
            }
        }
        if depth > 0 {
            return Err("unclosed '{'".to_owned());
        }
        Ok(())
    }

    fn parse_argument(&mut self) -> Result<(), String> {
        let name = self.read_until(&[',', '}'])?.trim().to_owned();
        validate_name(&name)?;
        self.out.placeholders.insert(name.clone());
        match self.next() {
            Some('}') => Ok(()),
            Some(',') => self.parse_typed_argument(name),
            _ => Err(format!("unclosed placeholder '{{{name}'")),
        }
    }

    fn parse_typed_argument(&mut self, name: String) -> Result<(), String> {
        let kind = self.read_until(&[',', '}'])?.trim().to_owned();
        match self.next() {
            // A bare type such as `{when, date}` takes no further arguments.
            Some('}') => Ok(()),
            Some(',') if is_choice(&kind) => self.parse_arms(name, kind),
            // Number and date skeletons are opaque to us, so skip past them.
            Some(',') => self.skip_balanced(),
            _ => Err(format!("unclosed placeholder '{{{name}'")),
        }
    }

    fn parse_arms(&mut self, name: String, kind: String) -> Result<(), String> {
        let mut selectors = Vec::new();
        loop {
            self.skip_whitespace();
            match self.peek() {
                Some('}') => {
                    self.index += 1;
                    break;
                }
                None => return Err(format!("unclosed '{kind}' for '{name}'")),
                _ => {}
            }
            let selector = self.read_until(&['{'])?.trim().to_owned();
            if selector.is_empty() {
                return Err(format!("empty arm selector in '{kind}' for '{name}'"));
            }
            self.index += 1;
            self.parse_body(1)?;
            if self.next() != Some('}') {
                return Err(format!(
                    "unclosed arm '{selector}' in '{kind}' for '{name}'"
                ));
            }
            selectors.push(selector);
        }
        self.out.choices.push(Choice {
            name,
            kind,
            selectors,
        });
        Ok(())
    }

    /// Consume up to, but not including, the first character in `stops`.
    fn read_until(&mut self, stops: &[char]) -> Result<String, String> {
        let start = self.index;
        while let Some(ch) = self.peek() {
            if stops.contains(&ch) {
                return Ok(self.chars[start..self.index].iter().collect());
            }
            self.index += 1;
        }
        Err("unclosed '{'".to_owned())
    }

    fn skip_balanced(&mut self) -> Result<(), String> {
        let mut depth = 1usize;
        while let Some(ch) = self.next() {
            match ch {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        return Ok(());
                    }
                }
                _ => {}
            }
        }
        Err("unclosed '{'".to_owned())
    }

    fn skip_whitespace(&mut self) {
        while self.peek().is_some_and(char::is_whitespace) {
            self.index += 1;
        }
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.index).copied()
    }

    fn next(&mut self) -> Option<char> {
        let ch = self.peek();
        if ch.is_some() {
            self.index += 1;
        }
        ch
    }
}

fn is_choice(kind: &str) -> bool {
    matches!(kind, "plural" | "select" | "selectordinal")
}

fn validate_name(name: &str) -> Result<(), String> {
    let mut chars = name.chars();
    let valid_start = chars
        .next()
        .is_some_and(|ch| ch.is_ascii_alphabetic() || ch == '_');
    if !valid_start || !chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_') {
        return Err(format!("'{name}' is not a valid placeholder name"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn placeholders(text: &str) -> Vec<String> {
        parse(text)
            .expect("should parse")
            .placeholders
            .into_iter()
            .collect()
    }

    #[test]
    fn reads_simple_placeholders() {
        assert_eq!(placeholders("Block {displayName}?"), ["displayName"]);
        assert_eq!(
            placeholders("{characters}/{remaining} left"),
            ["characters", "remaining"]
        );
    }

    #[test]
    fn plain_text_has_no_placeholders() {
        assert!(placeholders("Copied to clipboard").is_empty());
    }

    /// Arm bodies are literal text, so the words inside them must not be
    /// mistaken for placeholders. This is the case a regex gets wrong.
    #[test]
    fn arm_bodies_are_not_placeholders() {
        let parsed = parse(
            "{count, plural, =0 {No devices linked.} one {{count} device} other {{count} devices}}",
        )
        .expect("should parse");
        assert_eq!(
            parsed.placeholders.into_iter().collect::<Vec<_>>(),
            ["count"]
        );
        let choice = &parsed.choices[0];
        assert_eq!(choice.selectors, ["=0", "one", "other"]);
        assert!(choice.has_catch_all());
        assert!(choice.unknown_selectors().is_empty());
    }

    #[test]
    fn flags_unknown_arm_selectors() {
        let parsed = parse("{count, plural, singular {one} other {many}}").expect("should parse");
        assert_eq!(parsed.choices[0].unknown_selectors(), ["singular"]);
    }

    #[test]
    fn detects_missing_catch_all_arm() {
        let parsed = parse("{count, plural, one {a} two {b}}").expect("should parse");
        assert!(!parsed.choices[0].has_catch_all());
    }

    #[test]
    fn skips_number_format_skeletons() {
        assert_eq!(placeholders("{size, number, compactShort}"), ["size"]);
    }

    #[test]
    fn apostrophes_are_literal() {
        assert_eq!(
            placeholders("Type 'delete' to confirm."),
            Vec::<String>::new()
        );
    }

    #[test]
    fn rejects_unbalanced_braces() {
        assert!(parse("Hello {name").is_err());
        assert!(parse("Hello name}").is_err());
        assert!(parse("{count, plural, one {a} other {b}").is_err());
    }

    #[test]
    fn rejects_invalid_placeholder_names() {
        assert!(parse("Hello {2name}").is_err());
        assert!(parse("Hello {}").is_err());
        assert!(parse("Hello {first name}").is_err());
    }
}
