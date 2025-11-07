// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Duration, Utc};
use openmls::{
    prelude::{GroupEpoch, SignaturePublicKey},
    treesync::RatchetTree,
};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
struct PastGroupState {
    nodes: RatchetTree,
    creation_time: DateTime<Utc>,
    potential_joiners: HashSet<SignaturePublicKey>,
}

impl PastGroupState {
    /// Create a new [`PastGroupState`] with the creation time set to now.
    fn new(nodes: RatchetTree, potential_joiners: &[SignaturePublicKey]) -> Self {
        let mut potential_joiners_set = HashSet::with_capacity(potential_joiners.len());
        for joiner in potential_joiners {
            potential_joiners_set.insert(joiner.clone());
        }
        Self {
            nodes,
            creation_time: Utc::now(),
            potential_joiners: potential_joiners_set,
        }
    }

    /// Get the nodes of this group state.
    fn nodes(&self) -> &RatchetTree {
        &self.nodes
    }

    /// Returns true if the given joiner is authorized to obtain this group state.
    fn is_authorized(&self, joiner: &SignaturePublicKey) -> bool {
        self.potential_joiners.contains(joiner)
    }

    /// Returns true if the creation of this group state is at least
    /// `expiration_time` seconds ago.
    fn has_expired(&self, expiration_time: Duration) -> bool {
        Utc::now() - expiration_time >= self.creation_time
    }
}

#[derive(Serialize, Deserialize, Default)]
pub(super) struct PastGroupStates {
    past_group_states: HashMap<GroupEpoch, PastGroupState>,
}

// TODO: With every processing action, check for expired time stamps
// TODO: Remove the removal logic upon removal of a group member

impl PastGroupStates {
    /// Add a new group state with the given nodes for the given epoch
    /// retrievable by any of the `potential_joiners`.
    pub(super) fn add_state(
        &mut self,
        epoch: GroupEpoch,
        nodes: RatchetTree,
        potential_joiners: &[SignaturePublicKey],
    ) {
        if potential_joiners.is_empty() {
            return;
        }
        self.past_group_states
            .insert(epoch, PastGroupState::new(nodes, potential_joiners));
    }

    /// Get the nodes of the past group state with the given epoch for the given
    /// joiner. Returns `None` if there is no past group state for that epoch
    /// and the given joiner.
    pub(crate) fn get_for_joiner(
        &self,
        epoch: &GroupEpoch,
        joiner: &SignaturePublicKey,
    ) -> Option<&RatchetTree> {
        self.past_group_states
            .get(epoch)
            .and_then(|past_group_state| {
                // Check if the joiner is authorized to get these nodes.
                if past_group_state.is_authorized(joiner) {
                    Some(past_group_state.nodes())
                } else {
                    None
                }
            })
    }

    /// Remove all past group states where the time of creation was longer than
    /// `expiration_time` in seconds ago.
    pub(super) fn remove_expired_states(&mut self, expiration_time: Duration) {
        let mut expired_epochs = vec![];
        for (epoch, past_group_state) in self.past_group_states.iter() {
            if past_group_state.has_expired(expiration_time) {
                expired_epochs.push(*epoch)
            }
        }
        for expired_epoch in expired_epochs {
            self.past_group_states.remove(&expired_epoch);
        }
    }
}
