// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::identifiers::UserId;
use anyhow::ensure;
use mimi_content::{
    MimiContent,
    content_container::{NestedPart, PartSemantics},
};
use openmls::group::GroupId;

pub trait MimiContentExt {
    fn visit_attachments(
        &self,
        visitor: impl FnMut(&NestedPart) -> anyhow::Result<()>,
    ) -> anyhow::Result<()>;

    fn visit_attachments_mut(
        &mut self,
        visitor: impl FnMut(&mut NestedPart) -> anyhow::Result<()>,
    ) -> anyhow::Result<()>;

    fn mimi_id(&self, sender: &UserId, group_id: &GroupId) -> anyhow::Result<Vec<u8>>;
}

impl MimiContentExt for MimiContent {
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

    fn mimi_id(&self, sender: &UserId, group_id: &GroupId) -> anyhow::Result<Vec<u8>> {
        Ok(self.message_id(sender.to_bytes()?.as_slice(), group_id.as_slice())?)
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
