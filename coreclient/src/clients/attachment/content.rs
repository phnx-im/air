// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::identifiers::UserId;
use airprotos::client::self_group::SelfGroupAppMessage;
use anyhow::ensure;
use mimi_content::{
    Disposition, MimiContent,
    content_container::{NestedPart, PartSemantics},
};
use openmls::group::GroupId;

use crate::AttachmentUrl;

pub trait MimiContentExt {
    fn mimi_id(&self, sender: &UserId, group_id: &GroupId) -> anyhow::Result<Vec<u8>>;

    fn visit_parts<'a>(&'a self, visitor: impl FnMut(VisitedPart<'a>));

    fn visit_attachments(
        &self,
        visitor: impl FnMut(&NestedPart) -> anyhow::Result<()>,
    ) -> anyhow::Result<()>;

    fn visit_attachments_mut(
        &mut self,
        visitor: impl FnMut(&mut NestedPart) -> anyhow::Result<()>,
    ) -> anyhow::Result<()>;

    /// The self-group application message this content carries, if any.
    fn self_group_message(&self) -> Option<SelfGroupAppMessage>;
}

#[derive(Debug)]
pub enum VisitedPart<'a> {
    /// `text/markdown` or `text/plain`, not as an attachment.
    Text {
        content: &'a str,
        format: TextFormat,
    },
    /// Has an attachment record at `index`.
    Attachment {
        /// ExternalPart or non-text SinglePart
        part: &'a NestedPart,
        index: usize,
        /// disposition preview, or inside a preview container
        preview: bool,
        /// text/blurhash preview sibling
        blurhash: Option<&'a str>,
    },
    /// Meant as content, but not renderable.
    Unknown(&'a NestedPart),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextFormat {
    Markdown,
    Plain,
}

impl TextFormat {
    fn from_media_type(media_type: &str) -> Option<Self> {
        match media_type {
            "text/markdown" => Some(Self::Markdown),
            "text/plain" => Some(Self::Plain),
            _ => None,
        }
    }
}

impl MimiContentExt for MimiContent {
    fn mimi_id(&self, sender: &UserId, group_id: &GroupId) -> anyhow::Result<Vec<u8>> {
        Ok(self.message_id(sender.to_bytes()?.as_slice(), group_id.as_slice())?)
    }

    fn visit_parts<'a>(&'a self, mut visitor: impl FnMut(VisitedPart<'a>)) {
        Ctx {
            depth: 0,
            in_preview: false,
        }
        .walk(&self.nested_part, None, &mut 0, &mut visitor)
    }

    fn visit_attachments(
        &self,
        mut visitor: impl FnMut(&NestedPart) -> anyhow::Result<()>,
    ) -> anyhow::Result<()> {
        visit_attachments_impl(&self.nested_part, &mut visitor, 0)
    }

    fn visit_attachments_mut(
        &mut self,
        mut visitor: impl FnMut(&mut NestedPart) -> anyhow::Result<()>,
    ) -> anyhow::Result<()> {
        visit_attachments_mut_impl(&mut self.nested_part, &mut visitor, 0)
    }

    fn self_group_message(&self) -> Option<SelfGroupAppMessage> {
        SelfGroupAppMessage::from_mimi_content(self)
    }
}

const MAX_RECURSION_DEPTH: usize = 3;

fn visit_attachments_impl(
    part: &NestedPart,
    visitor: &mut impl FnMut(&NestedPart) -> anyhow::Result<()>,
    recursion_depth: usize,
) -> anyhow::Result<()> {
    ensure!(
        recursion_depth < MAX_RECURSION_DEPTH,
        "Failed to handle attachment due to maximum recursion depth reached"
    );

    match &part {
        external_part @ NestedPart::ExternalPart { .. } => {
            visitor(external_part)?;
        }
        NestedPart::MultiPart {
            part_semantics: PartSemantics::ProcessAll,
            parts,
            ..
        } => {
            for part in parts {
                visit_attachments_impl(part, visitor, recursion_depth + 1)?;
            }
        }
        _ => (),
    }

    Ok(())
}

fn visit_attachments_mut_impl(
    part: &mut NestedPart,
    visitor: &mut impl FnMut(&mut NestedPart) -> anyhow::Result<()>,
    recursion_depth: usize,
) -> anyhow::Result<()> {
    ensure!(
        recursion_depth < MAX_RECURSION_DEPTH,
        "Failed to handle attachment due to maximum recursion depth reached"
    );

    match part {
        external_part @ NestedPart::ExternalPart { .. } => {
            visitor(external_part)?;
        }
        NestedPart::MultiPart {
            part_semantics: PartSemantics::ProcessAll,
            parts,
            ..
        } => {
            for part in parts {
                visit_attachments_mut_impl(part, visitor, recursion_depth + 1)?;
            }
        }
        _ => (),
    }

    Ok(())
}

const MAX_DEPTH: usize = 8;

enum Leaf<'a> {
    Text(&'a str, TextFormat),
    Attachment,
    Blurhash(&'a str),
    Ignored,
    Unknown,
}

fn is_skipped(disposition: Disposition) -> bool {
    matches!(
        disposition,
        Disposition::Reaction | Disposition::Icon | Disposition::Profile
    )
}

#[derive(Clone, Copy)]
struct Ctx {
    depth: usize,
    in_preview: bool,
}

impl Ctx {
    fn walk<'a>(
        self,
        part: &'a NestedPart,
        blurhash: Option<&'a str>,
        next_index: &mut usize,
        visitor: &mut impl FnMut(VisitedPart<'a>),
    ) {
        if MAX_DEPTH <= self.depth {
            visitor(VisitedPart::Unknown(part));
            return;
        }
        if is_skipped(part.disposition()) {
            return;
        }
        let ctx = self.enter(part);
        match part {
            NestedPart::NullPart { .. } => {}
            // leaf
            NestedPart::SinglePart { .. } | NestedPart::ExternalPart { .. } => {
                match ctx.classify_leaf(part) {
                    Leaf::Text(content, format) => visitor(VisitedPart::Text { content, format }),
                    Leaf::Attachment => {
                        let index = *next_index;
                        *next_index += 1;
                        visitor(VisitedPart::Attachment {
                            part,
                            index,
                            preview: ctx.in_preview,
                            blurhash,
                        })
                    }
                    Leaf::Blurhash(_) => {}
                    Leaf::Ignored => {}
                    Leaf::Unknown => visitor(VisitedPart::Unknown(part)),
                }
            }
            // nested
            NestedPart::MultiPart {
                part_semantics,
                parts,
                ..
            } => {
                let child_ctx = Ctx {
                    depth: ctx.depth + 1,
                    ..ctx
                };
                match part_semantics {
                    PartSemantics::ChooseOne => {
                        if let Some((chosen, _coverage)) = child_ctx.best_alternative(parts) {
                            child_ctx.walk(chosen, None, next_index, visitor);
                        } else if child_ctx.children_coverage(parts).missing {
                            visitor(VisitedPart::Unknown(part));
                        }
                    }
                    PartSemantics::SingleUnit => {
                        if child_ctx.children_coverage(parts).is_full() {
                            child_ctx.walk_children(parts, next_index, visitor);
                        } else {
                            visitor(VisitedPart::Unknown(part));
                        }
                    }
                    PartSemantics::ProcessAll | PartSemantics::Custom(_) => {
                        child_ctx.walk_children(parts, next_index, visitor);
                    }
                }
            }
        }
    }

    fn sibling_blurhash(self, parts: &[NestedPart]) -> Option<(usize, &str)> {
        let mut attachment = None;
        let mut blurhash = None;
        for (index, part) in parts.iter().enumerate() {
            if is_skipped(part.disposition()) {
                continue;
            }
            match self.enter(part).classify_leaf(part) {
                Leaf::Attachment => {
                    if attachment.is_some() {
                        return None;
                    }
                    attachment = Some(index);
                }
                Leaf::Blurhash(hash) => {
                    if blurhash.is_none() {
                        blurhash = Some(hash);
                    }
                }
                Leaf::Text(..) | Leaf::Ignored | Leaf::Unknown => {}
            }
        }
        Some((attachment?, blurhash?))
    }

    fn walk_children<'a>(
        self,
        parts: &'a [NestedPart],
        next_index: &mut usize,
        visitor: &mut impl FnMut(VisitedPart<'a>),
    ) {
        let blurhash = self.sibling_blurhash(parts);
        for (index, child) in parts.iter().enumerate() {
            let child_blurhash = match blurhash {
                Some((attachment, blurhash)) if attachment == index => Some(blurhash),
                _ => None,
            };
            self.walk(child, child_blurhash, next_index, visitor);
        }
    }

    fn coverage(self, part: &NestedPart) -> Coverage {
        if MAX_DEPTH <= self.depth {
            return Coverage {
                shown: false,
                missing: true,
            };
        }
        if is_skipped(part.disposition()) {
            return Coverage::default();
        }
        let ctx = self.enter(part);
        match part {
            NestedPart::NullPart { .. } => Coverage::default(),
            NestedPart::SinglePart { .. } | NestedPart::ExternalPart { .. } => {
                match ctx.classify_leaf(part) {
                    Leaf::Text(..) => Coverage {
                        shown: true,
                        missing: false,
                    },
                    Leaf::Attachment => Coverage {
                        shown: !ctx.in_preview,
                        missing: false,
                    },
                    Leaf::Blurhash(_) | Leaf::Ignored => Coverage::default(),
                    Leaf::Unknown => Coverage {
                        shown: false,
                        missing: true,
                    },
                }
            }
            NestedPart::MultiPart {
                part_semantics,
                parts,
                ..
            } => {
                let child_ctx = Ctx {
                    depth: ctx.depth + 1,
                    ..ctx
                };
                match part_semantics {
                    PartSemantics::ChooseOne => match child_ctx.best_alternative(parts) {
                        Some((_chosen, coverage)) => coverage,
                        None => child_ctx.children_coverage(parts),
                    },
                    PartSemantics::SingleUnit => {
                        let coverage = child_ctx.children_coverage(parts);
                        if coverage.is_full() {
                            coverage
                        } else {
                            Coverage {
                                shown: false,
                                missing: true,
                            }
                        }
                    }
                    PartSemantics::ProcessAll | PartSemantics::Custom(_) => {
                        child_ctx.children_coverage(parts)
                    }
                }
            }
        }
    }

    fn children_coverage(self, parts: &[NestedPart]) -> Coverage {
        let mut sum = Coverage::default();
        for part in parts {
            sum = sum.merge(self.coverage(part));
        }
        sum
    }

    fn enter(self, part: &NestedPart) -> Self {
        Self {
            in_preview: self.in_preview || part.disposition() == Disposition::Preview,
            ..self
        }
    }

    fn best_alternative(self, parts: &[NestedPart]) -> Option<(&NestedPart, Coverage)> {
        let mut best: Option<(&NestedPart, Coverage, u8)> = None;
        for part in parts {
            let coverage = self.coverage(part);
            let rank = coverage.rank();
            let better = match best {
                Some((_, _, best_rank)) => best_rank < rank,
                None => 0 < rank,
            };
            if better {
                best = Some((part, coverage, rank));
            }
        }
        best.map(|(part, coverage, _)| (part, coverage))
    }

    fn classify_leaf(self, part: &NestedPart) -> Leaf<'_> {
        match part {
            NestedPart::SinglePart {
                disposition,
                content_type,
                content,
                ..
            } => {
                let media_type = media_type(content_type);
                if self.in_preview {
                    if media_type == "text/blurhash" {
                        str::from_utf8(content)
                            .map(Leaf::Blurhash)
                            .unwrap_or(Leaf::Ignored)
                    } else if media_type.starts_with("text/") {
                        Leaf::Ignored
                    } else {
                        Leaf::Attachment
                    }
                } else {
                    match TextFormat::from_media_type(&media_type) {
                        Some(format) if *disposition != Disposition::Attachment => {
                            str::from_utf8(content)
                                .map(|text| Leaf::Text(text, format))
                                .unwrap_or(Leaf::Unknown)
                        }
                        _ => Leaf::Attachment,
                    }
                }
            }
            NestedPart::ExternalPart { url, .. } => match url.parse::<AttachmentUrl>() {
                Ok(_) => Leaf::Attachment,
                Err(_) => Leaf::Unknown, // Non-Air URL => For now unknown
            },
            NestedPart::NullPart { .. } | NestedPart::MultiPart { .. } => Leaf::Ignored,
        }
    }
}

fn media_type(content_type: &str) -> String {
    let essence = match content_type.split_once(';') {
        Some((essence, _)) => essence,
        None => content_type,
    };
    essence.trim().to_ascii_lowercase()
}

#[derive(Default, Clone, Copy)]
struct Coverage {
    shown: bool,
    missing: bool,
}

impl Coverage {
    fn merge(self, other: Self) -> Self {
        Self {
            shown: self.shown || other.shown,
            missing: self.missing || other.missing,
        }
    }

    fn is_full(self) -> bool {
        self.shown && !self.missing
    }

    /// 2 full, 1 partial, 0 nothing shown
    fn rank(self) -> u8 {
        match (self.shown, self.missing) {
            (true, false) => 2,
            (true, true) => 1,
            (false, _) => 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use mimi_content::content_container::{EncryptionAlgorithm, HashAlgorithm};
    use uuid::Uuid;

    use aircommon::identifiers::RemoteAttachmentId;

    use super::*;

    #[derive(Debug, PartialEq, Eq)]
    enum Seen {
        Text(String, TextFormat),
        Att {
            index: usize,
            preview: bool,
            blurhash: Option<String>,
        },
        Unknown,
    }

    fn att(index: usize, preview: bool, blurhash: Option<&str>) -> Seen {
        Seen::Att {
            index,
            preview,
            blurhash: blurhash.map(str::to_owned),
        }
    }

    fn md(s: &str) -> Seen {
        Seen::Text(s.to_owned(), TextFormat::Markdown)
    }

    fn visit(content: &MimiContent) -> Vec<Seen> {
        let mut seen = Vec::new();
        content.visit_parts(|part| {
            seen.push(match part {
                VisitedPart::Text { content, format } => Seen::Text(content.to_owned(), format),
                VisitedPart::Attachment {
                    index,
                    preview,
                    blurhash,
                    ..
                } => Seen::Att {
                    index,
                    preview,
                    blurhash: blurhash.map(str::to_owned),
                },
                VisitedPart::Unknown(_) => Seen::Unknown,
            })
        });
        seen
    }

    fn content(nested_part: NestedPart) -> MimiContent {
        MimiContent {
            nested_part,
            ..Default::default()
        }
    }

    fn visit_part(part: NestedPart) -> Vec<Seen> {
        visit(&content(part))
    }

    fn single(disposition: Disposition, content_type: &str, content: &[u8]) -> NestedPart {
        NestedPart::SinglePart {
            disposition,
            language: String::new(),
            content_type: content_type.to_owned(),
            content: content.to_vec(),
        }
    }

    fn text(content_type: &str, s: &str) -> NestedPart {
        single(Disposition::Render, content_type, s.as_bytes())
    }

    fn blurhash() -> NestedPart {
        single(Disposition::Preview, "text/blurhash", b"LEHV6nWB2yk8")
    }

    fn external_url(disposition: Disposition, content_type: &str, url: String) -> NestedPart {
        NestedPart::ExternalPart {
            disposition,
            language: String::new(),
            content_type: content_type.to_owned(),
            url,
            expires: 0,
            size: 1,
            enc_alg: EncryptionAlgorithm::None,
            key: Vec::new(),
            nonce: Vec::new(),
            aad: Vec::new(),
            hash_alg: HashAlgorithm::Unspecified,
            content_hash: Vec::new(),
            description: String::new(),
            filename: String::new(),
        }
    }

    fn external(disposition: Disposition, content_type: &str) -> NestedPart {
        let url = AttachmentUrl::new(RemoteAttachmentId::new(Uuid::new_v4()), None).to_string();
        external_url(disposition, content_type, url)
    }

    fn file() -> NestedPart {
        external(Disposition::Attachment, "image/png")
    }

    fn multi(
        disposition: Disposition,
        part_semantics: PartSemantics,
        parts: Vec<NestedPart>,
    ) -> NestedPart {
        NestedPart::MultiPart {
            disposition,
            language: String::new(),
            part_semantics,
            parts,
        }
    }

    fn all(parts: Vec<NestedPart>) -> NestedPart {
        multi(Disposition::Render, PartSemantics::ProcessAll, parts)
    }

    fn choose(parts: Vec<NestedPart>) -> NestedPart {
        multi(Disposition::Render, PartSemantics::ChooseOne, parts)
    }

    fn unit(parts: Vec<NestedPart>) -> NestedPart {
        multi(Disposition::Render, PartSemantics::SingleUnit, parts)
    }

    fn null() -> NestedPart {
        NestedPart::NullPart {
            disposition: Disposition::Render,
            language: String::new(),
        }
    }

    fn invalid_utf8() -> NestedPart {
        single(Disposition::Render, "text/plain", &[0xff, 0xfe])
    }

    #[test]
    fn simple_markdown() {
        let content = MimiContent::simple_markdown_message("hi".to_owned(), [0; 16]);
        assert_eq!(visit(&content), [md("hi")]);
    }

    #[test]
    fn media_type_normalisation() {
        let seen = visit_part(all(vec![
            text("Text/Markdown; variant=GFM", "a"),
            text("text/plain;charset=utf-8", "b"),
        ]));
        assert_eq!(
            seen,
            [md("a"), Seen::Text("b".to_owned(), TextFormat::Plain)]
        );
    }

    #[test]
    fn air_attachment_layout() {
        let seen = visit_part(multi(
            Disposition::Attachment,
            PartSemantics::ProcessAll,
            vec![external(Disposition::Attachment, "image/png"), blurhash()],
        ));
        assert_eq!(seen, [att(0, false, Some("LEHV6nWB2yk8"))]);
    }

    #[test]
    fn link_preview_layout() {
        let seen = visit_part(all(vec![
            text("text/markdown", "see"),
            single(
                Disposition::Preview,
                "application/vnd.air.link-preview+cbor",
                b"\x00",
            ),
            external(Disposition::Preview, "image/webp"),
        ]));
        assert_eq!(seen, [md("see"), att(0, true, None), att(1, true, None)]);
    }

    #[test]
    fn consecutive_indices_across_nesting() {
        let seen = visit_part(all(vec![
            file(),
            all(vec![file(), all(vec![file()])]),
            file(),
        ]));
        assert_eq!(
            seen,
            [
                att(0, false, None),
                att(1, false, None),
                att(2, false, None),
                att(3, false, None)
            ]
        );
    }

    #[test]
    fn blurhash_dropped_with_two_attachments() {
        let seen = visit_part(all(vec![file(), file(), blurhash()]));
        assert_eq!(seen, [att(0, false, None), att(1, false, None)]);
    }

    #[test]
    fn text_as_attachment_and_html() {
        let seen = visit_part(all(vec![
            single(Disposition::Attachment, "text/plain", b"x"),
            text("text/html", "<b>"),
        ]));
        assert_eq!(seen, [att(0, false, None), att(1, false, None)]);
    }

    #[test]
    fn unknown_leaves() {
        assert_eq!(visit_part(invalid_utf8()), [Seen::Unknown]);
        let url = "https://example.com/x".to_owned();
        let part = external_url(Disposition::Attachment, "image/png", url);
        assert_eq!(visit_part(part), [Seen::Unknown]);
    }

    #[test]
    fn skipped_dispositions() {
        for disposition in [
            Disposition::Reaction,
            Disposition::Icon,
            Disposition::Profile,
        ] {
            assert_eq!(visit_part(text_with(disposition)), []);
            let skipped = multi(disposition, PartSemantics::ProcessAll, vec![file()]);
            let seen = visit_part(all(vec![skipped, file()]));
            assert_eq!(seen, [att(0, false, None)]);
        }
    }

    fn text_with(disposition: Disposition) -> NestedPart {
        single(disposition, "text/markdown", b"x")
    }

    #[test]
    fn preview_container() {
        let seen = visit_part(multi(
            Disposition::Preview,
            PartSemantics::ProcessAll,
            vec![
                single(Disposition::Render, "image/png", b"x"),
                external(Disposition::Render, "image/png"),
            ],
        ));
        assert_eq!(seen, [att(0, true, None), att(1, true, None)]);
    }

    #[test]
    fn choose_one_full_over_partial() {
        let partial = all(vec![text("text/plain", "p"), invalid_utf8()]);
        let full = all(vec![text("text/markdown", "f")]);
        let seen = visit_part(choose(vec![partial, full]));
        assert_eq!(seen, [md("f")]);
    }

    #[test]
    fn choose_one_first_on_tie() {
        let seen = visit_part(choose(vec![
            all(vec![text("text/markdown", "a")]),
            all(vec![text("text/markdown", "b")]),
        ]));
        assert_eq!(seen, [md("a")]);
    }

    #[test]
    fn choose_one_unselected_attachment_consumes_no_index() {
        let seen = visit_part(all(vec![
            choose(vec![text("text/markdown", "a"), file()]),
            file(),
        ]));
        assert_eq!(seen, [md("a"), att(0, false, None)]);
    }

    #[test]
    fn choose_one_partial_only_is_walked() {
        let seen = visit_part(choose(vec![all(vec![
            text("text/markdown", "p"),
            invalid_utf8(),
        ])]));
        assert_eq!(seen, [md("p"), Seen::Unknown]);
    }

    #[test]
    fn choose_one_all_unknown() {
        let seen = visit_part(choose(vec![invalid_utf8(), invalid_utf8()]));
        assert_eq!(seen, [Seen::Unknown]);
    }

    #[test]
    fn choose_one_all_neutral() {
        let seen = visit_part(choose(vec![null(), text_with(Disposition::Icon), null()]));
        assert_eq!(seen, []);
    }

    #[test]
    fn choose_one_preview_only_is_not_shown() {
        let seen = visit_part(choose(vec![external(Disposition::Preview, "image/webp")]));
        assert_eq!(seen, []);
    }

    #[test]
    fn single_unit_full() {
        let seen = visit_part(unit(vec![text("text/markdown", "a"), file()]));
        assert_eq!(seen, [md("a"), att(0, false, None)]);
    }

    #[test]
    fn single_unit_with_unknown() {
        let seen = visit_part(all(vec![unit(vec![file(), invalid_utf8()]), file()]));
        assert_eq!(seen, [Seen::Unknown, att(0, false, None)]);
    }

    #[test]
    fn custom_semantics_like_process_all() {
        let seen = visit_part(multi(
            Disposition::Render,
            PartSemantics::Custom(42),
            vec![text("text/markdown", "a"), invalid_utf8(), file()],
        ));
        assert_eq!(seen, [md("a"), Seen::Unknown, att(0, false, None)]);
    }

    #[test]
    fn depth_cap() {
        let nest = |levels: usize| {
            let mut part = text("text/markdown", "deep");
            for _ in 0..levels {
                part = all(vec![part]);
            }
            part
        };
        // leaf at depth 7 is visited, leaf at depth 8 is not
        assert_eq!(visit_part(nest(7)), [md("deep")]);
        assert_eq!(visit_part(nest(8)), [Seen::Unknown]);
    }

    #[test]
    fn null_root() {
        assert_eq!(visit_part(null()), []);
    }
}
