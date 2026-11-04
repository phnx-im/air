// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::identifiers::UserId;
use airprotos::client::self_group::SelfGroupAppMessage;
use mimi_content::{MimiContent, content_container::NestedPart};
use openmls::group::GroupId;

pub mod media_type;
mod walk;

pub trait MimiContentExt {
    fn mimi_id(&self, sender: &UserId, group_id: &GroupId) -> anyhow::Result<Vec<u8>>;

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

    /// Calls `f` on every `ExternalPart` in the tree, regardless of semantics
    /// and selection.
    fn visit_external_parts_mut(
        &mut self,
        f: impl FnMut(&mut NestedPart) -> anyhow::Result<()>,
    ) -> anyhow::Result<()>;

    /// The self-group application message this content carries, if any.
    fn self_group_message(&self) -> Option<SelfGroupAppMessage>;
}

/// Depth-first index among the non-container parts of a MIMI body.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PartIndex(pub(crate) u32);

impl PartIndex {
    /// Index of the first part of a body.
    pub(crate) const FIRST: PartIndex = PartIndex(0);
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
        /// Used to derive the AttachmentId from message id and this index
        index: PartIndex,
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

impl MimiContentExt for MimiContent {
    fn mimi_id(&self, sender: &UserId, group_id: &GroupId) -> anyhow::Result<Vec<u8>> {
        Ok(self.message_id(sender.to_bytes()?.as_slice(), group_id.as_slice())?)
    }

    fn visit_parts<'a>(
        &'a self,
        mut visitor: impl FnMut(VisitedPart<'a>) -> anyhow::Result<()>,
    ) -> anyhow::Result<()> {
        walk::walk(&self.nested_part, &mut visitor)
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
