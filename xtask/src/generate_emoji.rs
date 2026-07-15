// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{
    collections::{BTreeMap, HashSet},
    fs,
};

use anyhow::{Context, Result};
use askama::Template;
use camino::Utf8PathBuf;
use clap::Args;
use serde::Deserialize;
use xshell::{Shell, cmd};

use crate::util::workspace_root;

const DEFAULT_OUTPUT: &str = "app/lib/emojis/generated.dart";

/// Categories to omit from the generated output. "Component" only holds the
/// standalone skin-tone / hair modifiers, which aren't pickable emojis.
const EXCLUDED_CATEGORIES: &[&str] = &["Component"];

#[derive(Args, Debug)]
pub(crate) struct GenerateEmojiArgs {
    /// Path to the source `emoji_pretty.json` (emoji-data dataset).
    /// You can obtain a copy of this file from https://github.com/iamcal/emoji-data/blob/v16.0.0/emoji_pretty.json
    #[arg(long)]
    emoji_data_path: Utf8PathBuf,
    /// You can obtain a copy of the file from https://github.com/unicode-org/cldr-json/blob/48.2.1/cldr-json/cldr-annotations-full/annotations/en/annotations.json
    #[arg(long)]
    unicode_cldr_annotations_path: Utf8PathBuf,
    /// Destination Dart file. Relative paths resolve against the workspace root.
    #[arg(long, default_value = DEFAULT_OUTPUT)]
    output: Utf8PathBuf,
}

/// A single entry from `emoji_pretty.json`. Only the fields we emit are kept;
/// the rest of the (large) schema is ignored by serde.
#[derive(Debug, Deserialize)]
struct SourceEmoji {
    /// Hyphen-separated hex code points, e.g. `0023-FE0F-20E3`.
    unified: String,
    short_name: String,
    short_names: Vec<String>,
    category: String,
    /// Canonical ordering across the whole dataset.
    sort_order: u32,
    /// Skin-tone variants, keyed by the tone modifier code(s) — a single tone
    /// (`1F3FB`) or a pair for two-person emojis (`1F3FB-1F3FC`). Absent for
    /// emojis that don't support skin tones. BTreeMap keeps key order stable.
    #[serde(default)]
    skin_variations: BTreeMap<String, SourceVariation>,
}

#[derive(Debug, Deserialize)]
struct SourceVariation {
    /// Full code-point sequence of this variant, e.g. `1F385-1F3FB`.
    unified: String,
}

/// Top-level shape of a CLDR `annotations.json` file.
#[derive(Debug, Deserialize)]
struct CldrRoot {
    annotations: CldrAnnotationsSection,
}

#[derive(Debug, Deserialize)]
struct CldrAnnotationsSection {
    /// Emoji glyph (as a literal string, not code points) -> annotation.
    annotations: BTreeMap<String, CldrAnnotation>,
}

#[derive(Debug, Deserialize)]
struct CldrAnnotation {
    /// Extra search keywords for the emoji, e.g. `["face", "grin", ...]`.
    #[serde(default)]
    default: Vec<String>,
}

/// What we emit per emoji.
struct OutEmoji {
    /// Dart escape sequence for the code points, e.g. `\u{0023}\u{FE0F}\u{20E3}`.
    escape: String,
    short_name: String,
    short_names: HashSet<String>,
    sort_order: u32,
    /// `(tone-modifier escape, variant glyph escape)` pairs, in key order.
    skin_variations: Vec<(String, String)>,
}

/// Render context for `templates/emoji_data.dart.jinja`.
#[derive(Template)]
#[template(path = "emoji_data.dart.jinja", escape = "none")]
struct EmojiDataTemplate {
    categories: Vec<TemplateCategory>,
    shortcodes: Vec<TemplateShortcode>,
}

struct TemplateCategory {
    /// Dart string literal for the category name, e.g. `'Smileys & Emotion'`.
    name: String,
    emojis: Vec<TemplateEmoji>,
}

struct TemplateEmoji {
    /// Dart escape sequence for the glyph (no surrounding quotes).
    escape: String,
    short_name: String,
    variations: Vec<TemplateVariation>,
}

struct TemplateVariation {
    /// Tone-modifier escape sequence (no surrounding quotes).
    tone: String,
    /// Variant glyph escape sequence (no surrounding quotes).
    glyph: String,
}

struct TemplateShortcode {
    /// Dart string literal for the short code, e.g. `'grinning'`.
    code: String,
    refs: Vec<TemplateRef>,
}

struct TemplateRef {
    category_id: usize,
    index: usize,
}

pub(crate) fn run(args: GenerateEmojiArgs) -> Result<()> {
    let raw = fs::read_to_string(&args.emoji_data_path)
        .with_context(|| format!("reading emoji source {}", args.emoji_data_path))?;
    let source: Vec<SourceEmoji> =
        serde_json::from_str(&raw).context("parsing emoji_pretty.json")?;

    let cldr_keywords = load_cldr_keywords(&args.unicode_cldr_annotations_path)?;
    let mut original_short_names = 0usize;
    let mut cldr_matched = 0usize;

    // Group by category. BTreeMap keeps a stable iteration order while we
    // collect; categories are re-ordered below by their canonical sort_order.
    let mut by_category: BTreeMap<String, Vec<OutEmoji>> = BTreeMap::new();
    for entry in source {
        if EXCLUDED_CATEGORIES.contains(&entry.category.as_str()) {
            continue;
        }
        let mut short_names: HashSet<String> = entry
            .short_names
            .into_iter()
            .flat_map(|s| split_shortcode(s))
            .collect();
        original_short_names += short_names.len();
        if let Some(extra) = cldr_keywords.get(&entry.unified) {
            cldr_matched += 1;
            for keyword in extra {
                if !short_names.iter().any(|s| s.eq_ignore_ascii_case(keyword)) {
                    short_names.insert(keyword.clone());
                }
            }
        }
        by_category
            .entry(entry.category)
            .or_default()
            .push(OutEmoji {
                escape: to_dart_escape(&entry.unified),
                short_name: normalize_shortcode(&entry.short_name),
                short_names,
                sort_order: entry.sort_order,
                skin_variations: entry
                    .skin_variations
                    .iter()
                    .map(|(tone, variation)| {
                        (to_dart_escape(tone), to_dart_escape(&variation.unified))
                    })
                    .collect(),
            });
    }
    println!(
        "Matched {} emojis against unicode-data shortnames and {} against CLDR annotations",
        original_short_names, cldr_matched,
    );

    // Sort emojis within each category, and order the categories themselves by
    // the smallest sort_order they contain (the dataset's natural grouping).
    let mut categories: Vec<(String, Vec<OutEmoji>)> = by_category.into_iter().collect();
    for (_, emojis) in &mut categories {
        emojis.sort_by_key(|e| e.sort_order);
    }
    categories.sort_by_key(|(_, emojis)| {
        emojis
            .iter()
            .map(|e| e.sort_order)
            .min()
            .unwrap_or(u32::MAX)
    });

    // Word -> every (category id, index) whose shortcode contains that word.
    // Shortcodes are split on '-' and '_' (e.g. "keycap_star", "medium-light")
    // so each constituent word is independently searchable, and a word can
    // resolve to several emojis.
    let mut shortcode_refs: BTreeMap<String, Vec<(usize, usize)>> = Default::default();
    let mut duplicate_refs = 0usize;
    for (category_id, (_, group)) in categories.iter().enumerate() {
        for (index, emoji) in group.iter().enumerate() {
            // Insert the full emoji shortcode to match when typed entirely
            let refs = shortcode_refs
                .entry(emoji.short_name.clone())
                .or_insert_with(|| Vec::new());
            refs.push((category_id, index));

            for name in &emoji.short_names {
                // Split the shortcodes so search can find words
                for word in split_shortcode(name.clone()) {
                    let refs = shortcode_refs
                        .entry(word.clone())
                        .or_insert_with(|| Vec::new());
                    if refs.contains(&(category_id, index)) {
                        duplicate_refs += 1;
                    } else {
                        refs.push((category_id, index));
                    }
                }
            }
        }
    }

    let template = EmojiDataTemplate {
        categories: categories
            .iter()
            .map(|(name, group)| TemplateCategory {
                name: dart_string(name),
                emojis: group
                    .iter()
                    .map(|emoji| TemplateEmoji {
                        escape: emoji.escape.clone(),
                        short_name: emoji.short_name.clone(),
                        variations: emoji
                            .skin_variations
                            .iter()
                            .map(|(tone, glyph)| TemplateVariation {
                                tone: tone.clone(),
                                glyph: glyph.clone(),
                            })
                            .collect(),
                    })
                    .collect(),
            })
            .collect(),
        shortcodes: shortcode_refs
            .iter()
            .map(|(code, refs)| TemplateShortcode {
                code: dart_string(code),
                refs: refs
                    .iter()
                    .map(|(category_id, index)| TemplateRef {
                        category_id: *category_id,
                        index: *index,
                    })
                    .collect(),
            })
            .collect(),
    };
    let mut dart = template.render().context("rendering emoji template")?;
    // Askama strips the template's trailing newline; dartfmt wants one.
    if !dart.ends_with('\n') {
        dart.push('\n');
    }

    let output = if args.output.is_absolute() {
        args.output.clone()
    } else {
        workspace_root().join(&args.output)
    };
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent).with_context(|| format!("creating {parent}"))?;
    }
    fs::write(&output, dart).with_context(|| format!("writing {output}"))?;

    let emoji_count: usize = categories.iter().map(|(_, g)| g.len()).sum();
    println!(
        "Wrote {emoji_count} emojis across {} categories, {} shortcodes \
         ({duplicate_refs} duplicate refs skipped) to {output}",
        categories.len(),
        shortcode_refs.len(),
    );

    let shell = Shell::new()?;
    cmd!(shell, "dart format {output}").run()?;

    Ok(())
}

/// Loads the CLDR `annotations.json` file and returns its `default` keyword
/// lists, keyed by the same hyphen-separated hex code-point format as
/// `SourceEmoji::unified` (e.g. `1F972`), so entries can be looked up
/// directly by `entry.unified`.
fn load_cldr_keywords(path: &Utf8PathBuf) -> Result<BTreeMap<String, Vec<String>>> {
    let raw =
        fs::read_to_string(path).with_context(|| format!("reading CLDR annotations {path}"))?;
    let root: CldrRoot = serde_json::from_str(&raw).context("parsing CLDR annotations.json")?;

    Ok(root
        .annotations
        .annotations
        .into_iter()
        .filter(|(_, annotation)| !annotation.default.is_empty())
        .map(|(glyph, annotation)| (unified_key(&glyph), annotation.default))
        .collect())
}

/// Converts an emoji glyph (e.g. `"🥲"`) into the same hyphen-separated,
/// zero-padded hex code-point format used by `SourceEmoji::unified` (e.g.
/// `"1F972"`), so it can be matched against entries from `emoji_pretty.json`.
fn unified_key(glyph: &str) -> String {
    glyph
        .chars()
        .map(|c| format!("{:04X}", c as u32))
        .collect::<Vec<_>>()
        .join("-")
}

/// Convert dashes into underscores, but avoid replacing shortcodes like :+1: or :-1:
fn normalize_shortcode(code: &str) -> String {
    let (first, rest) = code.split_at(1);
    first.to_string() + &rest.replace("-", "_")
}

/// Splits a shortcode into its constituent lowercase words on '-' and '_',
/// e.g. `"keycap_star"` -> `["keycap", "star"]`, so each word is
/// independently searchable.
fn split_shortcode(name: String) -> Vec<String> {
    name.split(['-', '_'])
        .filter(|word| !word.is_empty())
        .map(|word| word.to_lowercase())
        .collect()
}

/// Converts `0023-FE0F-20E3` into `\u{0023}\u{FE0F}\u{20E3}`.
fn to_dart_escape(unified: &str) -> String {
    unified
        .split('-')
        .map(|cp| format!("\\u{{{cp}}}"))
        .collect()
}

/// Escapes a short name for use inside a single-quoted Dart string literal.
fn dart_string(value: &str) -> String {
    let escaped = value
        .replace('\\', "\\\\")
        .replace('\'', "\\'")
        .replace('$', "\\$");
    format!("'{escaped}'")
}
