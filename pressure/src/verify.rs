// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Sampled convergence checks. Instead of asserting the invariants of every
//! member against every other member (O(n^2), the approach used by
//! `test_harness`), a handful of members are drained and compared against
//! the hub's view, which is authoritative for chat membership since the hub
//! owns the group.

use std::collections::{HashMap, HashSet};

use aircommon::identifiers::UserId;
use aircoreclient::{ChatId, clients::CoreUser};

use crate::ops;

/// What every convergence check needs: the hub to compare against, the group,
/// the clients to resolve members in, and how wide to fan the checks out.
pub struct CheckContext<'a> {
    pub hub: &'a CoreUser,
    pub chat_id: ChatId,
    pub clients_by_id: &'a HashMap<UserId, &'a CoreUser>,
    pub concurrency: usize,
}

/// The hub's state at one instant, taken once so a batch of checks can run
/// concurrently against it.
///
/// Snapshotting is not just an optimisation: the checks are fanned out, and
/// draining the hub from each of them would mean concurrent drains on one
/// member, which corrupts its queue ratchet.
#[derive(Clone)]
pub struct HubView {
    pub epoch: u64,
    pub participants: HashSet<UserId>,
}

/// How many times a member may drain while trying to reach the hub's epoch.
///
/// One pass is not enough once steps run concurrently: a round lands many
/// commits, and the hub's own outbound service retries pending ones in the
/// background, so a member can drain fully and still be behind through no
/// fault of its own.
const SETTLE_ATTEMPTS: usize = 5;

/// Drains the hub and snapshots its view. Must be called from the sequential
/// part of the loop, never from inside a fan-out.
pub async fn hub_view(hub: &CoreUser, chat_id: ChatId) -> anyhow::Result<HubView> {
    ops::drain(hub).await?;
    let participants = hub
        .chat_participants(chat_id)
        .await
        .ok_or_else(|| anyhow::anyhow!("hub has no view of its own chat"))?;
    let epoch = hub
        .group_epoch_and_own_index(chat_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("hub has no local group state for its own chat"))?
        .0;
    Ok(HubView {
        epoch,
        participants,
    })
}

/// Compares `member`'s local view of `chat_id` against `view`, draining it
/// until it reaches the hub's epoch. Returns a description of any divergence
/// found, or `None` if the member's state is consistent with the hub's.
pub async fn check_member(
    view: &HubView,
    chat_id: ChatId,
    member: &CoreUser,
    member_id: &UserId,
) -> anyhow::Result<Option<String>> {
    if !view.participants.contains(member_id) {
        // The hub removed or saw this member leave. We don't require anything
        // further from a departed member's local state.
        ops::drain(member).await?;
        return Ok(None);
    }

    let mut member_epoch = None;
    for _ in 0..SETTLE_ATTEMPTS {
        ops::drain(member).await?;
        let Some((epoch, _own_index)) = member.group_epoch_and_own_index(chat_id).await? else {
            continue;
        };
        member_epoch = Some(epoch);
        if epoch >= view.epoch {
            break;
        }
    }

    let Some(member_epoch) = member_epoch else {
        return Ok(Some(format!(
            "{member_id:?} is an active hub participant but has no local group state"
        )));
    };

    if member_epoch < view.epoch {
        return Ok(Some(format!(
            "{member_id:?} epoch {member_epoch} is still behind hub epoch {} after \
             {SETTLE_ATTEMPTS} catch-up attempts",
            view.epoch
        )));
    }

    if member_epoch > view.epoch {
        // The member has moved past the epoch the snapshot was taken at, so
        // the group changed under us. Membership at two different epochs is
        // legitimately different, so there is nothing to compare.
        return Ok(None);
    }

    let member_participants = member
        .chat_participants(chat_id)
        .await
        .ok_or_else(|| anyhow::anyhow!("{member_id:?} has no local view of the chat"))?;
    if member_participants != view.participants {
        return Ok(Some(format!(
            "{member_id:?} membership set diverges from hub at epoch {}: \
             missing {:?}, extra {:?}",
            view.epoch,
            view.participants.difference(&member_participants),
            member_participants.difference(&view.participants),
        )));
    }

    Ok(None)
}
