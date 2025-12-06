// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::sync::LazyLock;

use syntect::{
    easy::HighlightLines,
    highlighting::{Highlighter, RangedHighlightIterator, Style, ThemeSet},
    parsing::SyntaxSet,
};

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct HighlightRange {
    pub start: u16,
    pub end: u16,
    pub style: HighlightStyle,
}

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct HighlightStyle {
    pub fg: Option<HighlightColor>,
    pub bg: Option<HighlightColor>,
    pub style: Option<HighlightFontStyle>,
}

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct HighlightColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub struct HighlightFontStyle {
    pub bits: u8,
}

pub const HIGHLIGHT_FONT_STYLE_BOLD: u8 = 1 << 0;
pub const HIGHLIGHT_FONT_STYLE_UNDERLINE: u8 = 1 << 1;
pub const HIGHLIGHT_FONT_STYLE_ITALIC: u8 = 1 << 2;

impl From<Style> for HighlightStyle {
    fn from(style: Style) -> Self {
        Self {
            fg: Some(HighlightColor {
                r: style.foreground.r,
                g: style.foreground.g,
                b: style.foreground.b,
                a: style.foreground.a,
            }),
            bg: Some(HighlightColor {
                r: style.background.r,
                g: style.background.g,
                b: style.background.b,
                a: style.background.a,
            }),
            style: Some(HighlightFontStyle {
                bits: style.font_style.bits(),
            }),
        }
    }
}

static SYNTAX_SET: LazyLock<SyntaxSet> = LazyLock::new(SyntaxSet::load_defaults_newlines);

static THEME_SET: LazyLock<ThemeSet> = LazyLock::new(ThemeSet::load_defaults);

pub(crate) fn highlight_code_block(
    code: &str,
    lang: &str,
) -> anyhow::Result<Option<Vec<HighlightRange>>> {
    let syntax_set = &*SYNTAX_SET;

    let Some(syntax) = syntax_set.find_syntax_by_token(lang) else {
        return Ok(None);
    };

    for theme in THEME_SET.themes.keys() {
        println!("{:?}", theme);
    }

    let theme = THEME_SET.themes.get("InspiredGitHub").unwrap();
    let highlight_lines = HighlightLines::new(syntax, theme);
    let (mut highlight_state, mut parse_state) = highlight_lines.state();

    let highlighter = Highlighter::new(theme);

    let mut res = Vec::new();

    let mut offset: u16 = 0;
    for line in code.lines() {
        let ops = parse_state.parse_line(line, syntax_set)?;
        let iter = RangedHighlightIterator::new(&mut highlight_state, &ops[..], line, &highlighter);
        for (style, _s, range) in iter {
            let range = HighlightRange {
                start: offset + range.start as u16,
                end: offset + range.end as u16,
                style: style.into(),
            };
            res.push(range);
        }
        if let Some(range) = res.last_mut() {
            range.end += 1; // include newline
        }
        offset += line.len() as u16 + 1;
    }
    if let Some(range) = res.last_mut() {
        range.end -= 1;
    }

    Ok(Some(res))
}
