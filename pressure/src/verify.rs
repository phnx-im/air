// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Sampled convergence checks. Instead of asserting the invariants of every
//! member against every other member (O(n^2), the approach used by
//! `test_harness`), a handful of members are drained and compared against
//! the hub's view, which is authoritative for chat membership since the hub
//! is always the committer of structural changes in the star topology.

use aircommon::identifiers::UserId;
use aircoreclient::{ChatId, clients::CoreUser};

use crate::ops;

/// Compares `member`'s local view of `chat_id` against the hub's view after
/// draining `member`'s queue. Returns a human-readable description of any
/// divergence found, or `None` if the member's state is consistent with the
/// hub's.
pub async fn check_member(
    hub: &CoreUser,
    hub_chat_id: ChatId,
    member: &CoreUser,
    member_id: &UserId,
) -> anyhow::Result<Option<String>> {
    ops::drain(member).await?;

    let hub_participants = hub
        .chat_participants(hub_chat_id)
        .await
        .ok_or_else(|| anyhow::anyhow!("hub has no view of its own chat"))?;
    let hub_epoch = hub
        .group_epoch_and_own_index(hub_chat_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("hub has no local group state for its own chat"))?
        .0;

    let should_be_active = hub_participants.contains(member_id);
    let member_epoch = member.group_epoch_and_own_index(hub_chat_id).await?;

    if !should_be_active {
        // The hub removed or saw this member leave. We don't require
        // anything further from a departed member's local state.
        return Ok(None);
    }

    let Some((member_epoch, _own_index)) = member_epoch else {
        return Ok(Some(format!(
            "{member_id:?} is an active hub participant but has no local group state"
        )));
    };

    if member_epoch != hub_epoch {
        return Ok(Some(format!(
            "{member_id:?} epoch {member_epoch} diverges from hub epoch {hub_epoch}"
        )));
    }

    let member_participants = member
        .chat_participants(hub_chat_id)
        .await
        .ok_or_else(|| anyhow::anyhow!("{member_id:?} has no local view of the chat"))?;
    if member_participants != hub_participants {
        return Ok(Some(format!(
            "{member_id:?} membership set diverges from hub at epoch {hub_epoch}: \
             missing {:?}, extra {:?}",
            hub_participants.difference(&member_participants),
            member_participants.difference(&hub_participants),
        )));
    }

    Ok(None)
}
