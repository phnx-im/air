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
    input: Utf8PathBuf,
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

/// What we emit per emoji.
struct OutEmoji {
    /// Dart escape sequence for the code points, e.g. `\u{0023}\u{FE0F}\u{20E3}`.
    escape: String,
    short_names: Vec<String>,
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
    category_id: usize,
    index: usize,
}

pub(crate) fn run(args: GenerateEmojiArgs) -> Result<()> {
    let raw = fs::read_to_string(&args.input)
        .with_context(|| format!("reading emoji source {}", args.input))?;
    let source: Vec<SourceEmoji> =
        serde_json::from_str(&raw).context("parsing emoji_pretty.json")?;

    // Group by category. BTreeMap keeps a stable iteration order while we
    // collect; categories are re-ordered below by their canonical sort_order.
    let mut by_category: BTreeMap<String, Vec<OutEmoji>> = BTreeMap::new();
    for entry in source {
        if EXCLUDED_CATEGORIES.contains(&entry.category.as_str()) {
            continue;
        }
        by_category
            .entry(entry.category)
            .or_default()
            .push(OutEmoji {
                escape: to_dart_escape(&entry.unified),
                short_names: entry.short_names,
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

    // Short code -> (category id, index within that category). Category ids are
    // the position in `categories`. First occurrence wins so the mapping is
    // stable; duplicates across emojis are skipped (and counted).
    let mut shortcodes: Vec<(String, usize, usize)> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    let mut duplicate_shortcodes = 0usize;
    for (category_id, (_, group)) in categories.iter().enumerate() {
        for (index, emoji) in group.iter().enumerate() {
            for name in &emoji.short_names {
                let key = name.to_lowercase();
                if seen.insert(key.clone()) {
                    shortcodes.push((key, category_id, index));
                } else {
                    duplicate_shortcodes += 1;
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
        shortcodes: shortcodes
            .iter()
            .map(|(code, category_id, index)| TemplateShortcode {
                code: dart_string(code),
                category_id: *category_id,
                index: *index,
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
         ({duplicate_shortcodes} duplicates skipped) to {output}",
        categories.len(),
        shortcodes.len(),
    );

    let shell = Shell::new()?;
    cmd!(shell, "dart format {output}").run()?;

    Ok(())
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
