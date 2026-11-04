// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::identifiers::RemoteAttachmentId;
use mimi_content::{
    Disposition,
    content_container::{NestedPart, PartSemantics},
};

use crate::{
    MediaType,
    media_type::{TEXT_BLURHASH, TEXT_MARKDOWN, TEXT_PLAIN},
};

use super::{PartIndex, TextFormat, VisitedPart};

pub(super) fn walk<'a>(
    part: &'a NestedPart,
    visitor: &mut impl FnMut(VisitedPart<'a>) -> anyhow::Result<()>,
) -> anyhow::Result<()> {
    Ctx {
        depth: 0,
        in_preview: false,
    }
    .walk(part, None, &mut 0, visitor)
}

impl TextFormat {
    fn from_media_type(media_type: MediaType<'_>) -> Option<Self> {
        if media_type == TEXT_MARKDOWN {
            Some(Self::Markdown)
        } else if media_type == TEXT_PLAIN {
            Some(Self::Plain)
        } else {
            None
        }
    }
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

#[derive(Default, Clone, Copy)]
pub(super) struct Ctx {
    depth: usize,
    in_preview: bool,
}

impl Ctx {
    fn walk<'a>(
        self,
        part: &'a NestedPart,
        blurhash: Option<&'a str>,
        next_part_index: &mut usize,
        visitor: &mut impl FnMut(VisitedPart<'a>) -> anyhow::Result<()>,
    ) -> anyhow::Result<()> {
        if is_skipped(part.disposition()) {
            *next_part_index += leaf_count(part);
            return Ok(());
        }
        if MAX_DEPTH <= self.depth {
            *next_part_index += leaf_count(part);
            visitor(VisitedPart::Unknown(part))?;
            return Ok(());
        }
        let ctx = self.enter(part);
        match part {
            NestedPart::NullPart { .. } => *next_part_index += 1,
            // leaf
            NestedPart::SinglePart { .. } | NestedPart::ExternalPart { .. } => {
                let part_index = *next_part_index;
                *next_part_index += 1;
                match ctx.classify_leaf(part) {
                    Leaf::Text(content, format) => {
                        visitor(VisitedPart::Text { content, format })?;
                    }
                    Leaf::Attachment => {
                        visitor(VisitedPart::Attachment {
                            part,
                            index: PartIndex(u32::try_from(part_index)?),
                            preview: ctx.in_preview,
                            blurhash,
                        })?;
                    }
                    Leaf::Blurhash(_) => {}
                    Leaf::Ignored => {}
                    Leaf::Unknown => {
                        visitor(VisitedPart::Unknown(part))?;
                    }
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
                        let chosen = child_ctx.best_alternative(parts).map(|(i, _)| i);
                        if chosen.is_none() && child_ctx.children_coverage(parts).missing {
                            visitor(VisitedPart::Unknown(part))?;
                        }
                        for (i, child) in parts.iter().enumerate() {
                            if Some(i) == chosen {
                                child_ctx.walk(child, None, next_part_index, visitor)?;
                            } else {
                                *next_part_index += leaf_count(child);
                            }
                        }
                    }
                    PartSemantics::SingleUnit => {
                        if child_ctx.children_coverage(parts).is_full() {
                            child_ctx.walk_children(parts, next_part_index, visitor)?;
                        } else {
                            *next_part_index += parts.iter().map(leaf_count).sum::<usize>();
                            visitor(VisitedPart::Unknown(part))?;
                        }
                    }
                    PartSemantics::ProcessAll | PartSemantics::Custom(_) => {
                        child_ctx.walk_children(parts, next_part_index, visitor)?;
                    }
                }
            }
        }
        Ok(())
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
        next_part_index: &mut usize,
        visitor: &mut impl FnMut(VisitedPart<'a>) -> anyhow::Result<()>,
    ) -> anyhow::Result<()> {
        let blurhash = self.sibling_blurhash(parts);
        for (index, child) in parts.iter().enumerate() {
            let child_blurhash = match blurhash {
                Some((attachment, blurhash)) if attachment == index => Some(blurhash),
                _ => None,
            };
            self.walk(child, child_blurhash, next_part_index, visitor)?;
        }
        Ok(())
    }

    fn coverage(self, part: &NestedPart) -> Coverage {
        if is_skipped(part.disposition()) {
            return Coverage::default();
        }
        if MAX_DEPTH <= self.depth {
            return Coverage::MISSING;
        }
        let ctx = self.enter(part);
        match part {
            NestedPart::NullPart { .. } => Coverage::default(),
            NestedPart::SinglePart { .. } | NestedPart::ExternalPart { .. } => {
                match ctx.classify_leaf(part) {
                    Leaf::Text(..) => Coverage {
                        content: true,
                        ..Coverage::default()
                    },
                    Leaf::Attachment => Coverage {
                        content: !ctx.in_preview,
                        preview: ctx.in_preview,
                        ..Coverage::default()
                    },
                    Leaf::Blurhash(_) | Leaf::Ignored => Coverage::default(),
                    Leaf::Unknown => Coverage::MISSING,
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
                            Coverage::MISSING
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

    fn best_alternative(self, parts: &[NestedPart]) -> Option<(usize, Coverage)> {
        let mut best: Option<(usize, Coverage, u8)> = None;
        for (index, part) in parts.iter().enumerate() {
            let coverage = self.coverage(part);
            let rank = coverage.rank();
            let better = match best {
                Some((_, _, best_rank)) => best_rank < rank,
                None => 0 < rank,
            };
            if better {
                best = Some((index, coverage, rank));
            }
        }
        best.map(|(index, coverage, _)| (index, coverage))
    }

    fn classify_leaf(self, part: &NestedPart) -> Leaf<'_> {
        match part {
            NestedPart::SinglePart {
                disposition,
                content_type,
                content,
                ..
            } => {
                let media_type = MediaType::of(content_type);
                if self.in_preview {
                    if media_type == TEXT_BLURHASH {
                        str::from_utf8(content)
                            .map(Leaf::Blurhash)
                            .unwrap_or(Leaf::Ignored)
                    } else if media_type.has_type("text") {
                        Leaf::Ignored
                    } else {
                        Leaf::Attachment
                    }
                } else {
                    match TextFormat::from_media_type(media_type) {
                        Some(format) if *disposition != Disposition::Attachment => {
                            str::from_utf8(content)
                                .map(|text| Leaf::Text(text, format))
                                .unwrap_or(Leaf::Unknown)
                        }
                        _ => Leaf::Attachment,
                    }
                }
            }
            // Non-Air URLs are unknown for now
            NestedPart::ExternalPart { url, .. } => {
                if RemoteAttachmentId::is_canonical(url) {
                    Leaf::Attachment
                } else {
                    Leaf::Unknown
                }
            }
            NestedPart::NullPart { .. } | NestedPart::MultiPart { .. } => Leaf::Ignored,
        }
    }
}

/// Number of leaves in the subtree, independent of `MAX_DEPTH`.
fn leaf_count(part: &NestedPart) -> usize {
    // We assume that mimi_content already enforced a recursion bound.
    match part {
        NestedPart::NullPart { .. }
        | NestedPart::SinglePart { .. }
        | NestedPart::ExternalPart { .. } => 1,
        NestedPart::MultiPart { parts, .. } => parts.iter().map(leaf_count).sum(),
    }
}

#[derive(Default, Clone, Copy)]
struct Coverage {
    /// Something outside a preview is shown
    content: bool,
    /// A preview attachment is visited
    preview: bool,
    missing: bool,
}

impl Coverage {
    const MISSING: Self = Self {
        content: false,
        preview: false,
        missing: true,
    };

    fn merge(self, other: Self) -> Self {
        Self {
            content: self.content || other.content,
            preview: self.preview || other.preview,
            missing: self.missing || other.missing,
        }
    }

    fn is_full(self) -> bool {
        (self.content || self.preview) && !self.missing
    }

    /// Content beats preview, complete beats partial, anything beats nothing
    fn rank(self) -> u8 {
        match (self.content, self.missing, self.preview) {
            (true, false, _) => 4,
            (true, true, _) => 3,
            (false, false, true) => 2,
            (false, true, true) => 1,
            (false, _, false) => 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use mimi_content::{
        MimiContent,
        content_container::{EncryptionAlgorithm, HashAlgorithm},
    };
    use uuid::Uuid;

    use super::*;
    use crate::{AttachmentUrl, MimiContentExt};

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
        content
            .visit_parts(|part| {
                seen.push(match part {
                    VisitedPart::Text { content, format } => Seen::Text(content.to_owned(), format),
                    VisitedPart::Attachment {
                        index,
                        preview,
                        blurhash,
                        ..
                    } => Seen::Att {
                        index: index.0 as usize,
                        preview,
                        blurhash: blurhash.map(str::to_owned),
                    },
                    VisitedPart::Unknown(_) => Seen::Unknown,
                });
                Ok(())
            })
            .unwrap();
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
        assert_eq!(seen, [md("see"), att(1, true, None), att(2, true, None)]);
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
            assert_eq!(seen, [att(1, false, None)]);
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
    fn choose_one_unselected_alternative_keeps_its_index() {
        let seen = visit_part(all(vec![
            choose(vec![text("text/markdown", "a"), file()]),
            file(),
        ]));
        assert_eq!(seen, [md("a"), att(2, false, None)]);
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
    fn choose_one_preview_only_is_visited() {
        let seen = visit_part(choose(vec![external(Disposition::Preview, "image/webp")]));
        assert_eq!(seen, [att(0, true, None)]);
    }

    #[test]
    fn choose_one_in_preview_container() {
        let seen = visit_part(multi(
            Disposition::Preview,
            PartSemantics::ProcessAll,
            vec![choose(vec![
                external(Disposition::Render, "image/avif"),
                external(Disposition::Render, "image/jpeg"),
            ])],
        ));
        assert_eq!(seen, [att(0, true, None)]);
    }

    #[test]
    fn single_unit_in_preview_container() {
        let seen = visit_part(multi(
            Disposition::Preview,
            PartSemantics::ProcessAll,
            vec![unit(vec![
                external(Disposition::Render, "image/png"),
                text("text/markdown", "caption"),
            ])],
        ));
        assert_eq!(seen, [att(0, true, None)]);
    }

    #[test]
    fn choose_one_prefers_content_over_preview() {
        let seen = visit_part(choose(vec![
            external(Disposition::Preview, "image/webp"),
            text("text/markdown", "a"),
        ]));
        assert_eq!(seen, [md("a")]);
    }

    #[test]
    fn single_unit_full() {
        let seen = visit_part(unit(vec![text("text/markdown", "a"), file()]));
        assert_eq!(seen, [md("a"), att(1, false, None)]);
    }

    #[test]
    fn single_unit_with_unknown() {
        let seen = visit_part(all(vec![unit(vec![file(), invalid_utf8()]), file()]));
        assert_eq!(seen, [Seen::Unknown, att(2, false, None)]);
    }

    #[test]
    fn custom_semantics_like_process_all() {
        let seen = visit_part(multi(
            Disposition::Render,
            PartSemantics::Custom(42),
            vec![text("text/markdown", "a"), invalid_utf8(), file()],
        ));
        assert_eq!(seen, [md("a"), Seen::Unknown, att(2, false, None)]);
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
    fn depth_cap_after_skipped_disposition() {
        let mut part = text_with(Disposition::Reaction);
        for _ in 0..8 {
            part = all(vec![part]);
        }
        assert_eq!(visit_part(part), []);
    }

    #[test]
    fn index_is_independent_of_classification() {
        for (media_type, body) in [("text/html", "<b>"), ("text/markdown", "a")] {
            let seen = visit_part(all(vec![text(media_type, body), file()]));
            assert_eq!(seen.last(), Some(&att(1, false, None)));
        }
    }

    #[test]
    fn skipped_and_unknown_subtrees_consume_their_leaves() {
        // nulls count as leaves, empty multiparts as none
        let seen = visit_part(all(vec![null(), all(vec![]), file()]));
        assert_eq!(seen, [att(1, false, None)]);
        // chooseOne without a usable alternative
        let seen = visit_part(all(vec![choose(vec![invalid_utf8(), null()]), file()]));
        assert_eq!(seen, [Seen::Unknown, att(2, false, None)]);
        // depth cap
        let mut deep = all(vec![file(), file()]);
        for _ in 0..(MAX_DEPTH - 1) {
            deep = all(vec![deep]);
        }
        let seen = visit_part(all(vec![deep, file()]));
        assert_eq!(seen, [Seen::Unknown, att(2, false, None)]);
    }

    #[test]
    fn leaf_count_ignores_depth_cap() {
        let mut deep = file();
        for _ in 0..(MAX_DEPTH * 4) {
            deep = all(vec![deep, null()]);
        }
        assert_eq!(leaf_count(&deep), MAX_DEPTH * 4 + 1);
    }

    #[test]
    fn null_root() {
        assert_eq!(visit_part(null()), []);
    }
}
