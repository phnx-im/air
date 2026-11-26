// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::identifiers::UserId;
use airprotos::client::self_group::SelfGroupAppMessage;
use mimi_content::{
    Disposition, MimiContent,
    content_container::{NestedPart, PartSemantics},
};

use crate::{chats::messages::mentions::extract_mentions, link_preview::editable_text};

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

    /// Mutable part at `index`
    fn part_at_mut(&mut self, index: PartIndex) -> Option<&mut NestedPart>;

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

    /// Whether this is an attachment message with a caption slot.
    ///
    /// The slot is the first part of the body. It is a markdown part, or a
    /// null part without a caption.
    fn has_caption_slot(&self) -> bool;

    /// Caption of an attachment message with a caption slot.
    ///
    /// It is empty without a caption. Returns `None` if the message has no
    /// caption slot.
    fn caption(&self) -> Option<&str>;

    /// Users that the markdown body or the caption links to, not yet checked
    /// against the chat members. This parses the whole text and is therefore
    /// expensive. Call it only once per message.
    fn extract_mentioned_users(&self) -> Vec<UserId> {
        self.markdown_text()
            .or_else(|| self.caption())
            .map(extract_mentions)
            .unwrap_or_default()
    }
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

    fn part_at_mut(&mut self, index: PartIndex) -> Option<&mut NestedPart> {
        walk::part_at_mut(&mut self.nested_part, index)
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

    fn has_caption_slot(&self) -> bool {
        let has_slot = match &self.nested_part {
            NestedPart::MultiPart {
                disposition: Disposition::Render,
                part_semantics: PartSemantics::ProcessAll,
                parts,
                ..
            } => parts.first().is_some_and(is_caption_slot),
            _ => false,
        };
        if !has_slot {
            return false;
        }
        let mut has_attachment = false;
        let _infallible = self.visit_parts(|part| {
            if let VisitedPart::Attachment { preview: false, .. } = part {
                has_attachment = true;
            }
            Ok(())
        });
        has_attachment
    }

    fn caption(&self) -> Option<&str> {
        if !self.has_caption_slot() {
            return None;
        }
        let NestedPart::MultiPart { parts, .. } = &self.nested_part else {
            return None;
        };
        match parts.first()? {
            NestedPart::SinglePart { content, .. } => str::from_utf8(content).ok(),
            _ => Some(""),
        }
    }
}

/// Whether `part` can be the caption slot of an attachment message.
pub(crate) fn is_caption_slot(part: &NestedPart) -> bool {
    match part {
        NestedPart::NullPart { .. } => true,
        NestedPart::SinglePart {
            disposition: Disposition::Render,
            content_type,
            ..
        } => content_type == media_type::TEXT_MARKDOWN,
        _ => false,
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

#[cfg(test)]
mod tests {
    use aircommon::identifiers::RemoteAttachmentId;
    use mimi_content::content_container::{EncryptionAlgorithm, HashAlgorithm};
    use uuid::Uuid;

    use super::*;
    use crate::AttachmentUrl;

    fn content(nested_part: NestedPart) -> MimiContent {
        MimiContent {
            nested_part,
            ..Default::default()
        }
    }

    fn markdown(s: &str) -> NestedPart {
        NestedPart::SinglePart {
            disposition: Disposition::Render,
            language: String::new(),
            content_type: "text/markdown".to_owned(),
            content: s.as_bytes().to_vec(),
        }
    }

    fn null() -> NestedPart {
        NestedPart::NullPart {
            disposition: Disposition::Render,
            language: String::new(),
        }
    }

    fn external(disposition: Disposition) -> NestedPart {
        let url = AttachmentUrl::new(RemoteAttachmentId::new(Uuid::new_v4()), None);
        NestedPart::ExternalPart {
            disposition,
            language: String::new(),
            content_type: "image/png".to_owned(),
            url: url.to_string(),
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

    fn render_all(parts: Vec<NestedPart>) -> NestedPart {
        NestedPart::MultiPart {
            disposition: Disposition::Render,
            language: String::new(),
            part_semantics: PartSemantics::ProcessAll,
            parts,
        }
    }

    #[test]
    fn caption_slot_with_markdown_and_attachment() {
        let content = content(render_all(vec![
            markdown("caption"),
            external(Disposition::Attachment),
        ]));
        assert!(content.has_caption_slot());
        assert_eq!(content.caption(), Some("caption"));
    }

    #[test]
    fn caption_slot_with_null_part_and_attachment() {
        let content = content(render_all(vec![null(), external(Disposition::Attachment)]));
        assert!(content.has_caption_slot());
        assert_eq!(content.caption(), Some(""));
    }

    #[test]
    fn flat_attachment_has_no_caption_slot() {
        let content = content(external(Disposition::Attachment));
        assert!(!content.has_caption_slot());
    }

    #[test]
    fn plain_markdown_has_no_caption_slot() {
        let content = content(markdown("hi"));
        assert!(!content.has_caption_slot());
    }

    #[test]
    fn slot_without_attachment_is_no_caption_slot() {
        let content = content(render_all(vec![markdown("caption"), markdown("more")]));
        assert!(!content.has_caption_slot());
    }

    #[test]
    fn slot_with_only_a_preview_is_no_caption_slot() {
        let content = content(render_all(vec![
            markdown("caption"),
            external(Disposition::Preview),
        ]));
        assert!(!content.has_caption_slot());
    }
}
