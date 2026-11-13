// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

pub const TEXT_MARKDOWN: &str = "text/markdown";
pub const TEXT_PLAIN: &str = "text/plain";
pub const TEXT_BLURHASH: &str = "text/blurhash";
pub const MESSAGE_STATUS: &str = "application/mimi-message-status";

/// Media type essence of a content type, compared ASCII case-insensitively.
#[derive(Debug, Clone, Copy)]
pub struct MediaType<'a>(&'a str);

impl<'a> MediaType<'a> {
    /// Type and subtype of `content_type`, parameters dropped.
    pub fn of(content_type: &'a str) -> Self {
        let essence = match content_type.split_once(';') {
            Some((essence, _)) => essence,
            None => content_type,
        };
        Self(essence.trim())
    }

    /// Whether the top-level type is `ty`, e.g. "text".
    pub fn has_type(self, ty: &str) -> bool {
        match self.0.split_once('/') {
            Some((top, _)) => top.eq_ignore_ascii_case(ty),
            None => false,
        }
    }
}

impl PartialEq<str> for MediaType<'_> {
    fn eq(&self, other: &str) -> bool {
        self.0.eq_ignore_ascii_case(other)
    }
}

impl PartialEq<&str> for MediaType<'_> {
    fn eq(&self, other: &&str) -> bool {
        self.0.eq_ignore_ascii_case(other)
    }
}
