// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use airprotos::client::self_group::SelfGroupAppMessage;
use mimi_content::{MimiContent, content_container::NestedPart};

use crate::link_preview::{carry_over_link_preview, editable_text};

pub mod media_type;
pub(crate) mod walk;

pub trait MimiContentExt {
    /// Visits the parts of the body a client acts on, depth-first.
    ///
    /// Containers follow their part semantics: `processAll` visits all children, `chooseOne` only
    /// the alternative that can be shown best, and `singleUnit` all children or, if any of them
    /// can't be shown, a single `Unknown` for the whole unit. Parts with disposition `reaction`,
    /// `icon` or `profile` are not visited.
    ///
    /// Part indices count the non-container parts depth-first. Every such part
    /// counts, whether visited or not.
    ///
    /// Stops at the first error of `visitor` and returns it.
    fn visit_parts<'a>(
        &'a self,
        visitor: impl FnMut(VisitedPart<'a>) -> anyhow::Result<()>,
    ) -> anyhow::Result<()>;

    /// Part at `index`
    fn part_at(&self, index: PartIndex) -> Option<&NestedPart>;

    /// Calls `f` on every `ExternalPart` in the tree, regardless of semantics
    /// and selection.
    fn visit_external_parts_mut(
        &mut self,
        f: impl FnMut(&mut NestedPart) -> anyhow::Result<()>,
    ) -> anyhow::Result<()>;

    /// The self-group application message this content carries, if any.
    fn self_group_message(&self) -> Option<SelfGroupAppMessage>;

    /// Text of a plain markdown message or of a link preview message.
    ///
    /// Returns `None` for any other body.
    fn markdown_text(&self) -> Option<&str>;

    /// Copies the link preview of `original` if its url is still a link in the text.
    fn with_link_preview_of(self, original: &MimiContent) -> Self
    where
        Self: Sized;
}

/// Zero-indexed depth-first index of a part in a MIMI body.
///
/// Defined in
/// <https://www.ietf.org/archive/id/draft-ietf-mimi-content-10.html#name-message-bodies>.
///
/// See also for an example showing that containers count too:
/// <https://www.ietf.org/archive/id/draft-ietf-mimi-content-10.html#name-complicated-nested-example>
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PartIndex(pub(crate) u32);

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
        /// Used to derive the AttachmentId from message id and this index
        index: PartIndex,
        /// disposition preview, or inside a preview container
        preview: bool,
        /// text/blurhash preview sibling
        blurhash: Option<&'a str>,
    },
    /// A link preview card
    LinkPreview { card: &'a [u8], index: PartIndex },
    /// Meant as content, but not renderable.
    Unknown(&'a NestedPart),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextFormat {
    Markdown,
    Plain,
}

impl MimiContentExt for MimiContent {
    fn visit_parts<'a>(
        &'a self,
        mut visitor: impl FnMut(VisitedPart<'a>) -> anyhow::Result<()>,
    ) -> anyhow::Result<()> {
        walk::walk(&self.nested_part, &mut visitor)
    }

    fn part_at(&self, index: PartIndex) -> Option<&NestedPart> {
        walk::part_at(&self.nested_part, index)
    }

    fn visit_external_parts_mut(
        &mut self,
        mut f: impl FnMut(&mut NestedPart) -> anyhow::Result<()>,
    ) -> anyhow::Result<()> {
        visit_external_parts_mut_impl(&mut self.nested_part, &mut f)
    }

    fn self_group_message(&self) -> Option<SelfGroupAppMessage> {
        SelfGroupAppMessage::from_mimi_content(self)
    }

    fn markdown_text(&self) -> Option<&str> {
        editable_text(&self.nested_part)
    }

    fn with_link_preview_of(self, original: &MimiContent) -> Self {
        carry_over_link_preview(original, self)
    }
}

fn visit_external_parts_mut_impl(
    part: &mut NestedPart,
    f: &mut impl FnMut(&mut NestedPart) -> anyhow::Result<()>,
) -> anyhow::Result<()> {
    match part {
        NestedPart::NullPart { .. } | NestedPart::SinglePart { .. } => Ok(()),
        NestedPart::ExternalPart { .. } => f(part),
        NestedPart::MultiPart { parts, .. } => {
            for part in parts {
                visit_external_parts_mut_impl(part, f)?;
            }
            Ok(())
        }
    }
}
