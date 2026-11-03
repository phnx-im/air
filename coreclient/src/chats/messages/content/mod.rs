// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::identifiers::UserId;
use airprotos::client::self_group::SelfGroupAppMessage;
use anyhow::ensure;
use mimi_content::{
    MimiContent,
    content_container::{NestedPart, PartSemantics},
};
use openmls::group::GroupId;

mod walk;

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

impl MimiContentExt for MimiContent {
    fn mimi_id(&self, sender: &UserId, group_id: &GroupId) -> anyhow::Result<Vec<u8>> {
        Ok(self.message_id(sender.to_bytes()?.as_slice(), group_id.as_slice())?)
    }

    fn visit_parts<'a>(&'a self, mut visitor: impl FnMut(VisitedPart<'a>)) {
        walk::walk(&self.nested_part, &mut visitor);
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
