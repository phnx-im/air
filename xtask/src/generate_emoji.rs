// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{collections::BTreeMap, fs};

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

/// Category names in `group` id order. This is the canonical Unicode emoji
/// group ordering (see `emoji-test.txt`), which the source dataset's `group`
/// field indexes into.
const GROUP_NAMES: &[&str] = &[
    "Smileys & Emotion",
    "People & Body",
    "Component",
    "Animals & Nature",
    "Food & Drink",
    "Travel & Places",
    "Activities",
    "Objects",
    "Symbols",
    "Flags",
];

/// Skin-tone modifier code points, in `tone` id order (1-based in the source
/// data, so index with `tone - 1`).
const TONE_MODIFIERS: [&str; 5] = ["1F3FB", "1F3FC", "1F3FD", "1F3FE", "1F3FF"];

#[derive(Args, Debug)]
pub(crate) struct GenerateEmojiArgs {
    /// Path to the source emoji dataset JSON.
    /// You can get a copy from https://github.com/milesj/emojibase/blob/emojibase-data%4017.0.0/packages/data/en/data.raw.json
    #[arg(long)]
    emoji_data_path: Utf8PathBuf,
    /// Path to the shortcodes JSON, keyed by the same `hexcode` format as
    /// `emoji_data_path`. You can get a copy from https://github.com/milesj/emojibase/blob/emojibase-data%4017.0.0/packages/data/en/shortcodes/emojibase.raw.json
    #[arg(long)]
    shortcodes_path: Utf8PathBuf,
    /// Destination Dart file. Relative paths resolve against the workspace root.
    #[arg(long, default_value = DEFAULT_OUTPUT)]
    output: Utf8PathBuf,
}

/// A single entry from the source dataset. Only the fields we emit are kept;
/// the rest of the (large) schema is ignored by serde.
#[derive(Debug, Deserialize)]
struct SourceEmoji {
    /// Hyphen-separated hex code points, e.g. `0023-FE0F-20E3`.
    hexcode: String,
    /// Id into [`GROUP_NAMES`]. Absent for building-block entries (e.g. the
    /// regional-indicator letters flags are composed from), which aren't
    /// pickable emojis.
    #[serde(default)]
    group: Option<u32>,
    /// Canonical ordering across the whole dataset. Always present alongside
    /// `group`.
    #[serde(default)]
    order: Option<u32>,
    /// Extra search keywords.
    #[serde(default)]
    tags: Vec<String>,
    /// Skin-tone variants. Empty for emojis that don't support skin tones.
    #[serde(default)]
    skins: Vec<SourceVariation>,
}

#[derive(Debug, Deserialize)]
struct SourceVariation {
    /// Full code-point sequence of this variant, e.g. `1F385-1F3FB`.
    hexcode: String,
    /// Skin-tone modifier id(s): a single tone, or a pair for two-person
    /// emojis with mismatched tones.
    tone: Tone,
}

/// Maps a `hexcode` to its canonical shortcode plus any aliases.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum Shortcodes {
    One(String),
    Many(Vec<String>),
}

impl Shortcodes {
    fn into_vec(self) -> Vec<String> {
        match self {
            Shortcodes::One(code) => vec![code],
            Shortcodes::Many(codes) => codes,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum Tone {
    Single(u8),
    Multiple(Vec<u8>),
}

impl Tone {
    /// Hyphen-separated hex modifier code point(s), e.g. `1F3FB` or
    /// `1F3FB-1F3FC`, matching the format used as the skin-variation key in
    /// the generated Dart map.
    fn hex_key(&self) -> String {
        let modifier = |tone: u8| TONE_MODIFIERS[usize::from(tone - 1)];
        match self {
            Tone::Single(tone) => modifier(*tone).to_string(),
            Tone::Multiple(tones) => tones
                .iter()
                .map(|t| modifier(*t))
                .collect::<Vec<_>>()
                .join("-"),
        }
    }
}

/// What we emit per emoji.
struct OutEmoji {
    /// Dart escape sequence for the code points, e.g. `\u{0023}\u{FE0F}\u{20E3}`.
    escape: String,
    shortcode: String,
    search_tags: Vec<String>,
    order: u32,
    /// `(tone-modifier escape, variant glyph escape)` pairs, in source order.
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
    let source: Vec<SourceEmoji> = serde_json::from_str(&raw).context("parsing emoji dataset")?;

    let raw = fs::read_to_string(&args.shortcodes_path)
        .with_context(|| format!("reading shortcodes {}", args.shortcodes_path))?;
    let shortcode_map: BTreeMap<String, Shortcodes> =
        serde_json::from_str(&raw).context("parsing shortcodes")?;

    // Group by category. BTreeMap keeps a stable iteration order while we
    // collect; categories are re-ordered below by their canonical order.
    let mut by_category: BTreeMap<String, Vec<OutEmoji>> = BTreeMap::new();
    for entry in source {
        // Entries without a group (e.g. the regional-indicator letters flags
        // are built from) aren't standalone pickable emojis.
        let Some(group) = entry.group else {
            continue;
        };
        let category = GROUP_NAMES
            .get(group as usize)
            .with_context(|| format!("unknown group id {group} for {}", entry.hexcode))?;
        if EXCLUDED_CATEGORIES.contains(category) {
            continue;
        }
        let order = entry
            .order
            .with_context(|| format!("emoji {} has a group but no order", entry.hexcode))?;
        let shortcodes = shortcode_map
            .get(&entry.hexcode)
            .with_context(|| format!("no shortcodes for {}", entry.hexcode))?
            .clone()
            .into_vec();
        let canonical_shortcode = normalize_shortcode(
            shortcodes
                .first()
                .with_context(|| format!("emoji {} has no shortcode", entry.hexcode))?,
        );

        let mut search_tags = shortcodes;
        for tag in &entry.tags {
            let tag = tag.to_lowercase();
            if !search_tags.iter().any(|s| s.eq_ignore_ascii_case(&tag)) {
                search_tags.push(tag);
            }
        }
        by_category
            .entry(category.to_string())
            .or_default()
            .push(OutEmoji {
                escape: to_dart_escape(&entry.hexcode),
                shortcode: canonical_shortcode,
                search_tags,
                order,
                skin_variations: entry
                    .skins
                    .iter()
                    .map(|skin| {
                        (
                            to_dart_escape(&skin.tone.hex_key()),
                            to_dart_escape(&skin.hexcode),
                        )
                    })
                    .collect(),
            });
    }

    // Sort emojis within each category, and order the categories themselves by
    // the smallest order they contain (the dataset's natural grouping).
    let mut categories: Vec<(String, Vec<OutEmoji>)> = by_category.into_iter().collect();
    for (_, emojis) in &mut categories {
        emojis.sort_by_key(|e| e.order);
    }
    categories.sort_by_key(|(_, emojis)| emojis.iter().map(|e| e.order).min().unwrap_or(u32::MAX));

    // Word -> every (category id, index) whose shortcode contains that word.
    // Shortcodes are split on '-' and '_' (e.g. "keycap_star", "medium-light")
    // so each constituent word is independently searchable, and a word can
    // resolve to several emojis.
    let mut shortcode_refs: BTreeMap<String, Vec<(usize, usize)>> = Default::default();
    let mut duplicate_refs = 0usize;
    for (category_id, (_, group)) in categories.iter().enumerate() {
        for (index, emoji) in group.iter().enumerate() {
            for name in &emoji.search_tags {
                for word in split_shortcode(name.clone()) {
                    let refs = shortcode_refs.entry(word.clone()).or_default();
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
                        short_name: emoji.shortcode.clone(),
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

/// Convert dashes into underscores, but avoid replacing shortcodes like :+1: or :-1:
fn normalize_shortcode(code: &str) -> String {
    if code.is_empty() {
        return code.to_owned();
    }
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
