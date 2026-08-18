// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Structural checks over the ARB files.
//!
//! Flutter's `gen-l10n` accepts a translation whose placeholders do not match
//! the template and silently emits `null` in its place, so a machine
//! translation can break a screen without failing any build step. These checks
//! close that gap, and they require every template key to carry a description
//! so whoever translates it next has the context the key name alone does not
//! give.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
};

use anyhow::{Context, Result, bail, ensure};
use camino::{Utf8Path, Utf8PathBuf};
use clap::Args;
use serde_json::{Map, Value};

use crate::util::workspace_root;

mod icu;

const DEFAULT_PROJECT_ROOT: &str = "app";
const DEFAULT_ARB_DIR: &str = "lib/l10n";
const DEFAULT_TEMPLATE: &str = "app_en.arb";

/// Product names that carry identity and must survive translation.
const PROTECTED_TERMS: &[&str] = &["Air"];

/// A description like "Label" costs a translator more than it gives.
const MIN_DESCRIPTION_LENGTH: usize = 10;

/// Characters we treat as sentence-final for source and translation parity.
const TERMINAL_MARKS: &[char] = &['.', '?', '!', ':'];

#[derive(Args, Debug)]
pub(crate) struct ValidateArgs {
    /// Resolve relative paths against this directory.
    #[arg(long, default_value = DEFAULT_PROJECT_ROOT)]
    project_root: String,
    /// Directory holding the ARB files.
    #[arg(long, default_value = DEFAULT_ARB_DIR)]
    arb_dir: String,
    /// Template ARB file name, relative to the ARB directory.
    #[arg(long, default_value = DEFAULT_TEMPLATE)]
    template: String,
}

pub(crate) fn run(args: ValidateArgs) -> Result<()> {
    let project_root = resolve(workspace_root().as_ref(), &args.project_root);
    let arb_dir = resolve(project_root.as_ref(), &args.arb_dir);
    let template_path = arb_dir.join(&args.template);
    ensure!(arb_dir.is_dir(), "ARB directory not found: {arb_dir}");
    ensure!(
        template_path.exists(),
        "Template not found: {template_path}"
    );

    let template = ArbFile::load(&template_path)?;
    ensure!(
        !template.messages.is_empty(),
        "No messages found in {template_path}"
    );

    let mut report = Report::default();
    check_template(&template, &mut report);

    let locale_paths = sibling_arb_files(&arb_dir, &template_path)?;
    ensure!(
        !locale_paths.is_empty(),
        "No translations found next to {template_path}"
    );
    for path in &locale_paths {
        let locale = ArbFile::load(path)?;
        check_locale(&locale, &template, &mut report);
    }

    report.finish(template.messages.len(), locale_paths.len() + 1)
}

// -- loading ---------------------------------------------------------------

/// One ARB file split into messages and their `@`-prefixed metadata.
struct ArbFile {
    name: String,
    messages: BTreeMap<String, String>,
    metadata: BTreeMap<String, Metadata>,
}

#[derive(Default)]
struct Metadata {
    description: Option<String>,
    placeholders: BTreeSet<String>,
}

impl ArbFile {
    fn load(path: &Utf8Path) -> Result<Self> {
        let raw = fs::read_to_string(path).with_context(|| format!("Failed to read {path}"))?;
        let entries: Map<String, Value> =
            serde_json::from_str(&raw).with_context(|| format!("Failed to parse {path}"))?;

        let name = path.file_name().unwrap_or(path.as_str()).to_owned();
        let mut messages = BTreeMap::new();
        let mut metadata = BTreeMap::new();
        for (key, value) in entries {
            // `@@locale` and friends configure the file, not a message.
            if key.starts_with("@@") {
                continue;
            }
            if let Some(base) = key.strip_prefix('@') {
                metadata.insert(base.to_owned(), parse_metadata(&value));
                continue;
            }
            let text = value
                .as_str()
                .with_context(|| format!("{name}: '{key}' is not a string"))?;
            messages.insert(key, text.to_owned());
        }

        Ok(Self {
            name,
            messages,
            metadata,
        })
    }

    fn declared_placeholders(&self, key: &str) -> BTreeSet<String> {
        self.metadata
            .get(key)
            .map(|entry| entry.placeholders.clone())
            .unwrap_or_default()
    }
}

fn parse_metadata(value: &Value) -> Metadata {
    let Some(object) = value.as_object() else {
        return Metadata::default();
    };
    let description = object
        .get("description")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let placeholders = object
        .get("placeholders")
        .and_then(Value::as_object)
        .map(|entries| entries.keys().cloned().collect())
        .unwrap_or_default();
    Metadata {
        description,
        placeholders,
    }
}

fn sibling_arb_files(arb_dir: &Utf8Path, template: &Utf8Path) -> Result<Vec<Utf8PathBuf>> {
    let mut paths = Vec::new();
    for entry in arb_dir
        .read_dir_utf8()
        .with_context(|| format!("Failed to list {arb_dir}"))?
    {
        let path = entry
            .with_context(|| format!("Failed to read an entry in {arb_dir}"))?
            .path()
            .to_path_buf();
        if path != template && path.extension() == Some("arb") {
            paths.push(path);
        }
    }
    paths.sort();
    Ok(paths)
}

fn resolve(base: &Utf8Path, raw: &str) -> Utf8PathBuf {
    let path = Utf8PathBuf::from(raw);
    if path.is_absolute() {
        path
    } else {
        base.join(path)
    }
}

// -- reporting -------------------------------------------------------------

#[derive(Default)]
struct Report {
    findings: Vec<Finding>,
}

struct Finding {
    file: String,
    key: String,
    detail: String,
}

impl Report {
    fn push(&mut self, file: &str, key: &str, detail: impl Into<String>) {
        self.findings.push(Finding {
            file: file.to_owned(),
            key: key.to_owned(),
            detail: detail.into(),
        });
    }

    fn finish(self, message_count: usize, file_count: usize) -> Result<()> {
        if self.findings.is_empty() {
            println!(
                "✅ {message_count} message(s) across {file_count} ARB file(s) are consistent."
            );
            return Ok(());
        }

        let mut grouped: BTreeMap<&str, Vec<&Finding>> = BTreeMap::new();
        for finding in &self.findings {
            grouped.entry(&finding.file).or_default().push(finding);
        }
        for (file, findings) in grouped {
            println!("\n{file}");
            for finding in findings {
                println!("  • {}: {}", finding.key, finding.detail);
            }
        }
        bail!("\n{} localization problem(s) found.", self.findings.len());
    }
}

// -- template checks -------------------------------------------------------

fn check_template(template: &ArbFile, report: &mut Report) {
    for (key, text) in &template.messages {
        let parsed = match icu::parse(text) {
            Ok(parsed) => parsed,
            Err(error) => {
                report.push(&template.name, key, format!("invalid ICU message: {error}"));
                continue;
            }
        };
        check_description(template, key, report);
        check_declarations(template, key, &parsed.placeholders, report);
        check_choices(&template.name, key, &parsed, report);
    }
}

fn check_description(template: &ArbFile, key: &str, report: &mut Report) {
    let description = template
        .metadata
        .get(key)
        .and_then(|entry| entry.description.as_deref())
        .map(str::trim)
        .unwrap_or_default();

    if description.is_empty() {
        report.push(
            &template.name,
            key,
            format!(
                "missing a description. Add \"@{key}\" with a \"description\" \
                 saying what the string is and where it appears"
            ),
        );
    } else if description.chars().count() < MIN_DESCRIPTION_LENGTH {
        report.push(
            &template.name,
            key,
            format!("description \"{description}\" is too short to give a translator context"),
        );
    }
}

/// Every placeholder the template interpolates has to be declared, otherwise
/// `gen-l10n` types it as `Object` and no type carries into the Dart call site.
fn check_declarations(template: &ArbFile, key: &str, used: &BTreeSet<String>, report: &mut Report) {
    let declared = template.declared_placeholders(key);
    for name in used.difference(&declared) {
        report.push(
            &template.name,
            key,
            format!("placeholder '{name}' is used but not declared in \"@{key}\".placeholders"),
        );
    }
    for name in declared.difference(used) {
        report.push(
            &template.name,
            key,
            format!("placeholder '{name}' is declared but never used in the message"),
        );
    }
}

// -- locale checks ---------------------------------------------------------

fn check_locale(locale: &ArbFile, template: &ArbFile, report: &mut Report) {
    check_no_metadata(locale, report);
    for key in template.messages.keys() {
        if !locale.messages.contains_key(key) {
            report.push(&locale.name, key, "missing translation");
        }
    }
    for key in locale.messages.keys() {
        if !template.messages.contains_key(key) {
            report.push(
                &locale.name,
                key,
                "not present in the template. Remove it or add the key to the template",
            );
        }
    }

    for (key, translation) in &locale.messages {
        let Some(source) = template.messages.get(key) else {
            continue;
        };
        let parsed = match icu::parse(translation) {
            Ok(parsed) => parsed,
            Err(error) => {
                report.push(&locale.name, key, format!("invalid ICU message: {error}"));
                continue;
            }
        };
        check_choices(&locale.name, key, &parsed, report);
        let context = KeyContext {
            file: &locale.name,
            key,
            source,
            translation,
        };
        // A template that does not parse is reported against the template
        // itself, so comparing against it here would only add noise.
        if let Ok(source_parsed) = icu::parse(source) {
            check_placeholder_parity(
                &context,
                &source_parsed.placeholders,
                &parsed.placeholders,
                report,
            );
        }
        check_protected_terms(&context, report);
        check_line_breaks(&context, report);
        check_terminal_punctuation(&context, report);
    }
}

struct KeyContext<'a> {
    file: &'a str,
    key: &'a str,
    source: &'a str,
    translation: &'a str,
}

/// The check that catches the failure `gen-l10n` renders as a literal `null`.
fn check_placeholder_parity(
    context: &KeyContext<'_>,
    expected: &BTreeSet<String>,
    used: &BTreeSet<String>,
    report: &mut Report,
) {
    for name in used.difference(expected) {
        report.push(
            context.file,
            context.key,
            format!(
                "uses placeholder '{name}', which the template does not define. \
                 gen-l10n renders it as the text \"null\""
            ),
        );
    }
    for name in expected.difference(used) {
        report.push(
            context.file,
            context.key,
            format!("drops placeholder '{name}' that the template interpolates"),
        );
    }
}

/// `gen-l10n` reads placeholder types from the template, so metadata in a
/// translation is dead weight at best. At worst it declares a placeholder the
/// template does not have, which is how the literal `null` reached production.
fn check_no_metadata(locale: &ArbFile, report: &mut Report) {
    for key in locale.metadata.keys() {
        report.push(
            &locale.name,
            key,
            "translations carry no metadata. Describe the message and declare \
             its placeholders in the template instead",
        );
    }
}

fn check_choices(file: &str, key: &str, parsed: &icu::ParsedMessage, report: &mut Report) {
    for choice in &parsed.choices {
        let unknown = choice.unknown_selectors();
        if !unknown.is_empty() {
            report.push(
                file,
                key,
                format!(
                    "'{}' on '{}' has unrecognised arm(s): {}. Use a CLDR category or '=N'",
                    choice.kind,
                    choice.name,
                    unknown.join(", ")
                ),
            );
        }
        if !choice.has_catch_all() {
            report.push(
                file,
                key,
                format!(
                    "'{}' on '{}' has no 'other' arm, so unmatched values render empty",
                    choice.kind, choice.name
                ),
            );
        }
    }
}

fn check_protected_terms(context: &KeyContext<'_>, report: &mut Report) {
    for term in PROTECTED_TERMS {
        if contains_word(context.source, term) && !contains_word(context.translation, term) {
            report.push(
                context.file,
                context.key,
                format!("drops the product name \"{term}\" that the source uses"),
            );
        }
    }
}

/// Match a term only as a whole word so "Air" does not hit inside "Airport",
/// while still matching the compounds translations build, such as "Air-Konto".
fn contains_word(haystack: &str, term: &str) -> bool {
    let mut rest = haystack;
    while let Some(offset) = rest.find(term) {
        let before = rest[..offset].chars().next_back();
        let after = rest[offset + term.len()..].chars().next();
        let boundary_before = before.is_none_or(|ch| !ch.is_alphanumeric());
        let boundary_after = after.is_none_or(|ch| !ch.is_alphanumeric());
        if boundary_before && boundary_after {
            return true;
        }
        rest = &rest[offset + term.len()..];
    }
    false
}

/// Paragraph breaks are layout, not prose, so they have to survive intact.
fn check_line_breaks(context: &KeyContext<'_>, report: &mut Report) {
    let source = context.source.matches('\n').count();
    let translation = context.translation.matches('\n').count();
    if source != translation {
        report.push(
            context.file,
            context.key,
            format!("has {translation} line break(s) but the source has {source}"),
        );
    }
}

/// Gaining or losing sentence-final punctuation usually means a question became
/// a label, or a title picked up prose punctuation it should not have.
fn check_terminal_punctuation(context: &KeyContext<'_>, report: &mut Report) {
    let source = context.source.trim_end();
    let translation = context.translation.trim_end();
    // A trailing placeholder carries its own punctuation, so skip those.
    if source.ends_with('}') || translation.ends_with('}') {
        return;
    }
    let source_mark = terminal_mark(source);
    let translation_mark = terminal_mark(translation);
    if source_mark == translation_mark {
        return;
    }
    let detail = match (source_mark, translation_mark) {
        (Some(expected), None) => {
            format!("source ends with '{expected}' but the translation ends without punctuation")
        }
        (None, Some(found)) => {
            format!("ends with '{found}' but the source ends without punctuation")
        }
        (Some(expected), Some(found)) => {
            format!("ends with '{found}' but the source ends with '{expected}'")
        }
        (None, None) => return,
    };
    report.push(context.file, context.key, detail);
}

fn terminal_mark(text: &str) -> Option<char> {
    text.chars().last().filter(|ch| TERMINAL_MARKS.contains(ch))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn word_matching_respects_boundaries() {
        assert!(contains_word("New Air contact", "Air"));
        assert!(contains_word("Neuer Air-Kontakt", "Air"));
        assert!(contains_word("Air", "Air"));
        assert!(!contains_word("Airport shuttle", "Air"));
        assert!(!contains_word("Nouveau contact", "Air"));
    }

    #[test]
    fn word_matching_finds_later_occurrences() {
        assert!(contains_word("Airport near Air HQ", "Air"));
    }
}
