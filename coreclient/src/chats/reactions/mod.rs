// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Local state of emoji reactions.
//!
//! A reaction is a MIMI message (disposition "reaction") whose `in_reply_to`
//! references the reacted-to message. Reactions are not displayed as message
//! tiles; instead their aggregated state is kept in the `reaction` table and
//! surfaced alongside the message they target.

use aircommon::{
    crypto::secrets::Secret,
    identifiers::{MimiId, UserId},
    time::TimeStamp,
};
use mimi_content::{Disposition, MimiContent, NestedPart};

use crate::ChatId;

mod persistence;

/// MIME content type used for the emoji payload of a reaction, per
/// draft-ietf-mimi-content.
const REACTION_CONTENT_TYPE: &str = "text/plain;charset=utf-8";

fn random_salt() -> anyhow::Result<Vec<u8>> {
    Ok(Secret::<16>::random()?.secret().to_vec())
}

/// Build the MIMI content for adding `emoji` as a reaction to `target`.
pub(crate) fn reaction_content(target: &MimiId, emoji: &str) -> anyhow::Result<MimiContent> {
    Ok(MimiContent {
        salt: random_salt()?,
        in_reply_to: Some(target.as_slice().to_vec()),
        nested_part: NestedPart::SinglePart {
            disposition: Disposition::Reaction,
            language: String::new(),
            content_type: REACTION_CONTENT_TYPE.to_owned(),
            content: emoji.as_bytes().to_vec(),
        },
        ..Default::default()
    })
}

/// Build the MIMI content for retracting a previously sent reaction.
///
/// Per the draft, a retraction `replaces` the reaction with an empty body.
pub(crate) fn reaction_tombstone_content(
    target: &MimiId,
    reaction_mimi_id: &MimiId,
) -> anyhow::Result<MimiContent> {
    Ok(MimiContent {
        salt: random_salt()?,
        in_reply_to: Some(target.as_slice().to_vec()),
        replaces: Some(reaction_mimi_id.as_slice().to_vec()),
        nested_part: NestedPart::SinglePart {
            disposition: Disposition::Reaction,
            language: String::new(),
            content_type: REACTION_CONTENT_TYPE.to_owned(),
            content: Vec::new(),
        },
        ..Default::default()
    })
}

/// A single emoji reaction by one user on one message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Reaction {
    /// The reaction message's own MimiId. Used as the join key for retraction:
    /// removing a reaction sends a message that `replaces` this MimiId.
    pub(crate) reaction_mimi_id: MimiId,
    /// The message being reacted to.
    pub(crate) target_mimi_id: MimiId,
    pub(crate) chat_id: ChatId,
    pub(crate) sender: UserId,
    pub(crate) emoji: String,
    pub(crate) created_at: TimeStamp,
}

impl Reaction {
    pub(crate) fn new(
        reaction_mimi_id: MimiId,
        target_mimi_id: MimiId,
        chat_id: ChatId,
        sender: UserId,
        emoji: String,
        created_at: TimeStamp,
    ) -> Self {
        Self {
            reaction_mimi_id,
            target_mimi_id,
            chat_id,
            sender,
            emoji,
            created_at,
        }
    }
}
