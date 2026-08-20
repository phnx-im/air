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
use aircoreclient::{ChatId, UserProfile, clients::CoreUser};

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
///
/// Once the member has caught up to the snapshot's epoch, three invariants
/// are checked, and every failure is retried within the settle attempts
/// since a drain may still be delivering what is missing:
///
/// * its participant set matches the hub's,
/// * its participant set matches its own ratchet tree (the two halves of
///   group state -- room state and MLS tree -- must never drift apart; this
///   is the invariant the `ds_welcome_info` room-state bug broke),
/// * every participant's profile resolves to its real "Stress NNNN" display
///   name rather than the uuid-derived fallback a missing or undecryptable
///   profile key produces (the invariant the profile-key epoch bug broke).
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

    let mut last_failure = None;
    for attempt in 0..SETTLE_ATTEMPTS {
        if attempt > 0 {
            // Give the DS fan-out and the members' background retries a
            // moment; an immediate retry would only re-read an empty queue.
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        }
        ops::drain(member).await?;
        let Some((epoch, _own_index)) = member.group_epoch_and_own_index(chat_id).await? else {
            last_failure = Some(format!(
                "{member_id:?} is an active hub participant but has no local group state"
            ));
            continue;
        };
        if epoch < view.epoch {
            last_failure = Some(format!(
                "{member_id:?} epoch {epoch} is still behind hub epoch {} after \
                 catch-up attempts",
                view.epoch
            ));
            continue;
        }
        if epoch > view.epoch {
            // The member has moved past the epoch the snapshot was taken at,
            // so the group changed under us. Membership at two different
            // epochs is legitimately different: nothing to compare.
            return Ok(None);
        }
        match member_invariants(view, chat_id, member, member_id).await? {
            None => return Ok(None),
            Some(failure) => last_failure = Some(failure),
        }
    }

    Ok(last_failure)
}

/// The invariants checked once a member is at the hub snapshot's epoch.
/// Returns a description of the first violated one.
async fn member_invariants(
    view: &HubView,
    chat_id: ChatId,
    member: &CoreUser,
    member_id: &UserId,
) -> anyhow::Result<Option<String>> {
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

    // Room state and the member's own ratchet tree must describe the same
    // group.
    let tree_members = member
        .group_members(chat_id)
        .await
        .ok_or_else(|| anyhow::anyhow!("{member_id:?} has no local MLS group state"))?;
    if tree_members != member_participants {
        return Ok(Some(format!(
            "{member_id:?} room state diverges from its own ratchet tree at \
             epoch {}: in room state only {:?}, in tree only {:?}",
            view.epoch,
            member_participants.difference(&tree_members),
            tree_members.difference(&member_participants),
        )));
    }

    // Every participant's profile must resolve. The fleet gives each member
    // a "Stress NNNN" display name at creation, so the uuid-derived fallback
    // (or any other name) means the profile key was missing or did not
    // decrypt.
    for participant in &view.participants {
        let profile = member.user_profile(participant).await;
        let name: &str = profile.display_name.as_ref();
        if profile == UserProfile::from_user_id(participant) || !name.starts_with("Stress ") {
            return Ok(Some(format!(
                "{member_id:?} resolves {participant:?} to \"{name}\" instead \
                 of its Stress display name: its profile key is missing or \
                 did not decrypt"
            )));
        }
    }

    Ok(None)
}
